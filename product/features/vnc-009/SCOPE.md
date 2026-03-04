# vnc-009: Cross-Path Convergence

## Problem Statement

After vnc-006 (Service Layer), vnc-007 (Briefing Unification), and vnc-008 (Module Reorganization), the unimatrix-server has a clean `services/` + `mcp/` + `uds/` + `infra/` architecture. However, five convergence gaps remain between the MCP and UDS paths that prevent the server from being truly transport-agnostic:

1. **Fragmented usage recording**: MCP usage recording (`record_usage_for_entries` in `server.rs`, ~80 lines) handles access counting, helpful/unhelpful votes, vote correction, and confidence recomputation via `UsageDedup`. UDS has a completely separate path: injection log writes (`insert_injection_log_batch`), co-access pair recording (`record_co_access_pairs`), and feature entry association — none of which flow through the MCP usage pipeline. There is no unified service that both transports call for usage tracking.

2. **MCP has no session awareness**: MCP tool calls (`context_search`, `context_lookup`, `context_get`, `context_briefing`) have no `session_id` parameter. UDS operations are session-scoped (injection logs, co-access pairs, compaction state). When a future HTTP transport arrives, it will also need session awareness. The MCP path cannot participate in session-scoped features without an optional `session_id` parameter.

3. **No rate limiting**: Neither path has rate limiting (F-09). The `SecurityGateway` defines S2 as a rate limiting gate but it was deferred through Waves 1-3. With both transports now flowing through services, rate limiting can be enforced at the service layer. The security surface analysis reclassified F-09 from Medium to High priority.

4. **Manual JSON assembly for StatusReport**: The `StatusReport` struct (87 lines, 30+ fields) in `mcp/response/status.rs` uses manual `json!` macro assembly for its JSON format output (~130 lines of `json!({...})` calls). The struct could derive `Serialize` and use `serde_json::to_value()`, eliminating the manual assembly entirely while preserving the nested JSON structure. This is Refactor #9 from the refactoring analysis.

5. **UDS auth failures are silent**: When `auth::authenticate_connection()` fails in the UDS listener, the connection is closed with only a `tracing::warn!` log (line ~298 of `uds/listener.rs`). No record is written to `AUDIT_LOG`. This means authentication failures are invisible to the audit trail (F-23). With the `AuditLog` infrastructure now available via the service layer, auth failures should be recorded as audit events.

## Goals

1. Extract a `UsageService` in `services/usage.rs` that unifies MCP usage recording (access counts, votes, confidence) with UDS usage tracking (injection logs, co-access pairs, feature entries) into a single service callable from both transports
2. Add optional `session_id` parameter to `context_search`, `context_lookup`, `context_get`, and `context_briefing` MCP tools, with transport-prefixed session IDs (`mcp::{id}` vs `uds::{id}`) to prevent cross-contamination
3. Implement S2 rate limiting in `SecurityGateway`: search operations at 300/hour per caller, write operations at 60/hour per caller, using in-memory sliding window — closes F-09
4. Add `#[derive(Serialize)]` to `StatusReport` and sub-structs, replacing ~130 lines of manual `json!` assembly in `mcp/response/status.rs` with `serde_json::to_value()` — Refactor #9
5. Write UDS authentication failures to `AUDIT_LOG` as audit events — closes F-23
6. All changes confined to `crates/unimatrix-server/`

## Non-Goals

- **New tables or schema changes** — UsageService orchestrates existing tables (ENTRIES, INJECTION_LOG, CO_ACCESS, FEATURE_ENTRIES). No new tables. No schema version bump.
- **HTTP transport** — future work enabled by session-aware services and rate limiting. Not in this scope.
- **Persistent rate limiter state** — in-memory sliding window resets on server restart. No persistence needed.
- **MCP injection logging** — MCP does not participate in the hook injection pipeline. UsageService unifies what exists (access counts, votes, injection logs) but does not add injection logging to MCP.
- **UDS usage dedup changes** — the `UsageDedup` in-memory tracker continues to serve MCP. UDS injection log dedup stays session-scoped via `SessionRegistry`. UsageService orchestrates both, it does not merge the dedup mechanisms.
- **Changes outside `crates/unimatrix-server/`** — no new crates, no engine wire protocol changes.
- **Session lifecycle management** — session creation, timeout, and cleanup remain in `SessionRegistry`/`infra/session.rs`. UsageService uses sessions; it does not manage them.
- **Rate limiting on UDS path** — UDS operations are from the local hook system (same UID, trusted). Rate limiting targets MCP callers (agents making tool calls). UDS is exempt via `AuditSource::Internal`/`AuditSource::Uds` bypass.
- **Co-access pair dedup changes** — the existing co-access dedup logic in `UsageDedup` and the UDS session-scoped dedup continue to function. UsageService delegates to them.

## Background Research

### Existing Research (Completed)

- **`product/research/optimizations/server-refactoring-architecture.md`** — Wave 4 items 11-14: UsageService, session-aware MCP, search rate limiting, StatusReport derive(Serialize). Item 15: UDS auth failure audit.
- **`product/research/optimizations/security-surface-analysis.md`** — F-09 (no rate limiting, reclassified High), F-23 (UDS auth failures unaudited), S2 rate limiting design with `RateLimiter` struct and sliding window.
- **`product/research/optimizations/refactoring-analysis.md`** — Refactor #9: StatusReport ~130 lines of `json!` macro assembly replaceable by `#[derive(Serialize)]`.
- **`product/features/vnc-006/SCOPE.md`** — Service layer foundation. SearchService, StoreService, ConfidenceService, SecurityGateway established.
- **`product/features/vnc-007/SCOPE.md`** — BriefingService extraction. UDS operational writes explicitly out of scope for StoreService.
- **`product/features/vnc-008/SCOPE.md`** — Module reorganization. ToolContext, StatusService, SessionWrite capability. Deferred UsageService, session-aware MCP, rate limiting, StatusReport derive to vnc-009.

### Key Decisions Already Made

| Decision | Resolution | Source |
|----------|------------|--------|
| Session ID prefixing | Transport-prefixed: `mcp::{id}` vs `uds::{id}` | security-surface-analysis.md |
| Rate limit values | Search 300/hr, writes 60/hr per caller | server-refactoring-architecture.md, product vision |
| Rate limiter storage | In-memory sliding window, no persistence | security-surface-analysis.md |
| StatusReport approach | `#[derive(Serialize)]`, sub-structs preserve nested JSON | server-refactoring-architecture.md |
| Internal caller exemption | `AuditSource::Internal` bypasses rate limiting | security-surface-analysis.md |
| UDS rate limit exemption | UDS callers exempt (trusted local process) | security-surface-analysis.md |
| UsageService API | Unified `record_access` with `AccessSource` enum (not transport-oriented methods) | server-refactoring-architecture.md, human direction |
| CallerId type | Typed `CallerId` enum (Agent/UdsSession/ApiKey) — prevents cross-transport key collisions | server-refactoring-architecture.md, human direction |
| BriefingService rate limiting | `check_search_rate()` when `include_semantic=true` — embedding triggers must be rate-limited | server-refactoring-architecture.md S2 design, human direction |

### Current Codebase State (Post vnc-008)

**Module structure** (clean after vnc-008):
- `services/` — SearchService, StoreService, ConfidenceService, BriefingService, SecurityGateway, StatusService
- `mcp/` — tools.rs (handlers), context.rs (ToolContext), identity.rs, response/ (entries, mutations, status, briefing)
- `uds/` — listener.rs, hook.rs
- `infra/` — audit, categories, coherence, contradiction, embed_handle, outcome_tags, pidfile, registry, scanning, session, shutdown, usage_dedup, validation

**MCP usage recording** (`server.rs:612-679`):
- `record_usage_for_entries()` — access count via UsageDedup, vote processing (new/corrected/noop), `record_usage_with_confidence()` via spawn_blocking
- Called from 4 MCP tools: context_search, context_lookup, context_get, context_briefing

**UDS usage tracking** (`uds/listener.rs`):
- `insert_injection_log_batch()` — records which entries were injected into which session
- `record_co_access_pairs()` — records which entries were retrieved together
- Feature entry association via FEATURE_ENTRIES table
- All fire-and-forget via spawn_blocking

**SecurityGateway** (`services/gateway.rs`):
- S1 content scanning, S3 input validation, S4 quarantine exclusion, S5 audit emission
- No S2 rate limiting (interface defined but not enforced)

**StatusReport** (`mcp/response/status.rs`):
- 87-line struct definition, 30+ fields, no `Serialize` derive
- `format_status_report()` function: ~514 lines with ~130 lines of `json!` macros in the JSON branch

**UDS auth** (`uds/listener.rs:291-299`):
- Auth failure: `tracing::warn!` + connection close. No AUDIT_LOG write.

**Test baseline**: 739 server tests (post vnc-008)

## Proposed Approach

### 1. UsageService (`services/usage.rs`)

Extract a `UsageService` with a unified API following the research architecture's `record_access` pattern with `AccessSource` discrimination:

```
UsageService {
    store: Arc<Store>,
    usage_dedup: Arc<UsageDedup>,
}

/// Unified entry point — both transports call this with their AccessSource.
UsageService::record_access(entry_ids: &[u64], source: AccessSource, context: UsageContext)

enum AccessSource {
    McpTool,          // MCP tool call — triggers access count, votes, confidence
    HookInjection,    // UDS injection — triggers injection log, co-access pairs
    Briefing,         // Either transport's briefing assembly — triggers access count
}

struct UsageContext {
    session_id: Option<String>,       // Transport-prefixed session ID
    agent_id: Option<String>,         // MCP agent identity
    helpful: Option<bool>,            // MCP helpfulness vote
    feature_cycle: Option<String>,    // Feature entry association
    trust_level: Option<TrustLevel>,  // For confidence weighting
}
```

The unified `record_access` method routes internally based on `AccessSource`:
- `McpTool` → UsageDedup access count + vote processing + confidence recomputation
- `HookInjection` → injection log batch write + co-access pair recording + feature entry association
- `Briefing` → access count (no votes, no injection log)

This follows the research architecture's recommendation for a convergent API rather than transport-oriented methods. Both transports call the same method; the `AccessSource` determines which recording mechanisms activate. The dedup mechanisms remain separate (UsageDedup for MCP, session-scoped for UDS) — the unified API routes to them, it does not merge them.

### 2. Session-Aware MCP

Add optional `session_id: Option<String>` parameter to the MCP tool parameter structs for `context_search`, `context_lookup`, `context_get`, and `context_briefing`.

**Source**: The `session_id` is provided by hooks augmenting the MCP request — not self-reported by the agent. Hooks have access to the active UDS session and can inject `session_id` into the tool call parameters. The format may match UDS session IDs (UUID), but this is not guaranteed.

When provided:
- Prefix with `mcp::` before passing to services: `mcp::{session_id}`
- Include in `AuditContext.session_id` for audit trail
- Enable future session-scoped features (co-access tracking for MCP)

When absent (`None`):
- Behavior is identical to current — no session association, `AuditContext.session_id` remains `None`
- Full backward compatibility

UDS continues to prefix with `uds::` as it already does (or will after this change).

### 3. S2 Rate Limiting in SecurityGateway

Add a `RateLimiter` to `SecurityGateway` using typed `CallerId` to prevent cross-transport key collisions (per research architecture):

```
/// Transport-provided caller identity for rate limiting and audit.
/// Typed enum prevents key collisions between transports.
enum CallerId {
    Agent(String),       // MCP: agent_id from registry
    UdsSession(String),  // UDS: session_id (exempt from rate limiting)
    ApiKey(String),      // HTTP: API key hash (future)
}

struct RateLimiter {
    windows: Mutex<HashMap<CallerId, SlidingWindow>>,
    search_limit: u32,  // 300/hour default
    write_limit: u32,   // 60/hour default
}

SecurityGateway::check_search_rate(caller: &CallerId) -> Result<(), ServiceError>
SecurityGateway::check_write_rate(caller: &CallerId) -> Result<(), ServiceError>
```

- `CallerId` is constructed by transport layers: MCP builds `CallerId::Agent(agent_id)`, UDS builds `CallerId::UdsSession(session_id)`. Future HTTP builds `CallerId::ApiKey(hash)`.
- Sliding window: track timestamps of requests per caller, expire entries older than 1 hour
- `ServiceError::RateLimited { limit, window_secs, retry_after_secs }` error variant
- SearchService calls `check_search_rate()` before search
- StoreService calls `check_write_rate()` before insert/correct
- BriefingService calls `check_search_rate()` before assembly when `include_semantic=true` (briefing with semantic search triggers embedding — must be rate-limited)
- `CallerId::UdsSession` and internal callers are exempt (checked inside `check_*_rate` methods)
- Closes F-09

### 4. StatusReport `#[derive(Serialize)]`

Add `#[derive(serde::Serialize)]` to `StatusReport` and all sub-structs (`CoAccessClusterEntry`, contradiction/embedding inconsistency types).

In `format_status_report()`, replace the JSON branch:
```rust
// Before: ~130 lines of json!({...})
ResponseFormat::Json => {
    let json = json!({
        "total_active": report.total_active,
        // ... 30+ fields manually mapped
    });
    // ...
}

// After: ~5 lines
ResponseFormat::Json => {
    let json = serde_json::to_value(report).unwrap_or_default();
    CallToolResult::success(vec![Content::text(json.to_string())])
}
```

Field names in the Serialize output must match the existing JSON keys to maintain backward compatibility. Use `#[serde(rename = "...")]` where Rust field names differ from JSON keys.

### 5. UDS Auth Failure Audit

In `handle_connection()` in `uds/listener.rs`, when `auth::authenticate_connection()` returns `Err`:

```rust
Err(e) => {
    // Existing: tracing::warn
    tracing::warn!(error = %e, "UDS authentication failed, closing connection");

    // New: write to AUDIT_LOG
    audit_log.write(AuditEvent {
        action: "uds_auth_failure",
        outcome: Outcome::Failure,
        details: format!("Authentication failed: {e}"),
        // ... source/caller fields
    });

    return Ok(());
}
```

Requires passing `Arc<AuditLog>` into the connection handler. The `AuditLog` is already available in the server context.

## Acceptance Criteria

### UsageService

- AC-01: `UsageService` struct exists in `services/usage.rs` with a unified `record_access(entry_ids, source: AccessSource, context: UsageContext)` method
- AC-02: `AccessSource` enum has variants `McpTool`, `HookInjection`, `Briefing` that route to appropriate recording mechanisms
- AC-03: MCP tools (`context_search`, `context_lookup`, `context_get`, `context_briefing`) call `UsageService::record_access` with `AccessSource::McpTool` instead of `record_usage_for_entries()` directly
- AC-04: UDS listener calls `UsageService::record_access` with `AccessSource::HookInjection` instead of calling `store.insert_injection_log_batch()` / `record_co_access_pairs()` directly
- AC-05: `AccessSource::HookInjection` triggers injection log writes, co-access pair recording, and feature entry association
- AC-06: `UsageService` is registered in `ServiceLayer` and accessible from both MCP and UDS transports
- AC-07: Fire-and-forget pattern preserved — usage recording does not block request processing
- AC-08: `UsageDedup` continues to function for MCP dedup (access counts, votes) — routed via `AccessSource::McpTool`
- AC-09: UDS injection log dedup continues via session-scoped mechanisms — routed via `AccessSource::HookInjection`
- AC-10: Existing usage recording behavior is preserved — access counts, votes, injection logs, co-access pairs, feature entries all produce identical storage writes
- AC-11: `record_usage_for_entries()` in `server.rs` is removed in favor of `UsageService`

### Session-Aware MCP

- AC-12: `context_search` parameter struct has optional `session_id: Option<String>`
- AC-13: `context_lookup` parameter struct has optional `session_id: Option<String>`
- AC-14: `context_get` parameter struct has optional `session_id: Option<String>`
- AC-15: `context_briefing` parameter struct has optional `session_id: Option<String>`
- AC-16: When `session_id` is provided, it is prefixed with `mcp::` before passing to services
- AC-17: UDS session IDs are prefixed with `uds::` before passing to services
- AC-18: `AuditContext.session_id` is populated with the transport-prefixed session ID when available

### Rate Limiting

- AC-19: `CallerId` enum exists with variants `Agent(String)`, `UdsSession(String)`, `ApiKey(String)` — typed to prevent cross-transport key collisions
- AC-20: `SecurityGateway` has `check_search_rate(caller: &CallerId)` enforcing 300 searches/hour per caller
- AC-21: `SecurityGateway` has `check_write_rate(caller: &CallerId)` enforcing 60 writes/hour per caller
- AC-22: `SearchService` calls `check_search_rate()` before performing search operations
- AC-23: `StoreService` calls `check_write_rate()` before performing insert/correct operations
- AC-24: `BriefingService` calls `check_search_rate()` before assembly when `include_semantic=true` (semantic search triggers embedding)
- AC-25: Rate limit violations return `ServiceError::RateLimited` with limit, window, and retry information
- AC-26: `CallerId::UdsSession` callers are exempt from rate limiting (checked inside `check_*_rate`)
- AC-27: Internal callers (service-initiated operations) are exempt from rate limiting
- AC-28: Rate limiter state is in-memory (resets on server restart, no persistence)
- AC-29: MCP transport constructs `CallerId::Agent(agent_id)`, UDS constructs `CallerId::UdsSession(session_id)`
- AC-30: Closes finding F-09

### StatusReport Serialize

- AC-31: `StatusReport` has `#[derive(serde::Serialize)]`
- AC-32: `CoAccessClusterEntry` has `#[derive(serde::Serialize)]`
- AC-33: Sub-structs used in StatusReport fields (contradiction pairs, embedding inconsistencies) have `#[derive(serde::Serialize)]`
- AC-34: `format_status_report()` JSON branch uses `serde_json::to_value()` instead of manual `json!` assembly
- AC-35: JSON output field names match the existing output (use `#[serde(rename)]` where needed for backward compatibility)

### UDS Auth Failure Audit

- AC-36: UDS authentication failures write an `AuditEvent` to `AUDIT_LOG` with action `uds_auth_failure` and `Outcome::Failure`
- AC-37: The audit event includes available peer credential information (UID if extractable despite auth failure)
- AC-38: `AuditLog` (or `Arc<AuditLog>`) is accessible in the UDS connection handler
- AC-39: Closes finding F-23

### Behavioral Equivalence and Quality

- AC-40: No `session_id` provided = identical behavior to pre-vnc-009 for all MCP tools (full backward compatibility)
- AC-41: No net reduction in test count from the post-vnc-008 baseline (739 tests)
- AC-42: All changes confined to `crates/unimatrix-server/` — no new crates
- AC-43: No new tables, no schema version bump

## Constraints

1. **Post vnc-008 baseline**: This feature builds on the post-vnc-008 codebase with `services/`, `mcp/`, `uds/`, `infra/` module groups, ToolContext, StatusService, and SessionWrite capability all in place.
2. **serde dependency**: `serde` and `serde_json` are already workspace dependencies. Adding `#[derive(Serialize)]` to StatusReport requires `serde` as a dependency of the types used in its fields (contradiction pairs, embedding inconsistencies). These types are in `infra/contradiction.rs` — they may need `Serialize` derives added.
3. **Fire-and-forget pattern**: All usage recording (MCP and UDS) is fire-and-forget via `spawn_blocking`. UsageService must preserve this pattern — no blocking on the request hot path.
4. **rmcp 0.16.0**: MCP tool handler signatures are constrained by the `#[tool]` macro. Adding `session_id` means adding it to the parameter struct, which rmcp serializes/deserializes. Optional fields with `#[serde(default)]` should work.
5. **Sliding window memory**: The rate limiter stores timestamps per caller. With a bounded number of callers (agents) and a 1-hour window, memory usage is bounded. No eviction policy needed for the initial implementation.
6. **StatusReport backward compatibility**: The JSON output from `format_status_report()` must have the same field names and structure after switching to `#[derive(Serialize)]`. Integration tests should verify.
7. **No schema version bump**: UsageService orchestrates existing tables. No new tables or structural changes.
8. **Wave independence**: vnc-009 can ship independently. It does not create forward dependencies for future work.
9. **Existing test infrastructure**: Tests use `TestHarness` and tempdir-based fixtures. Extend, do not replace.

## Resolved Questions

1. **UsageService granularity**: Unified `record_access` with `AccessSource` enum, per research architecture recommendation. Not transport-oriented methods. The `AccessSource` determines which recording mechanisms activate internally. (Human direction)

2. **MCP session_id source**: Provided by hooks augmenting the MCP request, not self-reported by the agent. May match UDS session IDs (UUID format) but this is not guaranteed — transport prefixing (`mcp::` vs `uds::`) prevents cross-contamination regardless of format. (Human direction)

## Open Questions (architect/risk to resolve)

1. **MCP session_id validation**: Should `session_id` values be validated (format, length) before prefixing? Hooks provide it, but validation rules TBD.

2. **Rate limiter cleanup**: Lazy eviction (on next check) vs proactive (periodic timer)?

3. **StatusReport field naming**: Audit existing `json!` field names vs Rust struct field names for `#[serde(rename)]` needs.

4. **UDS session ID prefixing**: Add `uds::` prefix to existing UDS sessions too, or only prefix MCP? Behavioral change for existing injection logs and co-access pairs.

## Tracking

https://github.com/dug-21/unimatrix/issues/88
