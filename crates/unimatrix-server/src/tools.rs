//! MCP tool implementations: context_search, context_lookup, context_store, context_get.
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

use crate::audit::{AuditEvent, Outcome};
use crate::registry::Capability;
use crate::response::{
    format_duplicate_found, format_lookup_results, format_search_results, format_single_entry,
    format_store_success, parse_format,
};
use crate::scanning::ContentScanner;
use crate::server::UnimatrixServer;
use crate::validation::{
    validate_get_params, validate_lookup_params, validate_search_params, validate_store_params,
    validated_id, validated_k, validated_limit, parse_status,
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

        // 7. Embed query
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

        // 10. Format response
        let result = format_search_results(&results_with_scores, format);

        // 11. Audit (standalone, best-effort)
        let target_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_search".to_string(),
            target_ids,
            outcome: Outcome::Success,
            detail: format!("returned {} results", results_with_scores.len()),
        });

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
            agent_id: identity.agent_id,
            operation: "context_lookup".to_string(),
            target_ids,
            outcome: Outcome::Success,
            detail: format!("returned {result_count} results"),
        });

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
        let (_entry_id, record) = self
            .insert_with_audit(new_entry, embedding, audit_event)
            .await
            .map_err(rmcp::ErrorData::from)?;

        // 11. Format response
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
            agent_id: identity.agent_id,
            operation: "context_get".to_string(),
            target_ids: vec![id],
            outcome: Outcome::Success,
            detail: format!("retrieved entry #{id}"),
        });

        Ok(result)
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
            "format": "json"
        }"#;
        let params: SearchParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.query, "test");
        assert_eq!(params.topic.unwrap(), "auth");
        assert_eq!(params.k.unwrap(), 10);
        assert_eq!(params.format.unwrap(), "json");
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
}
