//! `StoreTagService` — the orchestration seam for the `context_tag` op (vnc-045).
//!
//! Sequences, in a load-bearing order (ADR-008 / ADR-009):
//!   1. `check_write_rate` — the ONE live throttle (UdsSession exempt, handled in the gateway),
//!   2. dispatch to the correct single-row store primitive per `TagAction`,
//!   3. emit exactly ONE fire-and-forget `context_tag` audit event AFTER the write commits.
//!
//! This is the directly-constructible seam where orchestration + audit are tested; the
//! `context_tag` `#[tool]` handler is NOT unit-constructible (needs a live `RequestContext`,
//! #5468), so the handler proves only route/format in the Stage-3c integration suite.
//!
//! Value-opacity (SD-8, R-04): the service NEVER interprets the tag value — no allow-list,
//! no `evaluate(tag)`, no `validate_outcome_tags`, no `min_trust_level`, no config. It writes
//! the tag verbatim. The lifecycle guard is applied UPSTREAM by the handler (which holds the
//! loaded `EntryRecord`); the service assumes an already-guarded, already-authorized call.

// rationale: this module is a forward-wired seam — its whole public surface is consumed by the
// `context_tag` #[tool] handler landing in vnc-045 Wave 3 (mcp/tools.rs). Until that delegate
// lands, non-test builds see the surface as unused. The seam IS exercised now by the
// store_tag_tests.rs seam suite. Remove this allow when the Wave 3 handler read lands.
#![allow(dead_code)]

use std::sync::Arc;

use unimatrix_core::{CoreError, Store};
use unimatrix_store::StoreError;

use crate::infra::audit::{AuditEvent, AuditLog, Outcome};
use crate::services::gateway::SecurityGateway;
use crate::services::{AuditContext, CallerId, ServiceError};

/// The client-supplied verb for a `context_tag` mutation.
///
/// A first-class client value (add/remove/replace); NOT split at the capability layer
/// (no `Capability::Tag` — a single `Capability::Write` gate covers all three, ADR-008).
/// Shared between the handler (parses the wire string) and the service (dispatches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagAction {
    Add,
    Remove,
    Replace,
}

impl TagAction {
    /// Audit/metadata string form — a variant STRING, never an integer (#4366).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            TagAction::Add => "add",
            TagAction::Remove => "remove",
            TagAction::Replace => "replace",
        }
    }

    /// Parse the client-supplied `action` wire string. Returns `None` on an unknown verb
    /// (the handler maps that to an `invalid_params` rejection before reaching the service).
    pub(crate) fn parse(s: &str) -> Option<TagAction> {
        match s {
            "add" => Some(TagAction::Add),
            "remove" => Some(TagAction::Remove),
            "replace" => Some(TagAction::Replace),
            _ => None,
        }
    }
}

/// Outcome of a successful `context_tag` mutation, returned to the handler for formatting.
#[derive(Debug, Clone)]
pub(crate) struct TagResult {
    pub action: TagAction,
    pub tag: String,
    pub namespace: Option<String>,
    /// The evicted prior value: the client's `tag` on `remove` (intent-of-record, ADR-009),
    /// the evicted `namespace:*` prior on `replace`, or `None` on `add` / no-prior replace.
    pub prior_value: Option<String>,
}

/// Orchestration seam between the `context_tag` handler and the store primitives.
#[derive(Clone)]
pub(crate) struct StoreTagService {
    store: Arc<Store>,
    gateway: Arc<SecurityGateway>,
    audit: Arc<AuditLog>,
}

impl StoreTagService {
    pub(crate) fn new(
        store: Arc<Store>,
        gateway: Arc<SecurityGateway>,
        audit: Arc<AuditLog>,
    ) -> Self {
        StoreTagService {
            store,
            gateway,
            audit,
        }
    }

    /// Throttle → dispatch to the store primitive → fire-and-forget audit. See module docs
    /// for the binding ordering. Returns `RateLimited` BEFORE any write when throttled.
    pub(crate) async fn tag(
        &self,
        id: u64,
        action: TagAction,
        tag: String,
        namespace: Option<String>,
        audit_ctx: &AuditContext,
        caller_id: &CallerId,
    ) -> Result<TagResult, ServiceError> {
        // Step 0: the one live throttle — BEFORE any work (mirror store_correct.rs:29).
        // UdsSession exemption is handled inside check_write_rate. On exceed the `?` returns
        // ServiceError::RateLimited with NO store write and NO audit event.
        self.gateway.check_write_rate(caller_id)?;

        // NOTE: NO value-hygiene here — no allow-list, no evaluate(tag). Value-opacity
        // (SD-8, R-04). Lifecycle guard already applied by the handler (holds the EntryRecord).

        // Step 1: dispatch to the store primitive → (prior_value, new_value).
        let (prior_value, new_value): (Option<String>, Option<String>) = match action {
            TagAction::Add => {
                self.store.add_tag(id, &tag).await.map_err(map_store_err)?;
                (None, Some(tag.clone()))
            }
            TagAction::Remove => {
                self.store
                    .remove_tag(id, &tag)
                    .await
                    .map_err(map_store_err)?;
                // ADR-009: prior_value MANDATORY and non-null on remove — the client named the
                // exact tag. Use the client's tag as prior_value regardless of whether a row
                // existed (intent-of-record); remove has no new_value.
                (Some(tag.clone()), None)
            }
            TagAction::Replace => match &namespace {
                // ns is non-empty + LIKE-safe (handler-guaranteed).
                Some(ns) => {
                    let prior = self
                        .store
                        .replace_tag(id, ns, &tag)
                        .await
                        .map_err(map_store_err)?;
                    (prior, Some(tag.clone())) // prior non-null iff a prior existed
                }
                // Colon-less / null-namespace → DEGRADE TO ADD (ADR-004 edge case): a pure
                // insert, prior_value:null, NEVER a hard error.
                None => {
                    self.store.add_tag(id, &tag).await.map_err(map_store_err)?;
                    (None, Some(tag.clone()))
                }
            },
        };

        // Step 2: build the audit event. session_id captured HERE, before spawn (#4388/#4389).
        let session_id = audit_ctx.session_id.clone().unwrap_or_default();
        let metadata_str =
            match build_tag_metadata(action.as_str(), &namespace, &tag, &prior_value, &new_value) {
                Ok(s) => s,
                Err(e) => {
                    // R-03 / #5468: do NOT emit the "{}" sentinel. The mutation already committed;
                    // accept the rare audit gap over a corrupt record. Return success, skip spawn.
                    tracing::warn!(error = %e, entry_id = id,
                    "context_tag audit metadata serialize failed; SKIPPING audit event");
                    return Ok(TagResult {
                        action,
                        tag,
                        namespace,
                        prior_value,
                    });
                }
            };

        let event = AuditEvent {
            event_id: 0, // assigned by the sink
            timestamp: 0,
            session_id,
            agent_id: audit_ctx.caller_id.clone(),
            operation: "context_tag".to_string(),
            target_ids: vec![id],
            outcome: Outcome::Success,
            detail: format!("context_tag {} on #{id}", action.as_str()),
            capability_used: "write".to_string(), // == Capability::Write audit str
            metadata: metadata_str,
            ..AuditEvent::default() // credential_type "none", agent_attribution ""
        };

        // Step 3: fire-and-forget AFTER commit (mirror store_correct.rs:98-102). ONE event.
        {
            let audit = Arc::clone(&self.audit);
            tokio::spawn(async move {
                let _ = audit.log_event_async(event).await;
            });
        }

        Ok(TagResult {
            action,
            tag,
            namespace,
            prior_value,
        })
    }
}

/// Serialize the `context_tag` audit metadata to a JSON string.
///
/// A well-formed object `{action, namespace, tag, prior_value, new_value}`. `None` fields
/// emit an explicit JSON `null` (forensic clarity — not key omission). `action` is a STRING,
/// never an integer (#4366). On a serialize error the caller warns and SKIPS the event; the
/// `"{}"` sentinel is NEVER emitted (R-03 / #5468).
fn build_tag_metadata(
    action: &str,
    namespace: &Option<String>,
    tag: &str,
    prior_value: &Option<String>,
    new_value: &Option<String>,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&serde_json::json!({
        "action": action,
        "namespace": namespace,
        "tag": tag,
        "prior_value": prior_value,
        "new_value": new_value,
    }))
}

/// Map a store-primitive error into the service error type.
fn map_store_err(e: StoreError) -> ServiceError {
    ServiceError::Core(CoreError::Store(e))
}

#[cfg(test)]
#[path = "store_tag_tests.rs"]
mod store_tag_tests;
