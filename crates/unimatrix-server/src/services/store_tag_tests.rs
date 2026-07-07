//! `StoreTagService` seam tests (vnc-045). Directly-constructible over a real store +
//! gateway + audit sink (the `#[tool]` handler is NOT unit-constructible, #5468). Covers
//! R-03 (audit completeness — the primary retrofit-hard control), R-04 (value-opacity),
//! R-02 (replace routing / one-tx→one-event), and R-07 (`check_write_rate` throttle).
//!
//! Fire-and-forget audit lands async, so every read-back is preceded by a 50ms settle
//! (#4377, the existing convention). Lifecycle guards live in the handler (Wave 3), not
//! this seam, so they are proven in the handler test plan — not here.

use std::sync::Arc;

use unimatrix_core::{NewEntry, Status, Store};
use unimatrix_store::pool_config::PoolConfig;

use crate::infra::audit::{AuditLog, Outcome};
use crate::services::gateway::{RateLimitConfig, SecurityGateway};
use crate::services::{AuditContext, AuditSource, CallerId, ServiceError};

use super::{StoreTagService, TagAction, build_tag_metadata};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Let the fire-and-forget audit `tokio::spawn` land before read-back (#4377).
async fn audit_settle() {
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
}

/// Build a `StoreTagService` over a fresh temp-DB store with the given write throttle.
/// Returns the shared `Arc<Store>` for direct audit/tag read-back.
async fn make_service(write_limit: u32) -> (Arc<Store>, StoreTagService) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.db");
    let store = Arc::new(
        Store::open(&path, PoolConfig::default())
            .await
            .expect("open store"),
    );
    std::mem::forget(dir);

    let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
    let gateway = Arc::new(SecurityGateway::with_rate_config(
        Arc::clone(&audit),
        RateLimitConfig {
            search_limit: 300,
            write_limit,
            window_secs: 3600,
        },
    ));
    let service = StoreTagService::new(Arc::clone(&store), gateway, audit);
    (store, service)
}

/// Insert an Active entry with the given seed tags; return its id.
async fn insert_entry(store: &Store, tags: &[&str]) -> u64 {
    store
        .insert(NewEntry {
            title: "t".to_string(),
            content: "c".to_string(),
            topic: "tp".to_string(),
            category: "cat".to_string(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            source: "test".to_string(),
            status: Status::Active,
            created_by: "tester".to_string(),
            feature_cycle: "vnc-045".to_string(),
            trust_source: "test".to_string(),
        })
        .await
        .expect("insert entry")
}

/// A context with a known caller/session for audit-field assertions.
fn ctx(session: &str) -> AuditContext {
    AuditContext {
        source: AuditSource::Internal {
            service: "test".to_string(),
        },
        caller_id: "agent-x".to_string(),
        session_id: Some(session.to_string()),
        feature_cycle: None,
    }
}

fn agent_caller() -> CallerId {
    CallerId::Agent("agent-x".to_string())
}

/// A single `context_tag` audit row, parsed for assertion.
struct TagRow {
    operation: String,
    agent_id: String,
    capability_used: String,
    session_id: String,
    timestamp: u64,
    target_ids: Vec<u64>,
    metadata: serde_json::Value,
    metadata_raw: String,
}

/// Read back every `context_tag` audit row (ordered by event_id).
async fn read_tag_rows(store: &Store) -> Vec<TagRow> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, String, i64)>(
        "SELECT operation, target_ids, agent_id, capability_used, metadata, session_id, timestamp \
         FROM audit_log WHERE operation = 'context_tag' ORDER BY event_id",
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("audit read must succeed");

    rows.into_iter()
        .map(
            |(
                operation,
                target_ids,
                agent_id,
                capability_used,
                metadata,
                session_id,
                timestamp,
            )| {
                let metadata_value: serde_json::Value = serde_json::from_str(&metadata)
                    .expect("metadata must be well-formed JSON, never a broken string");
                TagRow {
                    operation,
                    agent_id,
                    capability_used,
                    session_id,
                    timestamp: timestamp as u64,
                    target_ids: serde_json::from_str(&target_ids).expect("target_ids json"),
                    metadata: metadata_value,
                    metadata_raw: metadata,
                }
            },
        )
        .collect()
}

/// Current `entry_tags` for an entry (sorted).
async fn entry_tags(store: &Store, id: u64) -> Vec<String> {
    let mut rows = sqlx::query_as::<_, (String,)>("SELECT tag FROM entry_tags WHERE entry_id = ?1")
        .bind(id as i64)
        .fetch_all(store.read_pool_test())
        .await
        .expect("tags read")
        .into_iter()
        .map(|(t,)| t)
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

// ---------------------------------------------------------------------------
// R-03 — audit completeness (the primary, retrofit-hard control)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_prior_value_null_on_add() {
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &[]).await;

    let result = svc
        .tag(
            id,
            TagAction::Add,
            "x".to_string(),
            None,
            &ctx("s"),
            &agent_caller(),
        )
        .await
        .expect("add ok");
    assert_eq!(result.prior_value, None);
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0].metadata["prior_value"].is_null(),
        "add ⇒ prior null"
    );
    assert_eq!(rows[0].metadata["new_value"], "x", "add ⇒ new_value = tag");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_prior_value_mandatory_on_remove() {
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &["x"]).await;

    let result = svc
        .tag(
            id,
            TagAction::Remove,
            "x".to_string(),
            None,
            &ctx("s"),
            &agent_caller(),
        )
        .await
        .expect("remove ok");
    // ADR-009: prior_value sourced from the client's own tag (intent-of-record).
    assert_eq!(result.prior_value.as_deref(), Some("x"));
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].metadata["prior_value"], "x",
        "remove ⇒ prior non-null"
    );
    assert!(rows[0].metadata["new_value"].is_null(), "remove ⇒ new null");
    assert!(entry_tags(&store, id).await.is_empty(), "tag removed");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_prior_value_mandatory_on_remove_absent_tag() {
    // Removing a tag that was never present still records prior_value = the client's tag
    // (absent-remove reading, Gate 3a-confirmed): the audit is intent-of-record.
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &[]).await;

    svc.tag(
        id,
        TagAction::Remove,
        "ghost".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .expect("remove of absent tag is a no-op success");
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].metadata["prior_value"], "ghost");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_prior_value_mandatory_on_replace() {
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &["delivery:old"]).await;

    let result = svc
        .tag(
            id,
            TagAction::Replace,
            "delivery:new".to_string(),
            Some("delivery".to_string()),
            &ctx("s"),
            &agent_caller(),
        )
        .await
        .expect("replace ok");
    assert_eq!(result.prior_value.as_deref(), Some("delivery:old"));
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].metadata["prior_value"], "delivery:old");
    assert_eq!(rows[0].metadata["new_value"], "delivery:new");
    assert_eq!(rows[0].metadata["namespace"], "delivery");
    assert_eq!(
        entry_tags(&store, id).await,
        vec!["delivery:new".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_replace_colon_less_degrades_to_add() {
    // namespace=None ⇒ pure insert, prior_value null, evicts nothing (ADR-004 edge case).
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &["keep"]).await;

    let result = svc
        .tag(
            id,
            TagAction::Replace,
            "foo".to_string(),
            None,
            &ctx("s"),
            &agent_caller(),
        )
        .await
        .expect("colon-less replace degrades to add");
    assert_eq!(result.prior_value, None);
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].metadata["action"], "replace");
    assert!(rows[0].metadata["prior_value"].is_null());
    assert!(rows[0].metadata["namespace"].is_null());
    assert_eq!(rows[0].metadata["new_value"], "foo");
    // "keep" survived — nothing was evicted.
    assert_eq!(
        entry_tags(&store, id).await,
        vec!["foo".to_string(), "keep".to_string()]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_metadata_never_sentinel() {
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &["delivery:old"]).await;

    svc.tag(
        id,
        TagAction::Add,
        "a".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    svc.tag(
        id,
        TagAction::Remove,
        "a".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    svc.tag(
        id,
        TagAction::Replace,
        "delivery:new".to_string(),
        Some("delivery".to_string()),
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert_eq!(rows.len(), 3);
    for row in &rows {
        assert_ne!(row.metadata_raw, "{}", "never the '{{}}' sentinel");
        assert!(row.metadata.is_object(), "well-formed object");
        for key in ["action", "namespace", "tag", "prior_value", "new_value"] {
            assert!(
                row.metadata.get(key).is_some(),
                "metadata must carry key {key} (explicit null, not omission)"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_exactly_one_event_per_mutation() {
    // A replace = one atomic store tx = exactly ONE audit row (not one per DELETE/INSERT).
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &["delivery:old"]).await;

    svc.tag(
        id,
        TagAction::Replace,
        "delivery:new".to_string(),
        Some("delivery".to_string()),
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    audit_settle().await;
    assert_eq!(
        read_tag_rows(&store).await.len(),
        1,
        "one replace ⇒ one event"
    );

    svc.tag(
        id,
        TagAction::Add,
        "b".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    svc.tag(
        id,
        TagAction::Remove,
        "b".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    audit_settle().await;
    assert_eq!(
        read_tag_rows(&store).await.len(),
        3,
        "add+remove ⇒ one each"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_namespace_derived_recorded_never_validated() {
    // The service records whatever namespace the handler passed — verbatim, no validation,
    // even a value with LIKE metacharacters (opacity holds in the audit path, R-06).
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &[]).await;

    svc.tag(
        id,
        TagAction::Add,
        "delivery:x".to_string(),
        Some("delivery".to_string()),
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    svc.tag(
        id,
        TagAction::Add,
        "foo".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    svc.tag(
        id,
        TagAction::Add,
        "weird%_ns:y".to_string(),
        Some("weird%_ns".to_string()),
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    audit_settle().await;

    // Fire-and-forget audit events race on event_id assignment, so match rows by their
    // `tag` field rather than by index (non-deterministic ordering).
    let rows = read_tag_rows(&store).await;
    let ns_for = |tag: &str| -> serde_json::Value {
        rows.iter()
            .find(|r| r.metadata["tag"] == tag)
            .unwrap_or_else(|| panic!("no audit row for tag {tag}"))
            .metadata["namespace"]
            .clone()
    };
    assert_eq!(ns_for("delivery:x"), "delivery");
    assert!(ns_for("foo").is_null(), "colon-less ⇒ null namespace");
    assert_eq!(ns_for("weird%_ns:y"), "weird%_ns", "recorded verbatim");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_action_is_variant_string_not_integer() {
    // #4366: the action enum is serialized as a STRING in metadata, never an integer.
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &[]).await;

    svc.tag(
        id,
        TagAction::Add,
        "x".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert!(rows[0].metadata["action"].is_string(), "action is a string");
    assert_eq!(rows[0].metadata["action"], "add");

    // The reconstructed AuditEvent carries the Outcome enum (not a bare integer in the wire form).
    let event = store.read_audit_event(1).await.unwrap().unwrap();
    assert_eq!(event.outcome, Outcome::Success);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_session_id_captured_before_spawn() {
    // #4388/#4389: session_id is the ctx value, not a default filled after spawn.
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &[]).await;

    svc.tag(
        id,
        TagAction::Add,
        "x".to_string(),
        None,
        &ctx("mcp::sess-42"),
        &agent_caller(),
    )
    .await
    .unwrap();
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert_eq!(rows[0].session_id, "mcp::sess-42");
    assert!(!rows[0].session_id.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_audit_field_completeness() {
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &[]).await;

    svc.tag(
        id,
        TagAction::Add,
        "x".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    audit_settle().await;

    let rows = read_tag_rows(&store).await;
    assert_eq!(rows[0].operation, "context_tag");
    assert_eq!(rows[0].target_ids, vec![id]);
    assert_eq!(rows[0].agent_id, "agent-x");
    assert_eq!(rows[0].capability_used, "write");
    assert!(rows[0].timestamp > 0, "timestamp assigned by the sink");
}

// ---------------------------------------------------------------------------
// R-04 — value-opacity (NO rejection path exists; do not test one)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_value_opaque_acceptance_table() {
    let (store, svc) = make_service(1000).await;
    let id = insert_entry(&store, &[]).await;

    // (a) delivery:proven  (b) delivery:anythingelse  (c) free-form foo — all accepted+written
    // under the SAME Write caller; no allow-list, no vocabulary, no TrustLevel difference.
    for tag in ["delivery:proven", "delivery:anythingelse", "foo"] {
        svc.tag(
            id,
            TagAction::Add,
            tag.to_string(),
            None,
            &ctx("s"),
            &agent_caller(),
        )
        .await
        .unwrap_or_else(|e| panic!("value-opaque add of {tag} must succeed: {e}"));
    }
    audit_settle().await;

    let written = entry_tags(&store, id).await;
    assert!(written.contains(&"delivery:proven".to_string()));
    assert!(written.contains(&"delivery:anythingelse".to_string()));
    assert!(written.contains(&"foo".to_string()));
    assert_eq!(read_tag_rows(&store).await.len(), 3);
}

// ---------------------------------------------------------------------------
// R-07 — check_write_rate throttle (the one live throttle)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_check_write_rate_throttles_before_write() {
    let (store, svc) = make_service(2).await;
    let id = insert_entry(&store, &[]).await;

    svc.tag(
        id,
        TagAction::Add,
        "t1".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    svc.tag(
        id,
        TagAction::Add,
        "t2".to_string(),
        None,
        &ctx("s"),
        &agent_caller(),
    )
    .await
    .unwrap();
    let err = svc
        .tag(
            id,
            TagAction::Add,
            "t3".to_string(),
            None,
            &ctx("s"),
            &agent_caller(),
        )
        .await
        .expect_err("third write over the limit must be throttled");
    assert!(matches!(err, ServiceError::RateLimited { .. }));
    audit_settle().await;

    // Rejection-before-write: t3 left NO tag row and NO audit event.
    let written = entry_tags(&store, id).await;
    assert!(
        !written.contains(&"t3".to_string()),
        "throttled tag not written"
    );
    assert_eq!(
        read_tag_rows(&store).await.len(),
        2,
        "no audit row past the limit"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_uds_session_exempt_from_throttle() {
    let (store, svc) = make_service(1).await;
    let id = insert_entry(&store, &[]).await;
    let uds = CallerId::UdsSession("sess-uds".to_string());

    // write_limit=1, but a UdsSession caller is exempt (gateway.rs:60) — all three proceed.
    for tag in ["a", "b", "c"] {
        svc.tag(id, TagAction::Add, tag.to_string(), None, &ctx("s"), &uds)
            .await
            .unwrap_or_else(|e| panic!("UdsSession must be exempt, got {e}"));
    }
    audit_settle().await;
    assert_eq!(read_tag_rows(&store).await.len(), 3);
}

// ---------------------------------------------------------------------------
// Pure-fn + parse unit tests
// ---------------------------------------------------------------------------

#[test]
fn test_build_tag_metadata_valid_json_never_sentinel() {
    let s = build_tag_metadata(
        "replace",
        &Some("delivery".to_string()),
        "delivery:new",
        &Some("delivery:old".to_string()),
        &Some("delivery:new".to_string()),
    )
    .expect("serialize");
    assert_ne!(s, "{}");
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v["action"], "replace");
    assert_eq!(v["namespace"], "delivery");
    assert_eq!(v["prior_value"], "delivery:old");
}

#[test]
fn test_build_tag_metadata_emits_explicit_nulls() {
    let s = build_tag_metadata("add", &None, "foo", &None, &Some("foo".to_string())).unwrap();
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v["namespace"].is_null());
    assert!(v["prior_value"].is_null());
    assert_eq!(v["new_value"], "foo");
}

#[test]
fn test_tag_action_parse_roundtrip() {
    assert_eq!(TagAction::parse("add"), Some(TagAction::Add));
    assert_eq!(TagAction::parse("remove"), Some(TagAction::Remove));
    assert_eq!(TagAction::parse("replace"), Some(TagAction::Replace));
    assert_eq!(TagAction::parse("bogus"), None);
    assert_eq!(TagAction::Add.as_str(), "add");
    assert_eq!(TagAction::Remove.as_str(), "remove");
    assert_eq!(TagAction::Replace.as_str(), "replace");
}
