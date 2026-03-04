# vnc-009: Cross-Path Convergence — Specification

## Objective

Unify the remaining divergences between MCP and UDS transports in `crates/unimatrix-server/` by extracting UsageService, adding session-aware MCP, implementing S2 rate limiting, modernizing StatusReport serialization, and auditing UDS auth failures. This is Wave 4 (final) of the server refactoring series, completing the transport-agnostic architecture.

## Functional Requirements

### FR-01: UsageService Extraction

- FR-01.1: A `UsageService` struct exists in `services/usage.rs` with a `record_access` method accepting `(&[u64], AccessSource, UsageContext)`.
- FR-01.2: `AccessSource::McpTool` triggers: UsageDedup access count filtering, vote processing (NewVote/CorrectedVote/NoOp), and `record_usage_with_confidence` via spawn_blocking.
- FR-01.3: `AccessSource::HookInjection` triggers: `insert_injection_log_batch`, `record_co_access_pairs`, and FEATURE_ENTRIES association — all via spawn_blocking.
- FR-01.4: `AccessSource::Briefing` triggers: UsageDedup access count filtering and access count increment only (no votes, no injection log).
- FR-01.5: `record_access` returns immediately (fire-and-forget). Errors in spawned tasks are logged, never propagated to callers.
- FR-01.6: UsageService is a field on `ServiceLayer`, constructed in `ServiceLayer::new()`.
- FR-01.7: MCP tools `context_search`, `context_lookup`, `context_get`, `context_briefing` call `UsageService::record_access` with `AccessSource::McpTool` (or `Briefing` for context_briefing).
- FR-01.8: UDS listener injection path calls `UsageService::record_access` with `AccessSource::HookInjection`.
- FR-01.9: `record_usage_for_entries()` in `server.rs` is removed. All callers migrate to UsageService.

### FR-02: Session-Aware MCP

- FR-02.1: `SearchParams`, `LookupParams`, `GetParams`, and `BriefingParams` structs have an optional `session_id: Option<String>` field with `#[serde(default)]`.
- FR-02.2: When `session_id` is `Some(sid)`, `ToolContext.audit_ctx.session_id` is set to `Some(format!("mcp::{sid}"))`.
- FR-02.3: When `session_id` is `None`, `ToolContext.audit_ctx.session_id` remains `None`. Behavior is identical to pre-vnc-009.
- FR-02.4: UDS transport sets `audit_ctx.session_id` to `Some(format!("uds::{sid}"))` where `sid` is the raw session ID.
- FR-02.5: Session ID validation: `session_id` values are validated for length (max 256 characters) and control characters (reject if present). This uses the existing S3 validation pattern.
- FR-02.6: Prefixed session IDs are stripped before storage writes (injection logs, co-access pairs) using `strip_session_prefix()`.

### FR-03: S2 Rate Limiting

- FR-03.1: `SecurityGateway` contains a `RateLimiter` with configurable search and write limits.
- FR-03.2: `check_search_rate(caller: &CallerId)` enforces 300 requests per 3600-second window per caller. Returns `Ok(())` or `Err(ServiceError::RateLimited)`.
- FR-03.3: `check_write_rate(caller: &CallerId)` enforces 60 requests per 3600-second window per caller.
- FR-03.4: `SearchService::search()` calls `check_search_rate()` before performing search operations. The `CallerId` is passed as a parameter.
- FR-03.5: `StoreService::insert()` and `StoreService::correct()` call `check_write_rate()` before processing.
- FR-03.6: `BriefingService::assemble()` calls `check_search_rate()` when `include_semantic` is true (semantic search triggers embedding computation).
- FR-03.7: `CallerId::UdsSession` is exempt from rate limiting. `check_*_rate()` returns `Ok(())` immediately for UDS callers.
- FR-03.8: Internal service calls do not pass through rate limiting (they do not construct CallerIds).
- FR-03.9: `ServiceError::RateLimited` includes `limit: u32`, `window_secs: u64`, and `retry_after_secs: u64`.
- FR-03.10: Rate limiter uses lazy eviction — expired timestamps removed on each check, no background timer.
- FR-03.11: Rate limiter state is in-memory, resets on server restart.

### FR-04: StatusReport Serialization

- FR-04.1: `StatusReport` struct has `#[derive(serde::Serialize)]`.
- FR-04.2: `CoAccessClusterEntry` struct has `#[derive(serde::Serialize)]`.
- FR-04.3: `ContradictionPair` struct has `#[derive(serde::Serialize)]`.
- FR-04.4: `EmbeddingInconsistency` struct has `#[derive(serde::Serialize)]`.
- FR-04.5: An intermediate `StatusReportJson` struct maps `StatusReport` flat fields into the nested JSON structure used by the existing output (ADR-001).
- FR-04.6: `format_status_report()` JSON branch uses `StatusReportJson::from(report)` + `serde_json::to_string_pretty()` instead of manual `json!` assembly.
- FR-04.7: JSON output field names and nesting structure match the existing output exactly. Key mappings:
  - `entries_with_supersedes` -> nested under `correction_chains`
  - `trust_source_distribution` -> nested under `security`
  - `total_co_access_pairs` -> nested under `co_access`
  - `total_outcomes` -> nested under `outcomes`
  - `observation_file_count` -> nested under `observation`
- FR-04.8: `category_distribution`, `topic_distribution`, `trust_source_distribution` (Vec<(String, u64)>) serialize as JSON objects (key-value maps), matching existing output.
- FR-04.9: Conditional JSON sections (`contradictions`, `embedding_inconsistencies`, `outcomes`) are included only when their corresponding performed/total flags are set, matching existing behavior.
- FR-04.10: Summary and Markdown format branches are unchanged — only the JSON branch is modified.

### FR-05: UDS Auth Failure Audit

- FR-05.1: When `auth::authenticate_connection()` returns `Err`, an `AuditEvent` is written to `AUDIT_LOG`.
- FR-05.2: The audit event has: action `"uds_auth_failure"`, outcome `Outcome::Failure`, detail containing the error message.
- FR-05.3: The audit event agent_id is `"unknown"` (identity is not yet established at auth failure time).
- FR-05.4: `Arc<AuditLog>` is passed as a parameter to `handle_connection()`.
- FR-05.5: The existing `tracing::warn!` is preserved alongside the audit write.

### FR-06: CallerId

- FR-06.1: `CallerId` enum has two variants: `Agent(String)` and `UdsSession(String)`.
- FR-06.2: `CallerId` derives `Debug, Clone, PartialEq, Eq, Hash`.
- FR-06.3: MCP transport constructs `CallerId::Agent(agent_id.clone())` during `build_context()`.
- FR-06.4: UDS transport constructs `CallerId::UdsSession(session_id.clone())` in `handle_connection()`.
- FR-06.5: `CallerId` is defined in `services/mod.rs`.
- FR-06.6: `ToolContext` has a `caller_id: CallerId` field.

## Non-Functional Requirements

### NFR-01: Performance

- NFR-01.1: Rate limiter check (`check_*_rate`) completes in under 10 microseconds per call (Mutex acquire + VecDeque operations).
- NFR-01.2: Usage recording does not add latency to MCP or UDS response paths (fire-and-forget via spawn_blocking).
- NFR-01.3: StatusReport JSON serialization via `serde_json::to_string_pretty` is at least as fast as manual `json!` assembly (serde is typically faster due to pre-computed serialization paths).

### NFR-02: Memory

- NFR-02.1: Rate limiter memory is bounded by `num_callers * max_requests_per_window * sizeof(Instant)`. At 100 callers * 300 requests * 16 bytes = ~480KB worst case.
- NFR-02.2: No new persistent storage. All new state is in-memory.

### NFR-03: Backward Compatibility

- NFR-03.1: MCP tools without `session_id` parameter produce identical responses to pre-vnc-009.
- NFR-03.2: StatusReport JSON output has identical field names, nesting, and value types.
- NFR-03.3: Existing injection log and co-access pair data is readable without migration.
- NFR-03.4: No schema version bump. No new tables.

### NFR-04: Testability

- NFR-04.1: `SecurityGateway::new_permissive()` provides a gateway with permissive rate limits (or no limits) for unit tests.
- NFR-04.2: `RateLimiter` is testable without tokio runtime (uses `std::time::Instant`, `std::sync::Mutex`).
- NFR-04.3: UsageService is testable via `Arc<Store>` + `Arc<UsageDedup>` constructor.

## Acceptance Criteria

### UsageService (AC-01 through AC-11)

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-01 | `UsageService` struct exists in `services/usage.rs` with `record_access(entry_ids, source, ctx)` | Code inspection |
| AC-02 | `AccessSource` enum has `McpTool`, `HookInjection`, `Briefing` variants | Code inspection |
| AC-03 | MCP tools call `UsageService::record_access` with `AccessSource::McpTool` | Code inspection + unit test |
| AC-04 | UDS listener calls `UsageService::record_access` with `AccessSource::HookInjection` | Code inspection + unit test |
| AC-05 | `HookInjection` triggers injection log + co-access + feature entry writes | Integration test |
| AC-06 | `UsageService` is a field on `ServiceLayer` | Code inspection |
| AC-07 | Fire-and-forget preserved: `record_access` returns immediately | Unit test with timing assertion |
| AC-08 | `UsageDedup` functions for MCP dedup via `McpTool` variant | Unit test: duplicate access returns empty filter |
| AC-09 | UDS injection dedup via session-scoped mechanisms | Integration test |
| AC-10 | Storage writes identical to pre-vnc-009 | Snapshot/regression test |
| AC-11 | `record_usage_for_entries()` removed from `server.rs` | Compilation (no references) |

### Session-Aware MCP (AC-12 through AC-18)

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-12 | `SearchParams` has `session_id: Option<String>` | Code inspection |
| AC-13 | `LookupParams` has `session_id: Option<String>` | Code inspection |
| AC-14 | `GetParams` has `session_id: Option<String>` | Code inspection |
| AC-15 | `BriefingParams` has `session_id: Option<String>` | Code inspection |
| AC-16 | Provided session_id prefixed with `mcp::` | Unit test |
| AC-17 | UDS session_id prefixed with `uds::` | Unit test |
| AC-18 | `AuditContext.session_id` populated with prefixed ID | Unit test |

### Rate Limiting (AC-19 through AC-30)

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-19 | `CallerId` enum with `Agent`, `UdsSession` variants | Code inspection |
| AC-20 | `check_search_rate` enforces 300/hr | Unit test: 300 Ok, 301st Err |
| AC-21 | `check_write_rate` enforces 60/hr | Unit test: 60 Ok, 61st Err |
| AC-22 | SearchService calls `check_search_rate` | Code inspection + integration test |
| AC-23 | StoreService calls `check_write_rate` | Code inspection + integration test |
| AC-24 | BriefingService calls `check_search_rate` when `include_semantic=true` | Code inspection + unit test |
| AC-25 | `ServiceError::RateLimited` has limit, window_secs, retry_after_secs | Code inspection |
| AC-26 | `CallerId::UdsSession` exempt | Unit test: unlimited calls Ok |
| AC-27 | Internal callers exempt | Code inspection (no CallerId constructed) |
| AC-28 | In-memory state, resets on restart | Design (no persistence code) |
| AC-29 | MCP constructs `CallerId::Agent`, UDS constructs `CallerId::UdsSession` | Code inspection |
| AC-30 | Closes F-09 | Acceptance test: rate limit enforced on MCP search/write |

### StatusReport Serialize (AC-31 through AC-35)

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-31 | `StatusReport` has `#[derive(Serialize)]` | Code inspection |
| AC-32 | `CoAccessClusterEntry` has `#[derive(Serialize)]` | Code inspection |
| AC-33 | `ContradictionPair`, `EmbeddingInconsistency` have `#[derive(Serialize)]` | Code inspection |
| AC-34 | JSON branch uses `serde_json::to_string_pretty` via `StatusReportJson` | Code inspection |
| AC-35 | JSON field names match existing output | Snapshot test comparing old vs new JSON |

### UDS Auth Failure Audit (AC-36 through AC-39)

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-36 | Auth failure writes `AuditEvent` with action `uds_auth_failure` | Integration test |
| AC-37 | Audit event includes error details | Code inspection |
| AC-38 | `Arc<AuditLog>` accessible in `handle_connection()` | Code inspection |
| AC-39 | Closes F-23 | Acceptance test: auth failure recorded in AUDIT_LOG |

### Behavioral Equivalence (AC-40 through AC-43)

| AC-ID | Criterion | Verification |
|-------|-----------|-------------|
| AC-40 | No session_id = identical behavior | Regression test |
| AC-41 | No net test count reduction from 739 | `cargo test --list \| wc -l` |
| AC-42 | All changes in `crates/unimatrix-server/` | `git diff --stat` |
| AC-43 | No new tables, no schema version bump | Code inspection |

## Domain Models

### Key Entities

| Entity | Definition | Location |
|--------|-----------|----------|
| `UsageService` | Unified usage recording service for both transports | services/usage.rs |
| `AccessSource` | Enum discriminating the origin of a usage event (McpTool, HookInjection, Briefing) | services/usage.rs |
| `UsageContext` | Contextual data for usage recording (session_id, agent_id, helpful, feature_cycle, trust_level) | services/usage.rs |
| `CallerId` | Type-safe caller identity for rate limiting (Agent, UdsSession) | services/mod.rs |
| `RateLimiter` | In-memory sliding window rate limiter keyed by CallerId | services/gateway.rs |
| `SlidingWindow` | Per-caller request timestamp deque with lazy eviction | services/gateway.rs |
| `StatusReportJson` | Intermediate serialization struct mapping StatusReport to nested JSON format | mcp/response/status.rs |

### Relationships

```
ServiceLayer
  ├── SearchService  ──uses──> SecurityGateway.check_search_rate(CallerId)
  ├── StoreService   ──uses──> SecurityGateway.check_write_rate(CallerId)
  ├── BriefingService──uses──> SecurityGateway.check_search_rate(CallerId)
  ├── UsageService   ──uses──> Store, UsageDedup
  ├── ConfidenceService
  └── StatusService

MCP ToolContext
  ├── agent_id: String
  ├── caller_id: CallerId::Agent(agent_id)
  └── audit_ctx.session_id: Option<"mcp::...">

UDS Connection
  ├── caller_id: CallerId::UdsSession(session_id)
  └── audit_ctx.session_id: Option<"uds::...">

StatusReport ──mapped-by──> StatusReportJson ──serialized-to──> JSON string
```

## User Workflows

### Agent Search with Session Tracking

1. Hook augments MCP request with `session_id`
2. Agent calls `context_search(query: "...", session_id: "sess-abc")`
3. MCP handler builds ToolContext with `CallerId::Agent("agent-1")`, `audit_ctx.session_id = Some("mcp::sess-abc")`
4. SearchService checks rate limit for `CallerId::Agent("agent-1")`
5. Search executes, results returned
6. UsageService records access with `AccessSource::McpTool`
7. Audit event includes `session_id: "mcp::sess-abc"`

### Rate Limit Exceeded

1. Agent makes 301st search call within 1 hour
2. `check_search_rate` returns `ServiceError::RateLimited { limit: 300, window_secs: 3600, retry_after_secs: 42 }`
3. MCP handler converts to rmcp ErrorData
4. Agent receives error response with retry information

### UDS Injection (Unchanged Except UsageService)

1. Hook triggers injection for session "sess-123"
2. Listener dispatches, builds `CallerId::UdsSession("sess-123")`
3. SearchService skips rate check (UDS exempt)
4. Results returned, entries injected
5. UsageService records access with `AccessSource::HookInjection`
6. Injection log, co-access pairs, feature entries written (raw session_id, prefix stripped)

## Constraints

1. Post vnc-008 codebase with services/, mcp/, uds/, infra/ module groups
2. serde/serde_json already available as workspace dependencies
3. Fire-and-forget pattern for all usage recording
4. rmcp 0.16.0 tool handler signature constraints
5. No schema version bump, no new tables
6. Changes confined to `crates/unimatrix-server/`
7. Test infrastructure extends existing TestHarness and tempdir fixtures

## Dependencies

| Dependency | Type | Used By |
|-----------|------|---------|
| `serde` (workspace) | Existing | StatusReport, ContradictionPair, EmbeddingInconsistency derives |
| `serde_json` (workspace) | Existing | StatusReportJson serialization |
| `std::time::Instant` | Stdlib | RateLimiter sliding window timestamps |
| `std::collections::VecDeque` | Stdlib | SlidingWindow timestamp storage |
| `std::sync::Mutex` | Stdlib | RateLimiter thread safety |
| `tokio::task::spawn_blocking` | Existing | UsageService fire-and-forget |
| `Arc<Store>` | Existing | UsageService storage operations |
| `Arc<UsageDedup>` | Existing | UsageService MCP dedup |
| `Arc<AuditLog>` | Existing | UDS auth failure audit |

## NOT in Scope

- New tables or schema changes
- HTTP transport or `CallerId::ApiKey` variant (ADR-003)
- Persistent rate limiter state
- MCP injection logging (MCP does not participate in hook injection pipeline)
- UDS usage dedup mechanism changes
- Session lifecycle management
- Rate limiting on UDS path
- OperationalEvent log (deferred to GH issue #89)
- Changes outside `crates/unimatrix-server/`
