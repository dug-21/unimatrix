# vnc-009 Pseudocode Overview

## Component Interaction

```
MCP Transport                          UDS Transport
  tools.rs                               listener.rs
  +--> build_context()                   +--> handle_connection()
  |    - session_id from params          |    - session_id from peer
  |    - CallerId::Agent(agent_id)       |    - CallerId::UdsSession(sid)
  |    - prefix mcp::                    |    - prefix uds::
  |                                      |    - AuditLog for auth fail
  v                                      v
+---------------------------------------------------------+
|                    ServiceLayer                          |
|  SearchService::search(params, audit_ctx, &caller_id)   |
|    -> gateway.check_search_rate(&caller_id)             |
|  StoreService::insert(entry, emb, audit_ctx, &caller_id)|
|    -> gateway.check_write_rate(&caller_id)              |
|  BriefingService::assemble(params, audit_ctx, caller_id)|
|    -> if include_semantic: check_search_rate(&caller_id) |
|                                                         |
|  SecurityGateway                                        |
|    + RateLimiter { windows: Mutex<HashMap<CallerId,SW>> }|
|    + check_search_rate(&CallerId) -> Result<(),SE>      |
|    + check_write_rate(&CallerId) -> Result<(),SE>       |
|                                                         |
|  UsageService                                           |
|    + record_access(&[u64], AccessSource, UsageContext)   |
|    (fire-and-forget via spawn_blocking)                  |
|                                                         |
|  StatusService -> StatusReport -> StatusReportJson -> JSON|
+---------------------------------------------------------+
```

## Data Flow

### MCP Search with Session and Rate Limiting

1. `context_search(query, session_id: Some("abc"))` arrives
2. `build_context` resolves identity, constructs:
   - `CallerId::Agent(agent_id.clone())`
   - If session_id present: `audit_ctx.session_id = Some(prefix_session_id("mcp", sid))`
   - Validate session_id (S3: len <= 256, no control chars)
3. `SearchService::search(params, &audit_ctx, &caller_id)`
4. `gateway.check_search_rate(&caller_id)` -- UdsSession exempt, Agent checked
5. Normal search pipeline (embed, HNSW, rerank, boost)
6. Return results
7. `usage_service.record_access(entry_ids, AccessSource::McpTool, usage_ctx)` -- fire-and-forget

### UDS Injection with Usage

1. Hook sends inject request for session "sess-123"
2. `handle_connection` authenticates, constructs:
   - `CallerId::UdsSession("sess-123")`
   - `audit_ctx.session_id = Some(prefix_session_id("uds", "sess-123"))`
3. Search via SearchService (UdsSession exempt from rate limit)
4. `usage_service.record_access(entry_ids, AccessSource::HookInjection, usage_ctx)` -- fire-and-forget
   - Internally strips prefix before storage writes
   - Writes injection log, co-access pairs, feature entries

### Rate Limit Exceeded

1. Agent's 301st search call in 1 hour
2. `check_search_rate(CallerId::Agent("bot"))` -> sliding window full
3. Returns `ServiceError::RateLimited { limit: 300, window_secs: 3600, retry_after_secs: N }`
4. Mapped to rmcp::ErrorData by existing From impl

## Shared Types

### CallerId (services/mod.rs)

```
enum CallerId {
    Agent(String),
    UdsSession(String),
}
derives: Debug, Clone, PartialEq, Eq, Hash
```

### AccessSource (services/usage.rs)

```
enum AccessSource {
    McpTool,
    HookInjection,
    Briefing,
}
```

### UsageContext (services/usage.rs)

```
struct UsageContext {
    session_id: Option<String>,    // prefixed (mcp:: or uds::)
    agent_id: Option<String>,
    helpful: Option<bool>,
    feature_cycle: Option<String>,
    trust_level: Option<TrustLevel>,
}
```

### ServiceError::RateLimited (services/mod.rs)

```
RateLimited { limit: u32, window_secs: u64, retry_after_secs: u64 }
```

### Session ID Helpers (services/mod.rs)

```
fn prefix_session_id(transport: &str, raw: &str) -> String
fn strip_session_prefix(prefixed: &str) -> &str
```

## Integration Harness Plan

### Existing suites that apply

The project has integration tests embedded in server module test blocks (server.rs, listener.rs).
These are exercised via `cargo test --package unimatrix-server`.

### New integration tests needed

1. **UsageService regression**: Create entry, call `record_access(McpTool)`, verify store state
   matches what old `record_usage_for_entries` would produce. Same inputs, same outputs.
2. **Rate limit end-to-end**: Build ServiceLayer with rate limiter, call `search` 301 times
   through the service, verify 301st returns RateLimited.
3. **Session ID threading**: Build ToolContext with session_id, call search, verify AuditContext
   carries prefixed session_id through to audit event.
4. **StatusReport JSON snapshot**: Build StatusReport with known data, serialize through
   StatusReportJson, compare against golden JSON output from existing json! code path.
5. **UDS auth failure audit**: Simulate auth failure in handle_connection, verify AUDIT_LOG entry.

## Component Dependencies

```
usage-service  --depends-on-->  CallerId (from services/mod.rs)
rate-limiter   --depends-on-->  CallerId (from services/mod.rs)
session-aware-mcp  --depends-on-->  CallerId, prefix/strip helpers
status-serialize   --independent (no new type dependencies)
uds-auth-audit     --depends-on-->  AuditLog (existing)
```

## Patterns Used

- **Fire-and-forget spawn_blocking**: Same pattern as `record_usage_for_entries` in server.rs
  and `spawn_blocking_fire_and_forget` in listener.rs. Clone Arcs, move owned data into closure.
- **SecurityGateway injection**: Services hold `Arc<SecurityGateway>` and call methods directly.
  Rate limiting follows the same hybrid injection pattern as S1/S3/S4 checks.
- **ServiceLayer aggregation**: New service (UsageService) added as field, constructed in `new()`.
- **Permissive test gateway**: `SecurityGateway::new_permissive()` extended with permissive rate limits.
- **Mutex poison recovery**: `unwrap_or_else(|e| e.into_inner())` matching CategoryAllowlist pattern.
