# vnc-009: Cross-Path Convergence — Architecture

## System Overview

vnc-009 is Wave 4 (final) of the server refactoring series. It closes the remaining convergence gaps between the MCP and UDS transports by introducing three new service-layer components (UsageService, RateLimiter, CallerId) and modifying four existing components (SecurityGateway, ToolContext, StatusReport, UDS listener).

All changes are confined to `crates/unimatrix-server/`. No new crates, no schema changes, no wire protocol changes.

### Architectural Context

```
                  MCP Transport (stdio/rmcp)        UDS Transport (unix socket)
                  ┌──────────────────────┐           ┌──────────────────────┐
                  │  tools.rs            │           │  listener.rs         │
                  │  ToolContext         │           │  hook.rs             │
                  │  + session_id        │           │  + AuditLog (auth)   │
                  │  + CallerId::Agent   │           │  + CallerId::Uds     │
                  └────────┬─────────────┘           └────────┬─────────────┘
                           │                                  │
                           ▼                                  ▼
              ┌─────────────────────────────────────────────────────────┐
              │                    ServiceLayer                         │
              │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
              │  │SearchService │  │ StoreService  │  │BriefingService│  │
              │  │+rate check   │  │+rate check    │  │+rate check   │  │
              │  └──────┬───────┘  └──────┬────────┘  └──────┬───────┘  │
              │         │                 │                   │          │
              │  ┌──────┴─────────────────┴───────────────────┘          │
              │  │  SecurityGateway                                     │
              │  │  + RateLimiter (S2)                                  │
              │  │  + CallerId-based exemptions                        │
              │  └──────────────────────────────────────────────────────│
              │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
              │  │ UsageService │  │ConfidenceServ│  │ StatusService │  │
              │  │ (NEW)        │  │              │  │ +Serialize   │  │
              │  └──────────────┘  └──────────────┘  └──────────────┘  │
              └─────────────────────────────────────────────────────────┘
```

## Component Breakdown

### 1. UsageService (`services/usage.rs`) — NEW

**Responsibility**: Unified entry point for all usage recording across both transports.

**Key design**: Single `record_access` method with `AccessSource` enum for variant-based routing. Internally delegates to existing mechanisms without merging them.

```rust
pub(crate) struct UsageService {
    store: Arc<Store>,
    usage_dedup: Arc<UsageDedup>,
}

pub(crate) enum AccessSource {
    McpTool,         // MCP tool call retrieval
    HookInjection,   // UDS hook injection
    Briefing,        // Either transport's briefing assembly
}

pub(crate) struct UsageContext {
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub helpful: Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level: Option<TrustLevel>,
}
```

**Internal routing** (exhaustive match, no fallthrough):

| AccessSource | Operations Triggered |
|---|---|
| `McpTool` | UsageDedup filter_access + vote processing + `record_usage_with_confidence` via spawn_blocking |
| `HookInjection` | `insert_injection_log_batch` + `record_co_access_pairs` + FEATURE_ENTRIES writes — all via spawn_blocking |
| `Briefing` | UsageDedup filter_access + access count (no votes, no injection log) via spawn_blocking |

**Fire-and-forget**: All recording is fire-and-forget. `record_access` spawns blocking tasks and returns immediately. Errors are logged, never propagated. This preserves the existing pattern from `record_usage_for_entries`.

**Replaces**: `UnimatrixBackend::record_usage_for_entries()` in `server.rs` (MCP path) and inline injection/co-access recording in `uds/listener.rs` (UDS path).

### 2. CallerId (`services/mod.rs`) — NEW

**Responsibility**: Type-safe caller identity for rate limiting and audit. Prevents cross-transport key collisions structurally.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum CallerId {
    Agent(String),        // MCP: resolved agent_id
    UdsSession(String),   // UDS: session_id
    // ApiKey(String),    // HTTP: future (ADR-003)
}
```

**Construction**: MCP transport constructs `CallerId::Agent(agent_id)`. UDS transport constructs `CallerId::UdsSession(session_id)`. Each transport owns its CallerId construction — services never construct CallerIds.

**ADR-003 decision**: `ApiKey` variant deferred until HTTP transport ships. Including it now adds an unused variant that complicates match exhaustiveness. The enum is extensible — adding a variant later is a minor, non-breaking change within the crate.

### 3. RateLimiter (inside `services/gateway.rs`) — NEW

**Responsibility**: S2 rate limiting gate. Enforces per-caller request rate limits using a sliding window algorithm.

```rust
pub(crate) struct RateLimiter {
    windows: Mutex<HashMap<CallerId, SlidingWindow>>,
    search_limit: u32,  // 300 per window
    write_limit: u32,   // 60 per window
    window_secs: u64,   // 3600 (1 hour)
}

struct SlidingWindow {
    timestamps: VecDeque<Instant>,
}
```

**Algorithm**: On each `check_*_rate()` call:
1. Acquire mutex
2. Get or create window for this CallerId
3. Evict timestamps older than `window_secs` (lazy eviction — ADR-002)
4. Check if `timestamps.len() >= limit`
5. If under limit, push current timestamp and return Ok
6. If at limit, return `ServiceError::RateLimited`

**Exemptions**: `CallerId::UdsSession` is exempt (return Ok immediately). Internal service calls do not go through rate limiting (they don't construct CallerIds).

**Integration points**:
- `SecurityGateway::check_search_rate(caller: &CallerId)` — called by SearchService and BriefingService (when `include_semantic=true`)
- `SecurityGateway::check_write_rate(caller: &CallerId)` — called by StoreService

### 4. SecurityGateway Modifications (`services/gateway.rs`) — MODIFIED

**Changes**:
- Add `RateLimiter` field to `SecurityGateway`
- Add `check_search_rate()` and `check_write_rate()` methods
- Add `ServiceError::RateLimited` variant to `ServiceError`

```rust
pub(crate) struct SecurityGateway {
    pub(crate) audit: Arc<AuditLog>,
    rate_limiter: RateLimiter,  // NEW
}
```

**Test support**: `SecurityGateway::new_permissive()` creates a gateway with no rate limits (or very high limits) for unit tests.

### 5. ToolContext Modifications (`mcp/context.rs`) — MODIFIED

**Changes**: Add `caller_id: CallerId` field, populated from `agent_id`.

```rust
pub(crate) struct ToolContext {
    pub agent_id: String,
    pub trust_level: TrustLevel,
    pub format: ResponseFormat,
    pub audit_ctx: AuditContext,
    pub caller_id: CallerId,  // NEW: CallerId::Agent(agent_id.clone())
}
```

`build_context()` on `UnimatrixServer` constructs `CallerId::Agent(agent_id.clone())` during ToolContext creation. The `session_id` from MCP params (if provided) is prefixed with `mcp::` and set on `audit_ctx.session_id`.

### 6. MCP Tool Parameter Changes (`mcp/tools.rs`) — MODIFIED

**Changes**: Add `session_id: Option<String>` to SearchParams, LookupParams, GetParams, and BriefingParams.

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    // ... existing fields ...
    /// Optional session ID (provided by hooks, not agent-reported).
    #[serde(default)]
    pub session_id: Option<String>,
}
```

The `session_id` flows into `ToolContext::audit_ctx.session_id` as `Some(format!("mcp::{}", sid))`. When `None`, `audit_ctx.session_id` remains `None` — identical to pre-vnc-009 behavior.

### 7. StatusReport Serialize (`mcp/response/status.rs`) — MODIFIED

**Changes**: Add `#[derive(serde::Serialize)]` to `StatusReport`, `CoAccessClusterEntry`, and referenced types.

**JSON structure preservation**: The existing manual JSON output uses nested structures (`correction_chains`, `security`, `co_access`, `outcomes`, `observation`) that do not map 1:1 to `StatusReport` flat fields. Two approaches:

**(ADR-001 decision)**: Introduce intermediate serialization structs (`StatusReportJson`) that match the existing nested JSON structure and are built from `StatusReport`. This preserves exact backward compatibility without polluting the domain struct with serialization concerns. `StatusReport` itself gets `#[derive(Serialize)]` for potential future use, but the JSON format response uses the intermediate struct.

**Alternative rejected**: Restructure `StatusReport` itself to match the JSON nesting. This would change the internal API and ripple into StatusService — excessive churn for a formatting concern.

### 8. UDS Auth Failure Audit (`uds/listener.rs`) — MODIFIED

**Changes**: Pass `Arc<AuditLog>` into `handle_connection()`. On auth failure, write `AuditEvent` with action `uds_auth_failure` before closing the connection.

```rust
Err(e) => {
    tracing::warn!(error = %e, "UDS authentication failed, closing connection");
    audit_log.emit_audit(AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: "unknown".to_string(),
        operation: "uds_auth_failure".to_string(),
        target_ids: vec![],
        outcome: Outcome::Failure,
        detail: format!("Authentication failed: {e}"),
    });
    return Ok(());
}
```

**Dependency**: `Arc<AuditLog>` is already available at the `start_uds_listener` level (it's created during server startup and passed to ServiceLayer). Adding it as a parameter to `handle_connection` follows the existing pattern for store, embed, vector handles.

### 9. Session ID Prefixing Strategy

**Decision (ADR-004)**: Prefix at the transport-service boundary. Strip before storage writes.

- MCP: `session_id` from params (if present) becomes `mcp::{value}` in AuditContext
- UDS: `session_id` from peer session becomes `uds::{value}` in AuditContext
- Services see prefixed IDs for rate limiting, audit, and logging
- Storage writes (injection logs, co-access pairs) use raw (unprefixed) session IDs to maintain backward compatibility with existing data
- Prefix/strip helpers: `fn prefix_session_id(transport: &str, raw: &str) -> String` and `fn strip_session_prefix(prefixed: &str) -> &str`

## Component Interactions

### Data Flow: MCP Search with Session and Rate Limiting

```
1. MCP tool call: context_search(query, session_id: Some("abc"))
2. tools.rs: build ToolContext
   - CallerId::Agent("researcher")
   - audit_ctx.session_id = Some("mcp::abc")
3. SearchService::search(params, audit_ctx, &caller_id)
4. SecurityGateway::check_search_rate(&caller_id)
   - CallerIdAgent("researcher") -> check sliding window
   - Under 300/hr -> Ok
5. SecurityGateway::validate_search_query(...)
6. Embed, HNSW search, re-rank, boost, filter
7. Return results
8. UsageService::record_access(entry_ids, AccessSource::McpTool, usage_ctx)
   - Spawns blocking task (fire-and-forget)
   - UsageDedup filter + vote processing + confidence
```

### Data Flow: UDS Injection with Usage

```
1. UDS hook: inject entries for session "sess-123"
2. listener.rs: dispatch to handler
   - CallerId::UdsSession("sess-123")
   - audit_ctx.session_id = Some("uds::sess-123")
3. Search via SearchService (rate check: UdsSession exempt)
4. UsageService::record_access(entry_ids, AccessSource::HookInjection, usage_ctx)
   - Spawns blocking task (fire-and-forget)
   - insert_injection_log_batch (raw session_id, no prefix)
   - record_co_access_pairs
   - FEATURE_ENTRIES write (if feature_cycle present)
```

### Data Flow: Rate Limit Exceeded

```
1. MCP tool call: context_search(query)
2. SearchService::search(params, audit_ctx, &caller_id)
3. SecurityGateway::check_search_rate(&CallerId::Agent("bot"))
   - 301st call this hour -> Err(ServiceError::RateLimited { limit: 300, window_secs: 3600, retry_after_secs: 42 })
4. SearchService returns Err
5. tools.rs maps to rmcp::ErrorData
6. MCP returns error response to caller
```

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `UsageService::new(store, usage_dedup)` | `(Arc<Store>, Arc<UsageDedup>) -> Self` | services/usage.rs (NEW) |
| `UsageService::record_access(entry_ids, source, ctx)` | `(&[u64], AccessSource, UsageContext)` | services/usage.rs (NEW) |
| `CallerId` enum | `Agent(String) \| UdsSession(String)` | services/mod.rs (NEW) |
| `SecurityGateway::check_search_rate(caller)` | `(&CallerId) -> Result<(), ServiceError>` | services/gateway.rs (MODIFIED) |
| `SecurityGateway::check_write_rate(caller)` | `(&CallerId) -> Result<(), ServiceError>` | services/gateway.rs (MODIFIED) |
| `ServiceError::RateLimited` | `{ limit: u32, window_secs: u64, retry_after_secs: u64 }` | services/mod.rs (MODIFIED) |
| `ToolContext.caller_id` | `CallerId` | mcp/context.rs (MODIFIED) |
| `SearchParams.session_id` | `Option<String>` | mcp/tools.rs (MODIFIED) |
| `LookupParams.session_id` | `Option<String>` | mcp/tools.rs (MODIFIED) |
| `GetParams.session_id` | `Option<String>` | mcp/tools.rs (MODIFIED) |
| `BriefingParams.session_id` | `Option<String>` | mcp/tools.rs (MODIFIED) |
| `StatusReport: Serialize` | `#[derive(Serialize)]` | mcp/response/status.rs (MODIFIED) |
| `StatusReportJson` | intermediate serialization struct | mcp/response/status.rs (NEW) |
| `ContradictionPair: Serialize` | `#[derive(Serialize)]` | infra/contradiction.rs (MODIFIED) |
| `EmbeddingInconsistency: Serialize` | `#[derive(Serialize)]` | infra/contradiction.rs (MODIFIED) |
| `CoAccessClusterEntry: Serialize` | `#[derive(Serialize)]` | mcp/response/status.rs (MODIFIED) |
| `prefix_session_id(transport, raw)` | `(&str, &str) -> String` | services/mod.rs or infra/ (NEW) |
| `strip_session_prefix(prefixed)` | `(&str) -> &str` | services/mod.rs or infra/ (NEW) |
| `ServiceLayer.usage` | `UsageService` | services/mod.rs (MODIFIED) |
| `handle_connection(..., audit_log)` | `+Arc<AuditLog>` parameter | uds/listener.rs (MODIFIED) |

## Technology Decisions

| Decision | Rationale | ADR |
|---|---|---|
| Intermediate StatusReportJson struct for JSON serialization | Preserves backward-compatible nested JSON without coupling domain struct to formatting | ADR-001 |
| Lazy eviction for RateLimiter sliding window | Simpler than timer-based. Cleanup happens on each check. Memory bounded by caller count * window size | ADR-002 |
| Defer CallerId::ApiKey variant | No HTTP transport exists. Adding unused variants complicates match arms. Extensible later | ADR-003 |
| Session ID: prefix at boundary, strip before storage | Preserves storage compatibility. Services see prefixed IDs for cross-transport safety | ADR-004 |

## Scope Risk Mitigations

| Risk | Mitigation in Architecture |
|---|---|
| SR-01 (AccessSource routing complexity) | Exhaustive match with distinct internal methods per variant. No shared code paths between McpTool and HookInjection |
| SR-02 (Mutex contention on RateLimiter) | Minimal critical section: timestamp push + eviction. Sub-microsecond. No async across lock |
| SR-05 (Vote semantics preservation) | UsageService::record_mcp_usage() is a direct move of record_usage_for_entries() body — zero logic changes |
| SR-06 (Session ID storage compatibility) | ADR-004: prefix at boundary, strip before storage writes |
| SR-08 (Briefing sharing search rate bucket) | Documented. BriefingService calls check_search_rate when include_semantic=true. Separate bucket possible later |
| SR-10 (ServiceLayer constructor changes) | UsageService added to ServiceLayer struct following existing pattern |
| SR-12 (Lock ordering) | RateLimiter has single internal Mutex, acquired/released in one synchronous call, never nested |
