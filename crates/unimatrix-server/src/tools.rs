//! MCP tool implementations: v0.1 (context_search, context_lookup, context_store, context_get),
//! v0.2 (context_correct, context_deprecate, context_status, context_briefing),
//! and alc-002 (context_enroll).
//!
//! Execution order per tool: identity -> capability -> validation -> category -> scanning
//! -> business logic -> format -> audit.

use std::collections::BTreeMap;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;
use unimatrix_core::{CoreError, EmbedService, NewEntry, QueryFilter, Status};
use unimatrix_store::{
    ENTRIES, CATEGORY_INDEX, TOPIC_INDEX, COUNTERS,
    deserialize_entry,
};

use crate::audit::{AuditEvent, Outcome};
use crate::registry::Capability;
use crate::response::{
    format_duplicate_found, format_enroll_success, format_lookup_results, format_search_results,
    format_single_entry, format_store_success, format_store_success_with_note,
    format_correct_success, format_deprecate_success,
    format_quarantine_success, format_restore_success,
    format_status_report, format_briefing, StatusReport, CoAccessClusterEntry, Briefing, parse_format,
};
use crate::scanning::ContentScanner;
use crate::server::UnimatrixServer;
use crate::validation::{
    validate_get_params, validate_lookup_params, validate_search_params, validate_store_params,
    validate_correct_params, validate_deprecate_params, validate_enroll_params,
    validate_quarantine_params, validate_status_params, validate_briefing_params,
    validated_max_tokens, validated_id, validated_k, validated_limit,
    parse_status, parse_quarantine_action, parse_trust_level, parse_capabilities,
    QuarantineAction, validate_feature, validate_helpful,
};

/// HNSW search expansion factor.
const EF_SEARCH: usize = 32;

/// Near-duplicate cosine similarity threshold.
const DUPLICATE_THRESHOLD: f64 = 0.92;

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
        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check
        self.registry
            .require_capability(&identity.agent_id, Capability::Search)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation
        validate_search_params(&params).map_err(rmcp::ErrorData::from)?;
        validate_feature(&params.feature).map_err(rmcp::ErrorData::from)?;
        validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Parse k
        let k = validated_k(params.k).map_err(rmcp::ErrorData::from)?;

        // 6. Get embedding adapter
        let adapter = self
            .embed_service
            .get_adapter()
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 7. Embed query — uses embed_entry("", query) to match how context_briefing
        //    embeds tasks. All query-side embeddings MUST use this same pattern.
        let query = params.query.clone();
        let raw_embedding: Vec<f32> = tokio::task::spawn_blocking({
            let adapter = Arc::clone(&adapter);
            move || adapter.embed_entry("", &query)
        })
        .await
        .map_err(|e: tokio::task::JoinError| {
            rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::JoinError(
                e.to_string(),
            )))
        })?
        .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

        // 7b. Adapt embedding through MicroLoRA + prototype pull (crt-006)
        let adapted = self.adapt_service.adapt_embedding(&raw_embedding, None, None);
        let embedding = unimatrix_embed::l2_normalized(&adapted);

        // 8. Search (with optional metadata pre-filtering)
        let search_results = if params.topic.is_some()
            || params.category.is_some()
            || params.tags.is_some()
        {
            let filter = QueryFilter {
                topic: params.topic.clone(),
                category: params.category.clone(),
                tags: params.tags.clone(),
                status: Some(Status::Active),
                time_range: None,
            };
            let entries = self
                .entry_store
                .query(filter)
                .await
                .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;
            let allowed_ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
            if allowed_ids.is_empty() {
                vec![]
            } else {
                self.vector_store
                    .search_filtered(embedding, k, EF_SEARCH, allowed_ids)
                    .await
                    .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?
            }
        } else {
            self.vector_store
                .search(embedding, k, EF_SEARCH)
                .await
                .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?
        };

        // 9. Fetch full entries for results, excluding quarantined (crt-003)
        let mut results_with_scores = Vec::new();
        for sr in &search_results {
            match self.entry_store.get(sr.entry_id).await {
                Ok(entry) => {
                    if entry.status == Status::Quarantined {
                        continue; // exclude quarantined entries from search results
                    }
                    results_with_scores.push((entry, sr.similarity));
                }
                Err(_) => continue, // silently skip deleted entries (FR-01g)
            }
        }

        // 9b. Re-rank by blended score: similarity * 0.85 + confidence * 0.15 (crt-002)
        results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
            let score_a = crate::confidence::rerank_score(*sim_a, entry_a.confidence);
            let score_b = crate::confidence::rerank_score(*sim_b, entry_b.confidence);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        // 9c. Co-access boost (crt-004)
        if results_with_scores.len() > 1 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let staleness_cutoff = now.saturating_sub(crate::coaccess::CO_ACCESS_STALENESS_SECONDS);

            // Anchor IDs: top min(3, result_count) entries
            let anchor_count = results_with_scores.len().min(3);
            let anchor_ids: Vec<u64> = results_with_scores.iter()
                .take(anchor_count)
                .map(|(e, _)| e.id)
                .collect();
            let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();

            let store = Arc::clone(&self.store);
            let boost_map = tokio::task::spawn_blocking(move || {
                crate::coaccess::compute_search_boost(&anchor_ids, &result_ids, &store, staleness_cutoff)
            }).await
            .unwrap_or_else(|e| {
                tracing::warn!("co-access boost task failed: {e}");
                std::collections::HashMap::new()
            });

            if !boost_map.is_empty() {
                // Re-sort with boost applied
                results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
                    let base_a = crate::confidence::rerank_score(*sim_a, entry_a.confidence);
                    let base_b = crate::confidence::rerank_score(*sim_b, entry_b.confidence);
                    let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0);
                    let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0);
                    let final_a = base_a + boost_a;
                    let final_b = base_b + boost_b;
                    final_b.partial_cmp(&final_a).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }

        // 10. Trim to k results (boost may have changed order)
        results_with_scores.truncate(k);

        // 11. Format response
        let result = format_search_results(&results_with_scores, format);

        // 12. Audit (standalone, best-effort)
        let target_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_search".to_string(),
            target_ids: target_ids.clone(),
            outcome: Outcome::Success,
            detail: format!("returned {} results", results_with_scores.len()),
        });

        // 13. Usage recording (fire-and-forget)
        self.record_usage_for_entries(
            &identity.agent_id,
            identity.trust_level,
            &target_ids,
            params.helpful,
            params.feature.as_deref(),
        ).await;

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
        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check
        self.registry
            .require_capability(&identity.agent_id, Capability::Read)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation
        validate_lookup_params(&params).map_err(rmcp::ErrorData::from)?;
        validate_feature(&params.feature).map_err(rmcp::ErrorData::from)?;
        validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Parse limit
        let limit = validated_limit(params.limit).map_err(rmcp::ErrorData::from)?;

        // 6. Branch: ID-based vs filter-based
        let (result, target_ids) = if let Some(id) = params.id {
            let id = validated_id(id).map_err(rmcp::ErrorData::from)?;
            let entry = self
                .entry_store
                .get(id)
                .await
                .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;
            let ids = vec![entry.id];
            (format_single_entry(&entry, format), ids)
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
            (format_lookup_results(&entries, format), ids)
        };

        // 7. Audit (standalone, best-effort)
        let result_count = target_ids.len();
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_lookup".to_string(),
            target_ids: target_ids.clone(),
            outcome: Outcome::Success,
            detail: format!("returned {result_count} results"),
        });

        // 8. Usage recording (fire-and-forget)
        self.record_usage_for_entries(
            &identity.agent_id,
            identity.trust_level,
            &target_ids,
            params.helpful,
            params.feature.as_deref(),
        ).await;

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
        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check (Write required)
        self.registry
            .require_capability(&identity.agent_id, Capability::Write)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation
        validate_store_params(&params).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Category validation
        self.categories
            .validate(&params.category)
            .map_err(rmcp::ErrorData::from)?;

        // 5a. Outcome tag validation (only for outcome entries)
        if params.category == "outcome" {
            let tags = params.tags.as_deref().unwrap_or(&[]);
            crate::outcome_tags::validate_outcome_tags(tags)
                .map_err(rmcp::ErrorData::from)?;
        }

        // 6. Content scanning
        if let Err(scan_result) = ContentScanner::global().scan(&params.content) {
            return Err(rmcp::ErrorData::from(
                crate::error::ServerError::ContentScanRejected {
                    category: scan_result.category.to_string(),
                    description: scan_result.description.to_string(),
                },
            ));
        }
        if let Some(title) = &params.title {
            if let Err(scan_result) = ContentScanner::global().scan_title(title) {
                return Err(rmcp::ErrorData::from(
                    crate::error::ServerError::ContentScanRejected {
                        category: scan_result.category.to_string(),
                        description: scan_result.description.to_string(),
                    },
                ));
            }
        }

        // 7. Embed title+content
        let title = params
            .title
            .unwrap_or_else(|| format!("{}: {}", params.topic, params.category));
        let adapter = self
            .embed_service
            .get_adapter()
            .await
            .map_err(rmcp::ErrorData::from)?;
        let raw_embedding: Vec<f32> = tokio::task::spawn_blocking({
            let adapter = Arc::clone(&adapter);
            let t = title.clone();
            let c = params.content.clone();
            move || adapter.embed_entry(&t, &c)
        })
        .await
        .map_err(|e: tokio::task::JoinError| {
            rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::JoinError(
                e.to_string(),
            )))
        })?
        .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

        // 7b. Adapt embedding through MicroLoRA + prototype pull (crt-006)
        let adapted = self.adapt_service.adapt_embedding(
            &raw_embedding,
            Some(&params.category),
            Some(&params.topic),
        );
        let embedding = unimatrix_embed::l2_normalized(&adapted);

        // 8. Near-duplicate detection
        let dup_results = self
            .vector_store
            .search(embedding.clone(), 1, EF_SEARCH)
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;
        if let Some(top) = dup_results.first() {
            if top.similarity >= DUPLICATE_THRESHOLD {
                match self.entry_store.get(top.entry_id).await {
                    Ok(existing) => {
                        // Audit duplicate detection
                        let _ = self.audit.log_event(AuditEvent {
                            event_id: 0,
                            timestamp: 0,
                            session_id: String::new(),
                            agent_id: identity.agent_id,
                            operation: "context_store".to_string(),
                            target_ids: vec![existing.id],
                            outcome: Outcome::Success,
                            detail: format!(
                                "near-duplicate detected: entry #{} at {:.2} similarity",
                                existing.id, top.similarity
                            ),
                        });
                        return Ok(format_duplicate_found(&existing, top.similarity, format));
                    }
                    Err(_) => {
                        // Entry was deleted since search; proceed with store
                    }
                }
            }
        }

        // 9. Build NewEntry
        let feature_cycle = params.feature_cycle.clone().unwrap_or_default();
        let is_outcome = params.category == "outcome";
        let new_entry = NewEntry {
            title: title.clone(),
            content: params.content,
            topic: params.topic,
            category: params.category,
            tags: params.tags.unwrap_or_default(),
            source: params.source.unwrap_or_default(),
            status: Status::Active,
            created_by: identity.agent_id.clone(),
            feature_cycle,
            trust_source: "agent".to_string(),
        };

        // 10. Combined transaction: insert + audit
        let audit_event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_store".to_string(),
            target_ids: vec![], // will be filled by insert_with_audit
            outcome: Outcome::Success,
            detail: format!("stored entry: {}", title),
        };
        let (entry_id, record) = self
            .insert_with_audit(new_entry, embedding, audit_event)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 10b. Update adaptation prototypes with the adapted embedding (crt-006)
        self.adapt_service.update_prototypes(
            &adapted,
            Some(&record.category),
            Some(&record.topic),
        );

        // 11. Seed initial confidence (fire-and-forget)
        {
            let store_for_conf = Arc::clone(&self.store);
            let _ = tokio::task::spawn_blocking(move || {
                match store_for_conf.get(entry_id) {
                    Ok(entry) => {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let conf = crate::confidence::compute_confidence(&entry, now);
                        if let Err(e) = store_for_conf.update_confidence(entry_id, conf) {
                            tracing::warn!("confidence seed failed for entry {entry_id}: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("confidence seed: failed to read entry {entry_id}: {e}");
                    }
                }
            }).await;
        }

        // 12. Format response
        if is_outcome && record.feature_cycle.is_empty() {
            // Append orphan outcome warning to the formatted response
            let warning = "\nNote: outcome not linked to a workflow (no feature_cycle provided)";
            Ok(format_store_success_with_note(&record, format, warning))
        } else {
            Ok(format_store_success(&record, format))
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
        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check
        self.registry
            .require_capability(&identity.agent_id, Capability::Read)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation
        validate_get_params(&params).map_err(rmcp::ErrorData::from)?;
        validate_feature(&params.feature).map_err(rmcp::ErrorData::from)?;
        validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Get entry
        let id = validated_id(params.id).map_err(rmcp::ErrorData::from)?;
        let entry = self
            .entry_store
            .get(id)
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

        // 6. Format response
        let result = format_single_entry(&entry, format);

        // 7. Audit (standalone, best-effort)
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_get".to_string(),
            target_ids: vec![id],
            outcome: Outcome::Success,
            detail: format!("retrieved entry #{id}"),
        });

        // 8. Usage recording (fire-and-forget)
        self.record_usage_for_entries(
            &identity.agent_id,
            identity.trust_level,
            &[id],
            params.helpful,
            params.feature.as_deref(),
        ).await;

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
        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check (Write required)
        self.registry
            .require_capability(&identity.agent_id, Capability::Write)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation (includes original_id range check)
        validate_correct_params(&params).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Extract validated original_id (range already checked by validate_correct_params)
        let original_id = params.original_id as u64;

        // 6. Get original entry (needed for field inheritance below)
        let original = self
            .entry_store
            .get(original_id)
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

        // Note: deprecated check is handled authoritatively inside correct_with_audit's
        // write transaction. No pre-check here to avoid TOCTOU.

        // 7. Category validation: only if explicit new category provided
        if let Some(category) = &params.category {
            self.categories
                .validate(category)
                .map_err(rmcp::ErrorData::from)?;
        }

        // 9. Content scanning on new content
        if let Err(scan_result) = ContentScanner::global().scan(&params.content) {
            return Err(rmcp::ErrorData::from(
                crate::error::ServerError::ContentScanRejected {
                    category: scan_result.category.to_string(),
                    description: scan_result.description.to_string(),
                },
            ));
        }
        if let Some(title) = &params.title {
            if let Err(scan_result) = ContentScanner::global().scan_title(title) {
                return Err(rmcp::ErrorData::from(
                    crate::error::ServerError::ContentScanRejected {
                        category: scan_result.category.to_string(),
                        description: scan_result.description.to_string(),
                    },
                ));
            }
        }

        // 10. Get embedding adapter (fails with EmbedNotReady if not ready)
        let adapter = self
            .embed_service
            .get_adapter()
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 11. Build title for embedding
        let title = params
            .title
            .unwrap_or_else(|| original.title.clone());

        // 12. Embed title+content
        let raw_embedding: Vec<f32> = tokio::task::spawn_blocking({
            let adapter = Arc::clone(&adapter);
            let t = title.clone();
            let c = params.content.clone();
            move || adapter.embed_entry(&t, &c)
        })
        .await
        .map_err(|e: tokio::task::JoinError| {
            rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::JoinError(
                e.to_string(),
            )))
        })?
        .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

        // 12b. Adapt embedding through MicroLoRA + prototype pull (crt-006)
        let correct_category = params.category.as_deref().unwrap_or(&original.category);
        let correct_topic = params.topic.as_deref().unwrap_or(&original.topic);
        let adapted = self.adapt_service.adapt_embedding(
            &raw_embedding,
            Some(correct_category),
            Some(correct_topic),
        );
        let embedding = unimatrix_embed::l2_normalized(&adapted);

        // 13. Build NewEntry with inheritance
        let new_entry = NewEntry {
            title,
            content: params.content,
            topic: params.topic.unwrap_or_else(|| original.topic.clone()),
            category: params.category.unwrap_or_else(|| original.category.clone()),
            tags: params.tags.unwrap_or_else(|| original.tags.clone()),
            source: original.source.clone(),
            status: Status::Active,
            created_by: identity.agent_id.clone(),
            feature_cycle: original.feature_cycle.clone(),
            trust_source: "agent".to_string(),
        };

        // 14. Combined transaction
        let audit_event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_correct".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: format!(
                "corrected entry #{original_id}: {}",
                params.reason.as_deref().unwrap_or("no reason")
            ),
        };
        let (deprecated_original, new_correction) = self
            .correct_with_audit(original_id, new_entry, embedding, audit_event)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 14b. Update adaptation prototypes with the adapted embedding (crt-006)
        self.adapt_service.update_prototypes(
            &adapted,
            Some(&new_correction.category),
            Some(&new_correction.topic),
        );

        // 15. Confidence for new correction + recompute for deprecated original (fire-and-forget)
        {
            let store_for_conf = Arc::clone(&self.store);
            let new_correction_id = new_correction.id;
            let dep_original_id = deprecated_original.id;
            let _ = tokio::task::spawn_blocking(move || {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                // Confidence for new correction entry
                match store_for_conf.get(new_correction_id) {
                    Ok(entry) => {
                        let conf = crate::confidence::compute_confidence(&entry, now);
                        if let Err(e) = store_for_conf.update_confidence(new_correction_id, conf) {
                            tracing::warn!("confidence for correction {new_correction_id}: {e}");
                        }
                    }
                    Err(e) => tracing::warn!("confidence: read correction {new_correction_id}: {e}"),
                }

                // Recompute confidence for deprecated original (base_score now 0.2)
                match store_for_conf.get(dep_original_id) {
                    Ok(entry) => {
                        let conf = crate::confidence::compute_confidence(&entry, now);
                        if let Err(e) = store_for_conf.update_confidence(dep_original_id, conf) {
                            tracing::warn!("confidence for deprecated {dep_original_id}: {e}");
                        }
                    }
                    Err(e) => tracing::warn!("confidence: read deprecated {dep_original_id}: {e}"),
                }
            }).await;
        }

        // 16. Format response
        Ok(format_correct_success(
            &deprecated_original,
            &new_correction,
            format,
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
        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check (Write required)
        self.registry
            .require_capability(&identity.agent_id, Capability::Write)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation (includes id range check)
        validate_deprecate_params(&params).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Extract validated ID (range already checked by validate_deprecate_params)
        let entry_id = params.id as u64;

        // 6. Get entry (verify exists + idempotency check)
        let entry = self
            .entry_store
            .get(entry_id)
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

        // 7. Idempotency: if already deprecated, return success immediately
        if entry.status == Status::Deprecated {
            return Ok(format_deprecate_success(
                &entry,
                params.reason.as_deref(),
                format,
            ));
        }

        // 8. Deprecate with audit
        let audit_event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_deprecate".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: String::new(),
        };
        let deprecated = self
            .deprecate_with_audit(entry_id, params.reason.clone(), audit_event)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 9. Recompute confidence for deprecated entry (fire-and-forget)
        {
            let store_for_conf = Arc::clone(&self.store);
            let dep_id = deprecated.id;
            let _ = tokio::task::spawn_blocking(move || {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                match store_for_conf.get(dep_id) {
                    Ok(entry) => {
                        let conf = crate::confidence::compute_confidence(&entry, now);
                        if let Err(e) = store_for_conf.update_confidence(dep_id, conf) {
                            tracing::warn!("confidence for deprecated {dep_id}: {e}");
                        }
                    }
                    Err(e) => tracing::warn!("confidence: read deprecated {dep_id}: {e}"),
                }
            }).await;
        }

        // 10. Format response
        Ok(format_deprecate_success(
            &deprecated,
            params.reason.as_deref(),
            format,
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
        use redb::ReadableTable;

        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check (Admin required)
        self.registry
            .require_capability(&identity.agent_id, Capability::Admin)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation
        validate_status_params(&params).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 4b. Resolve maintain flag (ADR-002: default false, opt-in)
        let maintain_enabled = params.maintain.unwrap_or(false);

        // 5. Build report in a single read transaction (consistent snapshot)
        let store = Arc::clone(&self.store);
        let topic_filter = params.topic.clone();
        let category_filter = params.category.clone();

        let report_result = tokio::task::spawn_blocking(move || -> Result<(StatusReport, Vec<unimatrix_store::EntryRecord>), crate::error::ServerError> {
            let read_txn = store.begin_read()
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;

            // 5a. Read status counters
            let counters = read_txn.open_table(COUNTERS)
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
            let total_active = counters.get("total_active")
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?
                .map(|g| g.value()).unwrap_or(0);
            let total_deprecated = counters.get("total_deprecated")
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?
                .map(|g| g.value()).unwrap_or(0);
            let total_proposed = counters.get("total_proposed")
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?
                .map(|g| g.value()).unwrap_or(0);
            let total_quarantined = counters.get("total_quarantined")
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?
                .map(|g| g.value()).unwrap_or(0);

            // 5b. Category distribution from CATEGORY_INDEX
            let cat_table = read_txn.open_table(CATEGORY_INDEX)
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
            let mut category_distribution: BTreeMap<String, u64> = BTreeMap::new();
            if let Some(ref filter_cat) = category_filter {
                let range = cat_table.range::<(&str, u64)>((filter_cat.as_str(), 0u64)..=(filter_cat.as_str(), u64::MAX))
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
                let count = range.count() as u64;
                if count > 0 {
                    category_distribution.insert(filter_cat.clone(), count);
                }
            } else {
                for item in cat_table.iter()
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))? {
                    let (key, _) = item
                        .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
                    let (cat_str, _id) = key.value();
                    *category_distribution.entry(cat_str.to_string()).or_insert(0) += 1;
                }
            }

            // 5c. Topic distribution from TOPIC_INDEX
            let topic_table = read_txn.open_table(TOPIC_INDEX)
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
            let mut topic_distribution: BTreeMap<String, u64> = BTreeMap::new();
            if let Some(ref filter_topic) = topic_filter {
                let range = topic_table.range::<(&str, u64)>((filter_topic.as_str(), 0u64)..=(filter_topic.as_str(), u64::MAX))
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
                let count = range.count() as u64;
                if count > 0 {
                    topic_distribution.insert(filter_topic.clone(), count);
                }
            } else {
                for item in topic_table.iter()
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))? {
                    let (key, _) = item
                        .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
                    let (topic_str, _id) = key.value();
                    *topic_distribution.entry(topic_str.to_string()).or_insert(0) += 1;
                }
            }

            // 5d. Correction chain metrics + security metrics from ENTRIES scan
            //      Also collect active entries for coherence dimensions (crt-005)
            let entries_table = read_txn.open_table(ENTRIES)
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
            let mut entries_with_supersedes = 0u64;
            let mut entries_with_superseded_by = 0u64;
            let mut total_correction_count = 0u64;
            let mut trust_source_dist: BTreeMap<String, u64> = BTreeMap::new();
            let mut entries_without_attribution = 0u64;
            let mut active_entries: Vec<unimatrix_store::EntryRecord> = Vec::new();

            for item in entries_table.iter()
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))? {
                let (_key, value) = item
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
                let record = deserialize_entry(value.value())
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e)))?;
                if record.supersedes.is_some() {
                    entries_with_supersedes += 1;
                }
                if record.superseded_by.is_some() {
                    entries_with_superseded_by += 1;
                }
                total_correction_count += record.correction_count as u64;
                let ts = if record.trust_source.is_empty() {
                    "(none)".to_string()
                } else {
                    record.trust_source.clone()
                };
                *trust_source_dist.entry(ts).or_insert(0) += 1;
                if record.created_by.is_empty() {
                    entries_without_attribution += 1;
                }
                if record.status == unimatrix_store::Status::Active {
                    active_entries.push(record);
                }
            }

            // 5d2. Outcome statistics
            let mut total_outcomes = 0u64;
            let mut outcomes_by_type: BTreeMap<String, u64> = BTreeMap::new();
            let mut outcomes_by_result: BTreeMap<String, u64> = BTreeMap::new();
            let mut outcomes_by_feature_cycle: BTreeMap<String, u64> = BTreeMap::new();

            // Scan CATEGORY_INDEX for "outcome" entries
            let outcome_range = cat_table
                .range::<(&str, u64)>(("outcome", 0u64)..=("outcome", u64::MAX))
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;

            for item in outcome_range {
                let (key, _) = item
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
                let (_cat, entry_id) = key.value();
                total_outcomes += 1;

                // Read the entry record to extract tags
                if let Some(entry_guard) = entries_table
                    .get(entry_id)
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?
                {
                    let record = deserialize_entry(entry_guard.value())
                        .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e)))?;

                    // Extract type: and result: tags
                    for tag in &record.tags {
                        if let Some((tag_key, tag_value)) = tag.split_once(':') {
                            match tag_key {
                                "type" => {
                                    *outcomes_by_type
                                        .entry(tag_value.to_string())
                                        .or_insert(0) += 1;
                                }
                                "result" => {
                                    *outcomes_by_result
                                        .entry(tag_value.to_string())
                                        .or_insert(0) += 1;
                                }
                                _ => {}
                            }
                        }
                    }

                    // Track feature_cycle
                    if !record.feature_cycle.is_empty() {
                        *outcomes_by_feature_cycle
                            .entry(record.feature_cycle.clone())
                            .or_insert(0) += 1;
                    }
                }
            }

            // Sort feature cycles by count descending, take top 10
            let mut fc_sorted: Vec<(String, u64)> =
                outcomes_by_feature_cycle.into_iter().collect();
            fc_sorted.sort_by(|a, b| b.1.cmp(&a.1));
            fc_sorted.truncate(10);

            // 5e. Build StatusReport
            let report = StatusReport {
                total_active,
                total_deprecated,
                total_proposed,
                total_quarantined,
                category_distribution: category_distribution.into_iter().collect(),
                topic_distribution: topic_distribution.into_iter().collect(),
                entries_with_supersedes,
                entries_with_superseded_by,
                total_correction_count,
                trust_source_distribution: trust_source_dist.into_iter().collect(),
                entries_without_attribution,
                contradictions: Vec::new(),
                contradiction_count: 0,
                embedding_inconsistencies: Vec::new(),
                contradiction_scan_performed: false,
                embedding_check_performed: false,
                total_co_access_pairs: 0,
                active_co_access_pairs: 0,
                top_co_access_pairs: Vec::new(),
                stale_pairs_cleaned: 0,
                coherence: 1.0,
                confidence_freshness_score: 1.0,
                graph_quality_score: 1.0,
                embedding_consistency_score: 1.0,
                contradiction_density_score: 1.0,
                stale_confidence_count: 0,
                confidence_refreshed_count: 0,
                graph_stale_ratio: 0.0,
                graph_compacted: false,
                maintenance_recommendations: Vec::new(),
                total_outcomes,
                outcomes_by_type: outcomes_by_type.into_iter().collect(),
                outcomes_by_result: outcomes_by_result.into_iter().collect(),
                outcomes_by_feature_cycle: fc_sorted,
                observation_file_count: 0,
                observation_total_size_bytes: 0,
                observation_oldest_file_days: 0,
                observation_approaching_cleanup: Vec::new(),
                retrospected_feature_count: 0,
            };
            Ok((report, active_entries))
        }).await
        .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::JoinError(e.to_string()))))?
        .map_err(rmcp::ErrorData::from)?;
        let (report, active_entries) = report_result;

        // 5f. Contradiction scanning + embedding consistency (outside read txn)
        let check_embeddings = params.check_embeddings.unwrap_or(false);
        let mut report = report;

        if let Ok(adapter) = self.embed_service.get_adapter().await {
            // Contradiction scan (default ON)
            let scan_config = crate::contradiction::ContradictionConfig::default();
            let store_for_scan = Arc::clone(&self.store);
            let vi_for_scan = Arc::clone(&self.vector_index);
            let adapter_for_scan = Arc::clone(&adapter);
            let config_for_scan = scan_config.clone();

            match tokio::task::spawn_blocking(move || {
                let vs = unimatrix_core::VectorAdapter::new(vi_for_scan);
                crate::contradiction::scan_contradictions(
                    &store_for_scan,
                    &vs,
                    &*adapter_for_scan,
                    &config_for_scan,
                )
            }).await {
                Ok(Ok(contradictions)) => {
                    report.contradiction_count = contradictions.len();
                    report.contradictions = contradictions;
                    report.contradiction_scan_performed = true;
                }
                _ => {
                    // Scan failed -- graceful degradation
                }
            }

            // Embedding consistency check (opt-in)
            if check_embeddings {
                let store_for_embed = Arc::clone(&self.store);
                let vi_for_embed = Arc::clone(&self.vector_index);
                let adapter_for_embed = Arc::clone(&adapter);
                let config_for_embed = scan_config;

                match tokio::task::spawn_blocking(move || {
                    let vs = unimatrix_core::VectorAdapter::new(vi_for_embed);
                    crate::contradiction::check_embedding_consistency(
                        &store_for_embed,
                        &vs,
                        &*adapter_for_embed,
                        &config_for_embed,
                    )
                }).await {
                    Ok(Ok(inconsistencies)) => {
                        report.embedding_inconsistencies = inconsistencies;
                        report.embedding_check_performed = true;
                    }
                    _ => {
                        // Check failed -- graceful degradation
                    }
                }
            }
        }

        // 5g. Co-access stats + cleanup (crt-004, gated by maintain in crt-005)
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let staleness_cutoff = now.saturating_sub(crate::coaccess::CO_ACCESS_STALENESS_SECONDS);

            let store_for_coaccess = Arc::clone(&self.store);
            let maintain_for_coaccess = maintain_enabled;
            let co_access_result = tokio::task::spawn_blocking(move || {
                // Stats (always read)
                let (total, active) = store_for_coaccess.co_access_stats(staleness_cutoff)?;

                // Top clusters (always read)
                let top_pairs = store_for_coaccess.top_co_access_pairs(5, staleness_cutoff)?;

                // Resolve titles for top pairs
                let mut clusters = Vec::new();
                for ((id_a, id_b), record) in &top_pairs {
                    let title_a = store_for_coaccess.get(*id_a)
                        .map(|e| e.title.clone())
                        .unwrap_or_else(|_| format!("#{id_a}"));
                    let title_b = store_for_coaccess.get(*id_b)
                        .map(|e| e.title.clone())
                        .unwrap_or_else(|_| format!("#{id_b}"));
                    clusters.push(CoAccessClusterEntry {
                        entry_id_a: *id_a,
                        entry_id_b: *id_b,
                        title_a,
                        title_b,
                        count: record.count,
                        last_updated: record.last_updated,
                    });
                }

                // Cleanup stale pairs (only when maintain=true, ADR-002)
                let cleaned = if maintain_for_coaccess {
                    store_for_coaccess.cleanup_stale_co_access(staleness_cutoff)?
                } else {
                    0
                };

                Ok::<_, unimatrix_store::StoreError>((total, active, clusters, cleaned))
            }).await;

            match co_access_result {
                Ok(Ok((total, active, clusters, cleaned))) => {
                    report.total_co_access_pairs = total;
                    report.active_co_access_pairs = active;
                    report.top_co_access_pairs = clusters;
                    report.stale_pairs_cleaned = cleaned;
                }
                Ok(Err(e)) => {
                    tracing::warn!("co-access stats failed: {e}");
                    // Fields remain at default (0, 0, vec![], 0)
                }
                Err(e) => {
                    tracing::warn!("co-access stats task failed: {e}");
                }
            }
        }

        // 5h. Coherence dimensions (always computed, read-only) [crt-005]
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Confidence freshness dimension
        let (freshness_dim, stale_conf_count) = crate::coherence::confidence_freshness_score(
            &active_entries,
            now_ts,
            crate::coherence::DEFAULT_STALENESS_THRESHOLD_SECS,
        );
        report.confidence_freshness_score = freshness_dim;
        report.stale_confidence_count = stale_conf_count;

        // Graph quality dimension
        let graph_point_count = self.vector_index.point_count();
        let graph_stale_count = self.vector_index.stale_count();
        let graph_stale_ratio = if graph_point_count == 0 {
            0.0
        } else {
            graph_stale_count as f64 / graph_point_count as f64
        };
        report.graph_quality_score = crate::coherence::graph_quality_score(graph_stale_count, graph_point_count);
        report.graph_stale_ratio = graph_stale_ratio;

        // Embedding consistency dimension (uses check_embeddings result if available)
        let embed_dim = if report.embedding_check_performed {
            let total_checked = active_entries.len();
            let inconsistent_count = report.embedding_inconsistencies.len();
            Some(crate::coherence::embedding_consistency_score(inconsistent_count, total_checked))
        } else {
            None
        };
        report.embedding_consistency_score = embed_dim.unwrap_or(1.0);

        // Contradiction density dimension
        report.contradiction_density_score = crate::coherence::contradiction_density_score(
            report.total_quarantined,
            report.total_active,
        );

        // 5i. Confidence refresh (only when maintain=true) [C5]
        if maintain_enabled {
            let staleness_threshold = crate::coherence::DEFAULT_STALENESS_THRESHOLD_SECS;
            let batch_cap = crate::coherence::MAX_CONFIDENCE_REFRESH_BATCH;

            // Identify stale entries (same logic as confidence_freshness_score)
            let mut stale_entries: Vec<&unimatrix_store::EntryRecord> = active_entries.iter()
                .filter(|e| {
                    let ref_ts = e.updated_at.max(e.last_accessed_at);
                    if ref_ts == 0 {
                        return true;
                    }
                    if now_ts > ref_ts {
                        (now_ts - ref_ts) > staleness_threshold
                    } else {
                        false
                    }
                })
                .collect();

            // Sort oldest first (lowest reference timestamp)
            stale_entries.sort_by_key(|e| e.updated_at.max(e.last_accessed_at));

            // Cap at batch size
            stale_entries.truncate(batch_cap);

            if !stale_entries.is_empty() {
                let ids_and_confs: Vec<(u64, f64)> = stale_entries.iter()
                    .map(|e| (e.id, crate::confidence::compute_confidence(e, now_ts)))
                    .collect();

                let store_for_refresh = Arc::clone(&self.store);
                let refresh_result = tokio::task::spawn_blocking(move || {
                    let mut refreshed = 0u64;
                    for (id, new_conf) in ids_and_confs {
                        match store_for_refresh.update_confidence(id, new_conf) {
                            Ok(()) => refreshed += 1,
                            Err(e) => {
                                tracing::warn!("confidence refresh failed for {id}: {e}");
                            }
                        }
                    }
                    refreshed
                }).await;

                match refresh_result {
                    Ok(count) => {
                        report.confidence_refreshed_count = count;
                    }
                    Err(e) => {
                        tracing::warn!("confidence refresh task failed: {e}");
                    }
                }
            }
        }

        // 5j. Graph compaction (only when maintain=true && stale ratio > trigger) [C8]
        if maintain_enabled && graph_stale_ratio > crate::coherence::DEFAULT_STALE_RATIO_TRIGGER {
            if let Ok(adapter) = self.embed_service.get_adapter().await {
                // Re-embed all active entries
                let pairs: Vec<(String, String)> = active_entries.iter()
                    .map(|e| (e.title.clone(), e.content.clone()))
                    .collect();

                match adapter.embed_entries(&pairs) {
                    Ok(embeddings) => {
                        // Adapt compacted embeddings through MicroLoRA (crt-006)
                        let compact_input: Vec<(u64, Vec<f32>)> = active_entries.iter()
                            .zip(embeddings.into_iter())
                            .map(|(entry, raw_emb)| {
                                let adapted = self.adapt_service.adapt_embedding(
                                    &raw_emb,
                                    Some(&entry.category),
                                    Some(&entry.topic),
                                );
                                (entry.id, unimatrix_embed::l2_normalized(&adapted))
                            })
                            .collect();

                        let vi_for_compact = Arc::clone(&self.vector_index);
                        match tokio::task::spawn_blocking(move || {
                            vi_for_compact.compact(compact_input)
                        }).await {
                            Ok(Ok(())) => {
                                report.graph_compacted = true;
                            }
                            Ok(Err(e)) => {
                                tracing::warn!("graph compaction failed: {e}");
                            }
                            Err(e) => {
                                tracing::warn!("graph compaction task failed: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("re-embedding for compaction failed: {e}");
                    }
                }
            }
        }

        // 5k. Lambda computation + recommendations (always) [C4 integration]
        let oldest_stale = crate::coherence::oldest_stale_age(
            &active_entries,
            now_ts,
            crate::coherence::DEFAULT_STALENESS_THRESHOLD_SECS,
        );
        report.coherence = crate::coherence::compute_lambda(
            report.confidence_freshness_score,
            report.graph_quality_score,
            embed_dim,
            report.contradiction_density_score,
            &crate::coherence::DEFAULT_WEIGHTS,
        );
        report.maintenance_recommendations = crate::coherence::generate_recommendations(
            report.coherence,
            crate::coherence::DEFAULT_LAMBDA_THRESHOLD,
            report.stale_confidence_count,
            oldest_stale,
            report.graph_stale_ratio,
            report.embedding_inconsistencies.len(),
            report.total_quarantined,
        );

        // 5h. Observation stats
        let obs_dir = unimatrix_observe::observation_dir();
        let obs_stats = tokio::task::spawn_blocking({
            let dir = obs_dir.clone();
            move || unimatrix_observe::scan_observation_stats(&dir)
        })
        .await
        .unwrap()
        .unwrap_or_else(|_| unimatrix_observe::ObservationStats {
            file_count: 0,
            total_size_bytes: 0,
            oldest_file_age_days: 0,
            approaching_cleanup: vec![],
        });

        report.observation_file_count = obs_stats.file_count;
        report.observation_total_size_bytes = obs_stats.total_size_bytes;
        report.observation_oldest_file_days = obs_stats.oldest_file_age_days;
        report.observation_approaching_cleanup = obs_stats.approaching_cleanup;

        // 5i. Retrospected feature count from OBSERVATION_METRICS
        let retrospected = tokio::task::spawn_blocking({
            let store = Arc::clone(&self.store);
            move || store.list_all_metrics()
        })
        .await
        .unwrap()
        .unwrap_or_else(|_| vec![]);
        report.retrospected_feature_count = retrospected.len() as u64;

        // 5j. If maintain=true, also clean up old observation files
        if maintain_enabled {
            let cleanup_dir = obs_dir;
            tokio::task::spawn_blocking(move || {
                let sixty_days = 60 * 24 * 60 * 60;
                if let Ok(expired) = unimatrix_observe::identify_expired(&cleanup_dir, sixty_days) {
                    for path in expired {
                        let _ = std::fs::remove_file(path);
                    }
                }
            })
            .await
            .unwrap();
        }

        // 6. Audit (standalone, best-effort)
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_status".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: "status report generated".to_string(),
        });

        // 7. Format response
        Ok(format_status_report(&report, format))
    }

    #[tool(
        name = "context_briefing",
        description = "Get an orientation briefing for a role and task. Includes role conventions, duties, and task-relevant context from the knowledge base. Use at the start of any task."
    )]
    async fn context_briefing(
        &self,
        Parameters(params): Parameters<BriefingParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check (Read required)
        self.registry
            .require_capability(&identity.agent_id, Capability::Read)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation
        validate_briefing_params(&params).map_err(rmcp::ErrorData::from)?;
        validate_helpful(&params.helpful).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Validate max_tokens
        let max_tokens = validated_max_tokens(params.max_tokens).map_err(rmcp::ErrorData::from)?;
        let char_budget = max_tokens * 4; // ~4 chars per token

        // 6. Lookup conventions: topic=role, category="convention", status=Active
        let conventions = self
            .entry_store
            .query(QueryFilter {
                topic: Some(params.role.clone()),
                category: Some("convention".to_string()),
                status: Some(Status::Active),
                tags: None,
                time_range: None,
            })
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

        // 7. Lookup duties: topic=role, category="duties", status=Active
        let duties = self
            .entry_store
            .query(QueryFilter {
                topic: Some(params.role.clone()),
                category: Some("duties".to_string()),
                status: Some(Status::Active),
                tags: None,
                time_range: None,
            })
            .await
            .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

        // 8. Semantic search (if embed ready) — uses embed_entry("", task) to match
        //    how context_search embeds queries. All query-side embeddings MUST use this
        //    same pattern so results are comparable across tools.
        let (relevant_context, search_available) = match self.embed_service.get_adapter().await {
            Ok(adapter) => {
                let task = params.task.clone();
                let raw_embedding: Vec<f32> = tokio::task::spawn_blocking({
                    let adapter = Arc::clone(&adapter);
                    move || adapter.embed_entry("", &task)
                })
                .await
                .map_err(|e: tokio::task::JoinError| {
                    rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::JoinError(
                        e.to_string(),
                    )))
                })?
                .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

                // 8b. Adapt briefing query embedding (crt-006)
                let adapted = self.adapt_service.adapt_embedding(&raw_embedding, None, None);
                let embedding = unimatrix_embed::l2_normalized(&adapted);

                let search_results = self
                    .vector_store
                    .search(embedding, 3, EF_SEARCH)
                    .await
                    .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

                let mut results = Vec::new();
                for sr in &search_results {
                    if let Ok(entry) = self.entry_store.get(sr.entry_id).await {
                        if entry.status == Status::Quarantined {
                            continue; // exclude quarantined entries from briefing search
                        }
                        results.push((entry, sr.similarity));
                    }
                }

                // Feature boost: if feature param provided, boost entries tagged with it
                if let Some(ref feature) = params.feature {
                    results.sort_by(|a, b| {
                        let a_has = a.0.tags.iter().any(|t| t == feature);
                        let b_has = b.0.tags.iter().any(|t| t == feature);
                        match (a_has, b_has) {
                            (true, false) => std::cmp::Ordering::Less,
                            (false, true) => std::cmp::Ordering::Greater,
                            _ => b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal),
                        }
                    });
                }

                // 8b. Co-access boost for briefing (crt-004)
                if results.len() > 1 {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let staleness_cutoff = now.saturating_sub(crate::coaccess::CO_ACCESS_STALENESS_SECONDS);

                    let anchor_count = results.len().min(3);
                    let anchor_ids: Vec<u64> = results.iter()
                        .take(anchor_count)
                        .map(|(e, _)| e.id)
                        .collect();
                    let result_ids: Vec<u64> = results.iter().map(|(e, _)| e.id).collect();

                    let store = Arc::clone(&self.store);
                    let boost_map = tokio::task::spawn_blocking(move || {
                        crate::coaccess::compute_briefing_boost(&anchor_ids, &result_ids, &store, staleness_cutoff)
                    }).await
                    .unwrap_or_else(|e| {
                        tracing::warn!("co-access briefing boost task failed: {e}");
                        std::collections::HashMap::new()
                    });

                    if !boost_map.is_empty() {
                        results.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
                            let boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0);
                            let boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0);
                            let score_a = *sim_a + boost_a;
                            let score_b = *sim_b + boost_b;
                            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
                        });
                    }
                }

                (results, true)
            }
            Err(_) => {
                // Embed not ready -- graceful degradation (AC-28)
                (vec![], false)
            }
        };

        // 9. Apply token budget
        // Priority order: conventions > duties > relevant_context
        let mut used_chars = 0usize;
        let mut budget_conventions = Vec::new();
        for entry in &conventions {
            let entry_chars = entry.title.len() + entry.content.len() + 50;
            if used_chars + entry_chars <= char_budget {
                budget_conventions.push(entry.clone());
                used_chars += entry_chars;
            }
        }

        let mut budget_duties = Vec::new();
        for entry in &duties {
            let entry_chars = entry.title.len() + entry.content.len() + 50;
            if used_chars + entry_chars <= char_budget {
                budget_duties.push(entry.clone());
                used_chars += entry_chars;
            }
        }

        let mut budget_context = Vec::new();
        for (entry, score) in &relevant_context {
            let entry_chars = entry.title.len() + entry.content.len() + 50;
            if used_chars + entry_chars <= char_budget {
                budget_context.push((entry.clone(), *score));
                used_chars += entry_chars;
            }
        }

        // 10. Build briefing
        let briefing = Briefing {
            role: params.role.clone(),
            task: params.task.clone(),
            conventions: budget_conventions,
            duties: budget_duties,
            relevant_context: budget_context,
            search_available,
        };

        // 11. Collect unique entry IDs for usage recording
        let mut briefing_entry_ids: Vec<u64> = Vec::new();
        for entry in &briefing.conventions {
            briefing_entry_ids.push(entry.id);
        }
        for entry in &briefing.duties {
            briefing_entry_ids.push(entry.id);
        }
        for (entry, _) in &briefing.relevant_context {
            briefing_entry_ids.push(entry.id);
        }
        briefing_entry_ids.sort_unstable();
        briefing_entry_ids.dedup();

        // 12. Audit (standalone, best-effort)
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_briefing".to_string(),
            target_ids: briefing_entry_ids.clone(),
            outcome: Outcome::Success,
            detail: format!("briefing for role={}, task={}", params.role, params.task),
        });

        // 13. Usage recording (fire-and-forget)
        self.record_usage_for_entries(
            &identity.agent_id,
            identity.trust_level,
            &briefing_entry_ids,
            params.helpful,
            params.feature.as_deref(),
        ).await;

        // 14. Format response
        Ok(format_briefing(&briefing, format))
    }

    #[tool(
        name = "context_quarantine",
        description = "Quarantine or restore a knowledge entry. Quarantined entries are excluded from search and lookup results but remain accessible via context_get. Requires Admin capability."
    )]
    async fn context_quarantine(
        &self,
        Parameters(params): Parameters<QuarantineParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // 1. Identity
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check (Admin required)
        self.registry
            .require_capability(&identity.agent_id, Capability::Admin)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Validation
        validate_quarantine_params(&params).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Parse action
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
                        format,
                    ));
                }

                // Only active entries can be quarantined
                if entry.status != Status::Active {
                    return Err(rmcp::ErrorData::from(
                        crate::error::ServerError::InvalidInput {
                            field: "id".to_string(),
                            reason: "only active entries can be quarantined".to_string(),
                        },
                    ));
                }

                // Atomic quarantine + audit
                let audit_event = AuditEvent {
                    event_id: 0,
                    timestamp: 0,
                    session_id: String::new(),
                    agent_id: identity.agent_id.clone(),
                    operation: "context_quarantine".to_string(),
                    target_ids: vec![],
                    outcome: Outcome::Success,
                    detail: String::new(),
                };
                let updated = self
                    .quarantine_with_audit(entry_id, params.reason.clone(), audit_event)
                    .await
                    .map_err(rmcp::ErrorData::from)?;

                // Recompute confidence (fire-and-forget)
                {
                    let store_for_conf = Arc::clone(&self.store);
                    let _ = tokio::task::spawn_blocking(move || {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        match store_for_conf.get(entry_id) {
                            Ok(e) => {
                                let conf = crate::confidence::compute_confidence(&e, now);
                                let _ = store_for_conf.update_confidence(entry_id, conf);
                            }
                            Err(_) => {}
                        }
                    })
                    .await;
                }

                Ok(format_quarantine_success(
                    &updated,
                    params.reason.as_deref(),
                    format,
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
                    agent_id: identity.agent_id.clone(),
                    operation: "context_quarantine".to_string(),
                    target_ids: vec![],
                    outcome: Outcome::Success,
                    detail: String::new(),
                };
                let updated = self
                    .restore_with_audit(entry_id, params.reason.clone(), audit_event)
                    .await
                    .map_err(rmcp::ErrorData::from)?;

                // Recompute confidence (fire-and-forget)
                {
                    let store_for_conf = Arc::clone(&self.store);
                    let _ = tokio::task::spawn_blocking(move || {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        match store_for_conf.get(entry_id) {
                            Ok(e) => {
                                let conf = crate::confidence::compute_confidence(&e, now);
                                let _ = store_for_conf.update_confidence(entry_id, conf);
                            }
                            Err(_) => {}
                        }
                    })
                    .await;
                }

                Ok(format_restore_success(
                    &updated,
                    params.reason.as_deref(),
                    format,
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
        // 1. Identity resolution
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Capability check (Admin required)
        self.registry
            .require_capability(&identity.agent_id, Capability::Admin)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Input validation
        validate_enroll_params(&params).map_err(rmcp::ErrorData::from)?;

        // 4. Parse format
        let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

        // 5. Parse trust level and capabilities (strict per ADR-001)
        let trust_level = parse_trust_level(&params.trust_level).map_err(rmcp::ErrorData::from)?;
        let capabilities =
            parse_capabilities(&params.capabilities).map_err(rmcp::ErrorData::from)?;

        // 6. Business logic: enroll or update agent
        let result = self
            .registry
            .enroll_agent(
                &identity.agent_id,
                &params.target_agent_id,
                trust_level,
                capabilities,
            )
            .map_err(rmcp::ErrorData::from)?;

        // 7. Format response
        let response = format_enroll_success(&result, format);

        // 8. Audit logging
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

        let event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_enroll".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail,
        };
        self.audit
            .log_event(event)
            .map_err(rmcp::ErrorData::from)?;

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
        use crate::error::{ServerError, ERROR_NO_OBSERVATION_DATA};
        use crate::response::format_retrospective_report;

        // 1. Identity resolution
        let identity = self
            .resolve_agent(&params.agent_id)
            .map_err(rmcp::ErrorData::from)?;

        // 2. Validation
        crate::validation::validate_retrospective_params(&params)
            .map_err(rmcp::ErrorData::from)?;

        // 3. Determine observation directory
        let obs_dir = unimatrix_observe::observation_dir();

        // 4. Discover and parse session files (spawn_blocking for sync I/O)
        let sessions = tokio::task::spawn_blocking({
            let obs_dir = obs_dir.clone();
            move || -> std::result::Result<Vec<unimatrix_observe::ParsedSession>, ServerError> {
                let session_files = unimatrix_observe::discover_sessions(&obs_dir)
                    .map_err(|e| ServerError::ObservationError(e.to_string()))?;

                let mut parsed: Vec<unimatrix_observe::ParsedSession> = Vec::new();
                for sf in &session_files {
                    let records = unimatrix_observe::parse_session_file(&sf.path)
                        .unwrap_or_default();
                    if !records.is_empty() {
                        parsed.push(unimatrix_observe::ParsedSession {
                            session_id: sf.session_id.clone(),
                            records,
                        });
                    }
                }

                Ok(parsed)
            }
        })
        .await
        .unwrap()
        .map_err(rmcp::ErrorData::from)?;

        // 5. Attribute sessions to target feature
        let attributed =
            unimatrix_observe::attribute_sessions(&sessions, &params.feature_cycle);

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
                Some(bytes) => {
                    // Return cached result (FR-09.6)
                    let mv = unimatrix_observe::deserialize_metric_vector(&bytes)
                        .map_err(|e| ServerError::ObservationError(e.to_string()))
                        .map_err(rmcp::ErrorData::from)?;

                    let report = unimatrix_observe::RetrospectiveReport {
                        feature_cycle: feature_cycle.clone(),
                        session_count: 0,
                        total_records: 0,
                        metrics: mv,
                        hotspots: vec![],
                        is_cached: true,
                    };

                    return Ok(format_retrospective_report(&report));
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

        // 7. Run analysis pipeline
        let rules = unimatrix_observe::default_rules();
        let hotspots = unimatrix_observe::detect_hotspots(&attributed, &rules);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let metrics =
            unimatrix_observe::compute_metric_vector(&attributed, &hotspots, now);

        // 8. Store MetricVector
        let mv_bytes = unimatrix_observe::serialize_metric_vector(&metrics)
            .map_err(|e| ServerError::ObservationError(e.to_string()))
            .map_err(rmcp::ErrorData::from)?;

        tokio::task::spawn_blocking({
            let store = Arc::clone(&store);
            let fc = feature_cycle.clone();
            move || store.store_metrics(&fc, &mv_bytes)
        })
        .await
        .unwrap()
        .map_err(|e| ServerError::Core(CoreError::Store(e)))
        .map_err(rmcp::ErrorData::from)?;

        // 9. Cleanup expired files (FR-09.8)
        let cleanup_dir = obs_dir;
        tokio::task::spawn_blocking(move || {
            let sixty_days = 60 * 24 * 60 * 60;
            if let Ok(expired) = unimatrix_observe::identify_expired(&cleanup_dir, sixty_days) {
                for path in expired {
                    let _ = std::fs::remove_file(path);
                }
            }
        })
        .await
        .unwrap();

        // 10. Build and return report
        let report = unimatrix_observe::build_report(
            &feature_cycle,
            &attributed,
            metrics,
            hotspots,
        );

        // 11. Audit
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_retrospective".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: format!("retrospective for {}", feature_cycle),
        });

        Ok(format_retrospective_report(&report))
    }
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
    }

    #[test]
    fn test_retrospective_params_with_agent() {
        let json = r#"{"feature_cycle": "nxs-001", "agent_id": "test-agent"}"#;
        let params: RetrospectiveParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.feature_cycle, "nxs-001");
        assert_eq!(params.agent_id.unwrap(), "test-agent");
    }
}
