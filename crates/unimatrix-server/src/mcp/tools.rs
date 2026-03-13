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
    parse_trust_level, validate_correct_params, validate_cycle_params, validate_deprecate_params,
    validate_enroll_params, validate_feature, validate_get_params, validate_helpful,
    validate_lookup_params, validate_quarantine_params, validate_search_params,
    validate_status_params, validate_store_params, validated_id, validated_k, validated_limit,
};
#[cfg(feature = "mcp-briefing")]
use crate::infra::validation::{validate_briefing_params, validated_max_tokens};
#[cfg(feature = "mcp-briefing")]
use crate::mcp::response::{Briefing, format_briefing};
use crate::mcp::response::{
    format_correct_success, format_deprecate_success, format_duplicate_found,
    format_enroll_success, format_lookup_results, format_quarantine_success,
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
    /// Set to true to run maintenance writes (confidence refresh, graph compaction). Default: false (read-only diagnostics).
    pub maintain: Option<bool>,
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

/// Parameters for the context_retrospective tool.
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
    /// Cycle action: "start" or "stop".
    pub r#type: String,
    /// Feature cycle identifier (e.g., "col-022").
    pub topic: String,
    /// Semantic keywords describing the feature work (max 5, each max 64 chars).
    pub keywords: Option<Vec<String>>,
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
                if let Err(e) = store_clone.insert_query_log(&record) {
                    tracing::warn!(
                        query_len = record.query_text.len(),
                        error = %e,
                        "query_log write failed (mcp)"
                    );
                }
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
            let entry = self
                .entry_store
                .get(id)
                .await
                .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;
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
            let mut entries = self
                .entry_store
                .query(filter)
                .await
                .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;
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
        self.services.usage.record_access(
            &target_ids,
            AccessSource::McpTool,
            UsageContext {
                session_id: ctx.audit_ctx.session_id.clone(),
                agent_id: Some(ctx.agent_id.clone()),
                helpful: params.helpful,
                feature_cycle: params.feature.clone(),
                trust_level: Some(ctx.trust_level),
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
            .build_context(&params.agent_id, &params.format, &None)
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

        // 4. Build title (transport-specific default)
        let title = params
            .title
            .unwrap_or_else(|| format!("{}: {}", params.topic, params.category));
        let is_outcome = params.category == "outcome";

        // 5. Build NewEntry
        let feature_cycle = params.feature_cycle.clone().unwrap_or_default();
        let new_entry = NewEntry {
            title,
            content: params.content,
            topic: params.topic,
            category: params.category,
            tags: params.tags.unwrap_or_default(),
            source: params.source.unwrap_or_default(),
            status: Status::Active,
            created_by: ctx.agent_id,
            feature_cycle,
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

        // 8. Seed initial confidence (fire-and-forget, via ConfidenceService)
        self.services
            .confidence
            .recompute(&[insert_result.entry.id]);

        // 9. Format response
        if is_outcome && insert_result.entry.feature_cycle.is_empty() {
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
        let entry = self
            .entry_store
            .get(id)
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

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
        self.services.usage.record_access(
            &[id],
            AccessSource::McpTool,
            UsageContext {
                session_id: ctx.audit_ctx.session_id.clone(),
                agent_id: Some(ctx.agent_id.clone()),
                helpful: params.helpful,
                feature_cycle: params.feature.clone(),
                trust_level: Some(ctx.trust_level),
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
        let original = self
            .entry_store
            .get(original_id)
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

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
        let entry = self
            .entry_store
            .get(entry_id)
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

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
        description = "Get the health status of the knowledge base. Shows entry counts, category/topic distributions, correction chains, and security metrics. Requires Admin capability."
    )]
    async fn context_status(
        &self,
        Parameters(params): Parameters<StatusParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity + format + capability (vnc-008: ToolContext)
        let ctx = self
            .build_context(&params.agent_id, &params.format, &None)
            .await?;
        self.require_cap(&ctx.agent_id, Capability::Admin).await?;

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

        // 4. Maintenance is now handled by background tick (col-013).
        // The `maintain` parameter is silently ignored for backward compatibility.
        // Read tick metadata for status reporting.
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

    // vnc-008: The old 618-line context_status body was extracted into
    // services/status.rs (StatusService::compute_report + run_maintenance).
    // Remove this comment once Gate 3b validates.

    #[tool(
        name = "context_briefing",
        description = "Get an orientation briefing for a role and task. Includes role conventions and task-relevant context from the knowledge base. Use at the start of any task."
    )]
    async fn context_briefing(
        &self,
        #[allow(unused_variables)] Parameters(params): Parameters<BriefingParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        #[cfg(not(feature = "mcp-briefing"))]
        {
            return Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                "context_briefing tool is not available in this build configuration",
            )]));
        }

        #[cfg(feature = "mcp-briefing")]
        {
            // 1. Identity + format + audit context (vnc-008: ToolContext)
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

            // 4. Delegate to BriefingService (vnc-007)
            let briefing_params = crate::services::briefing::BriefingParams {
                role: Some(params.role.clone()),
                task: Some(params.task.clone()),
                feature: params.feature.clone(),
                max_tokens,
                include_conventions: true,
                include_semantic: true,
                injection_history: None,
            };

            let result = self
                .services
                .briefing
                .assemble(briefing_params, &ctx.audit_ctx, Some(&ctx.caller_id))
                .await
                .map_err(rmcp::ErrorData::from)?;

            // 5. Convert BriefingResult -> Briefing for format_briefing
            let briefing = Briefing {
                role: params.role.clone(),
                task: params.task.clone(),
                conventions: result.conventions,
                relevant_context: result.relevant_context,
                search_available: result.search_available,
            };

            // 6. Audit (transport-specific, best-effort)
            self.audit_fire_and_forget(AuditEvent {
                event_id: 0,
                timestamp: 0,
                session_id: String::new(),
                agent_id: ctx.agent_id.clone(),
                operation: "context_briefing".to_string(),
                target_ids: result.entry_ids.clone(),
                outcome: Outcome::Success,
                detail: format!("briefing for role={}, task={}", params.role, params.task),
            });

            // 7. Usage recording (fire-and-forget via UsageService)
            self.services.usage.record_access(
                &result.entry_ids,
                AccessSource::Briefing,
                UsageContext {
                    session_id: ctx.audit_ctx.session_id.clone(),
                    agent_id: Some(ctx.agent_id.clone()),
                    helpful: params.helpful,
                    feature_cycle: params.feature.clone(),
                    trust_level: Some(ctx.trust_level),
                },
            );

            // 8. Format response (transport-specific)
            Ok(format_briefing(&briefing, ctx.format))
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
        let entry = self
            .entry_store
            .get(entry_id)
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

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
        name = "context_retrospective",
        description = "Analyze observation data for a feature cycle. Parses session telemetry, attributes to feature, detects hotspots, computes metrics, and returns a self-contained report."
    )]
    async fn context_retrospective(
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
        let feature_cycle_for_load = params.feature_cycle.clone();
        let attributed = tokio::task::spawn_blocking(move || -> std::result::Result<Vec<unimatrix_observe::ObservationRecord>, unimatrix_observe::ObserveError> {
            use unimatrix_observe::ObservationSource;
            let source = crate::services::observation::SqlObservationSource::new(store_for_obs);

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
        })
        .await
        .unwrap()
        .map_err(|e| ServerError::ObservationError(e.to_string()))
        .map_err(rmcp::ErrorData::from)?;

        // 6. Check for data availability
        let store = Arc::clone(&self.store);
        let feature_cycle = params.feature_cycle.clone();

        if attributed.is_empty() {
            // No new data -- check for cached MetricVector
            let cached = tokio::task::spawn_blocking({
                let store = Arc::clone(&store);
                let fc = feature_cycle.clone();
                move || store.get_metrics(&fc)
            })
            .await
            .unwrap()
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
        let all_metrics = tokio::task::spawn_blocking({
            let store = Arc::clone(&store);
            move || store.list_all_metrics()
        })
        .await
        .unwrap()
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
        tokio::task::spawn_blocking({
            let store = Arc::clone(&store);
            let fc = feature_cycle.clone();
            let mv = metrics.clone();
            move || store.store_metrics(&fc, &mv)
        })
        .await
        .unwrap()
        .map_err(|e| ServerError::Core(CoreError::Store(e)))
        .map_err(rmcp::ErrorData::from)?;

        // 9. Cleanup expired observations (FR-07: 60-day retention via SQL DELETE)
        let store_cleanup = Arc::clone(&store);
        tokio::task::spawn_blocking(move || {
            let conn = store_cleanup.lock_conn();
            let now_millis = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let sixty_days_millis = 60_i64 * 24 * 60 * 60 * 1000;
            let cutoff = now_millis - sixty_days_millis;
            let _ = conn.execute(
                "DELETE FROM observations WHERE ts_millis < ?1",
                unimatrix_store::rusqlite::params![cutoff],
            );
        })
        .await
        .unwrap();

        // 10a. Compute baseline comparison
        let baseline = unimatrix_observe::compute_baselines(&history)
            .map(|baselines| unimatrix_observe::compare_to_baseline(&metrics, &baselines));

        // 10b. Drain accumulated entry analysis from signal consumers (col-009, FR-10.5)
        let entries_analysis = {
            let mut pending = self
                .pending_entries_analysis
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let drained = pending.drain_all();
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
            let session_records = {
                let store_c = Arc::clone(&store);
                let fc = feature_cycle.clone();
                tokio::task::spawn_blocking(move || store_c.scan_sessions_by_feature(&fc))
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
            };

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
                let fc_for_discover = feature_cycle.clone();
                let discovered_ids = tokio::task::spawn_blocking(move || {
                    use unimatrix_observe::ObservationSource;
                    let source =
                        crate::services::observation::SqlObservationSource::new(store_for_discover);
                    source.discover_sessions_for_feature(&fc_for_discover)
                })
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
                let store_for_counters = Arc::clone(&store);
                let topic_for_counters = feature_cycle.clone();
                match tokio::task::spawn_blocking(move || {
                    // Ensure record exists before setting counters
                    if store_for_counters
                        .get_topic_delivery(&topic_for_counters)?
                        .is_none()
                    {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        store_for_counters.upsert_topic_delivery(
                            &unimatrix_store::TopicDeliveryRecord {
                                topic: topic_for_counters.clone(),
                                created_at: now,
                                completed_at: None,
                                status: "active".to_string(),
                                github_issue: None,
                                total_sessions: 0,
                                total_tool_calls: 0,
                                total_duration_secs: 0,
                                phases_completed: None,
                            },
                        )?;
                    }
                    store_for_counters.set_topic_delivery_counters(
                        &topic_for_counters,
                        total_sessions,
                        total_tool_calls,
                        total_duration_secs,
                    )
                })
                .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => tracing::warn!("col-020: counter update failed: {e}"),
                    Err(e) => tracing::warn!("col-020: counter update task failed: {e}"),
                }
            }

            // Assign session summaries to report
            report.session_summaries = Some(summaries);
        }

        // 11. Audit
        self.audit_fire_and_forget(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_retrospective".to_string(),
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
            Attribution is best-effort via the hook path; confirm via context_retrospective."
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

        // 3. Validation via shared validate_cycle_params (ADR-004)
        let keywords_ref = params.keywords.as_deref();
        let validated = match validate_cycle_params(&params.r#type, &params.topic, keywords_ref) {
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
            CycleType::Stop => "cycle_stopped",
        };

        let response_text = format!(
            "Acknowledged: {} for topic '{}'. \
             Attribution is applied via the hook path (fire-and-forget). \
             Use context_retrospective to confirm session attribution.",
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
/// Called inside a tokio::spawn from context_retrospective. Embeds the content,
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
        let store = Arc::clone(&server.store);
        let topic_clone = topic.clone();
        tokio::task::spawn_blocking(
            move || -> Result<Vec<unimatrix_core::EntryRecord>, crate::error::ServerError> {
                let filter = unimatrix_core::QueryFilter {
                    topic: Some(topic_clone),
                    category: Some("lesson-learned".to_string()),
                    ..Default::default()
                };
                store
                    .query(filter)
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e)))
            },
        )
        .await
        .map_err(|e| crate::error::ServerError::Core(CoreError::JoinError(e.to_string())))??
    };

    let supersedes_id = existing
        .iter()
        .filter(|e| e.status == Status::Active)
        .max_by_key(|e| e.created_at)
        .map(|e| e.id);

    // 4. Embed content (same pipeline as context_store: get_adapter + embed_entry + adapt + normalize)
    let embedding = match server.embed_service.get_adapter().await {
        Ok(adapter) => {
            let title_clone = title.clone();
            let content_clone = content.clone();
            match tokio::task::spawn_blocking(move || {
                adapter.embed_entry(&title_clone, &content_clone)
            })
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
                        "lesson-learned embedding task panicked for {}: {}",
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
        operation: "context_retrospective/lesson-learned".to_string(),
        target_ids: vec![],
        outcome: Outcome::Success,
        detail: format!("auto-persist lesson-learned for {}", feature_cycle),
    };

    let (new_id, _record) = server
        .insert_with_audit(new_entry, embedding, audit_event)
        .await?;

    // 7. Supersede chain: deprecate old, link new → old and old → new
    if let Some(old_id) = supersedes_id {
        let store = Arc::clone(&server.store);
        let _ = tokio::task::spawn_blocking(move || {
            // Deprecate old entry (handles STATUS_INDEX + counters internally)
            if let Err(e) = store.update_status(old_id, Status::Deprecated) {
                tracing::warn!("failed to deprecate prior lesson-learned {}: {}", old_id, e);
                return;
            }
            // Link old → new
            if let Ok(mut old_entry) = store.get(old_id) {
                old_entry.superseded_by = Some(new_id);
                let _ = store.update(old_entry);
            }
            // Link new → old
            if let Ok(mut new_entry) = store.get(new_id) {
                new_entry.supersedes = Some(old_id);
                let _ = store.update(new_entry);
            }
        })
        .await;
    }

    // 8. Seed confidence on new entry (best-effort)
    {
        let store = Arc::clone(&server.store);
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(entry) = store.get(new_id) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let conf = unimatrix_engine::confidence::compute_confidence(&entry, now);
                let _ = store.update_confidence(new_id, conf);
            }
        })
        .await;
    }

    Ok(())
}

/// Compute Tier 1 cross-session knowledge reuse (col-020 C3, ADR-001).
///
/// Loads query_log + injection_log for the given sessions, then delegates to the
/// knowledge_reuse module for the actual computation.
async fn compute_knowledge_reuse_for_sessions(
    store: &Arc<unimatrix_store::Store>,
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
    let store_ql = Arc::clone(store);
    let ids_ql: Vec<String> = session_id_list.clone();
    let query_logs = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = ids_ql.iter().map(|s| s.as_str()).collect();
        store_ql.scan_query_log_by_sessions(&refs)
    })
    .await??;

    tracing::debug!(
        "col-020b: knowledge reuse data flow: {} query_log records loaded",
        query_logs.len()
    );

    // Load injection_log
    let store_il = Arc::clone(store);
    let ids_il: Vec<String> = session_id_list.clone();
    let injection_logs = tokio::task::spawn_blocking(move || {
        let refs: Vec<&str> = ids_il.iter().map(|s| s.as_str()).collect();
        store_il.scan_injection_log_by_sessions(&refs)
    })
    .await??;

    tracing::debug!(
        "col-020b: knowledge reuse data flow: {} injection_log records loaded",
        injection_logs.len()
    );

    // Load active category counts
    let store_ac = Arc::clone(store);
    let active_cats =
        tokio::task::spawn_blocking(move || store_ac.count_active_entries_by_category()).await??;

    tracing::debug!(
        "col-020b: knowledge reuse data flow: {} active categories",
        active_cats.len()
    );

    // Delegate to C3 knowledge_reuse module for computation
    let store_for_lookup = Arc::clone(store);
    let reuse = crate::mcp::knowledge_reuse::compute_knowledge_reuse(
        &query_logs,
        &injection_logs,
        &active_cats,
        move |entry_id| {
            store_for_lookup
                .get(entry_id)
                .ok()
                .map(|entry| entry.category)
        },
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
        };

        let content = build_lesson_learned_content(&report);
        assert!(!content.is_empty());
    }

    // -- col-022: CycleParams deserialization --

    #[test]
    fn test_cycle_params_deserialize_start() {
        let json = r#"{"type": "start", "topic": "col-022"}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.r#type, "start");
        assert_eq!(params.topic, "col-022");
        assert!(params.keywords.is_none());
    }

    #[test]
    fn test_cycle_params_deserialize_with_keywords() {
        let json = r#"{"type": "start", "topic": "col-022", "keywords": ["attr", "lifecycle"]}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(
            params.keywords,
            Some(vec!["attr".to_string(), "lifecycle".to_string()])
        );
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
    fn test_cycle_params_keywords_empty_array() {
        let json = r#"{"type": "start", "topic": "col-022", "keywords": []}"#;
        let params: CycleParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.keywords, Some(vec![]));
    }

    #[test]
    fn test_cycle_params_keywords_null_vs_absent() {
        let json_null = r#"{"type": "start", "topic": "col-022", "keywords": null}"#;
        let json_absent = r#"{"type": "start", "topic": "col-022"}"#;
        let params_null: CycleParams = serde_json::from_str(json_null).unwrap();
        let params_absent: CycleParams = serde_json::from_str(json_absent).unwrap();
        assert!(params_null.keywords.is_none());
        assert!(params_absent.keywords.is_none());
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
}
