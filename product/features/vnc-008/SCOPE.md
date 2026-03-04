# vnc-008: Module Reorganization

## Problem Statement

After vnc-006 (Service Layer + Security Gateway) and vnc-007 (Briefing Unification) land, the unimatrix-server crate will have a functional `services/` module containing SearchService, StoreService, ConfidenceService, BriefingService, and SecurityGateway. However, the remaining ~20 modules still sit flat in the `src/` root alongside `services/`, creating three problems:

1. **Flat module sprawl**: ~20 files at the same directory level with no grouping by concern. `tools.rs` (MCP transport), `uds_listener.rs` (UDS transport), `audit.rs` (infrastructure), `session.rs` (infrastructure), `response.rs` (MCP formatting), and `validation.rs` (infrastructure) all coexist at the same level. New contributors cannot tell which modules belong to which transport or are shared infrastructure.

2. **Monolith response formatting**: `response.rs` (~2,550 lines) is the second-largest file in the crate. It contains formatting for all MCP tool responses (search results, entries, store/correct success, deprecate/quarantine/restore, status reports, briefings, retrospectives) in three output modes (summary, markdown, JSON). The file has no internal structure — it is a sequence of format functions. The deprecate, quarantine, and restore formatters are near-identical (Refactor #6) and can be unified into a generic `format_status_change`.

3. **Handler ceremony duplication**: Each of the 12 MCP tool handlers in `tools.rs` repeats the same identity resolution, capability check, format parsing, AuditContext construction, and error mapping ceremony (~15-25 lines per handler, ~79 occurrences of `.map_err(rmcp::ErrorData::from)`). Extracting a `ToolContext` struct that encapsulates pre-validated identity, capabilities, format, and audit context would reduce each handler by 15-25 lines.

4. **Missing StatusService**: `context_status` is ~628 lines of inline computation in `tools.rs` — the largest single MCP handler by far. It computes entry counts, age distributions, stale entries, duplicate candidates, correction chains, and co-access clusters, all in one function. It should be extracted to `services/status.rs` as a StatusService, consistent with the pattern established by SearchService and BriefingService.

5. **No formal UDS capability model**: UDS connections are UID-authenticated but have no capability restrictions (F-26). With the service layer in place, the unified capability model can be formalized: UDS gets fixed `{Read, Search, SessionWrite}` capabilities, preventing UDS from performing Admin operations (e.g., `maintain=true` on context_status, agent enrollment). `SessionWrite` is a new capability that permits operational writes (injection logs, session records, signals) but not knowledge writes (context_store, context_correct).

## Goals

1. Restructure flat modules into `services/`, `mcp/`, `uds/`, `infra/` directory groups following the layout defined in `product/research/optimizations/server-refactoring-architecture.md`
2. Split `response.rs` (~2,550 lines) into `mcp/response/` sub-module with 4 files: `entries.rs`, `mutations.rs`, `status.rs`, `briefing.rs`
3. Unify deprecate/quarantine/restore response formatters into a generic `format_status_change` (Refactor #6)
4. Extract `ToolContext` struct to reduce per-handler ceremony in MCP tool handlers
5. Extract `context_status` computation into `services/status.rs` (StatusService)
6. Introduce `SessionWrite` capability and assign UDS fixed capabilities `{Read, Search, SessionWrite}` (closes F-26)
7. Enforce module visibility boundaries: transport modules (`mcp/`, `uds/`) import `services/` and `infra/` only, never foundation crates directly
8. Pure restructuring — no behavioral changes to any request path

## Non-Goals

- **New features or behavioral changes** — this is pure restructuring. All request paths must produce identical output before and after.
- **UsageService extraction** — deferred to vnc-009 (Wave 4). MCP usage recording and UDS injection logging remain separate.
- **Session-aware MCP** — deferred to vnc-009.
- **Rate limiting on search** — deferred to vnc-009 (S2 gate for search).
- **`#[derive(Serialize)]` on StatusReport** — deferred to vnc-009 (Refactor #9). StatusService extracts the computation; JSON formatting stays manual in `mcp/response/status.rs` for now.
- **HTTP transport** — future work enabled by the service layer.
- **Database replacement or storage abstraction** — the service layer must not introduce new direct-storage coupling, but no new abstraction is added.
- **Code changes outside `crates/unimatrix-server/`** — all changes within the server crate. No new crates.
- **UDS auth failure audit logging** — deferred to vnc-009 (closes F-23).
- **Merging `format_store_success` variants** — deferred or done opportunistically if it falls out of the response.rs split.

## Background Research

### Existing Research (Completed)

- **`product/research/optimizations/server-refactoring-architecture.md`** — Full 4-wave refactoring plan with module reorganization layout (lines 350-436), Wave 3 items 7-10 (lines 738-745), and the complete proposed directory structure.
- **`product/research/optimizations/refactoring-analysis.md`** — Line-by-line analysis identifying Refactor #2 (ToolContext), Refactor #3 (context_status 628 lines), Refactor #6 (identical deprecate/quarantine/restore formatters), and 79 occurrences of `.map_err(rmcp::ErrorData::from)`.
- **`product/research/optimizations/architecture-dependencies.md`** — Dependency graph between modules, import analysis, circular dependency risks.
- **`product/research/optimizations/security-surface-analysis.md`** — F-26 (UDS no authorization) and F-04 (maintain=true auth bypass) findings, with Wave 3 closure plan.

### Key Decisions Already Made

| Decision | Resolution | Source |
|----------|------------|--------|
| Module grouping names | `services/`, `mcp/`, `uds/`, `infra/` | server-refactoring-architecture.md |
| response.rs split | 4 files in `mcp/response/`: entries.rs, mutations.rs, status.rs, briefing.rs | server-refactoring-architecture.md |
| Refactor #6 approach | Generic `format_status_change` unifying deprecate/quarantine/restore | refactoring-analysis.md |
| ToolContext scope | Identity, capability, format, audit context — not business logic | refactoring-analysis.md |
| StatusService location | `services/status.rs` | server-refactoring-architecture.md |
| UDS capability set | `{Read, Search, SessionWrite}` (fixed, no runtime config) | server-refactoring-architecture.md |
| Import direction | transport → services → foundation (no reverse) | server-refactoring-architecture.md |
| Service bypass prevention | `pub(crate)` visibility on gateway-protected methods, enforced at module boundary | server-refactoring-architecture.md |

### Current Codebase State (Post vnc-006 + vnc-007)

After vnc-006 and vnc-007 land, the server crate will have:

**`services/` module** (already exists):
- `mod.rs` — ServiceLayer, AuditContext, AuditSource, ServiceError
- `gateway.rs` — SecurityGateway (S1/S3/S4/S5)
- `search.rs` — SearchService
- `store_ops.rs` — StoreService
- `store_correct.rs` — StoreService correction operations
- `confidence.rs` — ConfidenceService
- `briefing.rs` — BriefingService (from vnc-007)

**Flat root modules** (to be reorganized):
- `tools.rs` (~2,600 lines post vnc-007) — MCP tool handlers
- `response.rs` (~2,550 lines) — MCP response formatting
- `uds_listener.rs` (~2,000 lines post vnc-007) — UDS transport
- `server.rs` (~2,150 lines) — UnimatrixServer + backend
- `hook.rs` (1,280 lines) — Hook preprocessing
- `validation.rs` (1,209 lines) — Input validation
- `session.rs` (1,006 lines) — Session registry
- `registry.rs` (933 lines) — Agent registry
- `contradiction.rs` (820 lines) — Contradiction detection
- `coherence.rs` (581 lines) — Coherence computation
- `audit.rs` (599 lines) — Audit logging
- `error.rs` (567 lines) — Error types
- `pidfile.rs` (472 lines) — PID management
- `outcome_tags.rs` (435 lines) — Outcome tag parsing
- `scanning.rs` (423 lines) — Content scanning
- `usage_dedup.rs` (320 lines) — Usage deduplication
- `categories.rs` (242 lines) — Category allowlist
- `shutdown.rs` (179 lines) — Signal handling
- `embed_handle.rs` (161 lines) — Embedding service handle
- `identity.rs` (140 lines) — Identity resolution

### Handler Ceremony Pattern

Each of the 12 MCP tool handlers repeats this sequence:

```rust
// 1. Identity resolution
let identity = self.resolve_agent(&params.agent_id).map_err(rmcp::ErrorData::from)?;

// 2. Capability check
self.registry.require_capability(&identity.agent_id, Capability::Read)
    .map_err(rmcp::ErrorData::from)?;

// 3. Validation (tool-specific)
validate_xxx_params(&params).map_err(rmcp::ErrorData::from)?;

// 4. Parse format
let format = parse_format(&params.format).map_err(rmcp::ErrorData::from)?;

// 5. Build AuditContext
let audit_ctx = AuditContext {
    source: AuditSource::Mcp { agent_id: identity.agent_id.clone(), trust_level: identity.trust_level },
    caller_id: identity.agent_id.clone(),
    session_id: None,
    feature_cycle: None,
};
```

This ceremony is 15-25 lines per handler. `ToolContext` would encapsulate the resolved identity, format, and audit context, reducing this to a single function call.

## Proposed Approach

### 1. Create Module Groups

Move existing flat modules into logical directory groups:

- **`mcp/`** — MCP transport: `tools.rs`, `identity.rs`, `response/` sub-module
- **`uds/`** — UDS transport: `listener.rs` (renamed from `uds_listener.rs`), `handlers.rs`, `hook.rs`
- **`infra/`** — Cross-cutting infrastructure: `audit.rs`, `registry.rs`, `session.rs`, `scanning.rs`, `validation.rs`, `categories.rs`, `contradiction.rs`, `coherence.rs`, `pidfile.rs`, `shutdown.rs`, `embed_handle.rs`, `usage_dedup.rs`, `outcome_tags.rs`

`services/` already exists from vnc-006. `error.rs`, `main.rs`, `lib.rs`, and `server.rs` remain at the root.

### 2. Split response.rs into mcp/response/

Split the ~2,550-line monolith into:
- `mcp/response/mod.rs` — Re-exports, shared helpers (`format_timestamp`, `entry_to_json`, `parse_format`, `ResponseFormat`)
- `mcp/response/entries.rs` — `format_single_entry`, `format_search_results`, `format_lookup_results`, `format_store_success`, `format_correct_success`, `format_duplicate_found`, empty results (~300 lines)
- `mcp/response/mutations.rs` — Generic `format_status_change` unifying deprecate/quarantine/restore/enroll (~150 lines, Refactor #6)
- `mcp/response/status.rs` — `format_status_report` (~200 lines)
- `mcp/response/briefing.rs` — `format_briefing`, `format_retrospective_report` (~100 lines)

### 3. Extract ToolContext

Create a `ToolContext` struct in `mcp/` (or a `shared/` module) that encapsulates:
- Resolved agent identity (agent_id, trust_level)
- Response format
- AuditContext
- Helper methods for common operations

Each tool handler calls a single `ToolContext::from_params()` or similar, replacing 15-25 lines of ceremony with ~3 lines.

### 4. Extract StatusService

Move `context_status` computation from `tools.rs` to `services/status.rs`:
- `StatusService::compute_report()` — builds the StatusReport
- `StatusService::run_maintenance()` — confidence refresh, graph compaction, co-access cleanup
- Admin capability enforcement for `maintain=true` moves into the service (closes F-04)

### 5. Unified Capability Model

- Add `SessionWrite` variant to the `Capability` enum
- Assign UDS connections fixed capabilities: `{Read, Search, SessionWrite}`
- `SessionWrite` permits: injection log writes, session management, signal queue writes, co-access pair writes
- `SessionWrite` does NOT permit: `context_store`, `context_correct`, `context_deprecate`, `context_quarantine`, `context_enroll`, `maintain=true`
- Closes F-26: UDS connections now have a formal capability boundary

### 6. Module Visibility Enforcement

- Transport modules (`mcp/`, `uds/`) import from `services/` and `infra/` only
- `services/` imports from `infra/` and foundation crates (`unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`, `unimatrix-core`)
- `infra/` modules are standalone utilities with no upward dependencies
- Enforce via `pub(crate)` visibility and code review (Rust module system does not natively prevent sibling imports, but the directory structure makes violations obvious)

## Acceptance Criteria

### Module Reorganization

- AC-01: `mcp/` directory exists containing `tools.rs` (or `mod.rs` + split), `identity.rs`, and `response/` sub-module
- AC-02: `uds/` directory exists containing the UDS listener, handlers, and hook modules
- AC-03: `infra/` directory exists containing audit, registry, session, scanning, validation, categories, contradiction, coherence, pidfile, shutdown, embed_handle, usage_dedup, and outcome_tags modules
- AC-04: `services/` directory contains all service modules (search, briefing, store_ops, store_correct, confidence, gateway, status) plus mod.rs
- AC-05: `main.rs`, `lib.rs`, `error.rs`, and `server.rs` remain at the crate root
- AC-06: No flat-root transport or infrastructure modules remain (all moved to group directories)

### response.rs Split

- AC-07: `mcp/response/mod.rs` contains shared helpers and re-exports
- AC-08: `mcp/response/entries.rs` contains entry formatting functions (single_entry, search_results, lookup_results, store_success, correct, duplicate, empty)
- AC-09: `mcp/response/mutations.rs` contains a generic `format_status_change` function replacing the near-identical deprecate/quarantine/restore formatters (Refactor #6)
- AC-10: `mcp/response/status.rs` contains `format_status_report`
- AC-11: `mcp/response/briefing.rs` contains `format_briefing` and `format_retrospective_report`
- AC-12: No standalone `response.rs` file exists at the crate root

### ToolContext Extraction

- AC-13: A `ToolContext` struct exists that encapsulates resolved identity (agent_id, trust_level), response format, and AuditContext
- AC-14: All 12 MCP tool handlers use `ToolContext` for identity resolution, capability checking, format parsing, and AuditContext construction
- AC-15: The `.map_err(rmcp::ErrorData::from)` ceremony count is reduced by at least 50% from the pre-vnc-008 baseline

### StatusService

- AC-16: `StatusService` exists in `services/status.rs` with `compute_report()` and `run_maintenance()` methods
- AC-17: `context_status` in `mcp/tools.rs` delegates computation to `StatusService`
- AC-18: StatusService enforces Admin capability for `maintain=true` operations (closes F-04)

### Unified Capability Model

- AC-19: `SessionWrite` capability exists in the `Capability` enum
- AC-20: UDS connections are assigned fixed capabilities `{Read, Search, SessionWrite}`
- AC-21: `SessionWrite` permits operational writes (injection logs, session records, signals, co-access pairs)
- AC-22: `SessionWrite` does NOT permit knowledge writes (store, correct), mutations (deprecate, quarantine), admin operations (enroll, maintain), or retrospective operations
- AC-23: UDS capability enforcement closes finding F-26

### Behavioral Equivalence

- AC-24: All MCP tool responses are byte-identical for the same inputs before and after restructuring
- AC-25: All UDS responses are byte-identical for the same inputs before and after restructuring (except for capability enforcement on previously unrestricted operations)
- AC-26: No net reduction in test count from the post-vnc-007 baseline
- AC-27: All changes confined to `crates/unimatrix-server/` — no new crates

### Module Visibility

- AC-28: Transport modules (`mcp/`, `uds/`) do not directly import foundation crates (`unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`)
- AC-29: `services/` is the only module group that imports both `infra/` modules and foundation crates
- AC-30: No circular dependencies between module groups

## Constraints

1. **Pure restructuring**: No behavioral changes to any request path. The only exception is UDS capability enforcement (AC-22/23), which is additive security hardening, not a functional change.
2. **Rust module system**: Rust's `pub(crate)` visibility is the primary enforcement mechanism. Cross-module-group imports are discouraged but not compiler-prevented at the directory level.
3. **rmcp 0.16.0 constraints**: MCP tool handler signatures are constrained by the `#[tool]` macro. `ToolContext` must work within these constraints (likely constructed inside the handler, not injected).
4. **Post vnc-007 baseline**: This feature builds on the post-vnc-007 codebase. The services/ module already contains SearchService, StoreService, ConfidenceService, BriefingService, and SecurityGateway.
5. **Wave independence**: vnc-008 must ship without requiring vnc-009. No forward dependencies.
6. **Test migration**: Tests move with their code. When a module moves from root to a group directory, its tests move too. No test deletions.
7. **Re-export compatibility**: To avoid breaking all `use crate::response::*` imports in one commit, `mod.rs` re-exports from the old paths may be used temporarily during the migration.
8. **No new direct-storage coupling**: Per the product vision architect note, the restructuring must not introduce new paths to the database that bypass StoreService. Existing direct-storage paths in StatusService are inherited from the pre-refactoring code.
9. **Schema version**: No schema version bump. No new tables.

## Open Questions

1. **ToolContext construction pattern**: Should `ToolContext` be constructed via a method on `UnimatrixServer` (e.g., `self.tool_context(&params.agent_id, &params.format, Capability::Read)?`), or as a standalone constructor? The rmcp `#[tool]` macro constrains the handler signature to `(&self, Parameters(params))`, so ToolContext cannot be injected as a parameter.

2. **UDS module split granularity**: Should `uds_listener.rs` (~2,000 lines) be split into `uds/listener.rs` (accept loop, auth, dispatch) and `uds/handlers.rs` (individual request handlers), or kept as a single `uds/mod.rs`? The research doc proposes the split but it may introduce unnecessary churn.

3. **`shared/` module**: The research doc proposes a `shared/` directory for `ToolContext` and formatting utilities. Is this necessary, or can `ToolContext` live in `mcp/` since it is MCP-specific?

4. **Migration ordering**: Should the restructuring be done in one large commit (all moves + import updates) or broken into sequential steps (e.g., infra/ first, then mcp/, then uds/, then response.rs split)?

## Tracking

https://github.com/dug-21/unimatrix/issues/86
