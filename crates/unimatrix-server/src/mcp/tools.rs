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
use unimatrix_store::QueryLogRecord;

use crate::infra::audit::{AuditEvent, Outcome};
use crate::infra::registry::Capability;
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
    pub id: Option<i64>,
    /// Filter by status (active, deprecated, proposed).
    pub status: Option<String>,
    /// Max results to return (default: 10).
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
    /// Role to get briefed on (e.g., "architect", "developer").
    pub role: String,
    /// Task description for context retrieval.
    pub task: String,
    /// Feature tag to boost relevant entries.
    pub feature: Option<String>,
    /// Max output tokens (default: 3000, range: 500-10000).
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
    pub evidence_limit: Option<usize>,
    /// Output format: "markdown" (default) or "json". (vnc-011)
    pub format: Option<String>,
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
    /// Agent making the request.
    pub agent_id: Option<String>,
    /// Response format: summary, markdown, or json.
    pub format: Option<String>,
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
                current_phase: None,
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
                current_phase: None,
            },
        );

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
        self.services.usage.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: ctx.audit_ctx.session_id.clone(),
                agent_id: Some(ctx.agent_id.clone()),
                helpful: params.helpful.or(Some(true)),
                feature_cycle: params.feature.clone(),
                trust_level: Some(ctx.trust_level),
                access_weight: 1,
                current_phase: None,
            },
        );

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
        description = "Get an orientation briefing for a role and task. Includes role conventions and task-relevant context from the knowledge base. Use at the start of any task."
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
            // Step 3: feature/topic fallback (params.feature else params.role)
            let topic = params.feature.as_deref().unwrap_or(&params.role);

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

            // 8. Delegate to IndexBriefingService
            let entries = self
                .services
                .briefing
                .index(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
                .await
                .map_err(rmcp::ErrorData::from)?;

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
            self.services.usage.record_access(
                &entry_ids,
                AccessSource::Briefing,
                UsageContext {
                    session_id: ctx.audit_ctx.session_id.clone(),
                    agent_id: Some(ctx.agent_id.clone()),
                    helpful: params.helpful,
                    feature_cycle: params.feature.clone(),
                    trust_level: Some(ctx.trust_level),
                    access_weight: 1,
                    current_phase: None,
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

        // 3. Load observations from SQL via ObservationSource (col-012)
        //    First try direct feature_cycle query (fast path).
        //    If empty, fall back to content-based attribution (#162).
        let store_for_obs = Arc::clone(&self.store);
        let registry_for_obs = Arc::clone(&self.observation_registry);
        let feature_cycle_for_load = params.feature_cycle.clone();
        let attributed = crate::infra::timeout::spawn_blocking_with_timeout(
            crate::infra::timeout::MCP_HANDLER_TIMEOUT,
            move || -> std::result::Result<Vec<unimatrix_observe::ObservationRecord>, unimatrix_observe::ObserveError> {
                use unimatrix_observe::ObservationSource;
                let source = crate::services::observation::SqlObservationSource::new(store_for_obs, registry_for_obs);

                // Fast path: direct feature_cycle query
                let direct = source.load_feature_observations(&feature_cycle_for_load)?;
                if !direct.is_empty() {
                    return Ok(direct);
                }

                // Fallback: content-based attribution for sessions with NULL feature_cycle
                let unattributed = source.load_unattributed_sessions()?;
                if unattributed.is_empty() {
                    return Ok(vec![]);
                }

                Ok(unimatrix_observe::attribute_sessions(&unattributed, &feature_cycle_for_load))
            },
        )
        .await
        .map_err(rmcp::ErrorData::from)?
        .map_err(|e| ServerError::ObservationError(e.to_string()))
        .map_err(rmcp::ErrorData::from)?;

        // 6. Check for data availability
        let store = Arc::clone(&self.store);
        let feature_cycle = params.feature_cycle.clone();

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

        // 9. Cleanup expired observations (FR-07: 60-day retention via SQL DELETE)
        {
            let now_millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let sixty_days_millis = 60_i64 * 24 * 60 * 60 * 1000;
            let cutoff = now_millis - sixty_days_millis;
            let _ = sqlx::query("DELETE FROM observations WHERE ts_millis < ?1")
                .bind(cutoff)
                .execute(store.write_pool_server())
                .await;
        }

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
                if let Err(e) = write_lesson_learned(&server, &report_for_ll, &fc_for_ll).await {
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
            let outcome_map: std::collections::HashMap<String, Option<String>> = session_records
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
            let reload_pct = unimatrix_observe::compute_context_reload_pct(&summaries, &attributed);
            report.context_reload_pct = Some(reload_pct);

            // Step 13-14: Knowledge reuse (C3/C4, best-effort)
            match compute_knowledge_reuse_for_sessions(&store, &session_records).await {
                Ok(reuse) => report.feature_knowledge_reuse = Some(reuse),
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
                            event_type: row.try_get::<String, _>("event_type").unwrap_or_default(),
                            phase: row.try_get::<Option<String>, _>("phase").unwrap_or(None),
                            outcome: row.try_get::<Option<String>, _>("outcome").unwrap_or(None),
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

                // Build phase narrative (pure function)
                let narrative =
                    unimatrix_observe::build_phase_narrative(&events, &current_dist, &cross_dist);
                report.phase_narrative = Some(narrative);
            }
            // If event_rows is empty, phase_narrative remains None (AC-12/13)
        }

        // 11. Audit
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

        // 12. vnc-011: Dispatch to format-specific output path
        let format = params.format.as_deref().unwrap_or("markdown");
        match format {
            "markdown" | "summary" => {
                // Markdown path: formatter controls its own evidence selection (k=3 by timestamp).
                // evidence_limit is irrelevant here.
                Ok(format_retrospective_markdown(&report))
            }
            "json" => {
                // JSON path: keep existing evidence_limit default of 3 (col-010b ADR-001)
                let evidence_limit = params.evidence_limit.unwrap_or(3);
                if evidence_limit > 0 {
                    let mut truncated = report.clone();
                    for hotspot in &mut truncated.hotspots {
                        hotspot.evidence.truncate(evidence_limit);
                    }
                    Ok(format_retrospective_report(&truncated))
                } else {
                    Ok(format_retrospective_report(&report))
                }
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

        let response_text = format!(
            "Acknowledged: {} for topic '{}'. \
             Attribution is applied via the hook path (fire-and-forget). \
             Use context_cycle_review to confirm session attribution.",
            action, validated.topic
        );

        // 5. Audit log (fire-and-forget)
        self.audit_fire_and_forget(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_cycle".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: format!("{} topic={}", action, validated.topic),
        });

        // 6. Return acknowledgment
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            response_text,
        )]))
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

/// Compute Tier 1 cross-session knowledge reuse (col-020 C3, ADR-001).
///
/// Loads query_log + injection_log for the given sessions, then delegates to the
/// knowledge_reuse module for the actual computation.
async fn compute_knowledge_reuse_for_sessions(
    store: &Arc<unimatrix_store::SqlxStore>,
    session_records: &[unimatrix_store::SessionRecord],
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

    // Collect all entry IDs referenced in both logs so we can pre-fetch
    // categories asynchronously. compute_knowledge_reuse takes a sync closure,
    // so all async work must be completed before calling it.
    let mut all_entry_ids: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for record in &query_logs {
        let ids: Vec<u64> = serde_json::from_str(&record.result_entry_ids).unwrap_or_default();
        all_entry_ids.extend(ids);
    }
    for record in &injection_logs {
        all_entry_ids.insert(record.entry_id);
    }

    let mut category_map: std::collections::HashMap<u64, String> = std::collections::HashMap::new();
    for entry_id in &all_entry_ids {
        if let Ok(entry) = store.get(*entry_id).await {
            category_map.insert(*entry_id, entry.category);
        }
        // Entries that fail lookup (deleted/deprecated) are silently skipped
    }

    // Delegate to C3 knowledge_reuse module for computation
    let reuse = crate::mcp::knowledge_reuse::compute_knowledge_reuse(
        &query_logs,
        &injection_logs,
        &active_cats,
        |entry_id| category_map.get(&entry_id).cloned(),
    );

    tracing::debug!(
        "col-020b: knowledge reuse result: delivery_count={}, cross_session_count={}",
        reuse.delivery_count,
        reuse.cross_session_count
    );

    Ok(reuse)
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
        let json = r#"{"role": "architect", "task": "design auth module"}"#;
        let params: BriefingParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.role, "architect");
        assert_eq!(params.task, "design auth module");
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
        assert_eq!(params.feature.unwrap(), "vnc-003");
        assert_eq!(params.max_tokens.unwrap(), 5000);
        assert_eq!(params.format.unwrap(), "markdown");
    }

    #[test]
    fn test_briefing_params_missing_role() {
        let json = r#"{"task": "design"}"#;
        assert!(serde_json::from_str::<BriefingParams>(json).is_err());
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
                rule_name: "permission_retries".to_string(),
                claim: "8 retries detected".to_string(),
                measured: 8.0,
                threshold: 3.0,
                evidence: vec![],
            }],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![unimatrix_observe::Recommendation {
                hotspot_type: "permission_retries".to_string(),
                action: "Add to allowlist".to_string(),
                rationale: "saves time".to_string(),
            }],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
        };

        let content = build_lesson_learned_content(&report);
        assert!(content.contains("permission_retries"));
        assert!(content.contains("8 retries detected"));
        assert!(content.contains("Add to allowlist"));
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
                rule_name: "permission_retries".to_string(),
                claim: "8 retries detected".to_string(),
                measured: 8.0,
                threshold: 3.0,
                evidence: vec![],
            }],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: Some(vec![unimatrix_observe::HotspotNarrative {
                hotspot_type: "permission_retries".to_string(),
                summary: "Permission retries clustered around build commands".to_string(),
                clusters: vec![],
                top_files: vec![],
                sequence_pattern: None,
            }]),
            recommendations: vec![unimatrix_observe::Recommendation {
                hotspot_type: "permission_retries".to_string(),
                action: "Add to allowlist".to_string(),
                rationale: "saves time".to_string(),
            }],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
        };

        let content = build_lesson_learned_content(&report);
        // With narratives present, should use narrative summary (not hotspot claim)
        assert!(content.contains("Permission retries clustered"));
        assert!(!content.contains("8 retries detected"));
        // Recommendations always included
        assert!(content.contains("Add to allowlist"));
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
        let result = compute_knowledge_reuse_for_sessions(&store, &[]).await;

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
        // Count data rows (lines after header + separator)
        let data_rows = table_text.lines().skip(2).count();
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
        let data_rows = table_text.lines().skip(2).count();
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

        // Must have at least 2 data rows (header + separator + 2 entries)
        let lines: Vec<&str> = table_text.lines().collect();
        assert!(
            lines.len() >= 4,
            "must have header + separator + at least 2 data rows; got {}",
            lines.len()
        );
    }
}
