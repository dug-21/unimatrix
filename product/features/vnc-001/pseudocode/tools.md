# Pseudocode: tools.rs (C3 — Tool Stubs)

## Purpose

Defines the four MCP tool stubs with correct parameter schemas, descriptions, and annotations. Each stub resolves agent identity, logs an audit event with outcome NotImplemented, and returns a structured "not yet implemented" response.

## Types — Tool Parameter Structs

```
#[derive(Deserialize, JsonSchema)]
struct SearchParams {
    /// Natural language query for semantic search
    query: String,
    /// Filter by topic
    topic: Option<String>,
    /// Filter by category
    category: Option<String>,
    /// Filter by tags (all must match)
    tags: Option<Vec<String>>,
    /// Max results to return (default: 5)
    k: Option<i64>,
    /// Agent making the request
    agent_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct LookupParams {
    /// Filter by topic
    topic: Option<String>,
    /// Filter by category
    category: Option<String>,
    /// Filter by tags (all must match)
    tags: Option<Vec<String>>,
    /// Lookup by specific entry ID
    id: Option<i64>,
    /// Filter by status (active, deprecated, proposed)
    status: Option<String>,
    /// Max results to return (default: 10)
    limit: Option<i64>,
    /// Agent making the request
    agent_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct StoreParams {
    /// Content to store
    content: String,
    /// Topic for the entry
    topic: String,
    /// Category for the entry
    category: String,
    /// Tags for the entry
    tags: Option<Vec<String>>,
    /// Title for the entry
    title: Option<String>,
    /// Source identifier
    source: Option<String>,
    /// Agent making the request
    agent_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
struct GetParams {
    /// Entry ID to retrieve
    id: i64,
    /// Agent making the request
    agent_id: Option<String>,
}
```

## Tool Router Implementation

```
#[tool_router]
impl UnimatrixServer {

    #[tool(
        name = "context_search",
        description = "Search for relevant context using natural language. Returns semantically similar entries ranked by relevance. Use when you need to find patterns, conventions, or decisions related to a concept.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn context_search(&self, #[tool(aggr)] params: SearchParams) -> Result<CallToolResult, ErrorData> {
        // Resolve agent identity
        let identity = self.resolve_agent(&params.agent_id).await
            .map_err(ServerError::into)?;

        // [ENFORCEMENT POINT: vnc-002 capability check]
        // self.registry.require_capability(&identity.agent_id, Capability::Search)?;

        // [ENFORCEMENT POINT: vnc-002 input validation]
        // validate_search_params(&params)?;

        // Log audit event (best-effort — don't fail the request if audit fails)
        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,        // assigned by log_event
            timestamp: 0,       // assigned by log_event
            session_id: String::new(),  // TODO vnc-002: session tracking
            agent_id: identity.agent_id.clone(),
            operation: "context_search".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "Tool registered but not yet implemented (vnc-001 stub)".to_string(),
        });

        Ok(CallToolResult::success(vec![
            Content::text("Tool 'context_search' is registered but not yet implemented. Full implementation ships in vnc-002.")
        ]))
    }

    #[tool(
        name = "context_lookup",
        description = "Look up context entries by exact filters. Returns entries matching the specified topic, category, tags, status, or ID. Use when you know what you're looking for.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn context_lookup(&self, #[tool(aggr)] params: LookupParams) -> Result<CallToolResult, ErrorData> {
        let identity = self.resolve_agent(&params.agent_id).await
            .map_err(ServerError::into)?;

        // [ENFORCEMENT POINT: vnc-002 capability check]
        // [ENFORCEMENT POINT: vnc-002 input validation]

        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_lookup".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "Tool registered but not yet implemented (vnc-001 stub)".to_string(),
        });

        Ok(CallToolResult::success(vec![
            Content::text("Tool 'context_lookup' is registered but not yet implemented. Full implementation ships in vnc-002.")
        ]))
    }

    #[tool(
        name = "context_store",
        description = "Store a new context entry. Use to record patterns, conventions, architectural decisions, or other reusable knowledge discovered during work.",
        annotations(read_only_hint = false, destructive_hint = false)
    )]
    async fn context_store(&self, #[tool(aggr)] params: StoreParams) -> Result<CallToolResult, ErrorData> {
        let identity = self.resolve_agent(&params.agent_id).await
            .map_err(ServerError::into)?;

        // [ENFORCEMENT POINT: vnc-002 capability check — Write]
        // [ENFORCEMENT POINT: vnc-002 input validation]
        // [ENFORCEMENT POINT: vnc-002 content scanning]

        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_store".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "Tool registered but not yet implemented (vnc-001 stub)".to_string(),
        });

        Ok(CallToolResult::success(vec![
            Content::text("Tool 'context_store' is registered but not yet implemented. Full implementation ships in vnc-002.")
        ]))
    }

    #[tool(
        name = "context_get",
        description = "Get a specific context entry by its ID. Use when you have an entry ID from a previous search or lookup result.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn context_get(&self, #[tool(aggr)] params: GetParams) -> Result<CallToolResult, ErrorData> {
        let identity = self.resolve_agent(&params.agent_id).await
            .map_err(ServerError::into)?;

        // [ENFORCEMENT POINT: vnc-002 capability check — Read]
        // [ENFORCEMENT POINT: vnc-002 input validation]

        let _ = self.audit.log_event(AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: identity.agent_id.clone(),
            operation: "context_get".to_string(),
            target_ids: vec![],
            outcome: Outcome::NotImplemented,
            detail: "Tool registered but not yet implemented (vnc-001 stub)".to_string(),
        });

        Ok(CallToolResult::success(vec![
            Content::text("Tool 'context_get' is registered but not yet implemented. Full implementation ships in vnc-002.")
        ]))
    }
}
```

## Audit Event Pattern

Each tool stub follows the same pattern:
1. Resolve agent identity (extract + resolve_or_enroll + update_last_seen)
2. Enforcement point comments (for vnc-002)
3. Log audit event with Outcome::NotImplemented (best-effort, ignore errors)
4. Return stub CallToolResult

The `let _ = self.audit.log_event(...)` pattern intentionally discards audit failures per NFR-05 / FM-04 — audit write failure must not prevent tool responses.

## rmcp Types Used

- `CallToolResult` — tool response wrapper. `CallToolResult::success(content_vec)` for success.
- `Content` — response content. `Content::text(string)` for text content.
- `ErrorData` — MCP error. Has `code: i32`, `message: String`, `data: Option<Value>`.

## Key Test Scenarios

1. tools/list returns exactly 4 tools with correct names
2. context_search requires `query` parameter
3. context_store requires `content`, `topic`, `category` parameters
4. context_get requires `id` parameter
5. All tools have optional `agent_id` parameter
6. Tool descriptions match specification wording
7. Stub responses contain "not yet implemented" text
8. Audit events logged for each tool call with correct operation name
9. Agent identity resolved and threaded into audit event
10. Tool annotations: context_search and context_lookup have readOnlyHint=true
