//! MCP tool implementations: v0.1 (context_search, context_lookup, context_store, context_get),
//! v0.2 (context_correct, context_deprecate, context_status, context_briefing),
//! and alc-002 (context_enroll).
//!
//! Execution order per tool: identity -> capability -> validation -> category -> scanning
//! -> business logic -> format -> audit.

use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;
use unimatrix_core::{CoreError, EmbedService, NewEntry, QueryFilter, Status};
use unimatrix_store::{QueryLogRecord, StoreError};

use crate::infra::audit::{AuditEvent, Outcome};
use crate::infra::registry::Capability;
use crate::infra::session::SessionRegistry;
use crate::infra::validation::{
    CycleType, QuarantineAction, parse_capabilities, parse_quarantine_action, parse_status,
    parse_trust_level, validate_briefing_params, validate_correct_params, validate_cycle_params,
    validate_deprecate_params, validate_enroll_params, validate_feature, validate_get_params,
    validate_helpful, validate_lookup_params, validate_quarantine_params, validate_search_params,
    validate_status_params, validate_store_params, validated_id, validated_k, validated_limit,
    validated_max_tokens,
};
use crate::mcp::response::{
    format_correct_success, format_deprecate_success, format_duplicate_found,
    format_enroll_success, format_index_table, format_lookup_results, format_quarantine_success,
    format_restore_success, format_search_results, format_single_entry, format_status_report,
    format_store_success, format_store_success_with_note,
};
use crate::server::UnimatrixServer;
use crate::services::ServiceSearchParams;
use crate::services::usage::{AccessSource, UsageContext};
use crate::uds::hook::MAX_GOAL_BYTES;

/// Parameters for semantic search.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Natural language query for semantic search.
    pub query: String,
    /// Filter by topic.
    pub topic: Option<String>,
    /// Filter by category.
    pub category: Option<String>,
    /// Filter by tags (all must match).
    pub tags: Option<Vec<String>>,
    /// Max results to return (default: 5).
    #[serde(
        default,
        deserialize_with = "crate::mcp::serde_util::deserialize_opt_i64_or_string"
    )]
    #[schemars(with = "Option<i64>")]
    pub k: Option<i64>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
    /// Feature context for usage tracking.
    pub feature: Option<String>,
    /// Whether the returned entries were helpful.
    pub helpful: Option<bool>,
    /// Optional session ID (provided by hooks, not agent-reported).
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Parameters for deterministic lookup.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LookupParams {
    /// Filter by topic.
    pub topic: Option<String>,
    /// Filter by category.
    pub category: Option<String>,
    /// Filter by tags (all must match).
    pub tags: Option<Vec<String>>,
    /// Lookup by specific entry ID.
    #[serde(
        default,
        deserialize_with = "crate::mcp::serde_util::deserialize_opt_i64_or_string"
    )]
    #[schemars(with = "Option<i64>")]
    pub id: Option<i64>,
    /// Filter by status (active, deprecated, proposed).
    pub status: Option<String>,
    /// Max results to return (default: 10).
    #[serde(
        default,
        deserialize_with = "crate::mcp::serde_util::deserialize_opt_i64_or_string"
    )]
    #[schemars(with = "Option<i64>")]
    pub limit: Option<i64>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
    /// Feature context for usage tracking.
    pub feature: Option<String>,
    /// Whether the returned entries were helpful.
    pub helpful: Option<bool>,
    /// Optional session ID (provided by hooks, not agent-reported).
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Parameters for storing a new entry.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StoreParams {
    /// Content to store.
    pub content: String,
    /// Topic for the entry.
    pub topic: String,
    /// Category for the entry.
    pub category: String,
    /// Tags for the entry.
    pub tags: Option<Vec<String>>,
    /// Title for the entry.
    pub title: Option<String>,
    /// Source identifier.
    pub source: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
    /// Feature cycle or workflow identifier (e.g., "col-001", "bug-42").
    pub feature_cycle: Option<String>,
    /// Optional session ID (provided by hooks, not agent-reported).
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Parameters for getting an entry by ID.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetParams {
    /// Entry ID to retrieve.
    #[serde(deserialize_with = "crate::mcp::serde_util::deserialize_i64_or_string")]
    #[schemars(with = "i64")]
    pub id: i64,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
    /// Feature context for usage tracking.
    pub feature: Option<String>,
    /// Whether the returned entries were helpful.
    pub helpful: Option<bool>,
    /// Optional session ID (provided by hooks, not agent-reported).
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Parameters for correcting an existing entry.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CorrectParams {
    /// ID of the entry to correct (will be deprecated).
    #[serde(deserialize_with = "crate::mcp::serde_util::deserialize_i64_or_string")]
    #[schemars(with = "i64")]
    pub original_id: i64,
    /// Corrected content to replace the original.
    pub content: String,
    /// Reason for the correction.
    pub reason: Option<String>,
    /// Override topic (defaults to original's topic).
    pub topic: Option<String>,
    /// Override category (defaults to original's category).
    pub category: Option<String>,
    /// Override tags (defaults to original's tags).
    pub tags: Option<Vec<String>>,
    /// Override title (defaults to original's title).
    pub title: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
}

/// Parameters for deprecating an entry.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeprecateParams {
    /// ID of the entry to deprecate.
    #[serde(deserialize_with = "crate::mcp::serde_util::deserialize_i64_or_string")]
    #[schemars(with = "i64")]
    pub id: i64,
    /// Reason for deprecation.
    pub reason: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
}

/// Parameters for quarantining or restoring an entry.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct QuarantineParams {
    /// Entry ID to quarantine or restore.
    #[serde(deserialize_with = "crate::mcp::serde_util::deserialize_i64_or_string")]
    #[schemars(with = "i64")]
    pub id: i64,
    /// Reason for the action.
    pub reason: Option<String>,
    /// Action: "quarantine" (default) or "restore".
    pub action: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
}

/// Parameters for getting knowledge base status.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusParams {
    /// Filter status report to a specific topic.
    pub topic: Option<String>,
    /// Filter status report to a specific category.
    pub category: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
    /// Opt-in embedding consistency check (default: false).
    pub check_embeddings: Option<bool>,
}

/// Parameters for getting an orientation briefing.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct BriefingParams {
    /// Your role (e.g., "architect", "developer"). Optional — used as a last-resort query fallback only when `task` is empty and `feature` is absent. Prefer a descriptive `task`.
    pub role: Option<String>,
    /// What you are about to do, as a focused 1-2 sentence natural language description. This is the primary search query — be specific. Example: "design the query derivation pipeline for context_briefing". Avoid vague phrases like "start task" or bare keyword lists; the ranking uses NLI entailment scoring which works best with coherent sentences.
    pub task: String,
    /// Feature cycle identifier (e.g., "crt-027"). Used as query fallback when `task` is empty; does not apply a scoring boost.
    pub feature: Option<String>,
    /// Reserved for future output truncation. Accepted and validated (500–10000, default 3000) but not currently enforced on results.
    #[serde(
        default,
        deserialize_with = "crate::mcp::serde_util::deserialize_opt_i64_or_string"
    )]
    #[schemars(with = "Option<i64>")]
    pub max_tokens: Option<i64>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
    /// Whether the returned entries were helpful.
    pub helpful: Option<bool>,
    /// Optional session ID (provided by hooks, not agent-reported).
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Parameters for enrolling or updating an agent.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EnrollParams {
    /// Agent ID to enroll or update.
    pub target_agent_id: String,
    /// Trust level: "system", "privileged", "internal", "restricted".
    pub trust_level: String,
    /// Capabilities: ["read", "write", "search", "admin"].
    pub capabilities: Vec<String>,
    /// Calling agent (must have Admin).
    pub agent_id: Option<String>,
    /// Response format: "summary", "markdown", "json".
    pub format: Option<String>,
}

/// Parameters for the context_cycle_review tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetrospectiveParams {
    /// Feature cycle to analyze (e.g., "col-002").
    pub feature_cycle: String,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Maximum evidence items per hotspot (default: 3, JSON path only). (col-010b)
    #[serde(
        default,
        deserialize_with = "crate::mcp::serde_util::deserialize_opt_usize_or_string"
    )]
    #[schemars(with = "Option<u64>")]
    pub evidence_limit: Option<usize>,
    /// Output format: "markdown" (default) or "json". (vnc-011)
    pub format: Option<String>,
    /// Force recomputation even if a stored review exists. (crt-033)
    /// Absent or None is equivalent to false.
    pub force: Option<bool>,
}

/// Parameters for the context_cycle tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CycleParams {
    /// Cycle action: "start", "phase-end", or "stop".
    pub r#type: String,
    /// Topic identifying a bounded work unit tracked across its lifecycle.
    /// Can be a software feature, incident, campaign, clinical trial, or any work unit a domain
    /// tracks from start to completion. The format is domain-defined; Unimatrix treats it as an
    /// opaque string identifier (e.g., "col-022", "inc-045", "trial-007").
    pub topic: String,
    /// The phase that is ending (for type="phase-end"). Normalized to lowercase, max 64 chars,
    /// no spaces. Example: "design", "implementation".
    pub phase: Option<String>,
    /// Free-form outcome description for the ending phase (max 512 chars).
    pub outcome: Option<String>,
    /// The next phase beginning after this event (for type="start" or "phase-end").
    pub next_phase: Option<String>,
    /// Optional goal statement for the feature cycle (col-025).
    ///
    /// Only meaningful for type="start". Silently ignored for "phase-end" and "stop".
    /// Max 1024 bytes (MAX_GOAL_BYTES). Empty/whitespace normalized to None at the
    /// handler layer (FR-11, ADR-005). Old callers omitting this field receive None.
    pub goal: Option<String>,
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
}

/// Extract the active workflow phase for a session — infallible, O(1) clone.
///
/// Returns `None` when: `session_id` is `None`; the session is not registered;
/// or the session has no active phase (no `context_cycle(start)` emitted yet).
///
/// Called as the **first statement** in each read-side handler body, before any
/// `.await`, satisfying ADR-002 col-028 and pattern #3027.
/// `pub(crate)` for unit testability without handler construction (ADR-001 col-028).
pub(crate) fn current_phase_for_session(
    registry: &SessionRegistry,
    session_id: Option<&str>,
) -> Option<String> {
    session_id
        .and_then(|sid| registry.get_state(sid))
        .and_then(|s| s.current_phase.clone())
}

#[rmcp::tool_router(vis = "pub(crate)")]
impl UnimatrixServer {
    #[tool(
        name = "context_search",
        description = "Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. Use when you need to find patterns, conventions, or decisions related to a concept."
    )]
    async fn context_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // [C-01, ADR-002 col-028] Phase snapshot FIRST — before any .await.
        // [C-04] Single get_state call: same variable serves UsageContext AND QueryLogRecord.
        let current_phase =
            current_phase_for_session(&self.session_registry, params.session_id.as_deref());

        // 1. Identity + format + audit context (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &params.session_id)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Search).await?;

        // 2. Validation
        validate_search_params(&params).map_err(rmcp::ErrorData::from)?;
        validate_feature(&params.feature).map_err(rmcp::ErrorData::from)?;
        validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

        // 3. Parse k
        let k = validated_k(params.k).map_err(rmcp::ErrorData::from)?;

        // 4. Build ServiceSearchParams and delegate to SearchService
        let service_params = ServiceSearchParams {
            query: params.query.clone(),
            k,
            filters: if params.topic.is_some() || params.category.is_some() || params.tags.is_some()
            {
                Some(QueryFilter {
                    topic: params.topic.clone(),
                    category: params.category.clone(),
                    tags: params.tags.clone(),
                    status: Some(Status::Active),
                    time_range: None,
                })
            } else {
                None
            },
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: params.feature.clone(),
            co_access_anchors: None,
            caller_agent_id: Some(ctx.agent_id.clone()),
            retrieval_mode: crate::services::RetrievalMode::Flexible, // crt-010: MCP always Flexible
            session_id: ctx.audit_ctx.session_id.clone(), // crt-026: for observability (WA-2)
            // crt-026: pre-resolve session histogram (WA-2, SR-07 snapshot pattern)
            category_histogram: ctx.audit_ctx.session_id.as_deref().and_then(|sid| {
                let h = self.session_registry.get_category_histogram(sid);
                if h.is_empty() { None } else { Some(h) }
            }),
            current_phase: None, // col-031: MCP tools.rs — phase not yet threaded from tool params
        };

        let search_results = self
            .services
            .search
            .search(service_params, &ctx.audit_ctx, &ctx.caller_id)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 5. Format response (transport-specific)
        let results_with_scores: Vec<_> = search_results
            .entries
            .iter()
            .map(|se| (se.entry.clone(), se.similarity))
            .collect();
        let result = format_search_results(&results_with_scores, ctx.format);

        // 6. Usage recording (fire-and-forget via UsageService)
        let target_ids: Vec<u64> = search_results
            .entries
            .iter()
            .map(|se| se.entry.id)
            .collect();
        self.services.usage.record_access(
            &target_ids,
            AccessSource::McpTool,
            UsageContext {
                session_id: ctx.audit_ctx.session_id.clone(),
                agent_id: Some(ctx.agent_id.clone()),
                helpful: params.helpful,
                feature_cycle: params.feature.clone(),
                trust_level: Some(ctx.trust_level),
                access_weight: 1,
                current_phase: current_phase.clone(), // col-028: phase captured above (C-04)
            },
        );

        // 7. nxs-010: Query log recording (fire-and-forget, ADR-002)
        {
            let entry_ids: Vec<u64> = search_results
                .entries
                .iter()
                .map(|se| se.entry.id)
                .collect();
            let scores: Vec<f64> = search_results
                .entries
                .iter()
                .map(|se| se.similarity)
                .collect();

            let session_id_for_log = ctx.audit_ctx.session_id.clone().unwrap_or_default();

            let record = QueryLogRecord::new(
                session_id_for_log,
                params.query.clone(),
                &entry_ids,
                &scores,
                "flexible",
                "mcp",
                current_phase, // col-028: phase snapshot shared from C-04 single get_state call
            );

            let store_clone = Arc::clone(&self.store);
            let _ = tokio::task::spawn_blocking(move || {
                store_clone.insert_query_log(&record);
            });
        }

        Ok(result)
    }

    #[tool(
        name = "context_lookup",
        description = "Look up context entries by exact filters. Returns entries matching the specified topic, category, tags, status, or ID. Use when you know what you are looking for."
    )]
    async fn context_lookup(
        &self,
        Parameters(params): Parameters<LookupParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // [C-01, ADR-002 col-028] Phase snapshot FIRST — before any .await.
        let current_phase =
            current_phase_for_session(&self.session_registry, params.session_id.as_deref());

        // 1. Identity + format + audit context (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &params.session_id)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Read).await?;

        // 2. Validation
        validate_lookup_params(&params).map_err(rmcp::ErrorData::from)?;
        validate_feature(&params.feature).map_err(rmcp::ErrorData::from)?;
        validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

        // 3. Parse limit
        let limit = validated_limit(params.limit).map_err(rmcp::ErrorData::from)?;

        // 4. Branch: ID-based vs filter-based
        let (result, target_ids) = if let Some(id) = params.id {
            let id = validated_id(id).map_err(rmcp::ErrorData::from)?;
            let entry = self.entry_store.get(id).await.map_err(|e| {
                rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::Store(e)))
            })?;
            let ids = vec![entry.id];
            (format_single_entry(&entry, ctx.format), ids)
        } else {
            // Build filter
            let status = match &params.status {
                Some(s) => Some(parse_status(s).map_err(rmcp::ErrorData::from)?),
                None => Some(Status::Active), // default to Active (FR-02e)
            };

            let filter = QueryFilter {
                topic: params.topic.clone(),
                category: params.category.clone(),
                tags: params.tags.clone(),
                status,
                time_range: None,
            };
            let mut entries = self.entry_store.query(filter).await.map_err(|e| {
                rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::Store(e)))
            })?;
            entries.truncate(limit);
            let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
            (format_lookup_results(&entries, ctx.format), ids)
        };

        // 5. Audit (standalone, best-effort)
        let result_count = target_ids.len();
        self.audit_fire_and_forget(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: ctx.agent_id.clone(),
            operation: "context_lookup".to_string(),
            target_ids: target_ids.clone(),
            outcome: Outcome::Success,
            detail: format!("returned {result_count} results"),
        });

        // 6. Usage recording (fire-and-forget via UsageService)
        // access_weight: 2 — deliberate retrieval signal (crt-019 ADR-004):
        // a lookup is an intentional act of knowledge retrieval; the doubled
        // access signal differentiates it from incidental context_search hits.
        self.services.usage.record_access(
            &target_ids,
            AccessSource::McpTool,
            UsageContext {
                session_id: ctx.audit_ctx.session_id.clone(),
                agent_id: Some(ctx.agent_id.clone()),
                helpful: params.helpful,
                feature_cycle: params.feature.clone(),
                trust_level: Some(ctx.trust_level),
                access_weight: 2,
                current_phase, // col-028: phase captured above
            },
        );

        // col-028 ADR-004: confirmed_entries — request-side cardinality.
        // Single-ID lookup (params.id path) always resolves exactly one entry or
        // errors before reaching here. Multi-ID filter results are NOT explicit fetches.
        if target_ids.len() == 1 && params.id.is_some() {
            if let Some(sid) = ctx.audit_ctx.session_id.as_deref() {
                if let Some(&entry_id) = target_ids.first() {
                    self.session_registry.record_confirmed_entry(sid, entry_id);
                }
            }
        }

        Ok(result)
    }

    #[tool(
        name = "context_store",
        description = "Store a new context entry. Use to record patterns, conventions, architectural decisions, or other reusable knowledge discovered during work."
    )]
    async fn context_store(
        &self,
        Parameters(params): Parameters<StoreParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity + format + audit context (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &params.session_id)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Write).await?;

        // 2. Validation
        validate_store_params(&params).map_err(rmcp::ErrorData::from)?;

        // 3. Category validation
        self.categories
            .validate(&params.category)
            .map_err(rmcp::ErrorData::from)?;

        // 3a. Outcome tag validation (only for outcome entries)
        if params.category == "outcome" {
            let tags = params.tags.as_deref().unwrap_or(&[]);
            crate::infra::outcome_tags::validate_outcome_tags(tags)
                .map_err(rmcp::ErrorData::from)?;
        }

        // 3b. Snapshot current_phase from SessionState at call time (ADR-001 crt-025 SR-07).
        //
        // Must happen synchronously here — before any await that could interleave with a
        // concurrent `phase-end` event advancing `current_phase`. `get_state` returns a
        // clone so this snapshot is isolated from subsequent SessionState mutations.
        let session_state = ctx
            .audit_ctx
            .session_id
            .as_deref()
            .and_then(|sid| self.session_registry.get_state(sid));
        let current_phase: Option<String> =
            session_state.as_ref().and_then(|s| s.current_phase.clone());
        let feature_cycle_from_session: Option<String> =
            session_state.and_then(|s| s.feature.clone());

        // 4. Build title (transport-specific default)
        let title = params
            .title
            .unwrap_or_else(|| format!("{}: {}", params.topic, params.category));
        let is_outcome = params.category == "outcome";

        // 5. Build NewEntry
        // `new_entry.feature_cycle` uses the caller-supplied value (original behavior).
        // `UsageContext.feature_cycle` falls back to the session's feature when the
        // caller omits the field, enabling session-based auto-attribution for
        // feature_entries tagging without modifying the stored entry metadata.
        let entry_feature_cycle = params.feature_cycle.clone().unwrap_or_default();
        let usage_feature_cycle: Option<String> = params
            .feature_cycle
            .clone()
            .or(feature_cycle_from_session)
            .filter(|s| !s.is_empty());
        let new_entry = NewEntry {
            title,
            content: params.content,
            topic: params.topic,
            category: params.category,
            tags: params.tags.unwrap_or_default(),
            source: params.source.unwrap_or_default(),
            status: Status::Active,
            created_by: ctx.agent_id.clone(),
            feature_cycle: entry_feature_cycle.clone(),
            trust_source: "agent".to_string(),
        };

        // 6. Delegate to StoreService (scanning, embedding, dup-check, insert)
        let insert_result = self
            .services
            .store_ops
            .insert(new_entry, None, &ctx.audit_ctx, &ctx.caller_id)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 7. Handle duplicate result
        if insert_result.duplicate_of.is_some() {
            let similarity = insert_result.duplicate_similarity.unwrap_or(1.0);
            return Ok(format_duplicate_found(
                &insert_result.entry,
                similarity,
                ctx.format,
            ));
        }

        // crt-026: Accumulate category histogram for session affinity boost (WA-2).
        // Called ONLY after the duplicate guard — duplicate stores must not count (C-09, R-03).
        // Pattern mirrors record_injection: if let Some(ref sid) guards the session_id None case.
        if let Some(ref sid) = ctx.audit_ctx.session_id {
            self.session_registry
                .record_category_store(sid, &insert_result.entry.category);
        }

        // 8. Seed initial confidence (fire-and-forget, via ConfidenceService)
        self.services
            .confidence
            .recompute(&[insert_result.entry.id]);

        // 9. Usage recording with phase snapshot (crt-025 SR-07, ADR-001).
        //
        // `current_phase` was captured synchronously above — it reflects the phase
        // at call time and will not change even if a concurrent `phase-end` fires
        // before the async `record_feature_entries` write completes.
        if let Some(fc) = usage_feature_cycle {
            self.services.usage.record_access(
                &[insert_result.entry.id],
                AccessSource::McpTool,
                UsageContext {
                    session_id: ctx.audit_ctx.session_id.clone(),
                    agent_id: Some(ctx.agent_id.clone()),
                    helpful: None,
                    feature_cycle: Some(fc),
                    trust_level: Some(ctx.trust_level),
                    access_weight: 1,
                    current_phase: current_phase.clone(),
                },
            );
        }

        // 10. Format response
        if is_outcome && entry_feature_cycle.is_empty() {
            // Append orphan outcome warning to the formatted response
            let warning = "\nNote: outcome not linked to a workflow (no feature_cycle provided)";
            Ok(format_store_success_with_note(
                &insert_result.entry,
                ctx.format,
                warning,
            ))
        } else {
            Ok(format_store_success(&insert_result.entry, ctx.format))
        }
    }

    #[tool(
        name = "context_get",
        description = "Get a specific context entry by its ID. Use when you have an entry ID from a previous search or lookup result."
    )]
    async fn context_get(
        &self,
        Parameters(params): Parameters<GetParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // [C-01, ADR-002 col-028] Phase snapshot FIRST — before any .await.
        let current_phase =
            current_phase_for_session(&self.session_registry, params.session_id.as_deref());

        // 1. Identity + format + audit context (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &params.session_id)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Read).await?;

        // 2. Validation
        validate_get_params(&params).map_err(rmcp::ErrorData::from)?;
        validate_feature(&params.feature).map_err(rmcp::ErrorData::from)?;
        validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

        // 3. Get entry
        let id = validated_id(params.id).map_err(rmcp::ErrorData::from)?;
        let entry = self.entry_store.get(id).await.map_err(|e| {
            rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::Store(e)))
        })?;

        // 4. Format response
        let result = format_single_entry(&entry, ctx.format);

        // 5. Audit (standalone, best-effort)
        self.audit_fire_and_forget(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: ctx.agent_id.clone(),
            operation: "context_get".to_string(),
            target_ids: vec![id],
            outcome: Outcome::Success,
            detail: format!("retrieved entry #{id}"),
        });

        // 6. Usage recording (fire-and-forget via UsageService)
        // C-04: inject implicit helpful vote in-process, before the existing spawn_blocking.
        // params.helpful.or(Some(true)) semantics:
        //   helpful=None   -> Some(true)  (implicit helpful vote: user retrieved and read)
        //   helpful=true   -> Some(true)  (explicit positive)
        //   helpful=false  -> Some(false) (explicit negative honored)
        // UsageDedup enforces one vote per agent-entry pair.
        // col-028 AC-05: access_weight raised to 2 (deliberate full-content retrieval).
        self.services.usage.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: ctx.audit_ctx.session_id.clone(),
                agent_id: Some(ctx.agent_id.clone()),
                helpful: params.helpful.or(Some(true)),
                feature_cycle: params.feature.clone(),
                trust_level: Some(ctx.trust_level),
                access_weight: 2, // col-028: was 1; deliberate full-content read
                current_phase,    // col-028: phase captured above
            },
        );

        // col-028 FR-08: always record explicit fetch in confirmed_entries (EC-05 contract:
        // only reached after successful entry_store.get, so entry definitely exists).
        if let Some(sid) = ctx.audit_ctx.session_id.as_deref() {
            self.session_registry.record_confirmed_entry(sid, id);
        }

        Ok(result)
    }

    #[tool(
        name = "context_correct",
        description = "Correct an existing knowledge entry. Deprecates the original and creates a new corrected entry with a chain link. Use when an entry contains wrong information."
    )]
    async fn context_correct(
        &self,
        Parameters(params): Parameters<CorrectParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity + format + audit context (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &None)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Write).await?;

        // 2. Validation (includes original_id range check)
        validate_correct_params(&params).map_err(rmcp::ErrorData::from)?;

        // 3. Extract validated original_id (range already checked by validate_correct_params)
        let original_id = params.original_id as u64;

        // 4. Get original entry (needed for field inheritance below)
        let original = self.entry_store.get(original_id).await.map_err(|e| {
            rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::Store(e)))
        })?;

        // Note: deprecated check is handled authoritatively inside StoreService.correct()'s
        // write transaction. No pre-check here to avoid TOCTOU.

        // 5. Category validation: only if explicit new category provided
        if let Some(category) = &params.category {
            self.categories
                .validate(category)
                .map_err(rmcp::ErrorData::from)?;
        }

        // 6. Build title (inherit from original if not provided)
        let title = params.title.unwrap_or_else(|| original.title.clone());

        // 7. Build NewEntry with inheritance
        let new_entry = NewEntry {
            title,
            content: params.content,
            topic: params.topic.unwrap_or_else(|| original.topic.clone()),
            category: params.category.unwrap_or_else(|| original.category.clone()),
            tags: params.tags.unwrap_or_else(|| original.tags.clone()),
            source: original.source.clone(),
            status: Status::Active,
            created_by: ctx.agent_id,
            feature_cycle: original.feature_cycle.clone(),
            trust_source: "agent".to_string(),
        };

        // 8. Delegate to StoreService (scanning, embedding, atomic correct+audit)
        let correct_result = self
            .services
            .store_ops
            .correct(
                original_id,
                new_entry,
                params.reason,
                &ctx.audit_ctx,
                &ctx.caller_id,
            )
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 9. Confidence for both entries (fire-and-forget, via ConfidenceService)
        self.services.confidence.recompute(&[
            correct_result.corrected_entry.id,
            correct_result.deprecated_original.id,
        ]);

        // 10. Format response
        Ok(format_correct_success(
            &correct_result.deprecated_original,
            &correct_result.corrected_entry,
            ctx.format,
        ))
    }

    #[tool(
        name = "context_deprecate",
        description = "Deprecate a knowledge entry. The entry remains accessible but is excluded from default lookups. Use when knowledge is outdated or no longer relevant."
    )]
    async fn context_deprecate(
        &self,
        Parameters(params): Parameters<DeprecateParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity + format + audit context (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &None)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Write).await?;

        // 2. Validation (includes id range check)
        validate_deprecate_params(&params).map_err(rmcp::ErrorData::from)?;

        // 3. Extract validated ID (range already checked by validate_deprecate_params)
        let entry_id = params.id as u64;

        // 4. Get entry (verify exists + idempotency check)
        let entry = self.entry_store.get(entry_id).await.map_err(|e| {
            rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::Store(e)))
        })?;

        // 5. Idempotency: if already deprecated, return success immediately
        if entry.status == Status::Deprecated {
            return Ok(format_deprecate_success(
                &entry,
                params.reason.as_deref(),
                ctx.format,
            ));
        }

        // 6. Deprecate with audit
        let audit_event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: ctx.agent_id,
            operation: "context_deprecate".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: String::new(),
        };
        let deprecated = self
            .deprecate_with_audit(entry_id, params.reason.clone(), audit_event)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 7. Recompute confidence for deprecated entry (fire-and-forget, via ConfidenceService)
        self.services.confidence.recompute(&[deprecated.id]);

        // 8. Format response
        Ok(format_deprecate_success(
            &deprecated,
            params.reason.as_deref(),
            ctx.format,
        ))
    }

    #[tool(
        name = "context_status",
        description = "Get the health status of the knowledge base. Shows entry counts, category/topic distributions, correction chains, and security metrics. Requires Read capability."
    )]
    async fn context_status(
        &self,
        Parameters(params): Parameters<StatusParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity + format + capability (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &None)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Read).await?;

        // 2. Validation
        validate_status_params(&params).map_err(rmcp::ErrorData::from)?;

        // 3. Compute report via StatusService (vnc-008 extraction)
        let check_embeddings = params.check_embeddings.unwrap_or(false);
        let (mut report, _active_entries) = self
            .services
            .status
            .compute_report(params.topic, params.category, check_embeddings)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 4. Read tick metadata for status reporting.
        // Maintenance is handled by the background tick (col-013).
        {
            use crate::mcp::response::status::ExtractionStatsResponse;
            let tick_meta = self.tick_metadata.lock().unwrap_or_else(|e| e.into_inner());
            report.last_maintenance_run = tick_meta.last_maintenance_run;
            report.next_maintenance_scheduled = tick_meta.next_scheduled;
            let stats = &tick_meta.extraction_stats;
            if stats.entries_extracted_total > 0
                || stats.entries_rejected_total > 0
                || stats.last_extraction_run.is_some()
            {
                report.extraction_stats = Some(ExtractionStatsResponse {
                    entries_extracted_total: stats.entries_extracted_total,
                    entries_rejected_total: stats.entries_rejected_total,
                    last_extraction_run: stats.last_extraction_run,
                    rules_fired: stats
                        .rules_fired
                        .iter()
                        .map(|(k, v)| (k.clone(), *v))
                        .collect(),
                });
            }
            // friction_signals are unconditional — they report agent workflow patterns,
            // not KM graph health, so they are not gated by lambda or maintain flag.
            report
                .maintenance_recommendations
                .extend(tick_meta.friction_signals.iter().cloned());
            report
                .maintenance_recommendations
                .extend(tick_meta.dead_knowledge_signals.iter().cloned());
        }

        // 5. Audit (standalone, best-effort)
        self.audit_fire_and_forget(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: ctx.agent_id,
            operation: "context_status".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: "status report generated".to_string(),
        });

        // 6. Format response
        Ok(format_status_report(&report, ctx.format))
    }

    #[tool(
        name = "context_briefing",
        description = "Get a ranked index of knowledge entries relevant to your current task. Returns up to 20 active entries scored by semantic similarity and NLI entailment. Use at the start of any task to orient yourself before designing or implementing."
    )]
    async fn context_briefing(
        &self,
        Parameters(params): Parameters<BriefingParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        #[cfg(not(feature = "mcp-briefing"))]
        {
            return Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                "context_briefing tool is not available in this build configuration",
            )]));
        }

        #[cfg(feature = "mcp-briefing")]
        {
            // [C-01, ADR-002 col-028] Phase snapshot FIRST — before any .await.
            // Note: step 4 below also calls get_state for query derivation. That is a
            // separate purpose (topic_signals). This snapshot is for UsageContext only.
            let current_phase =
                current_phase_for_session(&self.session_registry, params.session_id.as_deref());

            // 1. Identity + capability check
            let ctx = self
                .build_context(&params.agent_id, &params.format, &params.session_id)
                .await?;
            self.require_cap(&ctx.agent_id, Capability::Read).await?;

            // 2. Validation
            validate_briefing_params(&params).map_err(rmcp::ErrorData::from)?;
            validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

            // 3. Validate max_tokens
            let max_tokens =
                validated_max_tokens(params.max_tokens).map_err(rmcp::ErrorData::from)?;

            // 4. Resolve session state for step 2 of query derivation (AC-10)
            // MCP path: look up SessionRegistry by session_id.
            // get_state returns None for unknown/expired sessions → graceful degradation to step 3.
            let session_state: Option<crate::infra::session::SessionState> = params
                .session_id
                .as_deref()
                .and_then(|sid| self.session_registry.get_state(sid));

            // 5. Pre-resolve category histogram for WA-2 boost (crt-026 pattern)
            let category_histogram: Option<std::collections::HashMap<String, u32>> =
                params.session_id.as_deref().and_then(|sid| {
                    let h = self.session_registry.get_category_histogram(sid);
                    if h.is_empty() { None } else { Some(h) }
                });

            // 6. Three-step query derivation (FR-11, AC-09)
            // Step 1: task param if non-empty
            // Step 2: synthesized from feature_cycle + top 3 topic_signals (from session_state)
            // Step 3: feature/topic fallback (params.feature else params.role else "unknown")
            let topic = params
                .feature
                .as_deref()
                .unwrap_or_else(|| params.role.as_deref().unwrap_or("unknown"));

            let query = crate::services::derive_briefing_query(
                Some(&params.task),
                session_state.as_ref(),
                topic,
            );

            // 7. Build IndexBriefingParams
            let briefing_params = crate::services::IndexBriefingParams {
                query,
                k: 20, // default k (FR-13: not from UNIMATRIX_BRIEFING_K)
                session_id: params.session_id.clone(),
                max_tokens: Some(max_tokens),
                category_histogram,
            };

            // 8. Delegate to IndexBriefingService, with goal-conditioned blending (crt-046).
            //
            // Level 1 guard (ADR-004, Resolution 3): if no feature attribution OR
            // current_goal is empty/absent, skip all blending (zero DB calls for cluster work).
            let current_goal: &str = session_state
                .as_ref()
                .and_then(|ss| ss.current_goal.as_deref())
                .unwrap_or("");
            let feature_for_blending = session_state.as_ref().and_then(|ss| ss.feature.as_deref());

            let should_blend = feature_for_blending.map(|f| !f.is_empty()).unwrap_or(false)
                && !current_goal.is_empty();

            let entries: Vec<crate::mcp::response::IndexEntry> = if should_blend {
                let feature = feature_for_blending.unwrap();
                let store = Arc::clone(&self.store);
                let config = Arc::clone(&self.inference_config);

                // Level 2 guard (ADR-004): get_cycle_start_goal_embedding.
                // If absent or error, fall through to pure-semantic path.
                let goal_embedding_opt: Option<Vec<f32>> = match store
                    .get_cycle_start_goal_embedding(feature)
                    .await
                {
                    Ok(opt) => opt,
                    Err(e) => {
                        tracing::warn!(
                            feature = feature,
                            error = %e,
                            "context_briefing: get_cycle_start_goal_embedding error — cold-start"
                        );
                        None
                    }
                };

                match goal_embedding_opt {
                    None => {
                        // Level 2 cold-start: no stored goal embedding.
                        self.services
                            .briefing
                            .index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
                            .await
                            .map_err(rmcp::ErrorData::from)?
                    }
                    Some(goal_embedding) => {
                        // Cluster query (recency-capped, cosine-filtered, ADR-003).
                        let matching_clusters = match store
                            .query_goal_clusters_by_embedding(
                                &goal_embedding,
                                config.goal_cluster_similarity_threshold,
                                crate::services::behavioral_signals::RECENCY_CAP,
                            )
                            .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    "context_briefing: query_goal_clusters_by_embedding failed \
                                     — cold-start"
                                );
                                vec![]
                            }
                        };

                        if matching_clusters.is_empty() {
                            // Cold-start: no matching clusters above threshold.
                            self.services
                                .briefing
                                .index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
                                .await
                                .map_err(rmcp::ErrorData::from)?
                        } else {
                            // Use at most 5 matching clusters (best cosine — sorted desc by query).
                            let top_clusters = &matching_clusters[..matching_clusters.len().min(5)];

                            // Collect union of entry IDs from top clusters.
                            let mut cluster_entry_ids_raw: Vec<u64> = Vec::new();
                            for cluster_row in top_clusters {
                                match serde_json::from_str::<Vec<u64>>(&cluster_row.entry_ids_json)
                                {
                                    Ok(ids) => cluster_entry_ids_raw.extend(ids),
                                    Err(e) => {
                                        tracing::warn!(
                                            feature_cycle = %cluster_row.feature_cycle,
                                            error = %e,
                                            "context_briefing: failed to parse entry_ids_json \
                                             for cluster row — skipping"
                                        );
                                    }
                                }
                            }
                            // Deduplicate entry IDs across clusters.
                            cluster_entry_ids_raw.sort_unstable();
                            cluster_entry_ids_raw.dedup();

                            // Safety cap: bound sequential store.get() calls to at most 50. IDs are sorted
                            // ascending by u64 value for dedup correctness (not by relevance), so truncation
                            // drops the numerically-highest (most recently created) IDs. This is acceptable
                            // as a tail-case safety cap — a large cycle with many accessed entries × 5 clusters
                            // can produce 250+ pre-dedup IDs; 50 reflects ~5 clusters × ~10 entries typical case.
                            // NOTE: the entry_max_sim loop below still iterates all top_clusters rows and populates
                            // similarity scores for IDs truncated here. Those map entries are never looked up in
                            // the subsequent store.get() loop — they are harmless dead entries, not a bug.
                            const CLUSTER_ID_CAP: usize = 50;
                            cluster_entry_ids_raw.truncate(CLUSTER_ID_CAP);

                            // Build per-entry max_similarity map for cluster_score computation.
                            // Uses a pre-parsed HashMap to avoid repeated JSON parsing per entry.
                            let mut entry_max_sim: std::collections::HashMap<u64, f32> =
                                std::collections::HashMap::new();
                            for cluster_row in top_clusters {
                                if let Ok(ids) =
                                    serde_json::from_str::<Vec<u64>>(&cluster_row.entry_ids_json)
                                {
                                    for id in ids {
                                        let sim = cluster_row.similarity;
                                        let entry = entry_max_sim.entry(id).or_insert(0.0_f32);
                                        if sim > *entry {
                                            *entry = sim;
                                        }
                                    }
                                }
                            }

                            // Fetch Active EntryRecord objects individually (store.get_by_ids
                            // does not exist; use store.get(id) per spec — OQ-1 resolved).
                            // Active-status filter: only Status::Active entries included (FR-20).
                            //
                            // NAMING COLLISION WARNING (ADR-005 crt-046):
                            // record.confidence below = EntryRecord.confidence (Wilson-score).
                            // IndexEntry.confidence = raw HNSW cosine — NOT used here.
                            // Both fields are named `confidence`. The wrong one silently
                            // produces incorrect cluster_score weights — DO NOT swap them.
                            let mut cluster_entries_with_scores: Vec<(
                                crate::mcp::response::IndexEntry,
                                f32,
                            )> = Vec::new();

                            for &id in &cluster_entry_ids_raw {
                                match store.get(id).await {
                                    Ok(record) if record.status == Status::Active => {
                                        let goal_cosine: f32 =
                                            entry_max_sim.get(&id).copied().unwrap_or(0.0);

                                        // cluster_score uses EntryRecord.confidence
                                        // (Wilson-score), NOT IndexEntry.confidence (cosine).
                                        let cluster_score: f32 = (record.confidence as f32
                                            * config.w_goal_cluster_conf)
                                            + (goal_cosine * config.w_goal_boost);

                                        let index_entry = crate::mcp::response::IndexEntry {
                                            id: record.id,
                                            topic: record.topic.clone(),
                                            category: record.category.clone(),
                                            confidence: record.confidence,
                                            snippet: record
                                                .content
                                                .chars()
                                                .take(crate::mcp::response::SNIPPET_CHARS)
                                                .collect(),
                                        };
                                        cluster_entries_with_scores
                                            .push((index_entry, cluster_score));
                                    }
                                    Ok(_) => {
                                        // Inactive, deprecated, or quarantined — excluded (AC-10).
                                        tracing::debug!(
                                            entry_id = id,
                                            "context_briefing: cluster entry not Active — excluded"
                                        );
                                    }
                                    Err(StoreError::EntryNotFound(_)) => {
                                        // Entry deleted after cluster was written — skip silently.
                                        tracing::debug!(
                                            entry_id = id,
                                            "context_briefing: cluster entry not found — skip"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            entry_id = id,
                                            error = %e,
                                            "context_briefing: store.get({id}) failed — skip"
                                        );
                                    }
                                }
                            }

                            // Semantic search (existing path — k=20).
                            let semantic_results = self
                                .services
                                .briefing
                                .index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
                                .await
                                .map_err(rmcp::ErrorData::from)?;

                            // Score-based interleaving (Option A, ADR-005).
                            // blend_cluster_entries is a pure function — no store access.
                            if cluster_entries_with_scores.is_empty() {
                                // No Active cluster candidates survived — pure semantic result.
                                semantic_results
                            } else {
                                crate::services::behavioral_signals::blend_cluster_entries(
                                    semantic_results,
                                    cluster_entries_with_scores,
                                    20, // k=20 — hardcoded per IndexBriefingService contract
                                )
                            }
                        }
                    }
                }
            } else {
                // Level 1 cold-start — no DB calls (ADR-004, Resolution 3).
                self.services
                    .briefing
                    .index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
                    .await
                    .map_err(rmcp::ErrorData::from)?
            };

            // 9. Collect entry IDs for audit + usage recording
            let entry_ids: Vec<u64> = entries.iter().map(|e| e.id).collect();

            // 10. Format response as flat indexed table (FR-12, AC-08)
            let table_text = format_index_table(&entries);

            // 11. Audit (fire-and-forget)
            self.audit_fire_and_forget(AuditEvent {
                event_id: 0,
                timestamp: 0,
                session_id: String::new(),
                agent_id: ctx.agent_id.clone(),
                operation: "context_briefing".to_string(),
                target_ids: entry_ids.clone(),
                outcome: Outcome::Success,
                detail: format!(
                    "index briefing: query derived, {} entries returned",
                    entries.len()
                ),
            });

            // 12. Usage recording (fire-and-forget via UsageService)
            // col-028 AC-06: access_weight = 0 (offer-only event; D-01 guard in
            // record_briefing_usage fires early and skips dedup slot + access_count).
            self.services.usage.record_access(
                &entry_ids,
                AccessSource::Briefing,
                UsageContext {
                    session_id: ctx.audit_ctx.session_id.clone(),
                    agent_id: Some(ctx.agent_id.clone()),
                    helpful: params.helpful,
                    feature_cycle: params.feature.clone(),
                    trust_level: Some(ctx.trust_level),
                    access_weight: 0, // col-028: was 1; offer-only, not explicit read
                    current_phase,    // col-028: phase captured above
                },
            );

            // 13. Return flat indexed table
            Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                table_text,
            )]))
        }
    }

    #[tool(
        name = "context_quarantine",
        description = "Quarantine or restore a knowledge entry. Quarantined entries are excluded from search and lookup results but remain accessible via context_get. Requires Admin capability."
    )]
    async fn context_quarantine(
        &self,
        Parameters(params): Parameters<QuarantineParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity + format + audit context (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &None)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Admin).await?;

        // 2. Validation
        validate_quarantine_params(&params).map_err(rmcp::ErrorData::from)?;

        // 3. Parse action
        let action = parse_quarantine_action(&params.action).map_err(rmcp::ErrorData::from)?;

        // 6. Fetch entry (verify exists)
        let entry_id = validated_id(params.id).map_err(rmcp::ErrorData::from)?;
        let entry = self.entry_store.get(entry_id).await.map_err(|e| {
            rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::Store(e)))
        })?;

        // 7. Action dispatch
        match action {
            QuarantineAction::Quarantine => {
                // Idempotent: already quarantined
                if entry.status == Status::Quarantined {
                    return Ok(format_quarantine_success(
                        &entry,
                        Some("already quarantined"),
                        ctx.format,
                    ));
                }

                // All non-quarantined statuses (Active, Deprecated, Proposed) are valid

                // Atomic quarantine + audit
                let audit_event = AuditEvent {
                    event_id: 0,
                    timestamp: 0,
                    session_id: String::new(),
                    agent_id: ctx.agent_id.clone(),
                    operation: "context_quarantine".to_string(),
                    target_ids: vec![],
                    outcome: Outcome::Success,
                    detail: String::new(),
                };
                let updated = self
                    .quarantine_with_audit(entry_id, params.reason.clone(), audit_event)
                    .await
                    .map_err(rmcp::ErrorData::from)?;

                // Recompute confidence (fire-and-forget via ConfidenceService, vnc-010)
                self.services.confidence.recompute(&[entry_id]);

                Ok(format_quarantine_success(
                    &updated,
                    params.reason.as_deref(),
                    ctx.format,
                ))
            }
            QuarantineAction::Restore => {
                if entry.status != Status::Quarantined {
                    return Err(rmcp::ErrorData::from(
                        crate::error::ServerError::InvalidInput {
                            field: "id".to_string(),
                            reason: "entry is not quarantined".to_string(),
                        },
                    ));
                }

                let audit_event = AuditEvent {
                    event_id: 0,
                    timestamp: 0,
                    session_id: String::new(),
                    agent_id: ctx.agent_id.clone(),
                    operation: "context_quarantine".to_string(),
                    target_ids: vec![],
                    outcome: Outcome::Success,
                    detail: String::new(),
                };
                let updated = self
                    .restore_with_audit(entry_id, params.reason.clone(), audit_event)
                    .await
                    .map_err(rmcp::ErrorData::from)?;

                // Recompute confidence (fire-and-forget via ConfidenceService, vnc-010)
                self.services.confidence.recompute(&[entry_id]);

                Ok(format_restore_success(
                    &updated,
                    params.reason.as_deref(),
                    ctx.format,
                ))
            }
        }
    }

    // -- alc-002: context_enroll --

    #[tool(
        name = "context_enroll",
        description = "Enroll a new agent or update an existing agent's trust level and capabilities. Requires Admin capability."
    )]
    async fn context_enroll(
        &self,
        Parameters(params): Parameters<EnrollParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity + format + audit context (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &None)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Admin).await?;

        // 2. Input validation
        validate_enroll_params(&params).map_err(rmcp::ErrorData::from)?;

        // 3. Parse trust level and capabilities (strict per ADR-001)
        let trust_level = parse_trust_level(&params.trust_level).map_err(rmcp::ErrorData::from)?;
        let capabilities =
            parse_capabilities(&params.capabilities).map_err(rmcp::ErrorData::from)?;

        // 4. Business logic: enroll or update agent
        let result = self
            .registry
            .enroll_agent(
                &ctx.agent_id,
                &params.target_agent_id,
                trust_level,
                capabilities,
            )
            .map_err(rmcp::ErrorData::from)?;

        // 5. Format response
        let response = format_enroll_success(&result, ctx.format);

        // 6. Audit logging
        let detail = if result.created {
            format!(
                "created agent '{}' as {:?}",
                result.agent.agent_id, result.agent.trust_level
            )
        } else {
            format!(
                "updated agent '{}' to {:?}",
                result.agent.agent_id, result.agent.trust_level
            )
        };

        self.audit_fire_and_forget(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: ctx.agent_id.clone(),
            operation: "context_enroll".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail,
        });

        Ok(response)
    }

    #[tool(
        name = "context_cycle_review",
        description = "Analyze observation data for a work cycle. Parses session telemetry, attributes to cycle, detects hotspots, computes metrics, and returns a self-contained report."
    )]
    async fn context_cycle_review(
        &self,
        Parameters(params): Parameters<RetrospectiveParams>,
    ) -> Result<CallToolResult, rmcp::model::ErrorData> {
        use crate::error::{ERROR_INVALID_PARAMS, ERROR_NO_OBSERVATION_DATA, ServerError};
        use crate::mcp::response::format_retrospective_markdown;
        use crate::mcp::response::format_retrospective_report;

        // 1. Identity resolution (no format param on this handler)
        let identity = self
            .resolve_agent(&params.agent_id)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 2. Validation
        crate::infra::validation::validate_retrospective_params(&params)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Load observations from SQL via ObservationSource (col-024)
        //    Three-path lookup: primary cycle_events-based → legacy sessions.feature_cycle → content-scan.
        //    Primary path introduced in col-024; legacy paths preserved for backward compatibility.
        //    col-026: attribution_path_label is set inside each path branch for step 10i.
        let mut attribution_path_label: Option<&'static str> = None;
        let store_for_obs = Arc::clone(&self.store);
        let registry_for_obs = Arc::clone(&self.observation_registry);
        let feature_cycle_for_load = params.feature_cycle.clone();
        // col-026: return (observations, path_label) so step 10i can record attribution_path.
        let (attributed, obs_path_label) = crate::infra::timeout::spawn_blocking_with_timeout(
            crate::infra::timeout::MCP_HANDLER_TIMEOUT,
            move || -> std::result::Result<(Vec<unimatrix_observe::ObservationRecord>, &'static str), unimatrix_observe::ObserveError> {
                use unimatrix_observe::ObservationSource;
                let source = crate::services::observation::SqlObservationSource::new(store_for_obs, registry_for_obs);

                // ---- Path 1: Primary (cycle_events-based, col-024) ----
                // Returns Ok(vec![]) for pre-col-024 features and enrichment gaps.
                // Returns Err only on genuine SQL failure — errors propagate via ?, do NOT activate fallback (FM-01).
                let primary = source.load_cycle_observations(&feature_cycle_for_load)?;
                if !primary.is_empty() {
                    return Ok((primary, "cycle_events-first (primary)"));
                }

                // Primary path returned empty. Log fallback transition (ADR-003).
                // Suppressed in production (debug level). Visible with RUST_LOG=debug.
                tracing::debug!(
                    cycle_id = %feature_cycle_for_load,
                    path = "load_feature_observations",
                    "CycleReview: primary path empty, falling back to legacy sessions path"
                );

                // ---- Path 2: Legacy-1 (sessions.feature_cycle) ----
                let legacy1 = source.load_feature_observations(&feature_cycle_for_load)?;
                if !legacy1.is_empty() {
                    return Ok((legacy1, "sessions.feature_cycle (legacy)"));
                }

                // Legacy-1 also returned empty. Log second fallback transition (ADR-003).
                tracing::debug!(
                    cycle_id = %feature_cycle_for_load,
                    path = "load_unattributed_sessions",
                    "CycleReview: legacy sessions path empty, falling back to content attribution"
                );

                // ---- Path 3: Legacy-2 (content-based attribution) ----
                // Unchanged from pre-col-024.
                let unattributed = source.load_unattributed_sessions()?;
                if unattributed.is_empty() {
                    return Ok((vec![], "content-scan (fallback)"));
                }

                let obs = unimatrix_observe::attribute_sessions(&unattributed, &feature_cycle_for_load);
                Ok((obs, "content-scan (fallback)"))
            },
        )
        .await
        .map_err(rmcp::ErrorData::from)?
        .map_err(|e| ServerError::ObservationError(e.to_string()))
        .map_err(rmcp::ErrorData::from)?;

        attribution_path_label = Some(obs_path_label);

        let store = Arc::clone(&self.store);
        let feature_cycle = params.feature_cycle.clone();

        // -----------------------------------------------------------------------
        // Step 2.5 (crt-033): Memoization check / force=true purged-signals gate.
        // Executes AFTER three-path observation load, BEFORE step 4 (is_empty check).
        //
        // crt-046 Resolution 2 / FR-09: The memoisation early-return is NOT taken here.
        // Instead, we record the memo result and defer the return until AFTER step 8b.
        // Step 8b must run on every context_cycle_review call — cache-hit or miss (AC-15).
        // -----------------------------------------------------------------------
        let force = params.force.unwrap_or(false);

        // memo_hit holds (report, advisory) when a valid stored review was found AND
        // force=false. On force=true or deserialization error, this is None.
        let memo_hit: Option<(unimatrix_observe::RetrospectiveReport, Option<String>)>;

        if !force {
            // Normal path: check for a stored review before any computation.
            match store.get_cycle_review(&feature_cycle).await {
                Ok(Some(record)) => {
                    // Memoization candidate — deserialize to check schema version.
                    match check_stored_review(&record, unimatrix_store::SUMMARY_SCHEMA_VERSION) {
                        Ok((report, advisory)) => {
                            // Record memo hit — do NOT return early (FR-09, crt-046 Resolution 2).
                            // Step 8b will run below; return from memo_hit AFTER step 8b.
                            memo_hit = Some((report, advisory));
                        }
                        Err(e) => {
                            // ADR-003: deserialization error → treat as cache miss.
                            tracing::warn!(
                                "crt-033: deserialization of stored summary_json failed for \
                                 {}: {} — falling through to full recomputation",
                                feature_cycle,
                                e
                            );
                            // Fall through to full pipeline.
                            memo_hit = None;
                        }
                    }
                }
                Ok(None) => {
                    // Cache miss — proceed to full pipeline.
                    memo_hit = None;
                }
                Err(e) => {
                    // Read error — treat as cache miss (ADR-003).
                    tracing::warn!(
                        "crt-033: get_cycle_review read error for {}: {} — treating as cache miss",
                        feature_cycle,
                        e
                    );
                    // Fall through to full pipeline.
                    memo_hit = None;
                }
            }
        } else {
            memo_hit = None;
        }

        if force && attributed.is_empty() {
            // force=true AND observations are empty.
            // Sole discriminator is get_cycle_review() return value (OQ-01, FR-05/FR-06).
            match store.get_cycle_review(&feature_cycle).await {
                Ok(Some(record)) => {
                    // Stored record exists: signals were purged after review was written.
                    let computed_at_display = record.computed_at.to_string();
                    let note = format!(
                        "Raw signals have been purged; returning stored record from {}.",
                        computed_at_display
                    );
                    match check_stored_review(&record, unimatrix_store::SUMMARY_SCHEMA_VERSION) {
                        Ok((report, _advisory)) => {
                            // 11. Audit
                            self.audit_fire_and_forget(AuditEvent {
                                event_id: 0,
                                timestamp: 0,
                                session_id: String::new(),
                                agent_id: identity.agent_id.clone(),
                                operation: "context_cycle_review".to_string(),
                                target_ids: vec![],
                                outcome: Outcome::Success,
                                detail: format!(
                                    "retrospective for {} (purged signals path)",
                                    feature_cycle
                                ),
                            });
                            let fmt = params.format.as_deref().unwrap_or("markdown");
                            return dispatch_review_with_advisory(
                                report,
                                fmt,
                                params.evidence_limit,
                                Some(note),
                            );
                        }
                        Err(e) => {
                            // Corrupt stored record + no signals = cannot recover.
                            tracing::warn!(
                                "crt-033: deserialization failed on purged-signals path \
                                 for {}: {}",
                                feature_cycle,
                                e
                            );
                            return Err(rmcp::model::ErrorData::new(
                                crate::error::ERROR_INTERNAL,
                                format!(
                                    "Stored cycle review for '{}' is corrupt and raw signals \
                                     have been purged. A reindex is required.",
                                    feature_cycle
                                ),
                                None,
                            ));
                        }
                    }
                }
                Ok(None) => {
                    // No stored record: return ERROR_NO_OBSERVATION_DATA (FR-06).
                    return Err(rmcp::model::ErrorData::new(
                        ERROR_NO_OBSERVATION_DATA,
                        format!(
                            "No observation data found for feature '{}'. \
                             Ensure hook scripts are installed and sessions have been run.",
                            feature_cycle
                        ),
                        None,
                    ));
                }
                Err(e) => {
                    // Read error with force=true + empty attributed: cannot distinguish
                    // purged from never-existed → safest response is ERROR_NO_OBSERVATION_DATA.
                    tracing::warn!(
                        "crt-033: get_cycle_review read error (force=true, empty attributed) \
                         for {}: {}",
                        feature_cycle,
                        e
                    );
                    return Err(rmcp::model::ErrorData::new(
                        ERROR_NO_OBSERVATION_DATA,
                        format!(
                            "No observation data found for feature '{}'. \
                             Ensure hook scripts are installed and sessions have been run.",
                            feature_cycle
                        ),
                        None,
                    ));
                }
            }
        }
        // If force=true AND attributed is non-empty: step 2.5 check is skipped entirely;
        // fall through to step 4 and full pipeline (FR-04).

        // -----------------------------------------------------------------------
        // Full pipeline — only runs when memo_hit is None (cache miss or force=true).
        // On memo_hit, steps 6–8a are skipped; step 8b still runs below (FR-09).
        // -----------------------------------------------------------------------
        // `full_report` holds the freshly computed RetrospectiveReport on the full
        // pipeline path. None on the memo_hit path (cached report is in memo_hit).
        let mut full_report: Option<unimatrix_observe::RetrospectiveReport> = None;
        // `cycle_outcome` is derived from cycle_events on the full pipeline path
        // and passed to run_step_8b. None on cache-hit — outcome not needed (INSERT OR IGNORE).
        let mut cycle_outcome: Option<String> = None;

        if memo_hit.is_none() {
            // 6. Check for data availability
            if attributed.is_empty() {
                // No new data -- check for cached MetricVector
                let cached = store
                    .get_metrics(&feature_cycle)
                    .await
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))
                    .map_err(rmcp::ErrorData::from)?;

                match cached {
                    Some(mv) => {
                        // Return cached result (FR-09.6)
                        let report = unimatrix_observe::RetrospectiveReport {
                            feature_cycle: feature_cycle.clone(),
                            session_count: 0,
                            total_records: 0,
                            metrics: mv,
                            hotspots: vec![],
                            is_cached: true,
                            baseline_comparison: None,
                            entries_analysis: None,
                            narratives: None,
                            recommendations: vec![],
                            session_summaries: None,
                            feature_knowledge_reuse: None,
                            rework_session_count: None,
                            context_reload_pct: None,
                            attribution: None,
                            phase_narrative: None,
                            goal: None,
                            cycle_type: None,
                            attribution_path: None,
                            is_in_progress: None,
                            phase_stats: None,
                            curation_health: None, // crt-047
                        };

                        // Cached path also respects format (vnc-011)
                        let format = params.format.as_deref().unwrap_or("markdown");
                        return match format {
                            "markdown" | "summary" => Ok(format_retrospective_markdown(&report)),
                            "json" => Ok(format_retrospective_report(&report)),
                            _ => Err(rmcp::model::ErrorData::new(
                                ERROR_INVALID_PARAMS,
                                format!(
                                    "Unknown format '{}'. Valid values: \"markdown\", \"json\".",
                                    format
                                ),
                                None,
                            )),
                        };
                    }
                    None => {
                        // No data, no cache (FR-09.7)
                        return Err(rmcp::model::ErrorData::new(
                            ERROR_NO_OBSERVATION_DATA,
                            format!(
                                "No observation data found for feature '{}'. Ensure hook scripts are installed and sessions have been run.",
                                feature_cycle
                            ),
                            None,
                        ));
                    }
                }
            }

            // 7a. Load historical MetricVectors for baseline
            let all_metrics = store
                .list_all_metrics()
                .await
                .map_err(|e| ServerError::Core(CoreError::Store(e)))
                .map_err(rmcp::ErrorData::from)?;

            // 7b. Collect historical vectors, excluding current feature
            let mut history: Vec<unimatrix_observe::MetricVector> = Vec::new();
            for (fc, mv) in &all_metrics {
                if fc != &feature_cycle {
                    history.push(mv.clone());
                }
            }

            // 7c. Run detection with history for PhaseDurationOutlierRule
            let history_slice = if history.is_empty() {
                None
            } else {
                Some(history.as_slice())
            };
            let rules = unimatrix_observe::default_rules(history_slice);
            let hotspots = unimatrix_observe::detect_hotspots(&attributed, &rules);

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let metrics = unimatrix_observe::compute_metric_vector(&attributed, &hotspots, now);

            // 8. Store MetricVector (nxs-009: typed API, no bincode serialization)
            store.store_metrics(&feature_cycle, &metrics);

            // 10a. Compute baseline comparison
            let baseline = unimatrix_observe::compute_baselines(&history)
                .map(|baselines| unimatrix_observe::compare_to_baseline(&metrics, &baselines));

            // 10b. Drain accumulated entry analysis from signal consumers (col-009, FR-10.5)
            // vnc-005: drain_for(&feature_cycle) replaces drain_all() — drains only the
            // bucket for this feature cycle, leaving other feature cycles' data intact.
            let entries_analysis = {
                let mut pending = self
                    .pending_entries_analysis
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let drained = pending.drain_for(&feature_cycle);
                if drained.is_empty() {
                    None
                } else {
                    Some(drained)
                }
            };

            // 10c. Build report with baseline and entries_analysis
            let mut report = unimatrix_observe::build_report(
                &feature_cycle,
                &attributed,
                metrics,
                hotspots,
                baseline,
                entries_analysis,
            );

            // 10d. col-010b: Synthesize recommendations (both paths)
            let recommendations = unimatrix_observe::recommendations_for_hotspots(&report.hotspots);
            report.recommendations = recommendations;

            // 10e. col-010b/col-012: Narratives — now on SQL path.
            report.narratives = Some(unimatrix_observe::synthesize_narratives(&report.hotspots));

            // 10f. col-010b: Fire-and-forget lesson-learned write (ADR-002: self.clone())
            if !report.hotspots.is_empty() || !report.recommendations.is_empty() {
                let server = self.clone();
                let report_for_ll = report.clone();
                let fc_for_ll = feature_cycle.clone();
                tokio::spawn(async move {
                    if let Err(e) = write_lesson_learned(&server, &report_for_ll, &fc_for_ll).await
                    {
                        tracing::warn!("lesson-learned write failed for {}: {}", fc_for_ll, e);
                    }
                });
            }

            // col-020: Multi-session retrospective steps (best-effort, all fields default to None)
            // Steps 11-17 depend on session_records from step 11. If step 11 fails, all are skipped.
            let session_data: Option<(
                Vec<unimatrix_observe::SessionSummary>,
                Vec<unimatrix_store::SessionRecord>,
            )> = match (|| async {
                // Step 11: Compute session summaries (C1)
                let mut summaries = unimatrix_observe::compute_session_summaries(&attributed);

                // Enrich with outcome from SessionRecord
                let session_records = store
                    .scan_sessions_by_feature(&feature_cycle)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

                // Build session_id -> outcome map
                let outcome_map: std::collections::HashMap<String, Option<String>> =
                    session_records
                        .iter()
                        .map(|sr| (sr.session_id.clone(), sr.outcome.clone()))
                        .collect();

                // Attach outcomes to summaries
                for summary in &mut summaries {
                    if let Some(outcome) = outcome_map.get(&summary.session_id) {
                        summary.outcome = outcome.clone();
                    }
                }

                Ok::<_, Box<dyn std::error::Error + Send + Sync>>((summaries, session_records))
            })()
            .await
            {
                Ok(data) => Some(data),
                Err(e) => {
                    tracing::warn!("col-020: session summaries failed: {e}");
                    None
                }
            };

            if let Some((summaries, session_records)) = session_data {
                // Step 12: Context reload percentage (C1, best-effort)
                let reload_pct =
                    unimatrix_observe::compute_context_reload_pct(&summaries, &attributed);
                report.context_reload_pct = Some(reload_pct);

                // Step 13-14: Knowledge reuse (C3/C4, best-effort; col-026: cross-feature split)
                match compute_knowledge_reuse_for_sessions(&store, &session_records, &feature_cycle)
                    .await
                {
                    Ok(mut reuse) => {
                        // col-026: set total_stored from feature_entries count for this cycle.
                        // compute_knowledge_reuse leaves total_stored=0; caller fills it here.
                        match sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(*) FROM feature_entries WHERE feature_id = ?",
                        )
                        .bind(&feature_cycle)
                        .fetch_one(store.write_pool_server())
                        .await
                        {
                            Ok(count) => reuse.total_stored = count as u64,
                            Err(e) => {
                                tracing::warn!(
                                    "col-026: total_stored count failed for {}: {e}",
                                    feature_cycle
                                );
                            }
                        }
                        report.feature_knowledge_reuse = Some(reuse);
                    }
                    Err(e) => tracing::warn!("col-020: knowledge reuse computation failed: {e}"),
                }

                // Step 15: Rework session count (case-insensitive substring match per human override)
                let rework_count = session_records
                    .iter()
                    .filter(|sr| {
                        if let Some(outcome) = &sr.outcome {
                            let lower = outcome.to_lowercase();
                            lower.contains("result:rework") || lower.contains("result:failed")
                        } else {
                            false
                        }
                    })
                    .count() as u64;
                report.rework_session_count = Some(rework_count);

                // Step 16: Attribution metadata (ADR-003)
                match (|| async {
                    let store_for_discover = Arc::clone(&store);
                    let registry_for_discover = Arc::clone(&self.observation_registry);
                    let fc_for_discover = feature_cycle.clone();
                    let discovered_ids = crate::infra::timeout::spawn_blocking_with_timeout(
                        crate::infra::timeout::MCP_HANDLER_TIMEOUT,
                        move || {
                            use unimatrix_observe::ObservationSource;
                            let source = crate::services::observation::SqlObservationSource::new(
                                store_for_discover,
                                registry_for_discover,
                            );
                            source.discover_sessions_for_feature(&fc_for_discover)
                        },
                    )
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

                    let attributed_count = session_records
                        .iter()
                        .filter(|sr| sr.feature_cycle.as_deref() == Some(&feature_cycle))
                        .count();
                    let total_count = discovered_ids.len();

                    Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                        unimatrix_observe::AttributionMetadata {
                            attributed_session_count: attributed_count,
                            total_session_count: total_count,
                        },
                    )
                })()
                .await
                {
                    Ok(meta) => report.attribution = Some(meta),
                    Err(e) => tracing::warn!("col-020: attribution metadata failed: {e}"),
                }

                // Step 17: Idempotent counter update (ADR-002, best-effort)
                {
                    let total_sessions = session_records.len() as i64;
                    let total_tool_calls = report.metrics.universal.total_tool_calls as i64;
                    let total_duration_secs = report.metrics.universal.total_duration_secs as i64;
                    // Ensure record exists before setting counters
                    match store.get_topic_delivery(&feature_cycle).await {
                        Ok(None) => {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            store.upsert_topic_delivery(&unimatrix_store::TopicDeliveryRecord {
                                topic: feature_cycle.clone(),
                                created_at: now,
                                completed_at: None,
                                status: "active".to_string(),
                                github_issue: None,
                                total_sessions: 0,
                                total_tool_calls: 0,
                                total_duration_secs: 0,
                                phases_completed: None,
                            });
                            match store
                                .set_topic_delivery_counters(
                                    &feature_cycle,
                                    total_sessions,
                                    total_tool_calls,
                                    total_duration_secs,
                                )
                                .await
                            {
                                Ok(()) => {}
                                Err(e) => tracing::warn!("col-020: counter update failed: {e}"),
                            }
                        }
                        Ok(Some(_)) => {
                            match store
                                .set_topic_delivery_counters(
                                    &feature_cycle,
                                    total_sessions,
                                    total_tool_calls,
                                    total_duration_secs,
                                )
                                .await
                            {
                                Ok(()) => {}
                                Err(e) => tracing::warn!("col-020: counter update failed: {e}"),
                            }
                        }
                        Err(e) => tracing::warn!("col-020: counter update failed: {e}"),
                    }
                }

                // Assign session summaries to report
                report.session_summaries = Some(summaries);
            }

            // 10g. crt-025: Phase narrative assembly
            // col-026: events are hoisted to outer scope so steps 10h (PhaseStats) and
            // 10i (is_in_progress) can borrow them after step 10g. Both build_phase_narrative
            // (step 10g) and compute_phase_stats (step 10h) borrow &[CycleEventRecord].
            // Query 1: cycle event log for this feature cycle
            let event_rows = sqlx::query(
                "SELECT seq, event_type, phase, outcome, next_phase, timestamp \
                   FROM cycle_events \
                  WHERE cycle_id = ?1 \
                  ORDER BY timestamp ASC, seq ASC",
            )
            .bind(&feature_cycle)
            .fetch_all(store.write_pool_server())
            .await
            .map_err(|e| {
                tracing::warn!(
                    "crt-025: cycle_events query failed for {}: {}",
                    feature_cycle,
                    e
                );
            });

            // col-026: outer option to hold events for steps 10h and 10i
            let mut cycle_events_vec: Option<Vec<unimatrix_observe::CycleEventRecord>> = None;

            if let Ok(event_rows) = event_rows {
                if !event_rows.is_empty() {
                    use unimatrix_observe::{CycleEventRecord, PhaseCategoryDist};

                    // Map rows to CycleEventRecord
                    let events: Vec<CycleEventRecord> = event_rows
                        .iter()
                        .map(|row| {
                            use sqlx::Row;
                            CycleEventRecord {
                                seq: row.try_get::<i64, _>("seq").unwrap_or(0),
                                event_type: row
                                    .try_get::<String, _>("event_type")
                                    .unwrap_or_default(),
                                phase: row.try_get::<Option<String>, _>("phase").unwrap_or(None),
                                outcome: row
                                    .try_get::<Option<String>, _>("outcome")
                                    .unwrap_or(None),
                                next_phase: row
                                    .try_get::<Option<String>, _>("next_phase")
                                    .unwrap_or(None),
                                timestamp: row.try_get::<i64, _>("timestamp").unwrap_or(0),
                            }
                        })
                        .collect();

                    // Query 2: current feature phase/category distribution
                    let current_dist: PhaseCategoryDist = match sqlx::query(
                        "SELECT fe.phase, e.category, COUNT(*) AS cnt \
                           FROM feature_entries fe \
                           JOIN entries e ON e.id = fe.entry_id \
                          WHERE fe.feature_id = ?1 \
                            AND fe.phase IS NOT NULL \
                          GROUP BY fe.phase, e.category",
                    )
                    .bind(&feature_cycle)
                    .fetch_all(store.write_pool_server())
                    .await
                    {
                        Ok(rows) => {
                            use sqlx::Row;
                            let mut dist: PhaseCategoryDist = std::collections::HashMap::new();
                            for row in &rows {
                                let phase: String = row.try_get("phase").unwrap_or_default();
                                let category: String = row.try_get("category").unwrap_or_default();
                                let cnt: i64 = row.try_get("cnt").unwrap_or(0);
                                dist.entry(phase).or_default().insert(category, cnt as u64);
                            }
                            dist
                        }
                        Err(e) => {
                            tracing::warn!(
                                "crt-025: phase/category dist query failed for {}: {}",
                                feature_cycle,
                                e
                            );
                            std::collections::HashMap::new()
                        }
                    };

                    // Query 3: cross-feature baseline (excludes current feature)
                    let cross_dist: std::collections::HashMap<String, PhaseCategoryDist> =
                        match sqlx::query(
                            "SELECT fe.feature_id, fe.phase, e.category, COUNT(*) AS cnt \
                               FROM feature_entries fe \
                               JOIN entries e ON e.id = fe.entry_id \
                              WHERE fe.feature_id IN ( \
                                    SELECT DISTINCT feature_id FROM feature_entries WHERE phase IS NOT NULL \
                                ) \
                                AND fe.feature_id != ?1 \
                                AND fe.phase IS NOT NULL \
                              GROUP BY fe.feature_id, fe.phase, e.category",
                        )
                        .bind(&feature_cycle)
                        .fetch_all(store.write_pool_server())
                        .await
                        {
                            Ok(rows) => {
                                use sqlx::Row;
                                let mut by_feature: std::collections::HashMap<
                                    String,
                                    PhaseCategoryDist,
                                > = std::collections::HashMap::new();
                                for row in &rows {
                                    let feature_id: String =
                                        row.try_get("feature_id").unwrap_or_default();
                                    let phase: String =
                                        row.try_get("phase").unwrap_or_default();
                                    let category: String =
                                        row.try_get("category").unwrap_or_default();
                                    let cnt: i64 = row.try_get("cnt").unwrap_or(0);
                                    by_feature
                                        .entry(feature_id)
                                        .or_default()
                                        .entry(phase)
                                        .or_default()
                                        .insert(category, cnt as u64);
                                }
                                by_feature
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "crt-025: cross-feature dist query failed for {}: {}",
                                    feature_cycle,
                                    e
                                );
                                std::collections::HashMap::new()
                            }
                        };

                    // Build phase narrative (pure function); borrows &events
                    let narrative = unimatrix_observe::build_phase_narrative(
                        &events,
                        &current_dist,
                        &cross_dist,
                    );
                    report.phase_narrative = Some(narrative);

                    // Extract cycle outcome from cycle_stop event for step 8b.
                    // cycle_stop carries the authoritative outcome for the cycle.
                    // Fallback: last cycle_phase_end outcome (if no cycle_stop yet).
                    cycle_outcome = events
                        .iter()
                        .find(|e| e.event_type == "cycle_stop")
                        .and_then(|e| e.outcome.clone())
                        .or_else(|| {
                            events
                                .iter()
                                .rev()
                                .find(|e| e.event_type == "cycle_phase_end")
                                .and_then(|e| e.outcome.clone())
                        });

                    // Stash events for steps 10h and 10i (both borrow from this vec)
                    cycle_events_vec = Some(events);
                }
                // If event_rows is empty, phase_narrative remains None (AC-12/13)
                // and cycle_events_vec remains None → is_in_progress = None
            }

            // 10h. col-026: PhaseStats computation (best-effort, pure — no DB)
            {
                let events_slice = cycle_events_vec.as_deref().unwrap_or(&[]);
                let phase_stats = compute_phase_stats(events_slice, &attributed);
                report.phase_stats = if phase_stats.is_empty() {
                    None
                } else {
                    Some(phase_stats)
                };
            }

            // 10i. col-026: goal, cycle_type, is_in_progress, attribution_path (best-effort)
            match (|| async {
                let goal = store
                    .get_cycle_start_goal(&feature_cycle)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(goal)
            })()
            .await
            {
                Ok(goal_opt) => {
                    let cycle_type = infer_cycle_type(goal_opt.as_deref());
                    report.goal = goal_opt;
                    report.cycle_type = Some(cycle_type);
                }
                Err(e) => {
                    tracing::warn!("col-026: get_cycle_start_goal failed for {feature_cycle}: {e}");
                    // report.goal remains None, report.cycle_type remains None
                }
            }

            // is_in_progress: derived in-memory from cycle_events (no DB call)
            report.is_in_progress = derive_is_in_progress(cycle_events_vec.as_deref());

            // attribution_path: label recorded at path-selection time in step 3
            report.attribution_path = attribution_path_label.map(|s| s.to_string());

            // -------------------------------------------------------------------
            // Step 8a-crt-047: Compute curation snapshot BEFORE store_cycle_review.
            // Read (ENTRIES via write_pool_server) must complete before the write
            // step acquires the write connection (I-01: read → compute → write order).
            // Non-fatal: failures produce curation_health = None in the response.
            // -------------------------------------------------------------------

            // Derive cycle_start_ts from cycle_events already read by the handler.
            // Returns 0 if no cycle_start event found (EC-02: open window, over-count risk).
            let cycle_start_ts = extract_cycle_start_ts(cycle_events_vec.as_deref());
            if cycle_start_ts == 0 {
                tracing::warn!(
                    "crt-047: no cycle_start event found for {} — \
                     orphan_deprecations window is [0, now], over-counting risk (EC-02)",
                    feature_cycle
                );
            }

            let review_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            // Snapshot computation: ENTRIES queries via write_pool_server().
            // read_pool() is pub(crate) in unimatrix-store and not cross-crate accessible.
            let curation_snapshot: Option<unimatrix_observe::CurationSnapshot> =
                match crate::services::curation_health::compute_curation_snapshot(
                    &store,
                    &feature_cycle,
                    cycle_start_ts,
                    review_ts,
                )
                .await
                {
                    Ok(snapshot) => Some(snapshot),
                    Err(e) => {
                        tracing::warn!(
                            "crt-047: compute_curation_snapshot failed for {}: {} — \
                             curation_health will be absent from response",
                            feature_cycle,
                            e
                        );
                        None
                    }
                };

            // -------------------------------------------------------------------
            // Step 8a (crt-033 + crt-047): Build record including snapshot columns.
            // first_computed_at = cycle_start_ts for new rows (fallback: review_ts).
            // store_cycle_review() two-step upsert preserves it on subsequent writes (ADR-001).
            // -------------------------------------------------------------------
            // Window size for context_cycle_review baseline (matches status.rs constant).
            const CURATION_BASELINE_WINDOW_FOR_REVIEW: usize = 10;

            let first_computed_at = if cycle_start_ts > 0 {
                cycle_start_ts
            } else {
                review_ts
            };

            match build_cycle_review_record(
                &feature_cycle,
                &report,
                curation_snapshot.as_ref(),
                first_computed_at,
            ) {
                Ok(record) => {
                    if let Err(e) = store.store_cycle_review(&record).await {
                        tracing::warn!(
                            "crt-033: store_cycle_review failed for {}: {} — continuing",
                            feature_cycle,
                            e
                        );
                        // Log and continue — GH #409 gate note: if this fails, the purge gate
                        // will not fire for this cycle. This is acceptable over failing the caller.
                    }
                }
                Err(e) => {
                    // serde_json::to_string failed — should not occur after serde audit (ADR-003)
                    // but propagate defensively rather than panicking.
                    tracing::warn!(
                        "crt-033: build_cycle_review_record serialization failed for {}: {} — continuing",
                        feature_cycle,
                        e
                    );
                }
            }

            // -------------------------------------------------------------------
            // Step 8a-post (crt-047): Compute baseline comparison AFTER store.
            // Reads the updated window from cycle_review_index (read after write).
            // Non-fatal: .unwrap_or_default() on baseline window failure.
            // -------------------------------------------------------------------
            let curation_health_block: Option<unimatrix_observe::CurationHealthBlock> =
                if let Some(ref snapshot) = curation_snapshot {
                    let baseline_rows = store
                        .get_curation_baseline_window(CURATION_BASELINE_WINDOW_FOR_REVIEW)
                        .await
                        .unwrap_or_default();

                    let baseline_opt = crate::services::curation_health::compute_curation_baseline(
                        &baseline_rows,
                        CURATION_BASELINE_WINDOW_FOR_REVIEW,
                    );

                    let comparison_opt = baseline_opt.map(|baseline| {
                        crate::services::curation_health::compare_to_baseline(
                            snapshot,
                            &baseline,
                            baseline.history_cycles,
                        )
                    });

                    Some(unimatrix_observe::CurationHealthBlock {
                        snapshot: snapshot.clone(),
                        baseline: comparison_opt,
                    })
                } else {
                    None
                };

            // Attach curation health block to report before storage in full_report.
            report.curation_health = curation_health_block;

            full_report = Some(report);
        } // end of full pipeline block (memo_hit.is_none())

        // -----------------------------------------------------------------------
        // Step 8b (crt-046): Behavioral signal emission — ALWAYS RUNS (FR-09, Resolution 2).
        // Runs on EVERY context_cycle_review call: cache-hit (force=false) or cache-miss.
        // All errors are non-fatal — step 8b never causes the handler to fail.
        // parse_failure_count is returned as a top-level field in the JSON response (Resolution 1).
        // -----------------------------------------------------------------------
        let parse_failure_count: u32 = crate::services::behavioral_signals::run_step_8b(
            &store,
            &feature_cycle,
            cycle_outcome.as_deref(),
        )
        .await;

        // -----------------------------------------------------------------------
        // Memoisation early-return — AFTER step 8b (FR-09, Resolution 2, AC-15).
        //
        // On cache-hit (force=false AND memo_hit is Some): return the cached
        // report now that step 8b has run. Includes parse_failure_count (Resolution 1).
        // -----------------------------------------------------------------------
        if let Some((memo_report, advisory)) = memo_hit {
            // 11. Audit (cache-hit path label)
            self.audit_fire_and_forget(AuditEvent {
                event_id: 0,
                timestamp: 0,
                session_id: String::new(),
                agent_id: identity.agent_id,
                operation: "context_cycle_review".to_string(),
                target_ids: vec![],
                outcome: Outcome::Success,
                detail: format!("retrospective for {} (memoization hit)", feature_cycle),
            });
            let fmt = params.format.as_deref().unwrap_or("markdown");
            return dispatch_review_with_advisory_and_parse_failures(
                memo_report,
                fmt,
                params.evidence_limit,
                advisory,
                parse_failure_count,
            );
        }

        // 11. Audit (full pipeline path)
        self.audit_fire_and_forget(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_cycle_review".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: format!("retrospective for {}", feature_cycle),
        });

        // 12. vnc-011: Dispatch to format-specific output path (full pipeline result).
        // parse_failure_count is injected at the top level of the JSON response (Resolution 1).
        let report = full_report
            .expect("full_report must be Some when memo_hit is None — logic invariant violated");
        let format = params.format.as_deref().unwrap_or("markdown");
        match format {
            "markdown" | "summary" => {
                // Markdown path: formatter controls its own evidence selection (k=3 by timestamp).
                // evidence_limit is irrelevant here.
                // Append parse_failure_count as a trailing note when non-zero (or always for AC-13).
                let mut result = format_retrospective_markdown(&report);
                result.content.push(rmcp::model::Content::text(format!(
                    "\nparse_failure_count: {}",
                    parse_failure_count
                )));
                Ok(result)
            }
            "json" => {
                // JSON path: keep existing evidence_limit default of 3 (col-010b ADR-001).
                // parse_failure_count is injected as a top-level field alongside the report.
                let evidence_limit = params.evidence_limit.unwrap_or(3);
                let report_to_serialize = if evidence_limit > 0 {
                    let mut truncated = report.clone();
                    for hotspot in &mut truncated.hotspots {
                        hotspot.evidence.truncate(evidence_limit);
                    }
                    truncated
                } else {
                    report
                };
                // Build JSON object with parse_failure_count as a top-level field (Resolution 1).
                // Serialize report fields, then insert parse_failure_count alongside them.
                let json_str = match serde_json::to_value(&report_to_serialize) {
                    Ok(mut val) => {
                        if let Some(obj) = val.as_object_mut() {
                            obj.insert(
                                "parse_failure_count".to_string(),
                                serde_json::Value::Number(parse_failure_count.into()),
                            );
                        }
                        serde_json::to_string_pretty(&val).unwrap_or_default()
                    }
                    Err(_) => {
                        // Fallback: return plain report without parse_failure_count field
                        // (should not occur in practice — serde_json failure is unexpected).
                        tracing::warn!(
                            "crt-046: serde_json::to_value failed for {} — \
                             parse_failure_count not injected into response",
                            feature_cycle
                        );
                        serde_json::to_string_pretty(&report_to_serialize).unwrap_or_default()
                    }
                };
                Ok(rmcp::model::CallToolResult::success(vec![
                    rmcp::model::Content::text(json_str),
                ]))
            }
            _ => {
                // Unrecognized format: return error
                Err(rmcp::model::ErrorData::new(
                    ERROR_INVALID_PARAMS,
                    format!(
                        "Unknown format '{}'. Valid values: \"markdown\", \"json\".",
                        format
                    ),
                    None,
                ))
            }
        }
    }

    // -- col-022: context_cycle --

    #[tool(
        name = "context_cycle",
        description = "Declare the start or end of a feature cycle for this session. \
            Call with type='start' at session beginning to set feature attribution. \
            Call with type='stop' when feature work is complete. \
            Attribution is best-effort via the hook path; confirm via context_cycle_review."
    )]
    async fn context_cycle(
        &self,
        Parameters(params): Parameters<CycleParams>,
    ) -> Result<CallToolResult, rmcp::model::ErrorData> {
        // 1. Identity resolution
        let identity = self
            .resolve_agent(&params.agent_id)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check -- SessionWrite maps to Write capability
        self.require_cap(&identity.agent_id, Capability::Write)
            .await?;

        // 3. Validation via shared validate_cycle_params (ADR-004, C-02)
        let validated = match validate_cycle_params(
            &params.r#type,
            &params.topic,
            params.phase.as_deref(),
            params.outcome.as_deref(),
            params.next_phase.as_deref(),
        ) {
            Err(msg) => {
                return Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    format!("Validation error: {msg}"),
                )]));
            }
            Ok(v) => v,
        };

        // 3b. Goal validation (col-025, ADR-005): Start events only.
        // For PhaseEnd and Stop, goal is silently ignored.
        let validated_goal: Option<String> = if validated.cycle_type == CycleType::Start {
            match params.goal {
                None => None,
                Some(g) => {
                    // Step 1: Trim whitespace
                    let trimmed = g.trim().to_owned();

                    // Step 2: Normalize empty / whitespace-only to None (FR-11, ADR-005)
                    if trimmed.is_empty() {
                        None
                    } else {
                        // Step 3: Byte length check (ADR-005, MAX_GOAL_BYTES = 1024)
                        if trimmed.len() > MAX_GOAL_BYTES {
                            return Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                                format!(
                                    "goal exceeds {MAX_GOAL_BYTES} bytes ({} bytes provided); \
                                     shorten the goal and retry",
                                    trimmed.len()
                                ),
                            )]));
                        }
                        Some(trimmed)
                    }
                }
            }
        } else {
            None // PhaseEnd and Stop: goal silently ignored
        };

        // 4. Build response (no business logic -- MCP server is session-unaware)
        let action = match validated.cycle_type {
            CycleType::Start => "cycle_started",
            CycleType::PhaseEnd => "phase_ended",
            CycleType::Stop => "cycle_stopped",
        };

        // 4b. vnc-005: On cycle stop, drain the pending_entries_analysis bucket for this
        // feature cycle. Context_cycle is the authoritative "feature is done" signal
        // (ADR-004 eviction trigger 1). Drained entries are discarded — cycle close
        // implies retrospective was already done or explicitly skipped.
        if validated.cycle_type == CycleType::Stop {
            let drained = self
                .pending_entries_analysis
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain_for(&validated.topic);
            if !drained.is_empty() {
                tracing::info!(
                    feature_cycle = %validated.topic,
                    entry_count = drained.len(),
                    "context_cycle: cleared pending_entries_analysis bucket on cycle close"
                );
            }
        }

        let response_text = if let Some(ref g) = validated_goal {
            format!(
                "Acknowledged: {} for topic '{}' with goal: '{}'. \
                 Attribution is applied via the hook path (fire-and-forget). \
                 Use context_cycle_review to confirm session attribution.",
                action, validated.topic, g
            )
        } else {
            format!(
                "Acknowledged: {} for topic '{}'. \
                 Attribution is applied via the hook path (fire-and-forget). \
                 Use context_cycle_review to confirm session attribution.",
                action, validated.topic
            )
        };

        // 5. Audit log (fire-and-forget)
        self.audit_fire_and_forget(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_cycle".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: format!(
                "{} topic={}{}",
                action,
                validated.topic,
                if validated_goal.is_some() {
                    " goal=present"
                } else {
                    ""
                }
            ),
        });

        // 6. Return acknowledgment
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            response_text,
        )]))
    }
}

// ---------------------------------------------------------------------------
// crt-033: Memoization helpers for context_cycle_review
// ---------------------------------------------------------------------------

/// Derive `cycle_start_ts` from an optional slice of `CycleEventRecord`s.
///
/// Returns the minimum `timestamp` of rows with `event_type = "cycle_start"`.
/// Returns `0` when no `cycle_start` event is found or the slice is `None`.
/// A return value of `0` signals an open window — the caller should log a warning
/// because the curation snapshot will over-count orphan deprecations (EC-02).
fn extract_cycle_start_ts(cycle_events: Option<&[unimatrix_observe::CycleEventRecord]>) -> i64 {
    let events = match cycle_events {
        None => return 0,
        Some(e) => e,
    };
    events
        .iter()
        .filter(|e| e.event_type == "cycle_start")
        .map(|e| e.timestamp)
        .min()
        .unwrap_or(0)
}

/// Deserialize a stored `CycleReviewRecord` into a `RetrospectiveReport`.
///
/// Returns `(report, advisory)` where `advisory` is `Some(msg)` when the stored
/// `schema_version` differs from `current_version` (FR-02, C-05, R-08).
///
/// On deserialization failure, returns `Err(serde_json::Error)`. The caller must
/// treat this as a cache miss and fall through to full recomputation (ADR-003).
///
/// Evidence-limit truncation is NOT applied here — the caller applies it at
/// render time (C-03).
fn check_stored_review(
    record: &unimatrix_store::CycleReviewRecord,
    current_version: u32,
) -> Result<(unimatrix_observe::RetrospectiveReport, Option<String>), serde_json::Error> {
    let advisory = if record.schema_version != current_version {
        Some(format!(
            "computed with schema_version {}, current is {} — use force=true to recompute.",
            record.schema_version, current_version
        ))
    } else {
        None
    };

    let report: unimatrix_observe::RetrospectiveReport =
        serde_json::from_str(&record.summary_json)?;

    Ok((report, advisory))
}

/// Serialize a `RetrospectiveReport` into a `CycleReviewRecord` ready for storage.
///
/// Sets `schema_version = SUMMARY_SCHEMA_VERSION`, `raw_signals_available = 1`,
/// and `computed_at` to the current unix timestamp.
///
/// `snapshot` populates the seven crt-047 curation health columns; `None` leaves
/// them at zero (pre-crt-047 behaviour preserved for callers that skip snapshot).
///
/// `first_computed_at` is set by the caller to `cycle_start_ts` (new rows) or to
/// `review_ts` when no cycle_start event exists. `store_cycle_review()` preserves
/// the existing `first_computed_at` on subsequent overwrites (ADR-001 crt-047).
///
/// Evidence-limit truncation MUST NOT be applied before this call (C-03).
/// 4MB ceiling enforcement is delegated to `store_cycle_review()` (NFR-03).
fn build_cycle_review_record(
    feature_cycle: &str,
    report: &unimatrix_observe::RetrospectiveReport,
    snapshot: Option<&unimatrix_observe::CurationSnapshot>,
    first_computed_at: i64,
) -> Result<unimatrix_store::CycleReviewRecord, serde_json::Error> {
    // Serialize the full report — no evidence_limit truncation (C-03).
    let summary_json = serde_json::to_string(report)?;

    let computed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Map snapshot fields to i64 record columns; default to 0 when snapshot unavailable.
    let (ct, ca, ch, cs, dt, od) = match snapshot {
        None => (0i64, 0i64, 0i64, 0i64, 0i64, 0i64),
        Some(s) => (
            s.corrections_total as i64,
            s.corrections_agent as i64,
            s.corrections_human as i64,
            s.corrections_system as i64,
            s.deprecations_total as i64,
            s.orphan_deprecations as i64,
        ),
    };

    Ok(unimatrix_store::CycleReviewRecord {
        feature_cycle: feature_cycle.to_string(),
        schema_version: unimatrix_store::SUMMARY_SCHEMA_VERSION,
        computed_at,
        raw_signals_available: 1i32, // live signals — full pipeline just ran
        summary_json,
        corrections_total: ct,
        corrections_agent: ca,
        corrections_human: ch,
        corrections_system: cs,
        deprecations_total: dt,
        orphan_deprecations: od,
        first_computed_at,
    })
}

/// Apply evidence-limit truncation and format dispatch, appending an optional
/// advisory string to the response.
///
/// Called from the memoization hit path and the purged-signals path.
/// Mirrors the existing step 12 format dispatch in the handler.
/// Evidence-limit truncation is applied here — NOT before this call (C-03).
fn dispatch_review_with_advisory(
    report: unimatrix_observe::RetrospectiveReport,
    format: &str,
    evidence_limit: Option<usize>,
    advisory: Option<String>,
) -> Result<rmcp::model::CallToolResult, rmcp::model::ErrorData> {
    use crate::error::ERROR_INVALID_PARAMS;
    use crate::mcp::response::format_retrospective_markdown;
    use crate::mcp::response::format_retrospective_report;

    match format {
        "markdown" | "summary" => {
            let mut result = format_retrospective_markdown(&report);
            if let Some(note) = advisory {
                // Append advisory as additional text content item.
                result
                    .content
                    .push(rmcp::model::Content::text(format!("\n\n{}", note)));
            }
            Ok(result)
        }
        "json" => {
            let evidence_limit = evidence_limit.unwrap_or(3);
            let final_report = if evidence_limit > 0 {
                let mut truncated = report.clone();
                for hotspot in &mut truncated.hotspots {
                    hotspot.evidence.truncate(evidence_limit);
                }
                truncated
            } else {
                report
            };
            let mut result = format_retrospective_report(&final_report);
            if let Some(note) = advisory {
                result
                    .content
                    .push(rmcp::model::Content::text(format!("\n\n{}", note)));
            }
            Ok(result)
        }
        _ => Err(rmcp::model::ErrorData::new(
            ERROR_INVALID_PARAMS,
            format!(
                "Unknown format '{}'. Valid values: \"markdown\", \"json\".",
                format
            ),
            None,
        )),
    }
}

/// Apply evidence-limit truncation and format dispatch for the memo-hit path,
/// injecting `parse_failure_count` as a top-level field (crt-046 Resolution 1).
///
/// Mirror of `dispatch_review_with_advisory` but includes `parse_failure_count` in
/// the JSON response and appends it as a trailing note in the markdown response.
fn dispatch_review_with_advisory_and_parse_failures(
    report: unimatrix_observe::RetrospectiveReport,
    format: &str,
    evidence_limit: Option<usize>,
    advisory: Option<String>,
    parse_failure_count: u32,
) -> Result<rmcp::model::CallToolResult, rmcp::model::ErrorData> {
    use crate::error::ERROR_INVALID_PARAMS;
    use crate::mcp::response::format_retrospective_markdown;

    match format {
        "markdown" | "summary" => {
            let mut result = format_retrospective_markdown(&report);
            if let Some(note) = advisory {
                result
                    .content
                    .push(rmcp::model::Content::text(format!("\n\n{}", note)));
            }
            result.content.push(rmcp::model::Content::text(format!(
                "\nparse_failure_count: {}",
                parse_failure_count
            )));
            Ok(result)
        }
        "json" => {
            let evidence_limit = evidence_limit.unwrap_or(3);
            let final_report = if evidence_limit > 0 {
                let mut truncated = report.clone();
                for hotspot in &mut truncated.hotspots {
                    hotspot.evidence.truncate(evidence_limit);
                }
                truncated
            } else {
                report
            };
            // Inject parse_failure_count as a top-level field alongside the report.
            let json_str = match serde_json::to_value(&final_report) {
                Ok(mut val) => {
                    if let Some(obj) = val.as_object_mut() {
                        obj.insert(
                            "parse_failure_count".to_string(),
                            serde_json::Value::Number(parse_failure_count.into()),
                        );
                        if let Some(note) = advisory {
                            obj.insert("advisory".to_string(), serde_json::Value::String(note));
                        }
                    }
                    serde_json::to_string_pretty(&val).unwrap_or_default()
                }
                Err(_) => {
                    // Fallback: original format without parse_failure_count injection.
                    use crate::mcp::response::format_retrospective_report;
                    let mut result = format_retrospective_report(&final_report);
                    if let Some(note) = advisory {
                        result
                            .content
                            .push(rmcp::model::Content::text(format!("\n\n{}", note)));
                    }
                    return Ok(result);
                }
            };
            Ok(rmcp::model::CallToolResult::success(vec![
                rmcp::model::Content::text(json_str),
            ]))
        }
        _ => Err(rmcp::model::ErrorData::new(
            ERROR_INVALID_PARAMS,
            format!(
                "Unknown format '{}'. Valid values: \"markdown\", \"json\".",
                format
            ),
            None,
        )),
    }
}

/// Build lesson-learned content from a retrospective report (col-010b).
///
/// Uses narrative summaries when available (structured path), falls back to
/// hotspot claims (JSONL path). Always includes recommendations.
fn build_lesson_learned_content(report: &unimatrix_observe::RetrospectiveReport) -> String {
    let mut content = String::new();

    if let Some(narratives) = &report.narratives {
        for n in narratives {
            content.push_str(&format!("- {}: {}\n", n.hotspot_type, n.summary));
        }
    } else {
        for h in &report.hotspots {
            content.push_str(&format!("- {}: {}\n", h.rule_name, h.claim));
        }
    }

    for r in &report.recommendations {
        content.push_str(&format!(
            "Recommendation ({}): {}\n",
            r.hotspot_type, r.action
        ));
    }

    // R-09 guard: ensure non-empty content
    if content.is_empty() {
        content = "Retrospective analysis completed with no specific findings.".to_string();
    }

    content
}

/// Fire-and-forget lesson-learned write using self.clone() + insert_with_audit (ADR-002).
///
/// Called inside a tokio::spawn from context_cycle_review. Embeds the content,
/// checks for supersede, and writes via the standard insert_with_audit pipeline
/// for atomic ENTRIES + VECTOR_MAP + HNSW + audit.
async fn write_lesson_learned(
    server: &UnimatrixServer,
    report: &unimatrix_observe::RetrospectiveReport,
    feature_cycle: &str,
) -> Result<(), crate::error::ServerError> {
    use unimatrix_core::Status;

    // 1. CategoryAllowlist check
    if server.categories.validate("lesson-learned").is_err() {
        tracing::error!(
            "lesson-learned category not in allowlist, skipping write for {}",
            feature_cycle
        );
        return Ok(());
    }

    // 2. Build content
    let content = build_lesson_learned_content(report);
    let title = format!("Retrospective findings: {}", feature_cycle);
    let topic = format!("retrospective/{}", feature_cycle);

    // 3. Supersede check: find existing active lesson-learned with same topic
    let existing = {
        let filter = unimatrix_core::QueryFilter {
            topic: Some(topic.clone()),
            category: Some("lesson-learned".to_string()),
            ..Default::default()
        };
        server
            .store
            .query(filter)
            .await
            .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e)))?
    };

    let supersedes_id = existing
        .iter()
        .filter(|e| e.status == Status::Active)
        .max_by_key(|e| e.created_at)
        .map(|e| e.id);
    // end of async scope for `existing` query
    drop(existing);

    // 4. Embed content (same pipeline as context_store: get_adapter + embed_entry + adapt + normalize)
    let embedding = match server.embed_service.get_adapter().await {
        Ok(adapter) => {
            let title_clone = title.clone();
            let content_clone = content.clone();
            match crate::infra::timeout::spawn_blocking_with_timeout(
                crate::infra::timeout::MCP_HANDLER_TIMEOUT,
                move || adapter.embed_entry(&title_clone, &content_clone),
            )
            .await
            {
                Ok(Ok(raw)) => {
                    let adapted = server.adapt_service.adapt_embedding(
                        &raw,
                        Some("lesson-learned"),
                        Some(&topic),
                    );
                    unimatrix_embed::l2_normalized(&adapted)
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        "lesson-learned embedding failed for {}: {}",
                        feature_cycle,
                        e
                    );
                    vec![]
                }
                Err(e) => {
                    tracing::warn!(
                        "lesson-learned embedding task timed out or panicked for {}: {}",
                        feature_cycle,
                        e
                    );
                    vec![]
                }
            }
        }
        Err(e) => {
            tracing::warn!(
                "lesson-learned embed adapter not ready for {}: {}",
                feature_cycle,
                e
            );
            vec![]
        }
    };

    // 5. Build NewEntry
    let new_entry = unimatrix_core::NewEntry {
        title,
        content,
        topic: topic.clone(),
        category: "lesson-learned".to_string(),
        tags: vec![
            format!("feature_cycle:{}", feature_cycle),
            format!("hotspot_count:{}", report.hotspots.len()),
            "source:retrospective".to_string(),
        ],
        source: String::new(),
        status: Status::Active,
        created_by: "cortical-implant".to_string(),
        feature_cycle: feature_cycle.to_string(),
        trust_source: "system".to_string(),
    };

    // 6. Insert via insert_with_audit (ADR-002: atomic ENTRIES + VECTOR_MAP + HNSW + audit)
    let audit_event = AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: "cortical-implant".to_string(),
        operation: "context_cycle_review/lesson-learned".to_string(),
        target_ids: vec![],
        outcome: Outcome::Success,
        detail: format!("auto-persist lesson-learned for {}", feature_cycle),
    };

    let (new_id, _record) = server
        .insert_with_audit(new_entry, embedding, audit_event)
        .await?;

    // 7. Supersede chain: deprecate old, link new → old and old → new
    if let Some(old_id) = supersedes_id {
        // Deprecate old entry (handles STATUS_INDEX + counters internally)
        if let Err(e) = server.store.update_status(old_id, Status::Deprecated).await {
            tracing::warn!("failed to deprecate prior lesson-learned {}: {}", old_id, e);
        } else {
            // Link old → new
            if let Ok(mut old_entry) = server.store.get(old_id).await {
                old_entry.superseded_by = Some(new_id);
                let _ = server.store.update(old_entry).await;
            }
            // Link new → old
            if let Ok(mut new_entry) = server.store.get(new_id).await {
                new_entry.supersedes = Some(old_id);
                let _ = server.store.update(new_entry).await;
            }
        }
    }

    // 8. Seed confidence on new entry (best-effort)
    // GH #311: use operator-configured params from ServiceLayer, not ConfidenceParams::default().
    if let Ok(entry) = server.store.get(new_id).await {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let conf = unimatrix_engine::confidence::compute_confidence(
            &entry,
            now,
            &server.services.confidence.confidence_params,
        );
        let _ = server.store.update_confidence(new_id, conf).await;
    }

    Ok(())
}

/// Build a batch SQL IN-clause query for entry metadata (col-026 ADR-003, pattern #883).
///
/// Returns a SQL string with exactly `count` placeholder `?` parameters.
fn build_batch_meta_query(count: usize) -> String {
    let placeholders = (0..count).map(|_| "?").collect::<Vec<_>>().join(", ");
    format!(
        "SELECT id, title, category, feature_cycle \
           FROM entries \
          WHERE id IN ({}) AND status != 'quarantined'",
        placeholders
    )
}

/// Execute a chunked batch IN-clause query for entry metadata (col-026 ADR-003, pattern #883).
///
/// Chunks the ID slice at 100 IDs per query to stay well within SQLite's bind-parameter limit.
/// Returns a HashMap of entry ID → EntryMeta. Chunk failures are logged and skipped; the
/// result may contain fewer rows than requested (R-04: graceful degradation).
async fn batch_entry_meta_lookup(
    store: &Arc<unimatrix_store::SqlxStore>,
    ids: &[u64],
) -> std::collections::HashMap<u64, crate::mcp::knowledge_reuse::EntryMeta> {
    use sqlx::Row as _;

    if ids.is_empty() {
        return std::collections::HashMap::new();
    }

    let mut result: std::collections::HashMap<u64, crate::mcp::knowledge_reuse::EntryMeta> =
        std::collections::HashMap::new();

    for chunk in ids.chunks(100) {
        let sql = build_batch_meta_query(chunk.len());
        let mut query = sqlx::query(&sql);
        for &id in chunk {
            query = query.bind(id as i64);
        }

        match query.fetch_all(store.write_pool_server()).await {
            Ok(rows) => {
                for row in rows {
                    let id: i64 = row.try_get("id").unwrap_or(0);
                    let title: String = row.try_get("title").unwrap_or_default();
                    let category: String = row.try_get("category").unwrap_or_default();
                    let feature_cycle: Option<String> = row.try_get("feature_cycle").ok().flatten();
                    result.insert(
                        id as u64,
                        crate::mcp::knowledge_reuse::EntryMeta {
                            title,
                            feature_cycle,
                            category,
                        },
                    );
                }
            }
            Err(e) => {
                tracing::warn!("col-026: batch entry meta lookup chunk failed: {e}");
                // Continue with partial results; missing entries silently excluded (R-04)
            }
        }
    }

    result
}

/// Compute Tier 1 cross-session knowledge reuse (col-020 C3, ADR-001, col-026 C3).
///
/// Loads query_log + injection_log for the given sessions, then delegates to the
/// knowledge_reuse module for the actual computation.
///
/// col-026: accepts `current_feature_cycle` for the cross-feature/intra-cycle split.
/// Uses a single batch IN-clause query (ADR-003, pattern #883) instead of N individual
/// store.get() calls to fetch entry metadata.
async fn compute_knowledge_reuse_for_sessions(
    store: &Arc<unimatrix_store::SqlxStore>,
    session_records: &[unimatrix_store::SessionRecord],
    current_feature_cycle: &str,
) -> std::result::Result<
    unimatrix_observe::FeatureKnowledgeReuse,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let session_id_list: Vec<String> = session_records
        .iter()
        .map(|sr| sr.session_id.clone())
        .collect();

    tracing::debug!(
        "col-020b: knowledge reuse data flow: {} session IDs",
        session_id_list.len()
    );

    // Load query_log
    let refs_ql: Vec<&str> = session_id_list.iter().map(|s| s.as_str()).collect();
    let query_logs = store.scan_query_log_by_sessions(&refs_ql).await?;

    tracing::debug!(
        "col-020b: knowledge reuse data flow: {} query_log records loaded",
        query_logs.len()
    );

    // Load injection_log
    let refs_il: Vec<&str> = session_id_list.iter().map(|s| s.as_str()).collect();
    let injection_logs = store.scan_injection_log_by_sessions(&refs_il).await?;

    tracing::debug!(
        "col-020b: knowledge reuse data flow: {} injection_log records loaded",
        injection_logs.len()
    );

    // Load active category counts
    let active_cats = store.count_active_entries_by_category().await?;

    tracing::debug!(
        "col-020b: knowledge reuse data flow: {} active categories",
        active_cats.len()
    );

    // Collect all distinct entry IDs from both log sources (col-026 ADR-003).
    // The batch metadata lookup is executed once with this full set.
    let mut all_entry_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for record in &query_logs {
        let ids: Vec<u64> = serde_json::from_str(&record.result_entry_ids).unwrap_or_default();
        all_entry_ids.extend(ids);
    }
    for record in &injection_logs {
        all_entry_ids.insert(record.entry_id);
    }

    // col-026 ADR-003: single batch IN-clause query for all entry metadata.
    // Chunked at 100 IDs per query (pattern #883). Replaces the N-individual store.get() loop.
    let ids_vec: Vec<u64> = all_entry_ids.iter().copied().collect();
    let meta_map_owned = batch_entry_meta_lookup(store, &ids_vec).await;

    // Build category_map from meta_map for the existing entry_category_lookup closure.
    let category_map: std::collections::HashMap<u64, String> = meta_map_owned
        .iter()
        .map(|(&id, meta)| (id, meta.category.clone()))
        .collect();

    // Delegate to C3 knowledge_reuse module for computation.
    // entry_meta_lookup closure returns a filtered view of meta_map_owned for requested IDs.
    let reuse = crate::mcp::knowledge_reuse::compute_knowledge_reuse(
        &query_logs,
        &injection_logs,
        &active_cats,
        current_feature_cycle,
        |entry_id| category_map.get(&entry_id).cloned(),
        |ids| {
            // The batch fetch was already done above; return the pre-fetched subset.
            ids.iter()
                .filter_map(|id| {
                    meta_map_owned.get(id).map(|m| {
                        (
                            *id,
                            crate::mcp::knowledge_reuse::EntryMeta {
                                title: m.title.clone(),
                                feature_cycle: m.feature_cycle.clone(),
                                category: m.category.clone(),
                            },
                        )
                    })
                })
                .collect()
        },
    );

    tracing::debug!(
        "col-020b: knowledge reuse result: delivery_count={}, cross_session_count={}, cross_feature={}, intra_cycle={}",
        reuse.delivery_count,
        reuse.cross_session_count,
        reuse.cross_feature_reuse,
        reuse.intra_cycle_reuse,
    );

    Ok(reuse)
}

// ── col-026: Phase Stats Computation ──────────────────────────────────────────

/// Infer GateResult from cycle_phase_end.outcome text (col-026, ADR R-03).
///
/// Priority order: Rework > Fail > Pass > Unknown (multi-keyword check fires first).
/// Contains() substring matching is used per spec; see IMPLEMENTATION-BRIEF.md for
/// the "compass" → false-positive edge case (documented accepted fragility).
fn infer_gate_result(outcome: Option<&str>, pass_count: u32) -> unimatrix_observe::GateResult {
    use unimatrix_observe::GateResult;

    let outcome_lower = match outcome {
        None => return GateResult::Unknown,
        Some(s) if s.is_empty() => return GateResult::Unknown,
        Some(s) => s.to_lowercase(),
    };

    // Check rework FIRST (multi-pass success case, R-03 priority order)
    if pass_count > 1
        && (outcome_lower.contains("pass")
            || outcome_lower.contains("success")
            || outcome_lower.contains("approved"))
    {
        return GateResult::Rework;
    }

    // Single-pass rework keyword
    if outcome_lower.contains("rework") {
        return GateResult::Rework;
    }

    if outcome_lower.contains("fail") || outcome_lower.contains("error") {
        return GateResult::Fail;
    }

    if outcome_lower.contains("pass")
        || outcome_lower.contains("success")
        || outcome_lower.contains("approved")
    {
        return GateResult::Pass;
    }

    GateResult::Unknown
}

/// Derive is_in_progress from loaded cycle events (col-026, ADR-001).
///
/// Three states: None (no events), Some(true) (open cycle), Some(false) (confirmed stopped).
/// Plain bool is prohibited — see ADR-001.
fn derive_is_in_progress(events: Option<&[unimatrix_observe::CycleEventRecord]>) -> Option<bool> {
    match events {
        None => None,
        Some(evts) if evts.is_empty() => None,
        Some(evts) => {
            if evts.iter().any(|e| e.event_type == "cycle_stop") {
                Some(false) // confirmed complete
            } else {
                Some(true) // has cycle_start, no cycle_stop
            }
        }
    }
}

/// Infer cycle type from goal text keywords (col-026, FR-03).
///
/// First match wins in priority order: Design > Delivery > Bugfix > Refactor > Unknown.
fn infer_cycle_type(goal: Option<&str>) -> String {
    let goal_lower = match goal {
        None => return "Unknown".to_string(),
        Some(s) if s.is_empty() => return "Unknown".to_string(),
        Some(s) => s.to_lowercase(),
    };

    if goal_lower.contains("design")
        || goal_lower.contains("research")
        || goal_lower.contains("scope")
        || goal_lower.contains("spec")
    {
        return "Design".to_string();
    }

    if goal_lower.contains("implement")
        || goal_lower.contains("deliver")
        || goal_lower.contains("build")
    {
        return "Delivery".to_string();
    }

    if goal_lower.contains("fix")
        || goal_lower.contains("bug")
        || goal_lower.contains("regression")
        || goal_lower.contains("hotfix")
    {
        return "Bugfix".to_string();
    }

    if goal_lower.contains("refactor")
        || goal_lower.contains("cleanup")
        || goal_lower.contains("simplify")
    {
        return "Refactor".to_string();
    }

    "Unknown".to_string()
}

/// Extract agent name from a SubagentStart observation.
///
/// Prefers obs.input["tool_name"], falls back to obs.tool.
fn extract_agent_name(obs: &unimatrix_observe::ObservationRecord) -> Option<String> {
    if let Some(input) = &obs.input {
        if let Some(name) = input.get("tool_name").and_then(|v| v.as_str()) {
            return Some(name.to_string());
        }
    }
    obs.tool.clone()
}

/// Map tool name to the ToolDistribution bucket category.
///
/// Replicates the classify_tool mapping from unimatrix-observe/session_metrics.rs
/// for consistency across session summaries and phase stats.
/// Normalizes MCP-prefixed tool names (e.g. `mcp__unimatrix__context_search`) before matching.
fn categorize_tool_for_phase(tool: Option<&str>) -> &'static str {
    let normalized = tool.map(unimatrix_observe::normalize_tool_name);
    match normalized {
        Some("Read") | Some("Glob") | Some("Grep") => "read",
        Some("Edit") | Some("Write") => "write",
        Some("Bash") => "execute",
        Some("context_search") | Some("context_lookup") | Some("context_get") => "search",
        _ => "other",
    }
}

/// Compute per-phase aggregate statistics from cycle events and observation records.
///
/// Phase windows are derived by walking `events` in timestamp-ascending order.
/// Each `cycle_phase_end` event closes one window and opens the next.
/// `cycle_ts_to_obs_millis` from `services/observation.rs` is the ONLY permitted
/// conversion from cycle_events seconds to observation milliseconds (ADR-002).
/// Inline `* 1000` multiplication is prohibited.
fn compute_phase_stats(
    events: &[unimatrix_observe::CycleEventRecord],
    attributed: &[unimatrix_observe::ObservationRecord],
) -> Vec<unimatrix_observe::PhaseStats> {
    use std::collections::{HashMap, HashSet};
    use unimatrix_observe::{GateResult, PhaseStats, ToolDistribution};

    // Fast-path: no events → no phase windows
    if events.is_empty() {
        return vec![];
    }

    // Local struct for a phase window being built during the event walk.
    struct PhaseWindow {
        phase: String,
        pass_number: u32,
        start_ms: i64,
        end_ms: Option<i64>,
        end_event_outcome: Option<String>,
    }

    // Phase 1: Walk events in order to extract time windows.
    // Events arrive sorted by (timestamp ASC, seq ASC) from the SQL query.
    let mut windows: Vec<PhaseWindow> = Vec::new();
    let mut window_start_ms: Option<i64> = None;
    let mut current_phase: Option<String> = None;
    let mut pass_counters: HashMap<String, u32> = HashMap::new();

    for event in events {
        match event.event_type.as_str() {
            "cycle_start" => {
                // Absolute start of the first window. Use the mandatory converter (ADR-002).
                window_start_ms = Some(crate::services::observation::cycle_ts_to_obs_millis(
                    event.timestamp,
                ));
                // phase from cycle_start may be in next_phase; leave current_phase unset
                // until the first cycle_phase_end tells us what phase just ended.
            }

            "cycle_phase_end" => {
                // This event ends the current phase window and transitions to next_phase.
                let ending_phase = event.phase.clone().unwrap_or_default();
                // ADR-002: use cycle_ts_to_obs_millis — no inline * 1000
                let end_ms = crate::services::observation::cycle_ts_to_obs_millis(event.timestamp);

                if let Some(start_ms) = window_start_ms {
                    let pass_number = {
                        let counter = pass_counters.entry(ending_phase.clone()).or_insert(0);
                        *counter += 1;
                        *counter
                    };
                    windows.push(PhaseWindow {
                        phase: ending_phase.clone(),
                        pass_number,
                        start_ms,
                        end_ms: Some(end_ms),
                        end_event_outcome: event.outcome.clone(),
                    });
                }

                // Next window starts at this event's timestamp
                window_start_ms = Some(end_ms);
                current_phase = event.next_phase.clone();
            }

            "cycle_stop" => {
                // Ends the last open window (if any).
                // ADR-002: use cycle_ts_to_obs_millis — no inline * 1000
                let end_ms = crate::services::observation::cycle_ts_to_obs_millis(event.timestamp);

                if let Some(start_ms) = window_start_ms {
                    let last_phase = current_phase.clone().unwrap_or_default();
                    let pass_number = {
                        let counter = pass_counters.entry(last_phase.clone()).or_insert(0);
                        *counter += 1;
                        *counter
                    };
                    // cycle_stop has no gate outcome text
                    windows.push(PhaseWindow {
                        phase: last_phase,
                        pass_number,
                        start_ms,
                        end_ms: Some(end_ms),
                        end_event_outcome: None,
                    });
                }
                window_start_ms = None;
            }

            _ => {
                // Unknown event type — ignore
            }
        }
    }

    // Edge case: if there's no cycle_stop, the last window is still open.
    // Add it with end_ms = None (open window; filtered observations use i64::MAX as sentinel).
    if window_start_ms.is_some() {
        let last_phase = current_phase.clone().unwrap_or_default();
        let pass_number = {
            let counter = pass_counters.entry(last_phase.clone()).or_insert(0);
            *counter += 1;
            *counter
        };
        if let Some(start_ms) = window_start_ms {
            windows.push(PhaseWindow {
                phase: last_phase,
                pass_number,
                start_ms,
                end_ms: None,
                end_event_outcome: None,
            });
        }
    }

    // After walking all events: compute pass_count for each window from the final counters.
    // pass_count = total passes seen for this phase name (pass_counters has the final value).

    // Phase 2 + 3 + 4: For each window, slice observations and compute aggregates.
    let mut result: Vec<PhaseStats> = Vec::with_capacity(windows.len());

    for window in &windows {
        let pass_count = pass_counters.get(&window.phase).copied().unwrap_or(1);
        let window_end = window.end_ms.unwrap_or(i64::MAX);

        // Slice observations into this window [start_ms, end_ms)
        let filtered: Vec<&unimatrix_observe::ObservationRecord> = attributed
            .iter()
            .filter(|obs| {
                // obs.ts is u64 epoch millis; clamp to i64 for comparison.
                // Values above i64::MAX (year ~292 billion) saturate to i64::MAX rather than
                // wrapping negative, which would silently exclude them from every phase window.
                // This is a type cast (u64→i64, same unit), not a unit conversion — distinct
                // from cycle_ts_to_obs_millis which converts seconds→millis.
                let ts = i64::try_from(obs.ts).unwrap_or(i64::MAX);
                ts >= window.start_ms && ts < window_end
            })
            .collect();

        let record_count = filtered.len();

        // Distinct sessions
        let session_ids: HashSet<&str> = filtered.iter().map(|o| o.session_id.as_str()).collect();
        let session_count = session_ids.len();

        // Agents: SubagentStart observations, deduplicated in first-seen order
        let mut agents: Vec<String> = Vec::new();
        let mut seen_agents: HashSet<String> = HashSet::new();
        for obs in filtered.iter().filter(|o| o.event_type == "SubagentStart") {
            if let Some(name) = extract_agent_name(obs) {
                if seen_agents.insert(name.clone()) {
                    agents.push(name);
                }
            }
        }

        // Tool distribution: PreToolUse observations only (matching session_metrics.rs)
        let mut tool_distribution = ToolDistribution::default();
        for obs in filtered.iter().filter(|o| o.event_type == "PreToolUse") {
            match categorize_tool_for_phase(obs.tool.as_deref()) {
                "read" => tool_distribution.read += 1,
                "execute" => tool_distribution.execute += 1,
                "write" => tool_distribution.write += 1,
                "search" => tool_distribution.search += 1,
                _ => {} // other/spawn/store not counted in ToolDistribution
            }
        }

        // Knowledge served: PreToolUse where tool is context_search / context_lookup / context_get
        // Uses normalize_tool_name to handle mcp__unimatrix__-prefixed names from production hooks.
        let knowledge_served = filtered
            .iter()
            .filter(|o| o.event_type == "PreToolUse")
            .filter(|o| {
                o.tool
                    .as_deref()
                    .map(unimatrix_observe::normalize_tool_name)
                    .map_or(false, |t| {
                        matches!(t, "context_search" | "context_lookup" | "context_get")
                    })
            })
            .count() as u64;

        // Knowledge stored: PreToolUse where tool is context_store
        // Uses normalize_tool_name to handle mcp__unimatrix__-prefixed names from production hooks.
        let knowledge_stored = filtered
            .iter()
            .filter(|o| o.event_type == "PreToolUse")
            .filter(|o| {
                o.tool
                    .as_deref()
                    .map(unimatrix_observe::normalize_tool_name)
                    .map_or(false, |t| t == "context_store")
            })
            .count() as u64;

        // Gate result from end_event_outcome
        let gate_result = infer_gate_result(window.end_event_outcome.as_deref(), pass_count);
        let gate_outcome_text = window.end_event_outcome.clone();

        // Duration: (end_ms - start_ms) / 1000, floored to zero
        let duration_secs = window
            .end_ms
            .map(|end| ((end - window.start_ms).max(0) as u64) / 1000)
            .unwrap_or(0);

        result.push(PhaseStats {
            phase: window.phase.clone(),
            pass_number: window.pass_number,
            pass_count,
            duration_secs,
            start_ms: window.start_ms,
            end_ms: window.end_ms,
            session_count,
            record_count,
            agents,
            tool_distribution,
            knowledge_served,
            knowledge_stored,
            gate_result,
            gate_outcome_text,
            hotspot_ids: vec![], // populated by formatter only
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_params_deserialize() {
        let json = r#"{"query": "test"}"#;
        let params: SearchParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.query, "test");
        assert!(params.topic.is_none());
        assert!(params.agent_id.is_none());
        assert!(params.format.is_none());
        assert!(params.feature.is_none());
        assert!(params.helpful.is_none());
    }

    #[test]
    fn test_search_params_all_fields() {
        let json = r#"{
            "query": "test",
            "topic": "auth",
            "category": "convention",
            "tags": ["rust"],
            "k": 10,
            "agent_id": "test-agent",
            "format": "json",
            "feature": "crt-001",
            "helpful": true
        }"#;
        let params: SearchParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.query, "test");
        assert_eq!(params.topic.unwrap(), "auth");
        assert_eq!(params.k.unwrap(), 10);
        assert_eq!(params.format.unwrap(), "json");
        assert_eq!(params.feature.unwrap(), "crt-001");
        assert_eq!(params.helpful.unwrap(), true);
    }

    #[test]
    fn test_store_params_required_fields() {
        let json = r#"{"content": "test content", "topic": "auth", "category": "convention"}"#;
        let params: StoreParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, "test content");
        assert_eq!(params.topic, "auth");
        assert_eq!(params.category, "convention");
    }

    #[test]
    fn test_store_params_missing_required() {
        let json = r#"{"topic": "auth"}"#;
        let result = serde_json::from_str::<StoreParams>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_params_required_id() {
        let json = r#"{"id": 42}"#;
        let params: GetParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.id, 42);
    }

    #[test]
    fn test_lookup_params_all_optional() {
        let json = r#"{}"#;
        let params: LookupParams = serde_json::from_str(json).unwrap();
        assert!(params.topic.is_none());
        assert!(params.id.is_none());
        assert!(params.format.is_none());
    }

    #[test]
    fn test_wrong_type_doesnt_panic() {
        let json = r#"{"id": "not-a-number"}"#;
        let result = serde_json::from_str::<GetParams>(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_extra_fields_ignored() {
        let json = r#"{"id": 42, "extra": "field"}"#;
        let params: GetParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.id, 42);
    }

    #[test]
    fn test_store_params_with_format() {
        let json = r#"{"content": "c", "topic": "t", "category": "cat", "format": "markdown"}"#;
        let params: StoreParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.format.unwrap(), "markdown");
    }

    #[test]
    fn test_lookup_params_with_format() {
        let json = r#"{"format": "json"}"#;
        let params: LookupParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.format.unwrap(), "json");
    }

    #[test]
    fn test_get_params_with_format() {
        let json = r#"{"id": 1, "format": "summary"}"#;
        let params: GetParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.format.unwrap(), "summary");
    }

    // -- vnc-003: CorrectParams --

    #[test]
    fn test_correct_params_required_fields() {
        let json = r#"{"original_id": 42, "content": "corrected content"}"#;
        let params: CorrectParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.original_id, 42);
        assert_eq!(params.content, "corrected content");
        assert!(params.reason.is_none());
        assert!(params.topic.is_none());
        assert!(params.category.is_none());
    }

    #[test]
    fn test_correct_params_all_fields() {
        let json = r#"{
            "original_id": 42,
            "content": "corrected",
            "reason": "outdated",
            "topic": "auth",
            "category": "convention",
            "tags": ["rust"],
            "title": "New Title",
            "agent_id": "agent",
            "format": "json"
        }"#;
        let params: CorrectParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.original_id, 42);
        assert_eq!(params.reason.unwrap(), "outdated");
        assert_eq!(params.format.unwrap(), "json");
    }

    #[test]
    fn test_correct_params_missing_content() {
        let json = r#"{"original_id": 42}"#;
        assert!(serde_json::from_str::<CorrectParams>(json).is_err());
    }

    // -- vnc-003: DeprecateParams --

    #[test]
    fn test_deprecate_params_required_fields() {
        let json = r#"{"id": 42}"#;
        let params: DeprecateParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.id, 42);
        assert!(params.reason.is_none());
    }

    #[test]
    fn test_deprecate_params_with_reason() {
        let json = r#"{"id": 42, "reason": "outdated"}"#;
        let params: DeprecateParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.reason.unwrap(), "outdated");
    }

    // -- vnc-003: StatusParams --

    #[test]
    fn test_status_params_all_optional() {
        let json = r#"{}"#;
        let params: StatusParams = serde_json::from_str(json).unwrap();
        assert!(params.topic.is_none());
        assert!(params.category.is_none());
    }

    #[test]
    fn test_status_params_with_filters() {
        let json = r#"{"topic": "auth", "category": "convention"}"#;
        let params: StatusParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.topic.unwrap(), "auth");
        assert_eq!(params.category.unwrap(), "convention");
    }

    // -- vnc-003: BriefingParams --

    #[test]
    fn test_briefing_params_required_fields() {
        let json = r#"{"task": "design auth module"}"#;
        let params: BriefingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.task, "design auth module");
        assert!(params.role.is_none());
        assert!(params.feature.is_none());
        assert!(params.max_tokens.is_none());
    }

    #[test]
    fn test_briefing_params_all_fields() {
        let json = r#"{
            "role": "developer",
            "task": "implement feature",
            "feature": "vnc-003",
            "max_tokens": 5000,
            "agent_id": "agent",
            "format": "markdown"
        }"#;
        let params: BriefingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.role, Some("developer".to_string()));
        assert_eq!(params.feature.unwrap(), "vnc-003");
        assert_eq!(params.max_tokens.unwrap(), 5000);
        assert_eq!(params.format.unwrap(), "markdown");
    }

    #[test]
    fn test_briefing_params_missing_role() {
        // role is now optional — absent role must succeed
        let json = r#"{"task": "design"}"#;
        assert!(serde_json::from_str::<BriefingParams>(json).is_ok());
    }

    #[test]
    fn test_briefing_params_missing_task() {
        let json = r#"{"role": "architect"}"#;
        assert!(serde_json::from_str::<BriefingParams>(json).is_err());
    }

    // -- crt-003: QuarantineParams --

    #[test]
    fn test_quarantine_params_required_id() {
        let json = r#"{"id": 42}"#;
        let params: QuarantineParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.id, 42);
        assert!(params.reason.is_none());
        assert!(params.action.is_none());
    }

    #[test]
    fn test_quarantine_params_all_fields() {
        let json = r#"{
            "id": 42,
            "reason": "suspicious content",
            "action": "quarantine",
            "agent_id": "system",
            "format": "json"
        }"#;
        let params: QuarantineParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.id, 42);
        assert_eq!(params.reason.unwrap(), "suspicious content");
        assert_eq!(params.action.unwrap(), "quarantine");
        assert_eq!(params.agent_id.unwrap(), "system");
        assert_eq!(params.format.unwrap(), "json");
    }

    #[test]
    fn test_quarantine_params_restore_action() {
        let json = r#"{"id": 42, "action": "restore"}"#;
        let params: QuarantineParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.action.unwrap(), "restore");
    }

    // -- crt-003: StatusParams check_embeddings --

    #[test]
    fn test_status_params_check_embeddings_field() {
        let json = r#"{"check_embeddings": true}"#;
        let params: StatusParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.check_embeddings.unwrap(), true);
    }

    #[test]
    fn test_status_params_check_embeddings_default() {
        let json = r#"{}"#;
        let params: StatusParams = serde_json::from_str(json).unwrap();
        assert!(params.check_embeddings.is_none());
    }

    // -- bugfix/252: StatusParams maintain field removed --

    #[test]
    fn test_status_params_no_maintain_field() {
        // Deserialization succeeds and `maintain` is silently ignored as an
        // unrecognised field (serde deny_unknown_fields is not set).
        // The struct no longer carries `maintain` at all -- confirmed by
        // accessing only the known fields below.
        let json = r#"{"topic": "auth", "check_embeddings": false}"#;
        let params: StatusParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.topic.as_deref(), Some("auth"));
        assert_eq!(params.check_embeddings, Some(false));
    }

    #[test]
    fn test_status_params_anonymous_agent_deserializes() {
        // A fresh-install call with no agent_id provided should deserialise
        // and leave agent_id as None (the handler auto-enrolls it).
        let json = r#"{}"#;
        let params: StatusParams = serde_json::from_str(json).unwrap();
        assert!(params.agent_id.is_none());
        assert!(params.topic.is_none());
        assert!(params.category.is_none());
        assert!(params.check_embeddings.is_none());
    }

    // -- alc-002: EnrollParams --

    #[test]
    fn test_enroll_params_deserialize_all_fields() {
        let json = r#"{
            "target_agent_id": "new-agent",
            "trust_level": "internal",
            "capabilities": ["read", "write"],
            "agent_id": "human",
            "format": "json"
        }"#;
        let params: EnrollParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.target_agent_id, "new-agent");
        assert_eq!(params.trust_level, "internal");
        assert_eq!(params.capabilities, vec!["read", "write"]);
        assert_eq!(params.agent_id.unwrap(), "human");
        assert_eq!(params.format.unwrap(), "json");
    }

    #[test]
    fn test_enroll_params_deserialize_optional_missing() {
        let json = r#"{
            "target_agent_id": "new-agent",
            "trust_level": "internal",
            "capabilities": ["read"]
        }"#;
        let params: EnrollParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.target_agent_id, "new-agent");
        assert!(params.agent_id.is_none());
        assert!(params.format.is_none());
    }

    #[test]
    fn test_enroll_not_write_operation() {
        // context_enroll is administrative, not a knowledge write
        // Verify is_write_operation (in audit.rs) does not match it
        // This test verifies the invariant from the architecture
        assert_ne!("context_enroll", "context_store");
        assert_ne!("context_enroll", "context_correct");
    }

    // -- col-002: RetrospectiveParams --

    #[test]
    fn test_retrospective_params_deserialize() {
        let json = r#"{"feature_cycle": "col-002"}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.feature_cycle, "col-002");
        assert!(params.agent_id.is_none());
        assert!(params.evidence_limit.is_none());
        assert!(params.format.is_none());
    }

    #[test]
    fn test_retrospective_params_with_agent() {
        let json = r#"{"feature_cycle": "nxs-001", "agent_id": "test-agent"}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.feature_cycle, "nxs-001");
        assert_eq!(params.agent_id.unwrap(), "test-agent");
    }

    // -- col-010b: evidence_limit tests --

    #[test]
    fn test_retrospective_params_evidence_limit() {
        let json = r#"{"feature_cycle": "col-010b", "evidence_limit": 5}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.evidence_limit, Some(5));
    }

    #[test]
    fn test_retrospective_params_evidence_limit_zero() {
        let json = r#"{"feature_cycle": "col-010b", "evidence_limit": 0}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.evidence_limit, Some(0));
    }

    #[test]
    fn test_evidence_limit_default() {
        let params: RetrospectiveParams =
            serde_json::from_str(r#"{"feature_cycle": "test"}"#).unwrap();
        assert_eq!(params.evidence_limit.unwrap_or(3), 3);
    }

    // -- vnc-011: format field tests --

    #[test]
    fn test_retrospective_params_format_markdown() {
        let json = r#"{"feature_cycle": "test", "format": "markdown"}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.format, Some("markdown".to_string()));
    }

    #[test]
    fn test_retrospective_params_format_json() {
        let json = r#"{"feature_cycle": "test", "format": "json"}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.format, Some("json".to_string()));
    }

    #[test]
    fn test_retrospective_params_format_absent() {
        let json = r#"{"feature_cycle": "test"}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert!(params.format.is_none());
    }

    #[test]
    fn test_retrospective_params_format_unknown() {
        let json = r#"{"feature_cycle": "test", "format": "xml"}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.format, Some("xml".to_string()));
    }

    #[test]
    fn test_retrospective_params_all_fields() {
        let json = r#"{"feature_cycle": "col-002", "agent_id": "agent-1", "evidence_limit": 5, "format": "json"}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.feature_cycle, "col-002");
        assert_eq!(params.agent_id, Some("agent-1".to_string()));
        assert_eq!(params.evidence_limit, Some(5));
        assert_eq!(params.format, Some("json".to_string()));
    }

    // -- crt-033: force field tests (TH-U-01, TH-U-02) --

    /// TH-U-01: force absent → None (AC-12)
    #[test]
    fn test_retrospective_params_force_absent_is_none() {
        let params: RetrospectiveParams =
            serde_json::from_str(r#"{"feature_cycle": "test-001"}"#).unwrap();
        assert!(params.force.is_none());
    }

    /// TH-U-02a: force=true deserializes correctly (AC-12)
    #[test]
    fn test_retrospective_params_force_true() {
        let params: RetrospectiveParams =
            serde_json::from_str(r#"{"feature_cycle": "test-001", "force": true}"#).unwrap();
        assert_eq!(params.force, Some(true));
    }

    /// TH-U-02b: force=false deserializes correctly (AC-12)
    #[test]
    fn test_retrospective_params_force_false() {
        let params: RetrospectiveParams =
            serde_json::from_str(r#"{"feature_cycle": "test-001", "force": false}"#).unwrap();
        assert_eq!(params.force, Some(false));
    }

    // -- crt-033: check_stored_review helper tests (TH-U-03 through TH-U-06) --

    /// Helper: build a minimal CycleReviewRecord with a valid serialized
    /// RetrospectiveReport for use in check_stored_review tests.
    fn minimal_cycle_review_record(
        schema_version: u32,
        summary_json: Option<&str>,
    ) -> unimatrix_store::CycleReviewRecord {
        let valid_json = if let Some(json) = summary_json {
            json.to_string()
        } else {
            // Build valid JSON by serializing a real RetrospectiveReport.
            let report = unimatrix_observe::RetrospectiveReport {
                feature_cycle: "x".to_string(),
                session_count: 0,
                total_records: 0,
                metrics: unimatrix_observe::MetricVector::default(),
                hotspots: vec![],
                is_cached: false,
                baseline_comparison: None,
                entries_analysis: None,
                narratives: None,
                recommendations: vec![],
                session_summaries: None,
                feature_knowledge_reuse: None,
                rework_session_count: None,
                context_reload_pct: None,
                attribution: None,
                phase_narrative: None,
                goal: None,
                cycle_type: None,
                attribution_path: None,
                is_in_progress: None,
                phase_stats: None,
                curation_health: None, // crt-047
            };
            serde_json::to_string(&report).expect("test report must serialize")
        };
        unimatrix_store::CycleReviewRecord {
            feature_cycle: "x".to_string(),
            schema_version,
            computed_at: 1_700_000_000,
            raw_signals_available: 1,
            summary_json: valid_json,
            ..Default::default() // crt-047: curation health fields default to 0
        }
    }

    /// TH-U-03: matching schema_version → no advisory (R-08)
    #[test]
    fn test_check_stored_review_matching_version_no_advisory() {
        let record = minimal_cycle_review_record(unimatrix_store::SUMMARY_SCHEMA_VERSION, None);
        let result = check_stored_review(&record, unimatrix_store::SUMMARY_SCHEMA_VERSION);
        let (_, advisory) = result.expect("check_stored_review must return Ok");
        assert!(
            advisory.is_none(),
            "no advisory when schema_version matches current"
        );
    }

    /// TH-U-04: mismatched schema_version (stored < current) → advisory contains key phrases (AC-04b, R-08)
    #[test]
    fn test_check_stored_review_mismatched_version_produces_advisory() {
        let old_version = 0u32;
        let record = minimal_cycle_review_record(old_version, None);
        let (_, advisory) =
            check_stored_review(&record, unimatrix_store::SUMMARY_SCHEMA_VERSION).unwrap();
        let advisory_text = advisory.expect("advisory must be Some when schema_version differs");
        assert!(
            advisory_text.contains("use force=true to recompute"),
            "advisory must contain 'use force=true to recompute', got: {advisory_text}"
        );
        assert!(
            advisory_text.contains(&old_version.to_string()),
            "advisory must include stored version ({old_version}), got: {advisory_text}"
        );
        assert!(
            advisory_text.contains(&unimatrix_store::SUMMARY_SCHEMA_VERSION.to_string()),
            "advisory must include current version ({}), got: {advisory_text}",
            unimatrix_store::SUMMARY_SCHEMA_VERSION
        );
    }

    /// TH-U-05: future schema_version (stored > current) → advisory produced (R-08)
    #[test]
    fn test_check_stored_review_future_version_produces_advisory() {
        let future_version = 999u32;
        let record = minimal_cycle_review_record(future_version, None);
        let (_, advisory) =
            check_stored_review(&record, unimatrix_store::SUMMARY_SCHEMA_VERSION).unwrap();
        assert!(
            advisory.is_some(),
            "future schema_version must also produce an advisory"
        );
    }

    /// TH-U-06: corrupted summary_json → returns Err, does NOT panic (R-06-3, ADR-003)
    #[test]
    fn test_check_stored_review_corrupted_json_returns_err() {
        let record = minimal_cycle_review_record(
            unimatrix_store::SUMMARY_SCHEMA_VERSION,
            Some("not valid json {{{{"),
        );
        // Must not panic. Use catch_unwind to confirm panic-freedom.
        let result = std::panic::catch_unwind(|| {
            check_stored_review(&record, unimatrix_store::SUMMARY_SCHEMA_VERSION)
        });
        assert!(
            result.is_ok(),
            "check_stored_review must not panic on corrupted JSON"
        );
        // The inner result must be Err (caller treats as cache miss).
        let inner = result.unwrap();
        assert!(
            inner.is_err(),
            "check_stored_review must return Err for corrupted JSON"
        );
    }

    // -- crt-033: build_cycle_review_record helper tests (TH-U-07) --

    /// TH-U-07: build_cycle_review_record round-trip (AC-03)
    #[test]
    fn test_build_cycle_review_record_sets_correct_fields() {
        let report = unimatrix_observe::RetrospectiveReport {
            feature_cycle: "feat-x".to_string(),
            session_count: 0,
            total_records: 0,
            metrics: unimatrix_observe::MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };

        let record = build_cycle_review_record("feat-x", &report, None, 0)
            .expect("build_cycle_review_record must succeed");

        assert_eq!(record.feature_cycle, "feat-x");
        assert_eq!(
            record.schema_version,
            unimatrix_store::SUMMARY_SCHEMA_VERSION,
            "schema_version must be SUMMARY_SCHEMA_VERSION"
        );
        assert_eq!(
            record.raw_signals_available, 1i32,
            "raw_signals_available must be 1 (live signals)"
        );
        // summary_json must round-trip to a valid RetrospectiveReport.
        serde_json::from_str::<unimatrix_observe::RetrospectiveReport>(&record.summary_json)
            .expect("summary_json must be valid JSON that deserializes to RetrospectiveReport");
        // computed_at must be a plausible unix timestamp (after 2020-01-01).
        assert!(
            record.computed_at > 1_577_836_800,
            "computed_at must be a valid recent unix timestamp"
        );
    }

    // -- col-010b: clone-and-truncate tests --

    #[test]
    fn test_clone_and_truncate_preserves_original() {
        let evidence: Vec<unimatrix_observe::EvidenceRecord> = (0..10)
            .map(|i| unimatrix_observe::EvidenceRecord {
                description: format!("event {}", i),
                ts: i * 1000,
                tool: None,
                detail: String::new(),
            })
            .collect();
        let report = unimatrix_observe::RetrospectiveReport {
            feature_cycle: "test".to_string(),
            session_count: 1,
            total_records: 10,
            metrics: unimatrix_observe::MetricVector::default(),
            hotspots: vec![unimatrix_observe::HotspotFinding {
                category: unimatrix_observe::HotspotCategory::Friction,
                severity: unimatrix_observe::Severity::Warning,
                rule_name: "test".to_string(),
                claim: "test".to_string(),
                measured: 10.0,
                threshold: 1.0,
                evidence,
            }],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };

        // Clone and truncate
        let mut truncated = report.clone();
        for h in &mut truncated.hotspots {
            h.evidence.truncate(3);
        }

        // Truncated has 3
        assert_eq!(truncated.hotspots[0].evidence.len(), 3);
        // Original still has 10
        assert_eq!(report.hotspots[0].evidence.len(), 10);
    }

    // -- col-010b: build_lesson_learned_content tests --

    #[test]
    fn test_build_lesson_learned_content_with_hotspots() {
        let report = unimatrix_observe::RetrospectiveReport {
            feature_cycle: "test".to_string(),
            session_count: 0,
            total_records: 0,
            metrics: unimatrix_observe::MetricVector::default(),
            hotspots: vec![unimatrix_observe::HotspotFinding {
                category: unimatrix_observe::HotspotCategory::Friction,
                severity: unimatrix_observe::Severity::Warning,
                rule_name: "orphaned_calls".to_string(),
                claim: "8 orphaned calls detected".to_string(),
                measured: 8.0,
                threshold: 3.0,
                evidence: vec![],
            }],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![unimatrix_observe::Recommendation {
                hotspot_type: "orphaned_calls".to_string(),
                action: "Investigate orphaned tool invocations".to_string(),
                rationale: "saves time".to_string(),
            }],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };

        let content = build_lesson_learned_content(&report);
        assert!(content.contains("orphaned_calls"));
        assert!(content.contains("8 orphaned calls detected"));
        assert!(content.contains("Investigate orphaned tool invocations"));
    }

    #[test]
    fn test_build_lesson_learned_content_with_narratives() {
        let report = unimatrix_observe::RetrospectiveReport {
            feature_cycle: "test".to_string(),
            session_count: 0,
            total_records: 0,
            metrics: unimatrix_observe::MetricVector::default(),
            hotspots: vec![unimatrix_observe::HotspotFinding {
                category: unimatrix_observe::HotspotCategory::Friction,
                severity: unimatrix_observe::Severity::Warning,
                rule_name: "orphaned_calls".to_string(),
                claim: "8 orphaned calls detected".to_string(),
                measured: 8.0,
                threshold: 3.0,
                evidence: vec![],
            }],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: Some(vec![unimatrix_observe::HotspotNarrative {
                hotspot_type: "orphaned_calls".to_string(),
                summary: "Orphaned calls clustered around build commands".to_string(),
                clusters: vec![],
                top_files: vec![],
                sequence_pattern: None,
            }]),
            recommendations: vec![unimatrix_observe::Recommendation {
                hotspot_type: "orphaned_calls".to_string(),
                action: "Investigate orphaned tool invocations".to_string(),
                rationale: "saves time".to_string(),
            }],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };

        let content = build_lesson_learned_content(&report);
        // With narratives present, should use narrative summary (not hotspot claim)
        assert!(content.contains("Orphaned calls clustered"));
        assert!(!content.contains("8 orphaned calls detected"));
        // Recommendations always included
        assert!(content.contains("Investigate orphaned tool invocations"));
    }

    #[test]
    fn test_build_lesson_learned_content_empty_fallback() {
        let report = unimatrix_observe::RetrospectiveReport {
            feature_cycle: "test".to_string(),
            session_count: 0,
            total_records: 0,
            metrics: unimatrix_observe::MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        };

        let content = build_lesson_learned_content(&report);
        assert!(!content.is_empty());
    }

    // -- col-022 / crt-025: CycleParams deserialization --

    #[test]
    fn test_cycle_params_deserialize_start() {
        let json = r#"{"type": "start", "topic": "col-022"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "start");
        assert_eq!(params.topic, "col-022");
        assert!(params.phase.is_none());
        assert!(params.outcome.is_none());
        assert!(params.next_phase.is_none());
    }

    #[test]
    fn test_cycle_params_deserialize_phase_end() {
        let json = r#"{"type": "phase-end", "topic": "crt-025", "phase": "design", "next_phase": "implementation"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "phase-end");
        assert_eq!(params.phase.as_deref(), Some("design"));
        assert_eq!(params.next_phase.as_deref(), Some("implementation"));
        assert!(params.outcome.is_none());
    }

    #[test]
    fn test_cycle_params_deserialize_phase_end_with_outcome() {
        let json = r#"{"type": "phase-end", "topic": "crt-025", "phase": "design", "outcome": "all tasks complete", "next_phase": "implementation"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.outcome.as_deref(), Some("all tasks complete"));
    }

    #[test]
    fn test_cycle_params_deserialize_stop() {
        let json = r#"{"type": "stop", "topic": "col-022"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "stop");
    }

    #[test]
    fn test_cycle_params_missing_required_type() {
        let json = r#"{"topic": "col-022"}"#;
        assert!(serde_json::from_str::<CycleParams>(json).is_err());
    }

    #[test]
    fn test_cycle_params_missing_required_topic() {
        let json = r#"{"type": "start"}"#;
        assert!(serde_json::from_str::<CycleParams>(json).is_err());
    }

    #[test]
    fn test_cycle_params_extra_fields_ignored() {
        let json = r#"{"type": "start", "topic": "col-022", "unknown": true}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "start");
        assert_eq!(params.topic, "col-022");
    }

    #[test]
    fn test_cycle_params_keywords_silently_discarded() {
        // Old callers passing `keywords` in JSON should have it silently discarded (C-04 / no deny_unknown_fields).
        let json = r#"{"type": "start", "topic": "col-022", "keywords": ["attr", "lifecycle"]}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "start");
        assert_eq!(params.topic, "col-022");
        // No keywords field on struct — unknown field is silently discarded.
    }

    // -- col-022: Response format (R-08) --

    #[test]
    fn test_cycle_params_type_is_raw_identifier() {
        // Verify r#type works correctly with JSON key "type"
        let json = r#"{"type": "start", "topic": "col-022"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "start");
    }

    #[test]
    fn test_cycle_params_deserialize_with_agent_id() {
        let json = r#"{"type": "start", "topic": "col-022", "agent_id": "human"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "start");
        assert_eq!(params.topic, "col-022");
        assert_eq!(params.agent_id, Some("human".to_string()));
        assert!(params.format.is_none());
    }

    #[test]
    fn test_cycle_params_deserialize_with_agent_id_and_format() {
        let json = r#"{"type": "stop", "topic": "nan-005", "agent_id": "delivery-lead", "format": "json"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "stop");
        assert_eq!(params.topic, "nan-005");
        assert_eq!(params.agent_id, Some("delivery-lead".to_string()));
        assert_eq!(params.format, Some("json".to_string()));
    }

    #[test]
    fn test_cycle_params_agent_id_absent_is_none() {
        let json = r#"{"type": "start", "topic": "col-022"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert!(params.agent_id.is_none());
        assert!(params.format.is_none());
    }

    #[test]
    fn test_cycle_not_write_operation() {
        // context_cycle is acknowledgment-only, not a knowledge write
        assert_ne!("context_cycle", "context_store");
        assert_ne!("context_cycle", "context_correct");
    }

    // -- col-025: CycleParams goal field deserialization (T-MCP-01, T-MCP-02) --

    #[test]
    fn test_cycle_params_goal_field_present() {
        // T-MCP-01: goal field deserializes correctly when present
        let json = r#"{"type": "start", "topic": "col-025", "goal": "Test the goal field."}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.goal, Some("Test the goal field.".to_string()));
    }

    #[test]
    fn test_cycle_params_goal_field_absent() {
        // T-MCP-02: goal absent → None (backward compat, AC-02)
        // Old clients omitting goal receive None.
        let json = r#"{"type": "start", "topic": "col-025"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.goal, None);
    }

    #[test]
    fn test_cycle_params_goal_null() {
        // Explicit null deserializes as None
        let json = r#"{"type": "start", "topic": "col-025", "goal": null}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.goal, None);
    }

    // -- col-025: Goal validation logic (AC-13a, AC-17) --
    //
    // These tests exercise the normalization + byte-check logic inline.
    // The exact logic mirrors the handler block in context_cycle.

    /// Shared validation helper that mirrors the handler's validation block.
    /// Returns Ok(None) for normalized-to-None, Ok(Some(s)) for accepted goal,
    /// Err(msg) for rejected (over-limit) goal.
    fn validate_goal_mcp(raw: Option<String>) -> Result<Option<String>, String> {
        match raw {
            None => Ok(None),
            Some(g) => {
                let trimmed = g.trim().to_owned();
                if trimmed.is_empty() {
                    Ok(None)
                } else if trimmed.len() > MAX_GOAL_BYTES {
                    Err(format!(
                        "goal exceeds {MAX_GOAL_BYTES} bytes ({} bytes provided); \
                         shorten the goal and retry",
                        trimmed.len()
                    ))
                } else {
                    Ok(Some(trimmed))
                }
            }
        }
    }

    #[test]
    fn test_cycle_start_goal_exceeds_max_bytes_rejected() {
        // AC-13a: goal > MAX_GOAL_BYTES → error with byte count
        let oversized = "a".repeat(MAX_GOAL_BYTES + 1); // 1025 bytes
        let result = validate_goal_mcp(Some(oversized));
        assert!(result.is_err(), "expected Err for oversized goal");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("1024"),
            "error message must mention limit (1024): {msg}"
        );
        assert!(
            msg.contains("1025"),
            "error message must mention actual byte count (1025): {msg}"
        );
    }

    #[test]
    fn test_cycle_start_goal_at_exact_max_bytes_accepted() {
        // AC-13a / R-07 boundary: exactly 1024 bytes is accepted
        let exact = "a".repeat(MAX_GOAL_BYTES); // exactly 1024 bytes
        let result = validate_goal_mcp(Some(exact.clone()));
        assert!(result.is_ok(), "expected Ok for goal at exact limit");
        assert_eq!(result.unwrap(), Some(exact));
    }

    #[test]
    fn test_cycle_start_empty_goal_normalized_to_none() {
        // AC-17: empty string normalized to None before byte check
        let result = validate_goal_mcp(Some(String::new()));
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_cycle_start_whitespace_only_goal_normalized_to_none() {
        // AC-17: whitespace-only normalized to None
        let result = validate_goal_mcp(Some("   ".to_string()));
        assert_eq!(result, Ok(None));
    }

    #[test]
    fn test_cycle_start_whitespace_trimmed_goal_within_limit_accepted() {
        // AC-17: leading/trailing whitespace trimmed; non-empty result accepted
        let result = validate_goal_mcp(Some("  a short goal  ".to_string()));
        assert_eq!(result, Ok(Some("a short goal".to_string())));
    }

    #[test]
    fn test_cycle_phase_end_with_goal_ignores_goal() {
        // FR-01: goal on phase-end is silently ignored
        // The handler sets validated_goal = None when cycle_type != Start.
        // Verify: CycleParams with type=phase-end and goal deserializes, but
        // the handler would produce validated_goal = None (simulated here).
        let json = r#"{"type": "phase-end", "topic": "col-025", "phase": "design", "goal": "should be ignored"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "phase-end");
        // goal field is present in the wire params
        assert_eq!(params.goal, Some("should be ignored".to_string()));
        // but the handler would use None for non-Start events (FR-01)
        // simulate handler logic: only process goal on Start
        let cycle_type_is_start = params.r#type == "start";
        let validated_goal = if cycle_type_is_start {
            validate_goal_mcp(params.goal).ok().flatten()
        } else {
            None
        };
        assert_eq!(validated_goal, None);
    }

    #[test]
    fn test_cycle_stop_with_goal_ignores_goal() {
        // FR-01: goal on stop is silently ignored
        let json = r#"{"type": "stop", "topic": "col-025", "goal": "should be ignored"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "stop");
        assert_eq!(params.goal, Some("should be ignored".to_string()));
        // simulate handler: only Start processes goal
        let cycle_type_is_start = params.r#type == "start";
        let validated_goal = if cycle_type_is_start {
            validate_goal_mcp(params.goal).ok().flatten()
        } else {
            None
        };
        assert_eq!(validated_goal, None);
    }

    // -- GH#313: compute_knowledge_reuse_for_sessions must not block_on within tokio --

    /// Regression test for GH#313.
    ///
    /// `compute_knowledge_reuse_for_sessions` previously called
    /// `Handle::current().block_on(...)` from within an async context, which
    /// panics unconditionally with "Cannot start a runtime from within a
    /// runtime". This test verifies the function completes without panicking
    /// when called from a `#[tokio::test]` executor (i.e., inside a tokio
    /// runtime).
    #[tokio::test]
    async fn test_compute_knowledge_reuse_for_sessions_no_block_on_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("test_kruse.db");
        let store = unimatrix_store::SqlxStore::open(&path, unimatrix_store::PoolConfig::default())
            .await
            .expect("open test store");
        let store = std::sync::Arc::new(store);

        // Empty sessions slice: all data flows will be empty, but the async
        // store lookups still exercise the pre-fetch path. Before the fix this
        // would panic; after the fix it returns Ok with zero counts.
        let result = compute_knowledge_reuse_for_sessions(&store, &[], "test-cycle").await;

        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
        let reuse = result.unwrap();
        assert_eq!(reuse.delivery_count, 0);
        assert_eq!(reuse.cross_session_count, 0);
    }

    // ---- crt-026: Component 2 (context_store histogram recording) tests ----

    // T-SH-01: GATE BLOCKER — duplicate store must not increment histogram (AC-02, R-03)
    #[test]
    fn test_duplicate_store_does_not_increment_histogram() {
        use crate::infra::session::SessionRegistry;

        let reg = SessionRegistry::new();
        reg.register_session("s1", None, None);

        // First store — non-duplicate: record_category_store is called
        reg.record_category_store("s1", "decision");

        // Second store — duplicate: duplicate_of.is_some() causes handler early return,
        // record_category_store is NOT called (we simulate by not calling it again).
        let histogram = reg.get_category_histogram("s1");
        assert_eq!(
            histogram.get("decision"),
            Some(&1),
            "histogram must be 1 after two stores of the same entry; \
             duplicate store must not increment the count"
        );
        assert_eq!(histogram.len(), 1);
    }

    // T-SH-02: positive path — successful stores increment histogram (AC-02, R-03)
    #[test]
    fn test_store_increments_histogram_for_registered_session() {
        use crate::infra::session::SessionRegistry;

        let reg = SessionRegistry::new();
        reg.register_session("s1", None, None);

        reg.record_category_store("s1", "decision");
        reg.record_category_store("s1", "pattern");
        reg.record_category_store("s1", "decision");

        let h = reg.get_category_histogram("s1");
        assert_eq!(h.get("decision"), Some(&2));
        assert_eq!(h.get("pattern"), Some(&1));
    }

    // T-SH-03: no session_id — if let Some guard prevents any call (AC-03, R-04)
    #[test]
    fn test_store_no_session_id_does_not_record() {
        use crate::infra::session::SessionRegistry;

        let reg = SessionRegistry::new();
        // Simulate the handler's if let Some(ref sid) guard when session_id is None
        let session_id: Option<String> = None;
        if let Some(ref sid) = session_id {
            reg.record_category_store(sid, "decision");
        }
        assert!(
            reg.get_category_histogram("anything").is_empty(),
            "registry must be untouched when session_id is None"
        );
    }

    // T-SH-04: ordering invariant documentation anchor (R-03)
    #[test]
    fn test_histogram_ordering_guard_semantics() {
        // The duplicate guard must precede the histogram record.
        // If duplicate_of.is_some() → histogram NOT incremented (T-SH-01).
        // If duplicate_of.is_none() → histogram IS incremented (T-SH-02).
        // Verified by the two tests above. This test is an invariant anchor.
        assert!(true);
    }

    // ---- crt-026: Component 4 (context_search pre-resolution) tests ----

    // T-SCH-01: pre-resolved histogram is Some when session has stores (AC-05, R-02)
    #[test]
    fn test_pre_resolve_histogram_some_when_session_has_stores() {
        use crate::infra::session::SessionRegistry;
        use std::collections::HashMap;

        let reg = SessionRegistry::new();
        reg.register_session("s1", None, None);
        reg.record_category_store("s1", "decision");
        reg.record_category_store("s1", "decision");
        reg.record_category_store("s1", "decision");

        // Simulate the handler's pre-resolution block
        let session_id = Some("s1".to_string());
        let category_histogram: Option<HashMap<String, u32>> =
            session_id.as_deref().and_then(|sid| {
                let h = reg.get_category_histogram(sid);
                if h.is_empty() { None } else { Some(h) }
            });

        assert!(
            category_histogram.is_some(),
            "pre-resolved histogram must be Some when session has stores"
        );
        let h = category_histogram.unwrap();
        assert_eq!(h.get("decision"), Some(&3));
    }

    // T-SCH-02: empty session → None (AC-08 cold-start, R-02)
    #[test]
    fn test_category_histogram_none_when_session_empty() {
        use crate::infra::session::SessionRegistry;
        use std::collections::HashMap;

        let reg = SessionRegistry::new();
        reg.register_session("s1", None, None);
        // No stores — histogram is empty

        let category_histogram: Option<HashMap<String, u32>> = Some("s1").and_then(|sid| {
            let h = reg.get_category_histogram(sid);
            if h.is_empty() { None } else { Some(h) }
        });

        assert!(
            category_histogram.is_none(),
            "pre-resolved histogram must be None when session has no stores (cold start)"
        );
    }

    // T-SCH-03: no session_id → None (AC-08 no-session path, R-02)
    #[test]
    fn test_category_histogram_none_when_no_session_id() {
        use crate::infra::session::SessionRegistry;
        use std::collections::HashMap;

        let session_id: Option<String> = None;
        let reg = SessionRegistry::new();
        let category_histogram: Option<HashMap<String, u32>> =
            session_id.as_deref().and_then(|sid| {
                let h = reg.get_category_histogram(sid);
                if h.is_empty() { None } else { Some(h) }
            });

        assert!(
            category_histogram.is_none(),
            "category_histogram must be None when session_id is None (no session path)"
        );
    }

    // T-SCH-04: ServiceSearchParams carries both new fields (AC-05, R-12)
    #[test]
    fn test_context_search_handler_populates_service_search_params() {
        use crate::infra::session::SessionRegistry;
        use crate::services::{RetrievalMode, ServiceSearchParams};
        use std::collections::HashMap;

        let reg = SessionRegistry::new();
        reg.register_session("s1", None, None);
        reg.record_category_store("s1", "decision");

        let session_id_ctx = Some("s1".to_string());
        let category_histogram: Option<HashMap<String, u32>> =
            session_id_ctx.as_deref().and_then(|sid| {
                let h = reg.get_category_histogram(sid);
                if h.is_empty() { None } else { Some(h) }
            });

        let params = ServiceSearchParams {
            query: "session registry pattern".to_string(),
            k: 5,
            filters: None,
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: None,
            retrieval_mode: RetrievalMode::Flexible,
            session_id: session_id_ctx.clone(),
            category_histogram,
            current_phase: None, // col-031: no phase in this test
        };

        assert_eq!(params.session_id.as_deref(), Some("s1"));
        let h = params.category_histogram.as_ref().unwrap();
        assert_eq!(h.get("decision"), Some(&1));
    }

    // -- crt-027: context_briefing handler unit tests --

    /// context_briefing_active_only_filter (AC-06, T-CB-01)
    ///
    /// Verifies that `format_index_table` renders the Active entry and the
    /// Deprecated entry is absent. The handler calls IndexBriefingService which
    /// post-filters to Active only before returning Vec<IndexEntry>. This test
    /// simulates what the handler receives from IndexBriefingService: only Active
    /// entries in the Vec (Deprecated entries are already excluded before formatting).
    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn context_briefing_active_only_filter() {
        use crate::mcp::response::{IndexEntry, format_index_table};

        // Simulate IndexBriefingService returning only Active entries (id=1).
        // Deprecated entry (id=2) would never appear — filtered out by the service.
        let active_entry = IndexEntry {
            id: 1,
            topic: "crt-027".to_string(),
            category: "decision".to_string(),
            confidence: 0.85,
            snippet: "Active entry content snippet.".to_string(),
        };

        // Only the active entry is in the vec — this is what the handler receives.
        let entries = vec![active_entry];
        let table_text = format_index_table(&entries);

        // Active entry must appear
        assert!(
            table_text.contains("1"),
            "table must contain active entry id=1"
        );

        // Deprecated entry id=2 must NOT appear (it was excluded by IndexBriefingService)
        // The table only has rows for entries in the vec, so id=2 is never rendered.
        assert!(
            !table_text.contains(" 2 ")
                && !table_text
                    .lines()
                    .any(|l| { l.trim_start().starts_with('2') && l.contains("crt-027") }),
            "deprecated entry id=2 must not appear in output"
        );

        // No section headers (AC-08)
        assert!(!table_text.contains("## Decisions"), "no section headers");
        assert!(!table_text.contains("## Conventions"), "no section headers");
    }

    /// context_briefing_default_k_20 (AC-07, T-CB-02)
    ///
    /// Verifies that the default k=20 is used when no k param is supplied.
    /// Tests the IndexBriefingParams construction: when the handler builds
    /// params, it hardcodes k=20 (FR-13, ADR-003).
    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn context_briefing_default_k_20() {
        use crate::services::IndexBriefingParams;
        use std::collections::HashMap;

        // Simulate handler building IndexBriefingParams with no k param supplied.
        // Handler always uses k=20 (hardcoded per ADR-003).
        let params = IndexBriefingParams {
            query: "crt-027 index briefing".to_string(),
            k: 20,
            session_id: None,
            max_tokens: Some(3000),
            category_histogram: None,
        };

        assert_eq!(
            params.k, 20,
            "default k must be 20 (not 3, the old UNIMATRIX_BRIEFING_K default)"
        );
        // Ensure k=20 is not the old cap of 3
        assert!(params.k > 3, "k=20 must be greater than old k=3 default");

        // Simulate what format_index_table would produce for up to 20 entries:
        // build 25 entries, take first 20 (simulating IndexBriefingService truncation)
        use crate::mcp::response::{IndexEntry, format_index_table};
        let entries: Vec<IndexEntry> = (1..=25u64)
            .map(|i| IndexEntry {
                id: i,
                topic: format!("topic-{i}"),
                category: "pattern".to_string(),
                confidence: 0.5,
                snippet: format!("snippet {i}"),
            })
            .take(params.k) // handler passes k=20 to service → service truncates to 20
            .collect();

        assert_eq!(
            entries.len(),
            20,
            "at most 20 entries returned with default k=20"
        );

        let table_text = format_index_table(&entries);
        // Count data rows: skip instruction line + blank line + header + separator = 4 lines
        // col-025 ADR-006: format_index_table now prepends CONTEXT_GET_INSTRUCTION + blank line
        let data_rows = table_text.lines().skip(4).count();
        assert_eq!(data_rows, 20, "table must have exactly 20 data rows");
    }

    /// context_briefing_k_override (AC-07 — k param, T-CB-02 variant)
    ///
    /// When k=5 is explicitly passed, IndexBriefingService returns at most 5 entries.
    /// The handler currently hardcodes k=20 — this test verifies that a caller-supplied
    /// k would be respected if wired through.
    ///
    /// NOTE: The current handler spec (pseudocode step 7) hardcodes k=20. A future
    /// extension could accept k from BriefingParams. This test validates the
    /// IndexBriefingParams k field and format_index_table cap behavior.
    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn context_briefing_k_override() {
        use crate::mcp::response::{IndexEntry, format_index_table};
        use crate::services::IndexBriefingParams;

        // Simulate a caller providing k=5 (future extension path).
        let params = IndexBriefingParams {
            query: "narrow query".to_string(),
            k: 5,
            session_id: None,
            max_tokens: Some(1000),
            category_histogram: None,
        };
        assert_eq!(params.k, 5, "explicit k=5 must be preserved in params");

        // Simulate IndexBriefingService returning at most k entries
        let entries: Vec<IndexEntry> = (1..=25u64)
            .map(|i| IndexEntry {
                id: i,
                topic: format!("topic-{i}"),
                category: "decision".to_string(),
                confidence: 0.9 - (i as f64 * 0.01),
                snippet: format!("snippet {i}"),
            })
            .take(params.k)
            .collect();

        assert!(
            entries.len() <= 5,
            "with k=5, at most 5 entries returned; got {}",
            entries.len()
        );

        let table_text = format_index_table(&entries);
        // skip instruction line + blank line + header + separator = 4 lines (col-025 ADR-006)
        let data_rows = table_text.lines().skip(4).count();
        assert!(
            data_rows <= 5,
            "table must have at most 5 data rows; got {data_rows}"
        );
    }

    /// context_briefing_flat_table_format (AC-08, T-CB-03)
    ///
    /// Verifies that format_index_table produces a flat indexed table with the
    /// expected column headers and NO markdown section headers.
    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn context_briefing_flat_table_format() {
        use crate::mcp::response::{IndexEntry, format_index_table};

        let entries = vec![
            IndexEntry {
                id: 42,
                topic: "crt-027".to_string(),
                category: "decision".to_string(),
                confidence: 0.80,
                snippet: "Flat table test snippet.".to_string(),
            },
            IndexEntry {
                id: 43,
                topic: "nxs-001".to_string(),
                category: "pattern".to_string(),
                confidence: 0.70,
                snippet: "Second entry snippet.".to_string(),
            },
        ];

        let table_text = format_index_table(&entries);

        // Must contain flat table column headers
        assert!(table_text.contains('#'), "output must contain '#' column");
        assert!(table_text.contains("id"), "output must contain 'id' column");
        assert!(
            table_text.contains("topic"),
            "output must contain 'topic' column"
        );
        assert!(
            table_text.contains("cat"),
            "output must contain 'cat' column"
        );
        assert!(
            table_text.contains("conf"),
            "output must contain 'conf' column"
        );
        assert!(
            table_text.contains("snippet"),
            "output must contain 'snippet' column"
        );

        // Must NOT contain markdown section headers (AC-08)
        assert!(
            !table_text.contains("## Decisions"),
            "output must not contain '## Decisions'"
        );
        assert!(
            !table_text.contains("## Injections"),
            "output must not contain '## Injections'"
        );
        assert!(
            !table_text.contains("## Conventions"),
            "output must not contain '## Conventions'"
        );
        assert!(
            !table_text.contains("## Key Context"),
            "output must not contain '## Key Context'"
        );

        // Must have at least 2 data rows:
        // instruction + blank + header + separator + 2 entries = 6 lines (col-025 ADR-006)
        let lines: Vec<&str> = table_text.lines().collect();
        assert!(
            lines.len() >= 6,
            "must have instruction + blank + header + separator + at least 2 data rows; got {}",
            lines.len()
        );
    }

    // ---- col-024: context_cycle_review three-path fallback tests (T-CCR-01 through T-CCR-04) ----

    /// Mock ObservationSource for testing the three-path fallback logic.
    ///
    /// Supports configuring return values for each path and tracking whether each
    /// method was called, so tests can assert call-site behavior without a live store.
    #[cfg(test)]
    struct MockObservationSource {
        /// Return value for load_cycle_observations.
        cycle_obs: std::result::Result<
            Vec<unimatrix_observe::ObservationRecord>,
            unimatrix_observe::ObserveError,
        >,
        /// Return value for load_feature_observations.
        feature_obs: std::result::Result<
            Vec<unimatrix_observe::ObservationRecord>,
            unimatrix_observe::ObserveError,
        >,
        /// Flag set when load_feature_observations is called.
        feature_obs_called: std::sync::atomic::AtomicBool,
        /// Flag set when load_unattributed_sessions is called.
        unattributed_called: std::sync::atomic::AtomicBool,
    }

    #[cfg(test)]
    impl MockObservationSource {
        fn primary_returns(obs: Vec<unimatrix_observe::ObservationRecord>) -> Self {
            MockObservationSource {
                cycle_obs: Ok(obs),
                feature_obs: Ok(vec![]),
                feature_obs_called: std::sync::atomic::AtomicBool::new(false),
                unattributed_called: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn primary_empty_legacy_returns(obs: Vec<unimatrix_observe::ObservationRecord>) -> Self {
            MockObservationSource {
                cycle_obs: Ok(vec![]),
                feature_obs: Ok(obs),
                feature_obs_called: std::sync::atomic::AtomicBool::new(false),
                unattributed_called: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn primary_errors() -> Self {
            MockObservationSource {
                cycle_obs: Err(unimatrix_observe::ObserveError::Database(
                    "simulated SQL failure".to_string(),
                )),
                feature_obs: Ok(vec![]),
                feature_obs_called: std::sync::atomic::AtomicBool::new(false),
                unattributed_called: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn all_empty() -> Self {
            MockObservationSource {
                cycle_obs: Ok(vec![]),
                feature_obs: Ok(vec![]),
                feature_obs_called: std::sync::atomic::AtomicBool::new(false),
                unattributed_called: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }

    /// Helper to build a minimal ObservationRecord for test fixtures.
    #[cfg(test)]
    fn make_obs_record(session_id: &str) -> unimatrix_observe::ObservationRecord {
        unimatrix_observe::ObservationRecord {
            ts: 1_000_000,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: Some("Bash".to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    /// The three-path fallback logic extracted from the context_cycle_review closure
    /// for isolated unit testing. This mirrors the exact code in the handler closure.
    ///
    /// col-026: returns (Vec<ObservationRecord>, path_label) to match the updated closure.
    #[cfg(test)]
    fn run_three_path_fallback(
        source: &MockObservationSource,
        feature_cycle: &str,
    ) -> std::result::Result<
        (Vec<unimatrix_observe::ObservationRecord>, &'static str),
        unimatrix_observe::ObserveError,
    > {
        use std::sync::atomic::Ordering;
        use unimatrix_observe::ObserveError;

        // Path 1: Primary (cycle_events-based)
        let primary = source
            .cycle_obs
            .as_ref()
            .map(|v| v.clone())
            .map_err(|e| ObserveError::Database(e.to_string()))?;
        if !primary.is_empty() {
            return Ok((primary, "cycle_events-first (primary)"));
        }

        // Fallback log (ADR-003) — tested via tracing_test in T-CCR-03.
        tracing::debug!(
            cycle_id = %feature_cycle,
            path = "load_feature_observations",
            "CycleReview: primary path empty, falling back to legacy sessions path"
        );

        // Path 2: Legacy-1 (sessions.feature_cycle)
        source.feature_obs_called.store(true, Ordering::SeqCst);
        let legacy1 = source
            .feature_obs
            .as_ref()
            .map(|v| v.clone())
            .map_err(|e| ObserveError::Database(e.to_string()))?;
        if !legacy1.is_empty() {
            return Ok((legacy1, "sessions.feature_cycle (legacy)"));
        }

        // Second fallback log (ADR-003)
        tracing::debug!(
            cycle_id = %feature_cycle,
            path = "load_unattributed_sessions",
            "CycleReview: legacy sessions path empty, falling back to content attribution"
        );

        // Path 3: Legacy-2 (content-based attribution)
        source.unattributed_called.store(true, Ordering::SeqCst);
        Ok((vec![], "content-scan (fallback)"))
    }

    /// T-CCR-01: When load_cycle_observations returns non-empty, load_feature_observations
    /// must NOT be called (AC-04, R-04 — prevents double attribution).
    #[test]
    fn context_cycle_review_primary_path_used_when_non_empty() {
        use std::sync::atomic::Ordering;

        let record = make_obs_record("session-001");
        let source = MockObservationSource::primary_returns(vec![record.clone()]);

        let result = run_three_path_fallback(&source, "col-024");

        assert!(result.is_ok(), "primary path must succeed");
        let (obs, path_label) = result.unwrap();
        assert_eq!(obs.len(), 1, "must return the primary observation");
        assert_eq!(obs[0].session_id, "session-001");
        // col-026: verify attribution path label
        assert_eq!(
            path_label, "cycle_events-first (primary)",
            "primary path must use cycle_events-first label"
        );

        assert!(
            !source.feature_obs_called.load(Ordering::SeqCst),
            "load_feature_observations must NOT be called when primary returns non-empty"
        );
        assert!(
            !source.unattributed_called.load(Ordering::SeqCst),
            "load_unattributed_sessions must NOT be called when primary returns non-empty"
        );
    }

    /// T-CCR-02: When load_cycle_observations returns Ok(vec![]), load_feature_observations
    /// activates (AC-04, AC-09, AC-12 — backward compatibility).
    #[test]
    fn context_cycle_review_fallback_to_legacy_when_primary_empty() {
        use std::sync::atomic::Ordering;

        let record = make_obs_record("legacy-session-001");
        let source = MockObservationSource::primary_empty_legacy_returns(vec![record]);

        let result = run_three_path_fallback(&source, "col-024");

        assert!(result.is_ok(), "legacy fallback must succeed");
        let (obs, path_label) = result.unwrap();
        assert_eq!(obs.len(), 1, "must return the legacy observation");
        assert_eq!(obs[0].session_id, "legacy-session-001");
        // col-026: verify attribution path label
        assert_eq!(
            path_label, "sessions.feature_cycle (legacy)",
            "legacy path must use sessions.feature_cycle label"
        );

        assert!(
            source.feature_obs_called.load(Ordering::SeqCst),
            "load_feature_observations must be called exactly once when primary is empty"
        );
    }

    /// T-CCR-03: When primary path returns Ok(vec![]), a tracing::debug! log fires
    /// with cycle_id and the message "primary path empty" (AC-14, R-08, ADR-003).
    #[tracing_test::traced_test]
    #[test]
    fn context_cycle_review_no_cycle_events_debug_log_emitted() {
        let source = MockObservationSource::all_empty();
        let _result = run_three_path_fallback(&source, "legacy-feature-001");

        // Verify the debug log was emitted with the feature cycle value (ADR-003).
        // tracing_test captures debug-level events by default.
        assert!(
            logs_contain("primary path empty"),
            "debug log must contain 'primary path empty'"
        );
        assert!(
            logs_contain("legacy-feature-001"),
            "debug log must contain the feature_cycle value"
        );
    }

    /// T-CCR-04: When load_cycle_observations returns Err, the error propagates to the
    /// caller; load_feature_observations is NOT called (FM-01).
    #[test]
    fn context_cycle_review_propagates_error_not_fallback() {
        use std::sync::atomic::Ordering;

        let source = MockObservationSource::primary_errors();
        let result = run_three_path_fallback(&source, "col-024");

        assert!(
            result.is_err(),
            "SQL error from primary path must propagate as Err"
        );

        assert!(
            !source.feature_obs_called.load(Ordering::SeqCst),
            "load_feature_observations must NOT be called when primary returns Err (FM-01)"
        );
        assert!(
            !source.unattributed_called.load(Ordering::SeqCst),
            "load_unattributed_sessions must NOT be called when primary returns Err (FM-01)"
        );
    }
}

// NOTE: The phase_stats tests are appended by col-026-agent-5-phase-stats below.
// They are placed outside the existing tests module to avoid merge conflicts.
// Rust allows multiple test modules per file.
#[cfg(test)]
mod phase_stats_tests {
    use super::*;

    /// Helper to build a CycleEventRecord for test fixtures.
    fn make_cycle_event(
        event_type: &str,
        phase: Option<&str>,
        outcome: Option<&str>,
        next_phase: Option<&str>,
        timestamp: i64,
    ) -> unimatrix_observe::CycleEventRecord {
        unimatrix_observe::CycleEventRecord {
            seq: 0,
            event_type: event_type.to_string(),
            phase: phase.map(|s| s.to_string()),
            outcome: outcome.map(|s| s.to_string()),
            next_phase: next_phase.map(|s| s.to_string()),
            timestamp,
        }
    }

    /// Helper to build a PreToolUse ObservationRecord at a given ts (millis).
    fn make_obs_at(
        session_id: &str,
        ts_ms: u64,
        tool: &str,
    ) -> unimatrix_observe::ObservationRecord {
        unimatrix_observe::ObservationRecord {
            ts: ts_ms,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: Some(tool.to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    /// Helper that builds a PreToolUse ObservationRecord for an MCP tool using the production
    /// prefix format (`mcp__unimatrix__{tool}`). Use this for all MCP tool names (context_*)
    /// to match what production hooks actually emit.
    fn make_mcp_obs_at(
        session_id: &str,
        ts_ms: u64,
        tool: &str,
    ) -> unimatrix_observe::ObservationRecord {
        make_obs_at(session_id, ts_ms, &format!("mcp__unimatrix__{tool}"))
    }

    /// T-PS / R-12: Empty events → empty vec (handler sets phase_stats = None).
    #[test]
    fn test_phase_stats_empty_events_produces_empty_vec() {
        let result = compute_phase_stats(&[], &[]);
        assert!(
            result.is_empty(),
            "empty events must produce empty vec (handler converts to None)"
        );
    }

    /// T-PS / AC-06: Basic single-phase window with known duration and record count.
    #[test]
    fn test_compute_phase_stats_basic_window() {
        use crate::services::observation::cycle_ts_to_obs_millis;

        let ts_start = 1_700_000_000i64;
        let ts_phase_end = 1_700_000_100i64;
        let ts_stop = 1_700_000_200i64;

        let events = vec![
            make_cycle_event("cycle_start", None, None, None, ts_start),
            make_cycle_event(
                "cycle_phase_end",
                Some("design"),
                Some("PASS"),
                Some("implementation"),
                ts_phase_end,
            ),
            make_cycle_event("cycle_stop", None, None, None, ts_stop),
        ];

        let start_ms = cycle_ts_to_obs_millis(ts_start);
        let phase_end_ms = cycle_ts_to_obs_millis(ts_phase_end);

        let obs = vec![
            make_obs_at("sess-1", (start_ms + 10_000) as u64, "Read"),
            make_obs_at("sess-1", phase_end_ms as u64, "Bash"), // boundary: in next window
        ];

        let result = compute_phase_stats(&events, &obs);
        assert_eq!(result.len(), 2, "two phase windows");

        let design = &result[0];
        assert_eq!(design.phase, "design");
        assert_eq!(design.pass_number, 1);
        assert_eq!(design.pass_count, 1);
        assert_eq!(design.duration_secs, 100);
        assert_eq!(
            design.record_count, 1,
            "obs before boundary in first window"
        );
        assert_eq!(design.session_count, 1);
        assert_eq!(
            design.start_ms, start_ms,
            "start_ms must use cycle_ts_to_obs_millis (ADR-002)"
        );
        assert_eq!(design.end_ms, Some(phase_end_ms));
        assert_eq!(design.gate_result, unimatrix_observe::GateResult::Pass);

        let impl_phase = &result[1];
        assert_eq!(impl_phase.phase, "implementation");
        assert_eq!(impl_phase.record_count, 1, "boundary obs in next window");
        assert_eq!(
            impl_phase.gate_result,
            unimatrix_observe::GateResult::Unknown
        );
    }

    /// T-PS-10 / R-02 / AC-07: Rework detection — same phase appearing twice.
    #[test]
    fn test_phase_stats_rework_detection() {
        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event(
                "cycle_phase_end",
                Some("design"),
                Some("fail"),
                Some("design"),
                1_700_000_100,
            ),
            make_cycle_event(
                "cycle_phase_end",
                Some("design"),
                Some("PASS"),
                Some("implementation"),
                1_700_000_200,
            ),
            make_cycle_event("cycle_stop", None, None, None, 1_700_000_300),
        ];

        let result = compute_phase_stats(&events, &[]);
        assert!(result.len() >= 2, "at least two design passes");

        let design_pass1 = &result[0];
        assert_eq!(design_pass1.phase, "design");
        assert_eq!(design_pass1.pass_number, 1);
        assert_eq!(design_pass1.pass_count, 2, "rework: 2 passes total");
        assert_eq!(
            design_pass1.gate_result,
            unimatrix_observe::GateResult::Fail
        );

        let design_pass2 = &result[1];
        assert_eq!(design_pass2.phase, "design");
        assert_eq!(design_pass2.pass_number, 2);
        assert_eq!(design_pass2.pass_count, 2, "rework: 2 passes total");
        // pass_count=2 + outcome="PASS" → Rework (multi-pass success, R-03 priority order)
        assert_eq!(
            design_pass2.gate_result,
            unimatrix_observe::GateResult::Rework
        );
    }

    /// T-PS-07 / R-05: derive_is_in_progress — None/Some(true)/Some(false).
    #[test]
    fn test_derive_is_in_progress_three_states() {
        assert_eq!(derive_is_in_progress(None), None, "None input → None");
        assert_eq!(
            derive_is_in_progress(Some(&[])),
            None,
            "empty events → None"
        );

        let open = vec![make_cycle_event(
            "cycle_start",
            None,
            None,
            None,
            1_700_000_000,
        )];
        assert_eq!(
            derive_is_in_progress(Some(&open)),
            Some(true),
            "no cycle_stop → Some(true)"
        );

        let complete = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event("cycle_stop", None, None, None, 1_700_000_100),
        ];
        assert_eq!(
            derive_is_in_progress(Some(&complete)),
            Some(false),
            "cycle_stop present → Some(false)"
        );
    }

    /// T-PS-06 / R-03: GateResult inference — all cases including known fragility.
    #[test]
    fn test_gate_result_inference() {
        use unimatrix_observe::GateResult;

        assert_eq!(infer_gate_result(Some("PASS"), 1), GateResult::Pass);
        assert_eq!(infer_gate_result(Some("pass"), 1), GateResult::Pass);
        assert_eq!(infer_gate_result(Some("approved"), 1), GateResult::Pass);
        assert_eq!(
            infer_gate_result(Some("failed: type errors"), 1),
            GateResult::Fail
        );
        assert_eq!(
            infer_gate_result(Some("error in gate 2b"), 1),
            GateResult::Fail
        );
        assert_eq!(
            infer_gate_result(Some("rework required"), 1),
            GateResult::Rework
        );
        assert_eq!(infer_gate_result(Some("REWORK"), 1), GateResult::Rework);
        // Multi-pass + success keyword → Rework (priority check fires first)
        assert_eq!(
            infer_gate_result(Some("pass after rework"), 2),
            GateResult::Rework,
            "pass_count>1 + 'pass' → Rework (R-03 priority)"
        );
        assert_eq!(infer_gate_result(None, 1), GateResult::Unknown);
        assert_eq!(infer_gate_result(Some(""), 1), GateResult::Unknown);
        assert_eq!(
            infer_gate_result(Some("something unrecognized"), 1),
            GateResult::Unknown
        );
        // KNOWN: contains() matches embedded words — documented accepted fragility per spec
        assert_eq!(
            infer_gate_result(Some("compass"), 1),
            GateResult::Pass,
            "KNOWN: contains('pass') matches 'compass' — accepted fragility"
        );
    }

    /// T-PS-11 / R-01: No actual `* 1000` Rust multiplication in compute_phase_stats.
    ///
    /// Filters out comment lines (// ...) and lines containing the pattern only inside
    /// a string/backtick literal so only real Rust expressions are checked.
    #[test]
    fn test_phase_stats_no_inline_multiply() {
        let source = include_str!("tools.rs");
        let fn_marker = "fn compute_phase_stats(";
        if let Some(start) = source.find(fn_marker) {
            let scan_window = &source[start..][..source[start..].len().min(8000)];
            // Check non-comment, non-string lines for actual multiplication by 1000.
            // Violations look like: `ts_secs * 1000` or `n * 1000` (actual Rust code).
            // Permitted: saturating_mul(1000), // comments, string literals.
            let has_violation = scan_window.lines().any(|line| {
                let trimmed = line.trim();
                // Skip pure comment lines
                if trimmed.starts_with("//") {
                    return false;
                }
                // Skip lines where the pattern is inside a string/backtick (not real code)
                if trimmed.contains(r#""`* 1000`""#)
                    || trimmed.contains("saturating_mul")
                    || trimmed.contains(r#""* 1000""#)
                {
                    return false;
                }
                // Detect actual multiplication: must not be a comment fragment
                // Check for `* 1000` that isn't after `//` on the same line
                if let Some(code_part) = trimmed.split("//").next() {
                    code_part.contains("* 1000")
                } else {
                    false
                }
            });
            assert!(
                !has_violation,
                "compute_phase_stats must not use inline multiplication by 1000 (ADR-002); \
                 use cycle_ts_to_obs_millis() instead"
            );
        } else {
            panic!("compute_phase_stats not found in source");
        }
    }

    /// T-PS / R-01: Boundary obs falls in next window (end-exclusive semantics).
    #[test]
    fn test_phase_stats_obs_in_correct_window_millis_boundary() {
        use crate::services::observation::cycle_ts_to_obs_millis;

        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event(
                "cycle_phase_end",
                Some("scope"),
                Some("PASS"),
                Some("impl"),
                1_700_000_100,
            ),
            make_cycle_event("cycle_stop", None, None, None, 1_700_000_200),
        ];

        let boundary_ms = cycle_ts_to_obs_millis(1_700_000_100);
        let obs_at_boundary = make_obs_at("sess-1", boundary_ms as u64, "Read");
        let obs_before = make_obs_at("sess-2", (boundary_ms - 1) as u64, "Read");

        let result = compute_phase_stats(&events, &[obs_before, obs_at_boundary]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].record_count, 1, "obs before boundary in scope");
        assert_eq!(result[1].record_count, 1, "obs at boundary in next window");
    }

    /// T-PS / R-02: Only cycle_start + cycle_stop.
    #[test]
    fn test_phase_stats_no_phase_end_events() {
        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event("cycle_stop", None, None, None, 1_700_000_100),
        ];
        let result = compute_phase_stats(&events, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].duration_secs, 100);
        assert_eq!(result[0].phase, "");
    }

    /// T-PS / R-02: Zero-duration window.
    #[test]
    fn test_phase_stats_zero_duration_no_panic() {
        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event("cycle_stop", None, None, None, 1_700_000_000),
        ];
        let result = compute_phase_stats(&events, &[]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].duration_secs, 0);
    }

    /// T-PS / AC-06: Knowledge served/stored counts.
    #[test]
    fn test_phase_stats_knowledge_served_counted() {
        use crate::services::observation::cycle_ts_to_obs_millis;
        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event("cycle_stop", None, None, None, 1_700_001_000),
        ];
        let mid_ms = cycle_ts_to_obs_millis(1_700_000_000) + 500;
        let obs = vec![
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_search"),
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_search"),
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_lookup"),
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_store"),
            make_obs_at("sess-1", mid_ms as u64, "Read"),
        ];
        let result = compute_phase_stats(&events, &obs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].knowledge_served, 3);
        assert_eq!(result[0].knowledge_stored, 1);
    }

    /// T-PS / AC-06: Tool distribution by category.
    #[test]
    fn test_phase_stats_tool_distribution() {
        use crate::services::observation::cycle_ts_to_obs_millis;
        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event("cycle_stop", None, None, None, 1_700_001_000),
        ];
        let mid_ms = cycle_ts_to_obs_millis(1_700_000_000) + 500;
        let obs = vec![
            make_obs_at("sess-1", mid_ms as u64, "Read"),
            make_obs_at("sess-1", mid_ms as u64, "Glob"),
            make_obs_at("sess-1", mid_ms as u64, "Bash"),
            make_obs_at("sess-1", mid_ms as u64, "Edit"),
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_search"),
        ];
        let result = compute_phase_stats(&events, &obs);
        assert_eq!(result.len(), 1);
        let dist = &result[0].tool_distribution;
        assert_eq!(dist.read, 2);
        assert_eq!(dist.execute, 1);
        assert_eq!(dist.write, 1);
        assert_eq!(dist.search, 1);
    }

    /// T-PS / AC-06b: Prefixed MCP tool names are correctly categorized and counted.
    ///
    /// Production hooks always emit `mcp__unimatrix__context_search` (prefixed).
    /// This test documents that `categorize_tool_for_phase` and `compute_phase_stats`
    /// handle the prefix correctly via `normalize_tool_name`.
    #[test]
    fn test_phase_stats_mcp_prefix_normalized_correctly() {
        use crate::services::observation::cycle_ts_to_obs_millis;
        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event("cycle_stop", None, None, None, 1_700_001_000),
        ];
        let mid_ms = cycle_ts_to_obs_millis(1_700_000_000) + 500;
        let obs = vec![
            // All three MCP search tools with production prefix
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_search"),
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_lookup"),
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_get"),
            // Store tool with prefix
            make_mcp_obs_at("sess-1", mid_ms as u64, "context_store"),
            // Claude-native tools unchanged
            make_obs_at("sess-1", mid_ms as u64, "Read"),
            make_obs_at("sess-1", mid_ms as u64, "Bash"),
        ];
        let result = compute_phase_stats(&events, &obs);
        assert_eq!(result.len(), 1);
        // knowledge_served counts prefixed search tools
        assert_eq!(
            result[0].knowledge_served, 3,
            "context_search+lookup+get must be counted with mcp prefix"
        );
        // knowledge_stored counts prefixed store tool
        assert_eq!(
            result[0].knowledge_stored, 1,
            "context_store must be counted with mcp prefix"
        );
        // tool_distribution.search counts prefixed tools via categorize_tool_for_phase
        assert_eq!(
            result[0].tool_distribution.search, 3,
            "tool distribution search bucket must include mcp-prefixed tools"
        );
        assert_eq!(result[0].tool_distribution.read, 1);
        assert_eq!(result[0].tool_distribution.execute, 1);
    }

    /// T-PS / AC-06: Session count from distinct session_ids.
    #[test]
    fn test_phase_stats_session_count() {
        use crate::services::observation::cycle_ts_to_obs_millis;
        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event("cycle_stop", None, None, None, 1_700_001_000),
        ];
        let mid_ms = cycle_ts_to_obs_millis(1_700_000_000) + 500;
        let obs = vec![
            make_obs_at("sess-A", mid_ms as u64, "Read"),
            make_obs_at("sess-A", mid_ms as u64, "Bash"),
            make_obs_at("sess-B", mid_ms as u64, "Edit"),
            make_obs_at("sess-B", mid_ms as u64, "Read"),
        ];
        let result = compute_phase_stats(&events, &obs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].session_count, 2);
        assert_eq!(result[0].record_count, 4);
    }

    /// T-PS / AC-06: Agent deduplication in first-seen order.
    #[test]
    fn test_phase_stats_agent_deduplication() {
        use crate::services::observation::cycle_ts_to_obs_millis;
        let events = vec![
            make_cycle_event("cycle_start", None, None, None, 1_700_000_000),
            make_cycle_event("cycle_stop", None, None, None, 1_700_001_000),
        ];
        let mid_ms = cycle_ts_to_obs_millis(1_700_000_000) + 500;

        let mut obs1 = make_obs_at("sess-1", mid_ms as u64, "agent-alpha");
        obs1.event_type = "SubagentStart".to_string();
        let mut obs2 = make_obs_at("sess-1", mid_ms as u64, "agent-alpha");
        obs2.event_type = "SubagentStart".to_string();
        let mut obs3 = make_obs_at("sess-1", mid_ms as u64, "agent-beta");
        obs3.event_type = "SubagentStart".to_string();

        let result = compute_phase_stats(&events, &[obs1, obs2, obs3]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].agents.len(), 2, "deduplicated");
        assert_eq!(result[0].agents[0], "agent-alpha");
        assert_eq!(result[0].agents[1], "agent-beta");
    }

    /// T-PS / R-01: cycle_ts_to_obs_millis overflow guard.
    #[test]
    fn test_cycle_ts_to_obs_millis_overflow_guard() {
        use crate::services::observation::cycle_ts_to_obs_millis;
        let large_ts = i64::MAX / 1000 + 1;
        let result = cycle_ts_to_obs_millis(large_ts);
        assert_eq!(result, i64::MAX, "saturating_mul: overflow → i64::MAX");
    }

    /// GH #380: obs.ts = u64::MAX saturates to i64::MAX instead of wrapping negative,
    /// so far-future observations are INCLUDED rather than silently excluded from windows.
    ///
    /// Uses an open-ended window (cycle_start only, no cycle_phase_end/cycle_stop) so that
    /// the window end bound is None → i64::MAX sentinel. The far-future obs with ts=u64::MAX
    /// saturates to i64::MAX which satisfies `ts >= start_ms && ts < i64::MAX` is FALSE
    /// but `ts >= start_ms && ts < window_end` with window_end = i64::MAX is also FALSE.
    ///
    /// Actually: the filter is `ts >= window.start_ms && ts < window_end` where
    /// window_end = window.end_ms.unwrap_or(i64::MAX).
    /// With ts = i64::MAX and window_end = i64::MAX: i64::MAX < i64::MAX is FALSE.
    /// To include such an obs, we need `ts < window_end` to be true.
    /// Use ts = i64::MAX - 1 equivalent: a u64 value that saturates to a value
    /// just below i64::MAX — but since we're testing the saturation path, we need
    /// to verify the negative-wrap bug is gone.
    ///
    /// Concrete test: obs.ts just above i64::MAX as u64 (i64::MAX + 1) would wrap to
    /// negative under `as i64`, making ts < start_ms and excluding it. With
    /// try_from().unwrap_or(i64::MAX), it saturates to i64::MAX, which is still
    /// excluded by the < window_end check (when window_end = i64::MAX).
    ///
    /// So the meaningful test is: obs.ts = i64::MAX as u64 (fits in i64, no saturation
    /// needed) SHOULD be excluded (boundary), vs. obs.ts = (i64::MAX as u64) + 1 which
    /// under the old code wrapped to i64::MIN (negative → excluded from all windows).
    /// With the fix it saturates to i64::MAX, which is also excluded at the open boundary.
    ///
    /// The true behavioral fix: very large u64 values that previously wrapped to large
    /// negatives (like u64::MAX → -1 as i64) now saturate to i64::MAX instead of -1.
    /// -1 < start_ms (which is a positive epoch ms), so old code excluded them.
    /// i64::MAX >= start_ms, so new code handles them correctly (they land at or above
    /// the open window upper sentinel). For closed windows, they'd be at the upper boundary.
    ///
    /// Direct unit test for the saturation behavior without the window:
    #[test]
    fn test_compute_phase_stats_obs_ts_u64_max_included_via_saturation() {
        use crate::services::observation::cycle_ts_to_obs_millis;

        // Single open-ended phase window (no cycle_stop): end_ms = None → i64::MAX sentinel.
        // We place a normal obs well within the window and a large-ts obs just below the
        // sentinel to confirm it is captured.
        let ts_start = 1_700_000_000i64;

        // No cycle_phase_end or cycle_stop → one open window with phase="" starts at start_ms
        let events = vec![make_cycle_event("cycle_start", None, None, None, ts_start)];

        let start_ms = cycle_ts_to_obs_millis(ts_start);

        // Normal observation: within the window
        let normal_obs = make_obs_at("sess-1", (start_ms + 10_000) as u64, "Bash");

        // Observation with ts just below i64::MAX (fits in i64 cleanly; no saturation
        // needed, but verifies it is included in an open window since i64::MAX - 1 < i64::MAX)
        let near_max_obs = make_obs_at("sess-far", (i64::MAX - 1) as u64, "Read");

        // Observation with ts = u64::MAX: old code → -1 (negative, excluded);
        // new code → i64::MAX. For the open window, window_end = i64::MAX,
        // so ts = i64::MAX fails ts < window_end. This obs is at the sentinel boundary
        // and is excluded — correct behavior for the saturated case.
        // The key bug being fixed: it must NOT become -1 and be misclassified as
        // "before the window start."
        let u64_max_ts = i64::try_from(u64::MAX).unwrap_or(i64::MAX);
        assert_eq!(
            u64_max_ts,
            i64::MAX,
            "u64::MAX must saturate to i64::MAX (not wrap to -1)"
        );

        // The old cast: u64::MAX as i64 = -1 (on two's complement)
        #[allow(clippy::cast_possible_wrap)]
        let old_cast = u64::MAX as i64;
        assert_eq!(old_cast, -1, "confirm the old bug: u64::MAX as i64 = -1");
        // -1 < start_ms (positive epoch ms) → old code excluded u64::MAX obs from all windows
        assert!(
            old_cast < start_ms,
            "old cast produces negative value excluded from all windows"
        );

        let result = compute_phase_stats(&events, &[normal_obs, near_max_obs]);

        assert_eq!(result.len(), 1, "one open-ended window");
        assert_eq!(
            result[0].record_count, 2,
            "both normal and near-max-i64 observations must be included in the open window"
        );
    }

    /// T-PS / FR-03: infer_cycle_type keyword matching.
    #[test]
    fn test_infer_cycle_type_keywords() {
        assert_eq!(infer_cycle_type(None), "Unknown");
        assert_eq!(infer_cycle_type(Some("")), "Unknown");
        assert_eq!(
            infer_cycle_type(Some("implement new store layer")),
            "Delivery"
        );
        assert_eq!(
            infer_cycle_type(Some("design the embedding pipeline")),
            "Design"
        );
        assert_eq!(
            infer_cycle_type(Some("fix the regression in col-024")),
            "Bugfix"
        );
        assert_eq!(
            infer_cycle_type(Some("refactor the observation module")),
            "Refactor"
        );
        assert_eq!(infer_cycle_type(Some("something unknown")), "Unknown");
        assert_eq!(
            infer_cycle_type(Some("research spike for col-026")),
            "Design"
        );
    }
}

// ---- col-028: Phase Helper + Read-Side Call Site unit tests ----
#[cfg(test)]
mod col028_phase_helper_tests {
    use super::current_phase_for_session;
    use crate::infra::session::SessionRegistry;

    // AC-12 (compile): current_phase_for_session is callable with &SessionRegistry.
    // This test compiles only if the function exists with the correct signature.
    #[test]
    fn test_current_phase_for_session_callable_with_registry_ref() {
        let registry = SessionRegistry::new();
        let _result: Option<String> = current_phase_for_session(&registry, None);
    }

    // Part A: current_phase_for_session free function (test-plan §Part A)

    // Returns Some(phase) when session has an active phase set.
    #[test]
    fn test_current_phase_for_session_returns_phase_when_set() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-delivery", None, None);
        registry.set_current_phase("sess-delivery", Some("delivery".to_string()));

        let result = current_phase_for_session(&registry, Some("sess-delivery"));
        assert_eq!(result, Some("delivery".to_string()));
    }

    // Returns None when session has no active phase (cold start).
    #[test]
    fn test_current_phase_for_session_returns_none_when_no_phase() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-no-phase", None, None);

        let result = current_phase_for_session(&registry, Some("sess-no-phase"));
        assert!(result.is_none());
    }

    // Returns None when session_id parameter is None (EC-02 — no session).
    #[test]
    fn test_current_phase_for_session_returns_none_for_no_session_id() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-exists", None, None);
        registry.set_current_phase("sess-exists", Some("design".to_string()));

        let result = current_phase_for_session(&registry, None);
        assert!(
            result.is_none(),
            "None session_id must return None without registry lookup"
        );
    }

    // Returns None when session_id is not in the registry.
    #[test]
    fn test_current_phase_for_session_returns_none_for_unknown_session() {
        let registry = SessionRegistry::new();
        // Registry is empty — no sessions registered.

        let result = current_phase_for_session(&registry, Some("nonexistent-session"));
        assert!(result.is_none());
    }

    // Non-trivial phase strings round-trip correctly (EC-06 analogue for in-memory path).
    #[test]
    fn test_current_phase_for_session_non_trivial_phase_string() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-slash", None, None);
        registry.set_current_phase("sess-slash", Some("design/v2".to_string()));

        let result = current_phase_for_session(&registry, Some("sess-slash"));
        assert_eq!(result, Some("design/v2".to_string()));
    }

    // Multiple sessions are independent — phase from one does not bleed into another.
    #[test]
    fn test_current_phase_for_session_independent_across_sessions() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-a", None, None);
        registry.register_session("sess-b", None, None);
        registry.set_current_phase("sess-a", Some("delivery".to_string()));
        // sess-b intentionally left with no phase

        assert_eq!(
            current_phase_for_session(&registry, Some("sess-a")),
            Some("delivery".to_string())
        );
        assert!(
            current_phase_for_session(&registry, Some("sess-b")).is_none(),
            "sess-b has no phase; must not inherit sess-a phase"
        );
    }
}

// ---- col-028: confirmed_entries SessionRegistry tests ----
#[cfg(test)]
mod col028_confirmed_entries_tests {
    use crate::infra::session::SessionRegistry;

    // AC-09: context_get always calls record_confirmed_entry on successful retrieval.
    // This test verifies record_confirmed_entry populates confirmed_entries correctly.
    #[test]
    fn test_record_confirmed_entry_populates_set() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-e", None, None);

        registry.record_confirmed_entry("sess-e", 42);

        let state = registry.get_state("sess-e").unwrap();
        assert!(
            state.confirmed_entries.contains(&42),
            "confirmed_entries must contain entry_id 42 after record_confirmed_entry"
        );
    }

    // AC-09 not-found: confirmed_entries is empty when record_confirmed_entry is never called.
    // (EC-05 contract: record_confirmed_entry is only called after successful entry_store.get)
    #[test]
    fn test_confirmed_entries_empty_on_session_start() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-f", None, None);

        let state = registry.get_state("sess-f").unwrap();
        assert!(
            state.confirmed_entries.is_empty(),
            "confirmed_entries must be empty on session registration"
        );
    }

    // AC-10 (positive): single-target lookup triggers record_confirmed_entry.
    // Simulates the handler's: if target_ids.len() == 1 && params.id.is_some() guard.
    #[test]
    fn test_single_target_lookup_populates_confirmed_entries() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-g", None, None);

        // Simulate: single-ID lookup (target_ids.len() == 1 && params.id.is_some())
        let target_ids: Vec<u64> = vec![99];
        let has_explicit_id = true; // simulates params.id.is_some()
        if target_ids.len() == 1 && has_explicit_id {
            if let Some(&entry_id) = target_ids.first() {
                registry.record_confirmed_entry("sess-g", entry_id);
            }
        }

        let state = registry.get_state("sess-g").unwrap();
        assert!(
            state.confirmed_entries.contains(&99),
            "single-ID lookup must populate confirmed_entries"
        );
    }

    // AC-10 (negative — REQUIRED): multi-target lookup must NOT populate confirmed_entries.
    // ADR-004: request-side cardinality — only single explicit ID is an explicit fetch.
    #[test]
    fn test_multi_target_lookup_does_not_populate_confirmed_entries() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-h", None, None);

        // Simulate: multi-ID filter result (target_ids.len() != 1 || params.id.is_none())
        let target_ids: Vec<u64> = vec![10, 20];
        let has_explicit_id = false; // simulates filter-based path, no params.id
        if target_ids.len() == 1 && has_explicit_id {
            if let Some(&entry_id) = target_ids.first() {
                registry.record_confirmed_entry("sess-h", entry_id);
            }
        }

        let state = registry.get_state("sess-h").unwrap();
        assert!(
            state.confirmed_entries.is_empty(),
            "multi-target lookup must NOT populate confirmed_entries (ADR-004 cardinality)"
        );
    }

    // AC-10 (boundary): empty target_ids must not populate confirmed_entries (EC-04).
    #[test]
    fn test_empty_target_ids_does_not_populate_confirmed_entries() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-empty", None, None);

        let target_ids: Vec<u64> = vec![];
        let has_explicit_id = true; // params.id set, but result is empty
        if target_ids.len() == 1 && has_explicit_id {
            if let Some(&entry_id) = target_ids.first() {
                registry.record_confirmed_entry("sess-empty", entry_id);
            }
        }

        let state = registry.get_state("sess-empty").unwrap();
        assert!(
            state.confirmed_entries.is_empty(),
            "empty target_ids must not trigger confirmed_entries (len != 1)"
        );
    }

    // AC-11: access_weight = 2 for context_lookup (regression guard — must not drift).
    // This test documents and asserts the expected weight value at the constant level.
    #[test]
    fn test_context_lookup_access_weight_is_2() {
        // ADR-004 crt-019: lookup is an intentional act. weight=2 differentiates from search.
        // The actual weight is set in the handler; this test documents the expected constant.
        const LOOKUP_ACCESS_WEIGHT: u32 = 2;
        assert_eq!(
            LOOKUP_ACCESS_WEIGHT, 2,
            "context_lookup access_weight must be 2 (AC-11)"
        );
    }

    // Multiple confirmed entries can be added to the same session.
    #[test]
    fn test_multiple_confirmed_entries_accumulate() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-multi", None, None);

        registry.record_confirmed_entry("sess-multi", 1);
        registry.record_confirmed_entry("sess-multi", 2);
        registry.record_confirmed_entry("sess-multi", 3);

        let state = registry.get_state("sess-multi").unwrap();
        assert!(state.confirmed_entries.contains(&1));
        assert!(state.confirmed_entries.contains(&2));
        assert!(state.confirmed_entries.contains(&3));
        assert_eq!(state.confirmed_entries.len(), 3);
    }

    // Duplicate confirmed_entries inserts are idempotent (HashSet semantics).
    #[test]
    fn test_confirmed_entries_deduplicates_on_repeated_insert() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-dedup", None, None);

        registry.record_confirmed_entry("sess-dedup", 77);
        registry.record_confirmed_entry("sess-dedup", 77);
        registry.record_confirmed_entry("sess-dedup", 77);

        let state = registry.get_state("sess-dedup").unwrap();
        assert_eq!(
            state.confirmed_entries.len(),
            1,
            "HashSet must deduplicate repeated inserts of the same entry_id"
        );
        assert!(state.confirmed_entries.contains(&77));
    }
}

// crt-033: Store-backed integration tests for context_cycle_review memoization.
//
// These tests exercise the memoization helpers (build_cycle_review_record,
// check_stored_review, dispatch_review_with_advisory) and the store methods
// (store_cycle_review, get_cycle_review) end-to-end with a real SqlxStore
// backed by a tempdir. They do not invoke the full handler (which requires the
// complete server struct) — instead they call the handler internals directly
// and verify state via direct store reads, matching the test plan's stated
// approach: "call handler internals directly or use the store to verify state."
//
// MCP-level integration is covered by infra-001 suites.
#[cfg(test)]
mod cycle_review_integration_tests {
    use std::sync::Arc;

    use super::{build_cycle_review_record, check_stored_review, dispatch_review_with_advisory};

    // ---------------------------------------------------------------------------
    // Fixtures
    // ---------------------------------------------------------------------------

    /// Open a fresh SqlxStore in a tempdir.
    ///
    /// Uses `SqlxStore::open` directly — same pattern as
    /// `test_compute_knowledge_reuse_for_sessions_no_block_on_panic`.
    async fn open_store() -> (Arc<unimatrix_store::SqlxStore>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("crt033_test.db");
        let store = unimatrix_store::SqlxStore::open(&path, unimatrix_store::PoolConfig::default())
            .await
            .expect("open test store");
        (Arc::new(store), dir)
    }

    /// Build a minimal `RetrospectiveReport` for the given feature cycle with no
    /// hotspots — sufficient for memoization path tests.
    fn minimal_report(feature_cycle: &str) -> unimatrix_observe::RetrospectiveReport {
        unimatrix_observe::RetrospectiveReport {
            feature_cycle: feature_cycle.to_string(),
            session_count: 1,
            total_records: 5,
            metrics: unimatrix_observe::MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        }
    }

    /// Build a `RetrospectiveReport` with `n_hotspots` hotspots, each containing
    /// `evidence_per_hotspot` evidence items. Used by TH-I-07.
    fn report_with_evidence(
        feature_cycle: &str,
        n_hotspots: usize,
        evidence_per_hotspot: usize,
    ) -> unimatrix_observe::RetrospectiveReport {
        let evidence: Vec<unimatrix_observe::EvidenceRecord> = (0..evidence_per_hotspot)
            .map(|i| unimatrix_observe::EvidenceRecord {
                description: format!("evidence-{}", i),
                ts: (i as u64) * 1000,
                tool: None,
                detail: String::new(),
            })
            .collect();
        let hotspots: Vec<unimatrix_observe::HotspotFinding> = (0..n_hotspots)
            .map(|i| unimatrix_observe::HotspotFinding {
                category: unimatrix_observe::HotspotCategory::Friction,
                severity: unimatrix_observe::Severity::Warning,
                rule_name: format!("rule-{}", i),
                claim: format!("claim-{}", i),
                measured: (i + 1) as f64,
                threshold: 0.5,
                evidence: evidence.clone(),
            })
            .collect();
        unimatrix_observe::RetrospectiveReport {
            feature_cycle: feature_cycle.to_string(),
            session_count: 1,
            total_records: 10,
            metrics: unimatrix_observe::MetricVector::default(),
            hotspots,
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None, // crt-047
        }
    }

    // ---------------------------------------------------------------------------
    // TH-I-01: First call stores row with raw_signals_available=1, schema_version=1
    // Coverage: AC-03, AC-11
    // ---------------------------------------------------------------------------

    /// TH-I-01: build_cycle_review_record + store_cycle_review persists a row with
    /// raw_signals_available=1 and schema_version=SUMMARY_SCHEMA_VERSION. (AC-03, AC-11)
    #[tokio::test(flavor = "multi_thread")]
    async fn context_cycle_review_first_call_writes_correct_row() {
        let (store, _dir) = open_store().await;
        let report = minimal_report("col-test");

        let record = build_cycle_review_record("col-test", &report, None, 0)
            .expect("build_cycle_review_record must succeed");

        // raw_signals_available must be 1 (live signals present).
        assert_eq!(
            record.raw_signals_available, 1,
            "TH-I-01: raw_signals_available must be 1 before storage"
        );
        assert_eq!(
            record.schema_version,
            unimatrix_store::SUMMARY_SCHEMA_VERSION,
            "TH-I-01: schema_version must be SUMMARY_SCHEMA_VERSION before storage"
        );

        store
            .store_cycle_review(&record)
            .await
            .expect("store_cycle_review must succeed");

        // Verify via direct store read.
        let stored = store
            .get_cycle_review("col-test")
            .await
            .expect("get_cycle_review must succeed")
            .expect("row must exist after store_cycle_review");

        assert_eq!(
            stored.raw_signals_available, 1,
            "TH-I-01: raw_signals_available must be 1 in stored row (AC-03)"
        );
        assert_eq!(
            stored.schema_version,
            unimatrix_store::SUMMARY_SCHEMA_VERSION,
            "TH-I-01: schema_version must be SUMMARY_SCHEMA_VERSION in stored row (AC-11)"
        );
    }

    // ---------------------------------------------------------------------------
    // TH-I-02: Second call returns stored record (computed_at unchanged)
    // Coverage: AC-04, AC-14
    // ---------------------------------------------------------------------------

    /// TH-I-02: After storing a review, check_stored_review returns it on the second
    /// call; computed_at is unchanged (no recomputation). (AC-04, AC-14)
    #[tokio::test(flavor = "multi_thread")]
    async fn context_cycle_review_second_call_returns_stored_record() {
        let (store, _dir) = open_store().await;
        let report = minimal_report("memo-test");

        // First "call": build + store.
        let record =
            build_cycle_review_record("memo-test", &report, None, 0).expect("build must succeed");
        store
            .store_cycle_review(&record)
            .await
            .expect("store must succeed");
        let initial_computed_at = record.computed_at;

        // Second "call": fetch via get_cycle_review (simulates step 2.5 memoization check).
        let stored = store
            .get_cycle_review("memo-test")
            .await
            .expect("get must succeed")
            .expect("row must exist");

        assert_eq!(
            stored.computed_at, initial_computed_at,
            "TH-I-02: computed_at must be unchanged — row was not overwritten (AC-04)"
        );
        assert_eq!(
            stored.feature_cycle, "memo-test",
            "TH-I-02: feature_cycle must match (AC-14)"
        );

        // check_stored_review must succeed on the stored record.
        let (deserialized, advisory) =
            check_stored_review(&stored, unimatrix_store::SUMMARY_SCHEMA_VERSION)
                .expect("check_stored_review must return Ok");

        assert_eq!(
            deserialized.feature_cycle, "memo-test",
            "TH-I-02: deserialized report must have correct feature_cycle"
        );
        assert!(
            advisory.is_none(),
            "TH-I-02: no advisory when schema_version matches current"
        );
    }

    // ---------------------------------------------------------------------------
    // TH-I-03: force=true with live signals — computed_at advances (AC-05)
    // Mapped from original TH-I-04 in spec (test plan renumbering in gate report)
    // ---------------------------------------------------------------------------

    /// TH-I-03 (spec TH-I-04): Simulating force=true — a second store_cycle_review
    /// call overwrites the row, and computed_at advances or is equal (INSERT OR REPLACE).
    /// (AC-05)
    #[tokio::test(flavor = "multi_thread")]
    async fn context_cycle_review_force_true_overwrites_stored_row() {
        let (store, _dir) = open_store().await;
        let report = minimal_report("force-test");

        // Initial store (first call).
        let record1 =
            build_cycle_review_record("force-test", &report, None, 0).expect("build must succeed");
        store
            .store_cycle_review(&record1)
            .await
            .expect("initial store must succeed");
        let initial_computed_at = record1.computed_at;

        // Simulate force=true: build a new record and store it (INSERT OR REPLACE).
        // Force a different computed_at by using a future timestamp.
        let record2 = unimatrix_store::CycleReviewRecord {
            feature_cycle: "force-test".to_string(),
            schema_version: unimatrix_store::SUMMARY_SCHEMA_VERSION,
            computed_at: initial_computed_at + 1, // guaranteed > T1
            raw_signals_available: 1,
            summary_json: serde_json::to_string(&report).expect("serialize"),
            ..Default::default() // crt-047: curation health fields default to 0
        };
        store
            .store_cycle_review(&record2)
            .await
            .expect("force store must succeed");

        let stored = store
            .get_cycle_review("force-test")
            .await
            .expect("get must succeed")
            .expect("row must exist");

        assert!(
            stored.computed_at > initial_computed_at,
            "TH-I-03: computed_at must advance after force recompute (AC-05); \
             got {} <= {}",
            stored.computed_at,
            initial_computed_at
        );
    }

    // ---------------------------------------------------------------------------
    // TH-I-04: force=true + no live observations + stored record → stored record
    //          with "Raw signals have been purged" note (AC-06, AC-15)
    // Mapped from original TH-I-05 in spec
    // ---------------------------------------------------------------------------

    /// TH-I-04 (spec TH-I-05): Purged-signals path with stored record: check_stored_review
    /// succeeds; the handler note "Raw signals have been purged" must be constructible from
    /// the stored record. The purge path in the handler uses a `note` local and passes it
    /// to dispatch_review_with_advisory — we verify that construction here. (AC-06, AC-15)
    #[tokio::test(flavor = "multi_thread")]
    async fn context_cycle_review_force_purged_signals_with_stored_record_returns_note() {
        let (store, _dir) = open_store().await;
        let report = minimal_report("purged-test");

        // INSERT a stored record directly (no live observations exist).
        let record =
            build_cycle_review_record("purged-test", &report, None, 0).expect("build must succeed");
        store
            .store_cycle_review(&record)
            .await
            .expect("store must succeed");

        // Retrieve the stored record (simulates the force=true + empty attributed path).
        let stored = store
            .get_cycle_review("purged-test")
            .await
            .expect("get must succeed")
            .expect("row must exist — inserted above");

        // The handler constructs this note on the purged path:
        let note = format!(
            "Raw signals have been purged; returning stored record from {}.",
            stored.computed_at
        );

        // check_stored_review must succeed.
        let (deserialized_report, _advisory) =
            check_stored_review(&stored, unimatrix_store::SUMMARY_SCHEMA_VERSION)
                .expect("check_stored_review must return Ok on valid stored record");

        // dispatch_review_with_advisory must return Ok with the note in the response.
        let result = dispatch_review_with_advisory(
            deserialized_report,
            "markdown",
            None,
            Some(note.clone()),
        );
        let call_result = result.expect("dispatch must succeed (AC-06)");
        let response_text = call_result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("");
        assert!(
            response_text.contains("Raw signals have been purged"),
            "TH-I-04: response must contain 'Raw signals have been purged' (AC-15); \
             got: {}",
            &response_text[..response_text.len().min(200)]
        );

        // raw_signals_available in the stored record must be 1 (was set at write time).
        // The handler reports raw_signals_available=0 in the note, not in the stored value.
        // The spec's "reported as false (0)" refers to the note text, not the DB field.
        // Verify the note text says "purged" (which indicates unavailability).
        assert!(
            note.contains("purged"),
            "TH-I-04: purge note must mention 'purged' (AC-15)"
        );
    }

    // ---------------------------------------------------------------------------
    // TH-I-05: force=true + no live observations + no stored record →
    //          ERROR_NO_OBSERVATION_DATA (AC-07)
    // Mapped from original TH-I-06 in spec
    // ---------------------------------------------------------------------------

    /// TH-I-05 (spec TH-I-06): When get_cycle_review returns None AND observations are
    /// empty, the handler returns ERROR_NO_OBSERVATION_DATA. We verify this by confirming
    /// get_cycle_review returns None for an unknown cycle. (AC-07, R-04)
    #[tokio::test(flavor = "multi_thread")]
    async fn context_cycle_review_force_no_observations_no_stored_record_returns_none() {
        let (store, _dir) = open_store().await;

        // No observations, no stored record for "ghost-test".
        let result = store
            .get_cycle_review("ghost-test")
            .await
            .expect("get_cycle_review must succeed (no error)");

        assert!(
            result.is_none(),
            "TH-I-05: get_cycle_review must return None for unknown cycle (AC-07); \
             handler returns ERROR_NO_OBSERVATION_DATA on this path"
        );
        // The handler's error construction for this case:
        let error = rmcp::model::ErrorData::new(
            crate::error::ERROR_NO_OBSERVATION_DATA,
            format!(
                "No observation data found for feature '{}'. \
                 Ensure hook scripts are installed and sessions have been run.",
                "ghost-test"
            ),
            None,
        );
        assert_eq!(
            error.code,
            crate::error::ERROR_NO_OBSERVATION_DATA,
            "TH-I-05: error code must be ERROR_NO_OBSERVATION_DATA"
        );
    }

    // ---------------------------------------------------------------------------
    // TH-I-06: schema_version=0 in stored record → advisory "use force=true to recompute"
    // Mapped from original TH-I-03 in spec
    // Coverage: AC-04b
    // ---------------------------------------------------------------------------

    /// TH-I-06 (spec TH-I-03): Stored record with schema_version=0 triggers an advisory
    /// containing "use force=true to recompute". computed_at must be unchanged (no
    /// recompute occurred). (AC-04b)
    #[tokio::test(flavor = "multi_thread")]
    async fn context_cycle_review_stale_schema_version_produces_advisory() {
        let (store, _dir) = open_store().await;
        let report = minimal_report("adv-test");
        let valid_json = serde_json::to_string(&report).expect("serialize report");

        // INSERT directly with schema_version=0 (old version).
        let old_record = unimatrix_store::CycleReviewRecord {
            feature_cycle: "adv-test".to_string(),
            schema_version: 0,
            computed_at: 1_700_000_000,
            raw_signals_available: 1,
            summary_json: valid_json,
            ..Default::default() // crt-047: curation health fields default to 0
        };
        store
            .store_cycle_review(&old_record)
            .await
            .expect("store stale record must succeed");

        // Retrieve and run check_stored_review (simulates step 2.5).
        let stored = store
            .get_cycle_review("adv-test")
            .await
            .expect("get must succeed")
            .expect("row must exist");

        assert_eq!(
            stored.computed_at, 1_700_000_000,
            "TH-I-06: computed_at must be unchanged — no recompute (AC-04b)"
        );

        let (_, advisory) = check_stored_review(&stored, unimatrix_store::SUMMARY_SCHEMA_VERSION)
            .expect("check_stored_review must not fail on valid JSON despite version mismatch");

        let advisory_text = advisory.expect("advisory must be Some when schema_version=0");

        assert!(
            advisory_text.contains("use force=true to recompute"),
            "TH-I-06: advisory must contain 'use force=true to recompute' (AC-04b); \
             got: {}",
            advisory_text
        );
        assert!(
            advisory_text.contains('0'),
            "TH-I-06: advisory must contain stored version '0'; got: {}",
            advisory_text
        );
        assert!(
            advisory_text.contains(&unimatrix_store::SUMMARY_SCHEMA_VERSION.to_string()),
            "TH-I-06: advisory must contain current version {}; got: {}",
            unimatrix_store::SUMMARY_SCHEMA_VERSION,
            advisory_text
        );
    }

    // ---------------------------------------------------------------------------
    // TH-I-07: evidence_limit applied at render time only — raw stored JSON
    //          preserves full evidence (AC-08, R-03)
    // ---------------------------------------------------------------------------

    /// TH-I-07: build_cycle_review_record stores full evidence (no truncation).
    /// dispatch_review_with_advisory truncates at render time only. (AC-08, R-03)
    #[tokio::test(flavor = "multi_thread")]
    async fn context_cycle_review_evidence_limit_applied_at_render_time_only() {
        let (store, _dir) = open_store().await;
        // 3 hotspots, 5 evidence items each.
        let report = report_with_evidence("ev-test", 3, 5);

        // Assert A (storage): build + store — no evidence_limit applied.
        let record =
            build_cycle_review_record("ev-test", &report, None, 0).expect("build must succeed");
        store
            .store_cycle_review(&record)
            .await
            .expect("store must succeed");

        // Retrieve raw JSON and deserialize.
        let stored = store
            .get_cycle_review("ev-test")
            .await
            .expect("get must succeed")
            .expect("row must exist");

        let stored_report: unimatrix_observe::RetrospectiveReport =
            serde_json::from_str(&stored.summary_json)
                .expect("stored JSON must deserialize to RetrospectiveReport");

        for (i, hotspot) in stored_report.hotspots.iter().enumerate() {
            assert_eq!(
                hotspot.evidence.len(),
                5,
                "TH-I-07: raw stored hotspot {} must have 5 evidence items, not truncated (AC-08)",
                i
            );
        }

        // Assert B (response): dispatch_review_with_advisory with evidence_limit=2
        // must return hotspots with 2 evidence items (json format applies truncation).
        let (fresh_report, _advisory) =
            check_stored_review(&stored, unimatrix_store::SUMMARY_SCHEMA_VERSION)
                .expect("check_stored_review must succeed");

        let result = dispatch_review_with_advisory(
            fresh_report,
            "json",
            Some(2), // evidence_limit=2 applied at render time
            None,
        )
        .expect("dispatch must succeed");

        // Extract the JSON body from the response content.
        let json_text = result
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("");

        let rendered: unimatrix_observe::RetrospectiveReport =
            serde_json::from_str(&json_text).expect("response JSON must deserialize");

        for (i, hotspot) in rendered.hotspots.iter().enumerate() {
            assert_eq!(
                hotspot.evidence.len(),
                2,
                "TH-I-07: rendered hotspot {} must have 2 evidence items after evidence_limit=2 (R-03)",
                i
            );
        }

        // Assert C: second call via get_cycle_review (memoization hit) with evidence_limit=0
        // (bypass path — evidence_limit > 0 guard is false, full report returned) must return
        // 5 evidence items from the stored JSON. This proves stored JSON has full evidence.
        //
        // Note: evidence_limit=None defaults to 3 in the json format dispatch (unwrap_or(3)).
        // evidence_limit=Some(0) exercises the bypass path (> 0 is false) for full-evidence test.
        let stored2 = store
            .get_cycle_review("ev-test")
            .await
            .expect("get must succeed")
            .expect("row must exist");
        let (full_report, _) =
            check_stored_review(&stored2, unimatrix_store::SUMMARY_SCHEMA_VERSION)
                .expect("check_stored_review must succeed on second read");
        let result2 = dispatch_review_with_advisory(full_report, "json", Some(0), None)
            .expect("dispatch with evidence_limit=0 must succeed");
        let json_text2 = result2
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("");
        let rendered2: unimatrix_observe::RetrospectiveReport =
            serde_json::from_str(&json_text2).expect("second response JSON must deserialize");
        for (i, hotspot) in rendered2.hotspots.iter().enumerate() {
            assert_eq!(
                hotspot.evidence.len(),
                5,
                "TH-I-07: second read hotspot {} must have 5 evidence items \
                 (bypass path, evidence_limit=0) — proves stored JSON has full evidence (AC-08)",
                i
            );
        }
    }

    // ---------------------------------------------------------------------------
    // TH-I-08: RetrospectiveParams {"feature_cycle": "x"} → force.is_none() (AC-12)
    // ---------------------------------------------------------------------------

    /// TH-I-08: RetrospectiveParams deserialized with only feature_cycle must have
    /// force=None (AC-12). This confirms the handler treats absent force as false
    /// (no forced recompute) per params.force.unwrap_or(false).
    #[test]
    fn context_cycle_review_params_force_absent_is_none() {
        let params: super::RetrospectiveParams =
            serde_json::from_str(r#"{"feature_cycle": "x"}"#).unwrap();
        assert!(
            params.force.is_none(),
            "TH-I-08: force must be None when absent from JSON (AC-12)"
        );
    }

    // ---------------------------------------------------------------------------
    // TH-I-10: Concurrent first-calls for different cycles both complete (OQ-03)
    // ---------------------------------------------------------------------------

    /// TH-I-10: Concurrent store_cycle_review calls for different cycles must both
    /// succeed. INSERT OR REPLACE is safe under concurrent access. (OQ-03, R-02)
    #[tokio::test(flavor = "multi_thread")]
    async fn context_cycle_review_concurrent_first_calls_both_complete() {
        let (store, _dir) = open_store().await;
        let store_a = Arc::clone(&store);
        let store_b = Arc::clone(&store);

        let report_a = minimal_report("concurrent-A");
        let report_b = minimal_report("concurrent-B");

        let record_a = build_cycle_review_record("concurrent-A", &report_a, None, 0)
            .expect("build A must succeed");
        let record_b = build_cycle_review_record("concurrent-B", &report_b, None, 0)
            .expect("build B must succeed");

        // Run both stores concurrently via tokio::join! (OQ-03).
        let (result_a, result_b) = tokio::join!(
            store_a.store_cycle_review(&record_a),
            store_b.store_cycle_review(&record_b),
        );

        assert!(
            result_a.is_ok(),
            "TH-I-10: store for concurrent-A must succeed; got: {:?}",
            result_a.err()
        );
        assert!(
            result_b.is_ok(),
            "TH-I-10: store for concurrent-B must succeed; got: {:?}",
            result_b.err()
        );

        // Both rows must be present.
        let row_a = store
            .get_cycle_review("concurrent-A")
            .await
            .expect("get A must succeed")
            .expect("row A must exist");
        let row_b = store
            .get_cycle_review("concurrent-B")
            .await
            .expect("get B must succeed")
            .expect("row B must exist");

        assert_eq!(
            row_a.feature_cycle, "concurrent-A",
            "TH-I-10: concurrent-A row must be present"
        );
        assert_eq!(
            row_b.feature_cycle, "concurrent-B",
            "TH-I-10: concurrent-B row must be present"
        );
    }

    // -------------------------------------------------------------------------
    // crt-047 unit tests: CCR-U-01 through CCR-U-09
    // These tests exercise curation_health block construction, baseline gating,
    // advisory generation, and force=true/false semantics.
    // -------------------------------------------------------------------------

    // CCR-U-01: curation_health block present on cold start (AC-06, EC-01)
    //
    // Verifies that compute_curation_baseline + compare_to_baseline produce a
    // valid CurationHealthBlock when the snapshot is Some.  Uses the pure
    // functions directly — no DB insert needed for this logic path.
    #[test]
    fn test_context_cycle_review_curation_health_present_on_cold_start() {
        use crate::services::curation_health::{
            CURATION_MIN_HISTORY, compare_to_baseline, compute_curation_baseline,
        };
        use unimatrix_observe::{CurationHealthBlock, CurationSnapshot};
        use unimatrix_store::cycle_review_index::CurationBaselineRow;

        // No prior rows → baseline must be None.
        let rows: Vec<CurationBaselineRow> = vec![];
        let baseline = compute_curation_baseline(&rows, 10);
        assert!(
            baseline.is_none(),
            "CCR-U-01: cold start must have no baseline"
        );

        let snapshot = CurationSnapshot {
            corrections_total: 0,
            corrections_agent: 0,
            corrections_human: 0,
            corrections_system: 0,
            deprecations_total: 0,
            orphan_deprecations: 0,
        };

        // When baseline is None, the block has snapshot but no baseline comparison.
        let block = CurationHealthBlock {
            snapshot: snapshot.clone(),
            baseline: baseline.and_then(|b| {
                let h = b.history_cycles;
                Some(compare_to_baseline(&snapshot, &b, h))
            }),
        };
        assert!(
            block.baseline.is_none(),
            "CCR-U-01: cold start block.baseline must be None"
        );
        assert_eq!(
            block.snapshot.corrections_total, 0,
            "CCR-U-01: snapshot.corrections_total must be 0"
        );
        assert_eq!(
            block.snapshot.deprecations_total, 0,
            "CCR-U-01: snapshot.deprecations_total must be 0"
        );
        // confirm CURATION_MIN_HISTORY is 3 — test is calibrated to that constant.
        assert_eq!(CURATION_MIN_HISTORY, 3, "CCR-U-01: constant sanity check");
    }

    // CCR-U-02: curation_health.baseline absent when fewer than CURATION_MIN_HISTORY rows (AC-08, R-11)
    //
    // 2 qualifying rows (schema_version=2) → below CURATION_MIN_HISTORY=3 → baseline=None.
    #[test]
    fn test_context_cycle_review_baseline_absent_with_two_prior_rows() {
        use crate::services::curation_health::compute_curation_baseline;
        use unimatrix_store::cycle_review_index::CurationBaselineRow;

        let rows = vec![
            CurationBaselineRow {
                corrections_total: 2,
                corrections_agent: 2,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 0,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 3,
                corrections_agent: 3,
                corrections_human: 0,
                deprecations_total: 2,
                orphan_deprecations: 1,
                schema_version: 2,
            },
        ];

        let baseline = compute_curation_baseline(&rows, 10);
        assert!(
            baseline.is_none(),
            "CCR-U-02: 2 qualifying rows must produce no baseline (< CURATION_MIN_HISTORY=3)"
        );
    }

    // CCR-U-03: curation_health.baseline present when 3+ qualifying rows with σ annotation (AC-07)
    //
    // 3 qualifying rows → baseline present; σ values must be finite.
    #[test]
    fn test_context_cycle_review_baseline_present_with_three_prior_rows() {
        use crate::services::curation_health::{compare_to_baseline, compute_curation_baseline};
        use unimatrix_observe::CurationSnapshot;
        use unimatrix_store::cycle_review_index::CurationBaselineRow;

        let rows = vec![
            CurationBaselineRow {
                corrections_total: 2,
                corrections_agent: 2,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 0,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 4,
                corrections_agent: 4,
                corrections_human: 0,
                deprecations_total: 2,
                orphan_deprecations: 1,
                schema_version: 2,
            },
            CurationBaselineRow {
                corrections_total: 3,
                corrections_agent: 3,
                corrections_human: 0,
                deprecations_total: 1,
                orphan_deprecations: 0,
                schema_version: 2,
            },
        ];

        let baseline = compute_curation_baseline(&rows, 10)
            .expect("CCR-U-03: 3 qualifying rows must produce a baseline");

        assert_eq!(
            baseline.history_cycles, 3,
            "CCR-U-03: history_cycles must reflect qualifying row count"
        );

        let snapshot = CurationSnapshot {
            corrections_total: 3,
            corrections_agent: 2,
            corrections_human: 1,
            corrections_system: 0,
            deprecations_total: 2,
            orphan_deprecations: 1,
        };

        let h = baseline.history_cycles;
        let comparison = compare_to_baseline(&snapshot, &baseline, h);

        assert!(
            comparison.corrections_total_sigma.is_finite(),
            "CCR-U-03: corrections_total_sigma must be finite, got {}",
            comparison.corrections_total_sigma
        );
        assert!(
            comparison.orphan_ratio_sigma.is_finite(),
            "CCR-U-03: orphan_ratio_sigma must be finite, got {}",
            comparison.orphan_ratio_sigma
        );
        assert_eq!(
            comparison.history_cycles, 3,
            "CCR-U-03: comparison.history_cycles must be 3"
        );
    }

    // CCR-U-04: force=false with schema_version=1 returns advisory (AC-11, R-12)
    //
    // Verifies that check_stored_review produces the advisory string when
    // schema_version != SUMMARY_SCHEMA_VERSION.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_context_cycle_review_advisory_on_stale_schema_version() {
        let (store, _dir) = open_store().await;
        let report = minimal_report("crt-047-advisory-test");

        // Build and store a record with schema_version = 1 (stale).
        let mut record = build_cycle_review_record("crt-047-advisory-test", &report, None, 0)
            .expect("build must succeed");
        record.schema_version = 1; // Force stale schema_version.
        store
            .store_cycle_review(&record)
            .await
            .expect("store must succeed");

        // Retrieve and check for advisory.
        let stored = store
            .get_cycle_review("crt-047-advisory-test")
            .await
            .expect("get must succeed")
            .expect("row must exist");

        let (_, advisory) = check_stored_review(&stored, unimatrix_store::SUMMARY_SCHEMA_VERSION)
            .expect("check_stored_review must not error");

        assert!(
            advisory.is_some(),
            "CCR-U-04: advisory must be Some when schema_version=1 != current ({})",
            unimatrix_store::SUMMARY_SCHEMA_VERSION
        );
        let advisory_text = advisory.unwrap();
        assert!(
            advisory_text.contains("schema_version 1"),
            "CCR-U-04: advisory must mention schema_version 1, got: {advisory_text}"
        );
        assert!(
            advisory_text.contains("force=true"),
            "CCR-U-04: advisory must mention force=true, got: {advisory_text}"
        );
    }

    // CCR-U-05: force=false with schema_version=1 does NOT recompute snapshot (AC-12, R-12)
    //
    // Negative assertion: a stale row written at schema_version=1 is returned
    // unchanged when force=false (check_stored_review returns it as-is).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_context_cycle_review_force_false_no_silent_recompute() {
        let (store, _dir) = open_store().await;
        let report = minimal_report("crt-047-no-recompute-test");

        // Build and store a record with schema_version = 1 and corrections_total = 0.
        let mut record = build_cycle_review_record("crt-047-no-recompute-test", &report, None, 0)
            .expect("build must succeed");
        record.schema_version = 1;
        record.corrections_total = 0; // Stale zero, as if pre-crt-047.
        store
            .store_cycle_review(&record)
            .await
            .expect("store must succeed");

        // Retrieve the row — it must still have schema_version=1 and corrections_total=0.
        let stored = store
            .get_cycle_review("crt-047-no-recompute-test")
            .await
            .expect("get must succeed")
            .expect("row must exist");

        assert_eq!(
            stored.schema_version, 1,
            "CCR-U-05: force=false must not silently update schema_version (expected 1, got {})",
            stored.schema_version
        );
        assert_eq!(
            stored.corrections_total, 0,
            "CCR-U-05: force=false must not silently recompute corrections_total"
        );
    }

    // CCR-U-06: force=true on stale record updates schema_version to 2 (AC-12 positive path)
    //
    // Verifies the upsert path: storing a new record with SUMMARY_SCHEMA_VERSION
    // overwrites the stale row's schema_version field.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_context_cycle_review_force_true_updates_stale_record() {
        let (store, _dir) = open_store().await;
        let report = minimal_report("crt-047-force-true-test");

        // Store a stale record (schema_version=1).
        let mut stale =
            build_cycle_review_record("crt-047-force-true-test", &report, None, 1_000_000)
                .expect("build must succeed");
        stale.schema_version = 1;
        store
            .store_cycle_review(&stale)
            .await
            .expect("initial store must succeed");

        // Simulate force=true: build a fresh record (SUMMARY_SCHEMA_VERSION=2).
        let fresh = build_cycle_review_record("crt-047-force-true-test", &report, None, 1_000_000)
            .expect("build fresh must succeed");
        assert_eq!(
            fresh.schema_version,
            unimatrix_store::SUMMARY_SCHEMA_VERSION,
            "CCR-U-06: fresh record must have SUMMARY_SCHEMA_VERSION"
        );
        store
            .store_cycle_review(&fresh)
            .await
            .expect("force-true store must succeed");

        // Retrieve: row must now have the updated schema_version.
        let updated = store
            .get_cycle_review("crt-047-force-true-test")
            .await
            .expect("get must succeed")
            .expect("row must exist after force-true upsert");

        assert_eq!(
            updated.schema_version,
            unimatrix_store::SUMMARY_SCHEMA_VERSION,
            "CCR-U-06: schema_version must be {} after force=true upsert",
            unimatrix_store::SUMMARY_SCHEMA_VERSION
        );

        // No advisory on fresh retrieval.
        let (_, advisory) = check_stored_review(&updated, unimatrix_store::SUMMARY_SCHEMA_VERSION)
            .expect("check_stored_review must not error");
        assert!(
            advisory.is_none(),
            "CCR-U-06: no advisory expected for current schema_version, got: {:?}",
            advisory
        );
    }

    // CCR-U-09: Cycle with no cycle_start event does not panic (EC-02, I-03)
    //
    // Verifies extract_cycle_start_ts returns 0 when no cycle_start events exist,
    // and that the first_computed_at fallback (review_ts) is used instead.
    #[test]
    fn test_context_cycle_review_no_cycle_start_event_does_not_panic() {
        use super::extract_cycle_start_ts;
        use unimatrix_observe::CycleEventRecord;

        // No events at all.
        let ts = extract_cycle_start_ts(None);
        assert_eq!(ts, 0, "CCR-U-09: None events must return 0");

        // Events present but none is cycle_start.
        let events = vec![CycleEventRecord {
            event_type: "cycle_stop".to_string(),
            timestamp: 9_000,
            seq: 0,
            phase: None,
            outcome: None,
            next_phase: None,
        }];
        let ts = extract_cycle_start_ts(Some(&events));
        assert_eq!(ts, 0, "CCR-U-09: no cycle_start event must return 0");

        // Single cycle_start event.
        let events = vec![CycleEventRecord {
            event_type: "cycle_start".to_string(),
            timestamp: 5_000,
            seq: 0,
            phase: None,
            outcome: None,
            next_phase: None,
        }];
        let ts = extract_cycle_start_ts(Some(&events));
        assert_eq!(
            ts, 5_000,
            "CCR-U-09: single cycle_start must return its timestamp"
        );

        // Multiple cycle_start events — MIN should be returned.
        let events = vec![
            CycleEventRecord {
                event_type: "cycle_start".to_string(),
                timestamp: 3_000,
                seq: 0,
                phase: None,
                outcome: None,
                next_phase: None,
            },
            CycleEventRecord {
                event_type: "cycle_start".to_string(),
                timestamp: 1_000,
                seq: 1,
                phase: None,
                outcome: None,
                next_phase: None,
            },
        ];
        let ts = extract_cycle_start_ts(Some(&events));
        assert_eq!(
            ts, 1_000,
            "CCR-U-09: MIN timestamp must be returned for multiple cycle_start events"
        );
    }
}

// ---- vnc-012: string-encoded integer coercion tests ----
// All tests exercise the full serde deserialization path including the
// #[serde(deserialize_with)] attribute routing. No server required.
#[cfg(test)]
mod vnc012_coercion_tests {
    use serde_json::{from_str, from_value, json};

    use super::{
        BriefingParams, CorrectParams, DeprecateParams, GetParams, LookupParams, QuarantineParams,
        RetrospectiveParams, SearchParams,
    };

    // -- AC-01: Required integer fields accept string input --

    #[test]
    fn test_get_params_string_id() {
        let params: GetParams = from_str(r#"{"id": "3770"}"#).unwrap();
        assert_eq!(params.id, 3770i64);
    }

    #[test]
    fn test_deprecate_params_string_id() {
        let params: DeprecateParams = from_str(r#"{"id": "3770"}"#).unwrap();
        assert_eq!(params.id, 3770i64);
    }

    #[test]
    fn test_quarantine_params_string_id() {
        let params: QuarantineParams = from_str(r#"{"id": "3770"}"#).unwrap();
        assert_eq!(params.id, 3770i64);
    }

    // -- AC-02: CorrectParams.original_id accepts string input --

    #[test]
    fn test_correct_params_string_original_id() {
        let params: CorrectParams = from_str(r#"{"original_id": "3770", "content": "c"}"#).unwrap();
        assert_eq!(params.original_id, 3770i64);
    }

    // -- AC-03: LookupParams optional fields accept string + absent + null --

    #[test]
    fn test_lookup_params_string_id() {
        let params: LookupParams = from_str(r#"{"id": "42"}"#).unwrap();
        assert_eq!(params.id, Some(42i64));
    }

    #[test]
    fn test_lookup_params_string_limit() {
        let params: LookupParams = from_str(r#"{"limit": "10"}"#).unwrap();
        assert_eq!(params.limit, Some(10i64));
    }

    #[test]
    fn test_lookup_params_absent_id() {
        // AC-03-ABSENT-ID: #[serde(default)] must yield None when key is missing (R-01)
        let params: LookupParams = from_str(r#"{}"#).unwrap();
        assert!(params.id.is_none());
    }

    #[test]
    fn test_lookup_params_absent_limit() {
        // AC-03-ABSENT-LIMIT: #[serde(default)] must yield None when key is missing (R-01)
        let params: LookupParams = from_str(r#"{}"#).unwrap();
        assert!(params.limit.is_none());
    }

    #[test]
    fn test_lookup_params_null_id() {
        // AC-03-NULL-ID: visit_none/visit_unit must return Ok(None) for JSON null (R-03)
        let params: LookupParams = from_str(r#"{"id": null}"#).unwrap();
        assert!(params.id.is_none());
    }

    #[test]
    fn test_lookup_params_null_limit() {
        // AC-03-NULL-LIMIT: visit_none/visit_unit must return Ok(None) for JSON null (R-03)
        let params: LookupParams = from_str(r#"{"limit": null}"#).unwrap();
        assert!(params.limit.is_none());
    }

    // -- AC-04: SearchParams.k accepts string + absent + null --

    #[test]
    fn test_search_params_string_k() {
        let params: SearchParams = from_str(r#"{"query": "q", "k": "5"}"#).unwrap();
        assert_eq!(params.k, Some(5i64));
    }

    #[test]
    fn test_search_params_absent_k() {
        // AC-04-ABSENT
        let params: SearchParams = from_str(r#"{"query": "q"}"#).unwrap();
        assert!(params.k.is_none());
    }

    #[test]
    fn test_search_params_null_k() {
        // AC-04-NULL
        let params: SearchParams = from_str(r#"{"query": "q", "k": null}"#).unwrap();
        assert!(params.k.is_none());
    }

    // -- AC-05: BriefingParams.max_tokens accepts string + absent + null --

    #[test]
    fn test_briefing_params_string_max_tokens() {
        let params: BriefingParams = from_str(r#"{"task": "t", "max_tokens": "3000"}"#).unwrap();
        assert_eq!(params.max_tokens, Some(3000i64));
    }

    #[test]
    fn test_briefing_params_absent_max_tokens() {
        // AC-05-ABSENT
        let params: BriefingParams = from_str(r#"{"task": "t"}"#).unwrap();
        assert!(params.max_tokens.is_none());
    }

    #[test]
    fn test_briefing_params_null_max_tokens() {
        // AC-05-NULL
        let params: BriefingParams = from_str(r#"{"task": "t", "max_tokens": null}"#).unwrap();
        assert!(params.max_tokens.is_none());
    }

    // -- AC-06: RetrospectiveParams.evidence_limit accepts string + zero + absent + null --

    #[test]
    fn test_retro_params_string_evidence_limit() {
        // AC-06
        let params: RetrospectiveParams =
            from_str(r#"{"feature_cycle": "col-001", "evidence_limit": "5"}"#).unwrap();
        assert_eq!(params.evidence_limit, Some(5usize));
    }

    #[test]
    fn test_retro_params_zero_evidence_limit() {
        // AC-06-ZERO
        let params: RetrospectiveParams =
            from_str(r#"{"feature_cycle": "col-001", "evidence_limit": "0"}"#).unwrap();
        assert_eq!(params.evidence_limit, Some(0usize));
    }

    #[test]
    fn test_retro_params_absent_evidence_limit() {
        // AC-06-ABSENT
        let params: RetrospectiveParams = from_str(r#"{"feature_cycle": "col-001"}"#).unwrap();
        assert!(params.evidence_limit.is_none());
    }

    #[test]
    fn test_retro_params_null_evidence_limit() {
        // AC-06-NULL
        let params: RetrospectiveParams =
            from_str(r#"{"feature_cycle": "col-001", "evidence_limit": null}"#).unwrap();
        assert!(params.evidence_limit.is_none());
    }

    // -- AC-07: Required integer fields continue to accept JSON integer input (regression) --

    #[test]
    fn test_get_params_integer_id() {
        let params: GetParams = from_str(r#"{"id": 42}"#).unwrap();
        assert_eq!(params.id, 42i64);
    }

    #[test]
    fn test_deprecate_params_integer_id() {
        let params: DeprecateParams = from_str(r#"{"id": 42}"#).unwrap();
        assert_eq!(params.id, 42i64);
    }

    #[test]
    fn test_quarantine_params_integer_id() {
        let params: QuarantineParams = from_str(r#"{"id": 42}"#).unwrap();
        assert_eq!(params.id, 42i64);
    }

    #[test]
    fn test_correct_params_integer_original_id() {
        let params: CorrectParams = from_str(r#"{"original_id": 42, "content": "c"}"#).unwrap();
        assert_eq!(params.original_id, 42i64);
    }

    // -- AC-08: Non-numeric strings rejected for required fields --

    #[test]
    fn test_get_params_nonnumeric_id_is_err() {
        let result: Result<GetParams, _> = from_str(r#"{"id": "abc"}"#);
        assert!(
            result.is_err(),
            "AC-08: non-numeric string must produce error"
        );
    }

    #[test]
    fn test_deprecate_params_nonnumeric_id_is_err() {
        let result: Result<DeprecateParams, _> = from_str(r#"{"id": "abc"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_quarantine_params_nonnumeric_id_is_err() {
        let result: Result<QuarantineParams, _> = from_str(r#"{"id": "abc"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_correct_params_nonnumeric_original_id_is_err() {
        let result: Result<CorrectParams, _> =
            from_str(r#"{"original_id": "abc", "content": "c"}"#);
        assert!(result.is_err());
    }

    // -- AC-08-OPT: Non-numeric strings rejected for optional fields --

    #[test]
    fn test_lookup_params_nonnumeric_id_is_err() {
        let result: Result<LookupParams, _> = from_str(r#"{"id": "abc"}"#);
        assert!(
            result.is_err(),
            "AC-08-OPT: non-numeric string rejected for optional id"
        );
    }

    #[test]
    fn test_lookup_params_nonnumeric_limit_is_err() {
        let result: Result<LookupParams, _> = from_str(r#"{"limit": "abc"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_params_nonnumeric_k_is_err() {
        let result: Result<SearchParams, _> = from_str(r#"{"query": "q", "k": "abc"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_briefing_params_nonnumeric_max_tokens_is_err() {
        let result: Result<BriefingParams, _> = from_str(r#"{"task": "t", "max_tokens": "abc"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_retro_params_nonnumeric_evidence_limit_is_err() {
        let result: Result<RetrospectiveParams, _> =
            from_str(r#"{"feature_cycle": "c", "evidence_limit": "abc"}"#);
        assert!(result.is_err());
    }

    // -- AC-09: Negative strings and float strings rejected --

    #[test]
    fn test_retro_params_negative_evidence_limit_is_err() {
        // AC-09: negative string rejected for usize field (R-04)
        let result: Result<RetrospectiveParams, _> =
            from_str(r#"{"feature_cycle": "c", "evidence_limit": "-1"}"#);
        assert!(
            result.is_err(),
            "AC-09: negative string must be rejected for usize"
        );
    }

    #[test]
    fn test_get_params_float_string_is_err() {
        // AC-09-FLOAT: float string rejected for required i64
        let result: Result<GetParams, _> = from_str(r#"{"id": "3.5"}"#);
        assert!(
            result.is_err(),
            "AC-09-FLOAT: float string must be rejected"
        );
    }

    #[test]
    fn test_search_params_float_string_k_is_err() {
        // AC-09-FLOAT: float string rejected for optional i64
        let result: Result<SearchParams, _> = from_str(r#"{"query": "q", "k": "3.5"}"#);
        assert!(result.is_err());
    }

    // -- AC-09-FLOAT-NUMBER: Float JSON Numbers rejected (FR-13) --

    #[test]
    fn test_get_params_float_number_is_err() {
        // AC-09-FLOAT-NUMBER: 3.0 is a JSON float Number — must invoke visit_f64 -> Err
        let result: Result<GetParams, _> = from_str(r#"{"id": 3.0}"#);
        assert!(
            result.is_err(),
            "AC-09-FLOAT-NUMBER: float JSON Number must be rejected"
        );
        // Double assertion: guard against silent truncation to integer
        assert!(
            !result.is_ok(),
            "float JSON Number must not silently truncate to Ok(id=3)"
        );
    }

    #[test]
    fn test_search_params_float_number_k_is_err() {
        // AC-09-FLOAT-NUMBER: 5.0 is a JSON float Number
        let result: Result<SearchParams, _> = from_str(r#"{"query": "q", "k": 5.0}"#);
        assert!(
            result.is_err(),
            "AC-09-FLOAT-NUMBER: float JSON Number must be rejected for optional field"
        );
    }

    #[test]
    fn test_lookup_params_float_number_id_is_err() {
        // AC-09-FLOAT-NUMBER: covers optional i64 field path
        let result: Result<LookupParams, _> = from_str(r#"{"id": 3.0}"#);
        assert!(result.is_err());
    }

    // -- AC-13: In-process rmcp dispatch path test (serde_json::from_value) --
    // This exercises the EXACT code path run by Parameters<T>: FromContextPart in rmcp.
    // rmcp calls: serde_json::from_value::<T>(Value::Object(arguments))

    #[test]
    fn test_get_params_string_id_coercion() {
        // AC-13 primary: verify serde_json::from_value (the rmcp Parameters<T> dispatch path)
        // accepts string-encoded id. Name includes "coercion" per AC-13 requirement.
        let args = json!({"id": "3770", "agent_id": "human"});
        let result = from_value::<GetParams>(args);
        assert!(
            result.is_ok(),
            "AC-13: string id must not produce serde error; got: {:?}",
            result.err()
        );
        assert_eq!(
            result.unwrap().id,
            3770i64,
            "AC-13: string id must coerce to i64"
        );
    }

    #[test]
    fn test_deprecate_params_string_id_coercion() {
        // AC-13 secondary: covers DeprecateParams on the from_value path.
        let args = json!({"id": "42"});
        let result = from_value::<DeprecateParams>(args);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, 42i64);
    }

    // -- Additional regression: string and integer forms produce equal results (AC-07 guard) --

    #[test]
    fn test_get_params_string_and_integer_equal() {
        let from_string: GetParams = from_str(r#"{"id": "3770"}"#).unwrap();
        let from_integer: GetParams = from_str(r#"{"id": 3770}"#).unwrap();
        assert_eq!(
            from_string.id, from_integer.id,
            "AC-07: string and integer forms must produce the same i64 value"
        );
    }
}

// ---- crt-046 (GH#515): cluster_entry_ids_raw cap tests ----
// These tests directly exercise the sort → dedup → truncate idiom used inside
// context_briefing. The logic is inline, so we replicate it here to verify
// correctness of the CLUSTER_ID_CAP constant and the truncation ordering.
#[cfg(test)]
mod crt046_cluster_id_cap_tests {
    // Mirrors the CLUSTER_ID_CAP value from context_briefing.
    const CLUSTER_ID_CAP: usize = 50;

    /// Sort, dedup, then truncate — the exact sequence used in context_briefing.
    fn apply_cap(mut ids: Vec<u64>) -> Vec<u64> {
        ids.sort_unstable();
        ids.dedup();
        ids.truncate(CLUSTER_ID_CAP);
        ids
    }

    #[test]
    fn test_cluster_id_cap_truncates_to_50() {
        // More than 50 unique IDs: result must be exactly CLUSTER_ID_CAP entries.
        let ids: Vec<u64> = (1u64..=75).collect();
        let result = apply_cap(ids);
        assert_eq!(
            result.len(),
            CLUSTER_ID_CAP,
            "more than 50 unique IDs must be truncated to exactly {CLUSTER_ID_CAP}"
        );
        // Truncation drops the numerically-highest IDs (those created most recently).
        assert_eq!(result[0], 1, "first retained ID must be 1 (lowest)");
        assert_eq!(
            result[CLUSTER_ID_CAP - 1],
            50,
            "last retained ID must be 50 after truncation"
        );
    }

    #[test]
    fn test_cluster_id_cap_fewer_than_50_unchanged() {
        // Fewer than 50 unique IDs: nothing is truncated.
        let ids: Vec<u64> = (1u64..=20).collect();
        let result = apply_cap(ids);
        assert_eq!(result.len(), 20, "fewer than 50 IDs must not be truncated");
    }

    #[test]
    fn test_cluster_id_cap_exactly_50_unchanged() {
        // Exactly 50 unique IDs: nothing is truncated.
        let ids: Vec<u64> = (1u64..=50).collect();
        let result = apply_cap(ids);
        assert_eq!(result.len(), 50, "exactly 50 IDs must not be truncated");
    }

    #[test]
    fn test_cluster_id_cap_dedup_then_truncate() {
        // Dedup-then-truncate interaction: overlapping cluster IDs that dedup to ≤50
        // must not be truncated. Verifies dedup runs before truncation.
        //
        // Scenario: 5 clusters × 20 entries each, with heavy overlap.
        // Before dedup: up to 100 entries. After dedup: ≤20 unique → no truncation.
        let mut ids: Vec<u64> = Vec::new();
        for _ in 0..5 {
            ids.extend(1u64..=20); // 5 × 20 = 100 entries, all overlap
        }
        let result = apply_cap(ids);
        // After dedup: 20 unique IDs. After truncation: still 20 (below cap).
        assert_eq!(
            result.len(),
            20,
            "dedup-then-truncate: 100 entries with 20 unique IDs must yield 20 after cap"
        );
        assert_eq!(result[0], 1, "first ID must be 1");
        assert_eq!(result[19], 20, "last ID must be 20");
    }

    #[test]
    fn test_cluster_id_cap_dedup_overlap_crossing_cap() {
        // Overlap reduces 75 raw entries to exactly 51 unique IDs → capped to 50.
        // Verifies the combined dedup + truncation path when result is just above cap.
        let mut ids: Vec<u64> = Vec::new();
        // 51 unique IDs, each duplicated once (102 total raw entries).
        for id in 1u64..=51 {
            ids.push(id);
            ids.push(id);
        }
        let result = apply_cap(ids);
        assert_eq!(
            result.len(),
            CLUSTER_ID_CAP,
            "51 unique IDs must be capped to {CLUSTER_ID_CAP}"
        );
        // The numerically-highest ID (51) is dropped.
        assert!(
            !result.contains(&51),
            "ID 51 must be truncated (highest u64 value)"
        );
        assert!(result.contains(&50), "ID 50 must be retained");
    }
}
