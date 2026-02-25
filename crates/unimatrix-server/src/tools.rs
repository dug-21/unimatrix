//! MCP tool implementations: v0.1 (context_search, context_lookup, context_store, context_get)
//! and v0.2 (context_correct, context_deprecate, context_status, context_briefing).
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
    format_duplicate_found, format_lookup_results, format_search_results, format_single_entry,
    format_store_success, format_correct_success, format_deprecate_success,
    format_status_report, format_briefing, StatusReport, Briefing, parse_format,
};
use crate::scanning::ContentScanner;
use crate::server::UnimatrixServer;
use crate::validation::{
    validate_get_params, validate_lookup_params, validate_search_params, validate_store_params,
    validate_correct_params, validate_deprecate_params, validate_status_params,
    validate_briefing_params, validated_max_tokens,
    validated_id, validated_k, validated_limit, parse_status,
    validate_feature, validate_helpful,
};

/// HNSW search expansion factor.
const EF_SEARCH: usize = 32;

/// Near-duplicate cosine similarity threshold.
const DUPLICATE_THRESHOLD: f32 = 0.92;

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
        let embedding: Vec<f32> = tokio::task::spawn_blocking({
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

        // 9. Fetch full entries for results
        let mut results_with_scores = Vec::new();
        for sr in &search_results {
            match self.entry_store.get(sr.entry_id).await {
                Ok(entry) => results_with_scores.push((entry, sr.similarity)),
                Err(_) => continue, // silently skip deleted entries (FR-01g)
            }
        }

        // 9b. Re-rank by blended score: similarity * 0.85 + confidence * 0.15 (crt-002)
        results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)| {
            let score_a = crate::confidence::rerank_score(*sim_a, entry_a.confidence);
            let score_b = crate::confidence::rerank_score(*sim_b, entry_b.confidence);
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        // 10. Format response
        let result = format_search_results(&results_with_scores, format);

        // 11. Audit (standalone, best-effort)
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

        // 12. Usage recording (fire-and-forget)
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
        let embedding: Vec<f32> = tokio::task::spawn_blocking({
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
        let new_entry = NewEntry {
            title: title.clone(),
            content: params.content,
            topic: params.topic,
            category: params.category,
            tags: params.tags.unwrap_or_default(),
            source: params.source.unwrap_or_default(),
            status: Status::Active,
            created_by: identity.agent_id.clone(),
            feature_cycle: String::new(),
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
        Ok(format_store_success(&record, format))
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
        let embedding: Vec<f32> = tokio::task::spawn_blocking({
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

        // 5. Build report in a single read transaction (consistent snapshot)
        let store = Arc::clone(&self.store);
        let topic_filter = params.topic.clone();
        let category_filter = params.category.clone();

        let report = tokio::task::spawn_blocking(move || -> Result<StatusReport, crate::error::ServerError> {
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
            let entries_table = read_txn.open_table(ENTRIES)
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e.into())))?;
            let mut entries_with_supersedes = 0u64;
            let mut entries_with_superseded_by = 0u64;
            let mut total_correction_count = 0u64;
            let mut trust_source_dist: BTreeMap<String, u64> = BTreeMap::new();
            let mut entries_without_attribution = 0u64;

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
            }

            // 5e. Build StatusReport
            Ok(StatusReport {
                total_active,
                total_deprecated,
                total_proposed,
                category_distribution: category_distribution.into_iter().collect(),
                topic_distribution: topic_distribution.into_iter().collect(),
                entries_with_supersedes,
                entries_with_superseded_by,
                total_correction_count,
                trust_source_distribution: trust_source_dist.into_iter().collect(),
                entries_without_attribution,
            })
        }).await
        .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(CoreError::JoinError(e.to_string()))))?
        .map_err(rmcp::ErrorData::from)?;

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
                let embedding: Vec<f32> = tokio::task::spawn_blocking({
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

                let search_results = self
                    .vector_store
                    .search(embedding, 3, EF_SEARCH)
                    .await
                    .map_err(|e| rmcp::ErrorData::from(crate::error::ServerError::Core(e)))?;

                let mut results = Vec::new();
                for sr in &search_results {
                    if let Ok(entry) = self.entry_store.get(sr.entry_id).await {
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
}
