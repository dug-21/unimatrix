//! MCP tool stubs with parameter schemas and audit logging.
//!
//! vnc-001 provides stubs; vnc-002 replaces them with real implementations.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content};
use rmcp::tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::audit::{AuditEvent, Outcome};
use crate::server::UnimatrixServer;

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
}

/// Parameters for getting an entry by ID.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetParams {
    /// Entry ID to retrieve.
    pub id: i64,
    /// Agent making the request.
    pub agent_id: Option<String>,
}

#[rmcp::tool_router(vis = "pub(crate)")]
impl UnimatrixServer {
    #[tool(
        name = "context_search",
        description = "Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. Use when you need to find patterns, conventions, or decisions related to a concept."
    )]
    fn context_search(
        &self,
        Parameters(params): Parameters<SearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let identity = self.resolve_agent(&params.agent_id).map_err(rmcp::ErrorData::from)?;

        // [ENFORCEMENT POINT: vnc-002 capability check]
        // self.registry.require_capability(&identity.agent_id, Capability::Search)?;

        // [ENFORCEMENT POINT: vnc-002 input validation]
        // validate_search_params(&params)?;

        // Log audit event (best-effort)
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_search".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "Tool registered but not yet implemented (vnc-001 stub)".to_string(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            "Tool 'context_search' is registered but not yet implemented. Full implementation ships in vnc-002.",
        )]))
    }

    #[tool(
        name = "context_lookup",
        description = "Look up context entries by exact filters. Returns entries matching the specified topic, category, tags, status, or ID. Use when you know what you are looking for."
    )]
    fn context_lookup(
        &self,
        Parameters(params): Parameters<LookupParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let identity = self.resolve_agent(&params.agent_id).map_err(rmcp::ErrorData::from)?;

        // [ENFORCEMENT POINT: vnc-002 capability check]
        // [ENFORCEMENT POINT: vnc-002 input validation]

        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_lookup".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "Tool registered but not yet implemented (vnc-001 stub)".to_string(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            "Tool 'context_lookup' is registered but not yet implemented. Full implementation ships in vnc-002.",
        )]))
    }

    #[tool(
        name = "context_store",
        description = "Store a new context entry. Use to record patterns, conventions, architectural decisions, or other reusable knowledge discovered during work."
    )]
    fn context_store(
        &self,
        Parameters(params): Parameters<StoreParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let identity = self.resolve_agent(&params.agent_id).map_err(rmcp::ErrorData::from)?;

        // [ENFORCEMENT POINT: vnc-002 capability check -- Write]
        // [ENFORCEMENT POINT: vnc-002 input validation]
        // [ENFORCEMENT POINT: vnc-002 content scanning]

        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_store".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "Tool registered but not yet implemented (vnc-001 stub)".to_string(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            "Tool 'context_store' is registered but not yet implemented. Full implementation ships in vnc-002.",
        )]))
    }

    #[tool(
        name = "context_get",
        description = "Get a specific context entry by its ID. Use when you have an entry ID from a previous search or lookup result."
    )]
    fn context_get(
        &self,
        Parameters(params): Parameters<GetParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let identity = self.resolve_agent(&params.agent_id).map_err(rmcp::ErrorData::from)?;

        // [ENFORCEMENT POINT: vnc-002 capability check -- Read]
        // [ENFORCEMENT POINT: vnc-002 input validation]

        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id,
            operation: "context_get".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "Tool registered but not yet implemented (vnc-001 stub)".to_string(),
        });

        Ok(CallToolResult::success(vec![Content::text(
            "Tool 'context_get' is registered but not yet implemented. Full implementation ships in vnc-002.",
        )]))
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
    }

    #[test]
    fn test_search_params_all_fields() {
        let json = r#"{
            "query": "test",
            "topic": "auth",
            "category": "convention",
            "tags": ["rust"],
            "k": 10,
            "agent_id": "test-agent"
        }"#;
        let params: SearchParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.query, "test");
        assert_eq!(params.topic.unwrap(), "auth");
        assert_eq!(params.k.unwrap(), 10);
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
}
