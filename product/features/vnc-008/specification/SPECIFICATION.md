# Specification: vnc-008 — Module Reorganization

## Objective

Restructure the flat 23-module layout of `crates/unimatrix-server/` into four logical groups (`services/`, `mcp/`, `uds/`, `infra/`), extract `ToolContext` to reduce MCP handler ceremony, split `response.rs` into a sub-module hierarchy, extract `StatusService`, and introduce the `SessionWrite` capability with formal UDS authorization boundaries. All changes are pure restructuring with no behavioral changes except additive UDS capability enforcement.

## Functional Requirements

### FR-1: Module Group Creation

FR-1.1: An `infra/` module group exists containing all cross-cutting infrastructure modules: audit, registry, session, scanning, validation, categories, contradiction, coherence, pidfile, shutdown, embed_handle, usage_dedup, outcome_tags.

FR-1.2: An `mcp/` module group exists containing MCP transport modules: tools, identity, context (ToolContext), and the response sub-module.

FR-1.3: A `uds/` module group exists containing UDS transport modules: listener (renamed from uds_listener), hook.

FR-1.4: The `services/` module group (already existing from vnc-006/007) gains a `status.rs` module for StatusService.

FR-1.5: Root-level files are limited to: `main.rs`, `lib.rs`, `error.rs`, `server.rs`.

FR-1.6: `lib.rs` exposes the four module groups and root modules. No flat-root transport or infrastructure modules remain.

### FR-2: response.rs Split

FR-2.1: `mcp/response/mod.rs` contains `ResponseFormat` enum, `parse_format()`, `format_timestamp()`, `entry_to_json()`, and re-exports from sub-modules.

FR-2.2: `mcp/response/entries.rs` contains `format_single_entry`, `format_search_results`, `format_lookup_results`, `format_store_success`, `format_store_success_with_note`, `format_correct_success`, `format_duplicate_found`.

FR-2.3: `mcp/response/mutations.rs` contains `format_status_change()` (generic) and `format_enroll_success()`. The three former functions (`format_deprecate_success`, `format_quarantine_success`, `format_restore_success`) become thin wrappers calling `format_status_change`.

FR-2.4: `mcp/response/status.rs` contains `format_status_report()`, `StatusReport` struct, `CoAccessClusterEntry` struct.

FR-2.5: `mcp/response/briefing.rs` contains `format_briefing()`, `format_retrospective_report()`, `Briefing` struct.

FR-2.6: The root-level `response.rs` file no longer exists after migration.

### FR-3: ToolContext Extraction

FR-3.1: A `ToolContext` struct exists in `mcp/context.rs` with fields: `agent_id: String`, `trust_level: TrustLevel`, `format: ResponseFormat`, `audit_ctx: AuditContext`.

FR-3.2: `UnimatrixServer::build_context()` method constructs a `ToolContext` from `agent_id` and `format` parameters, performing identity resolution, format parsing, and AuditContext construction.

FR-3.3: `UnimatrixServer::require_cap()` method performs a capability check for a given agent and capability.

FR-3.4: All 12 MCP tool handlers (context_search, context_lookup, context_store, context_get, context_correct, context_deprecate, context_status, context_briefing, context_quarantine, context_enroll, context_retrospective, plus any added in vnc-007) use `build_context()` and `require_cap()` instead of inline identity resolution, format parsing, and AuditContext construction.

### FR-4: StatusService Extraction

FR-4.1: `StatusService` exists in `services/status.rs` with `compute_report()` and `run_maintenance()` methods.

FR-4.2: `compute_report()` produces an identical `StatusReport` to the current inline `context_status` implementation for the same inputs.

FR-4.3: `run_maintenance()` performs confidence refresh, graph compaction, and co-access cleanup — the same operations currently in the `maintain=true` branch.

FR-4.4: The `context_status` handler in `mcp/tools.rs` delegates all computation to `StatusService`.

### FR-5: SessionWrite Capability

FR-5.1: `Capability::SessionWrite` variant exists in the `Capability` enum.

FR-5.2: UDS connections are assigned fixed capabilities: `{Read, Search, SessionWrite}`.

FR-5.3: UDS dispatch checks capabilities before executing operations.

FR-5.4: `SessionWrite` permits the following UDS operations:
- `SessionRegister` / `SessionClose`
- `RecordEvent` / `RecordEvents`
- Injection log writes (fire-and-forget)
- Signal queue writes (fire-and-forget)
- Co-access pair writes (fire-and-forget)
- Session record updates (fire-and-forget)

FR-5.5: `SessionWrite` does NOT permit:
- Knowledge writes (`context_store`, `context_correct`)
- Mutations (`context_deprecate`, `context_quarantine`)
- Admin operations (`context_enroll`, `maintain=true`)
- Retrospective operations

FR-5.6: The `SessionWrite` capability is distinct from `Write`. An agent with `Write` can store/correct entries. An agent with `SessionWrite` can only perform session-scoped operational writes.

### FR-6: Import Direction Enforcement

FR-6.1: `mcp/` modules do not contain `use unimatrix_store::` or `use unimatrix_vector::` imports for storage access purposes. Type-only imports of `EntryRecord`, `Status`, and similar data types are permitted.

FR-6.2: `uds/` modules do not contain `use unimatrix_store::` or `use unimatrix_vector::` imports for storage access purposes. Type-only imports are permitted.

FR-6.3: `infra/` modules do not import from `services/`, `mcp/`, or `uds/`.

FR-6.4: No cross-transport imports (`mcp/` <-> `uds/`).

FR-6.5: Documented exception: `mcp/tools.rs` retains direct `unimatrix_store` table imports (`ENTRIES`, `COUNTERS`, etc.) for the `context_status` handler until StatusService absorbs all direct-table access. These are tracked.

## Non-Functional Requirements

### NFR-1: Behavioral Equivalence

NFR-1.1: All MCP tool responses are byte-identical for the same inputs before and after restructuring.

NFR-1.2: All UDS responses are byte-identical for the same inputs before and after restructuring, except where UDS capability enforcement rejects a previously unrestricted operation (no such operations exist in the current UDS command set).

NFR-1.3: Search result ordering and scores are identical before and after restructuring.

### NFR-2: Test Coverage

NFR-2.1: No net reduction in test count from the post-vnc-007 baseline.

NFR-2.2: Tests move with their modules — when a module moves from root to a group, its `#[cfg(test)] mod tests` block moves too.

NFR-2.3: Integration tests that reference `use unimatrix_server::response::*` or similar old paths are updated to new paths.

### NFR-3: Compilation

NFR-3.1: Each migration step (infra, mcp, uds, capability+StatusService, cleanup) compiles independently.

NFR-3.2: No new compiler warnings introduced (except temporary `#[allow(unused_imports)]` on re-export stubs during migration).

### NFR-4: Scope Containment

NFR-4.1: All changes confined to `crates/unimatrix-server/`. No new crates.

NFR-4.2: No schema version changes. No new database tables.

NFR-4.3: No changes to foundation crates (`unimatrix-store`, `unimatrix-core`, `unimatrix-vector`, `unimatrix-embed`, `unimatrix-engine`, `unimatrix-adapt`).

## Acceptance Criteria

### Module Reorganization

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-01 | `mcp/` directory contains tools.rs, context.rs, identity.rs, response/ | File existence check |
| AC-02 | `uds/` directory contains listener.rs, hook.rs | File existence check |
| AC-03 | `infra/` directory contains all 13 infrastructure modules | File existence check |
| AC-04 | `services/status.rs` exists with StatusService | File existence + type check |
| AC-05 | Root contains only main.rs, lib.rs, error.rs, server.rs | Directory listing |
| AC-06 | No flat-root transport or infrastructure modules remain | Directory listing |

### response.rs Split

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-07 | `mcp/response/mod.rs` has shared helpers and re-exports | Code review |
| AC-08 | `mcp/response/entries.rs` has entry formatting functions | Code review |
| AC-09 | `mcp/response/mutations.rs` has `format_status_change` generic function | Unit test: deprecate/quarantine/restore produce identical output via generic |
| AC-10 | `mcp/response/status.rs` has `format_status_report` | Code review |
| AC-11 | `mcp/response/briefing.rs` has `format_briefing`, `format_retrospective_report` | Code review |
| AC-12 | No standalone `response.rs` at crate root | File absence check |

### ToolContext

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-13 | `ToolContext` struct exists in `mcp/context.rs` | Code review |
| AC-14 | All MCP tool handlers use `build_context()` + `require_cap()` | Code review: no inline identity/format/audit ceremony |
| AC-15 | `.map_err(rmcp::ErrorData::from)` count reduced by at least 50% | Grep count comparison |

### StatusService

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-16 | `StatusService` in `services/status.rs` with `compute_report()` + `run_maintenance()` | Code review |
| AC-17 | `context_status` handler delegates to StatusService | Code review: handler < 30 lines |
| AC-18 | StatusService produce identical StatusReport to pre-refactoring code | Snapshot test with known data |

### Unified Capability Model

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-19 | `Capability::SessionWrite` variant exists | Code review |
| AC-20 | UDS connections assigned `{Read, Search, SessionWrite}` | Unit test |
| AC-21 | SessionWrite permits operational writes (session, injection log, signals, co-access) | Integration test |
| AC-22 | SessionWrite does NOT permit knowledge writes, mutations, admin ops | Unit test: capability check rejects |
| AC-23 | F-26 closed: UDS has formal capability boundary | Test: attempt Admin op via UDS, expect rejection |

### Behavioral Equivalence

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-24 | MCP responses byte-identical for same inputs | Existing test suite passes |
| AC-25 | UDS responses byte-identical for same inputs | Existing test suite passes |
| AC-26 | No net reduction in test count | Test count comparison |
| AC-27 | All changes in `crates/unimatrix-server/` only | Git diff scope check |

### Import Direction

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-28 | `mcp/` has no storage-access imports of foundation crates | Grep verification (data-type imports permitted) |
| AC-29 | `services/` is the only group with foundation crate storage access | Grep verification |
| AC-30 | No circular dependencies between module groups | Compilation succeeds; no `infra/ -> services/` imports |

## Domain Models

### Module Groups

| Group | Responsibility | Depends On |
|-------|---------------|------------|
| `services/` | Transport-agnostic business logic | `infra/`, foundation crates |
| `mcp/` | MCP stdio transport | `services/`, `infra/` |
| `uds/` | UDS hook transport | `services/`, `infra/` |
| `infra/` | Shared infrastructure utilities | Foundation crates only |
| root (`server.rs`, `main.rs`) | Wiring, bootstrap | All groups |

### Capability Hierarchy

```
Admin > Write > Read
Admin > SessionWrite > (none)
Search is orthogonal (any trust level may have it)
```

`SessionWrite` is not a subset of `Write`. They are distinct permission domains:
- `Write`: Knowledge persistence (entries, corrections, deprecations)
- `SessionWrite`: Session-scoped operational data (injection logs, session records, signals)

### ToolContext Lifecycle

```
Handler entry
  → self.build_context(agent_id, format)  // resolves identity, parses format, builds AuditContext
  → self.require_cap(agent_id, cap)       // checks capability
  → validate_*_params()                   // tool-specific validation
  → service call with ctx.audit_ctx       // business logic
  → format response with ctx.format       // transport-specific formatting
  → record usage                          // fire-and-forget
Handler exit
```

## User Workflows

### Developer: Adding a New MCP Tool

1. Define params struct in `mcp/tools.rs`
2. Implement handler using `build_context()` + `require_cap()` (3 lines of ceremony instead of 15-25)
3. Add response formatter in appropriate `mcp/response/` file
4. Add validation in `infra/validation.rs`

### Developer: Adding a New UDS Command

1. Add `HookRequest` variant in `uds/listener.rs`
2. Implement handler, checking against `UDS_CAPABILITIES`
3. No response formatting needed (UDS uses `HookResponse`)

### Developer: Finding a Module

Module location is predictable by function:
- MCP concerns? -> `mcp/`
- UDS concerns? -> `uds/`
- Business logic? -> `services/`
- Infrastructure utility? -> `infra/`
- Server bootstrap/wiring? -> root

## Constraints

1. **rmcp `#[tool]` macro**: Handler signatures constrained to `(&self, Parameters<T>) -> Result<CallToolResult, ErrorData>`. ToolContext must be constructed inside the handler.
2. **Post vnc-007 baseline**: services/ already contains SearchService, StoreService, ConfidenceService, BriefingService, SecurityGateway.
3. **Wave independence**: vnc-008 ships without requiring vnc-009.
4. **No new crates**: All changes within `crates/unimatrix-server/`.
5. **No schema changes**: No version bump, no new tables.
6. **Test migration**: Tests move with their code. No test deletions.
7. **Serde compatibility**: Adding `SessionWrite` to `Capability` enum must not break deserialization of existing AGENT_REGISTRY entries (serde handles new enum variants gracefully for JSON/bincode with `#[serde(default)]` or similar).
8. **No new direct-storage coupling**: The restructuring must not introduce new paths to the database that bypass StoreService, except the documented StatusService exception (ADR-001).

## Dependencies

| Dependency | Type | Status |
|---|---|---|
| vnc-006: Service Layer + Security Gateway | Feature prerequisite | Expected landed |
| vnc-007: Briefing Unification | Feature prerequisite | Expected landed |
| rmcp 0.16.0 | External crate | Stable |
| redb | External crate | Stable |
| serde + bincode v2 | External crate | Stable |

## NOT in Scope

- UsageService extraction (vnc-009)
- Session-aware MCP (vnc-009)
- Rate limiting on search (vnc-009)
- `#[derive(Serialize)]` on StatusReport (vnc-009)
- UDS auth failure audit logging (vnc-009)
- HTTP transport
- Database replacement or storage abstraction beyond existing service layer
- UDS module split into listener.rs + handlers.rs (single file sufficient for current size)
- `shared/` directory (ToolContext is MCP-specific, placed in `mcp/`)
