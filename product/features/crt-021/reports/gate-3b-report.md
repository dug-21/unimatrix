# Gate 3b Report: crt-021

> Gate: 3b (Code Review)
> Date: 2026-03-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All functions, structs, and algorithms match pseudocode |
| Architecture compliance | PASS | Component boundaries and ADR decisions followed |
| Interface implementation | PASS | All signatures, types, and error handling correct |
| Test case alignment | PASS | All 55 graph tests + 12 migration integration tests + 18 store tests pass |
| Code quality | PASS | Build clean; no stubs; no unwrap in new non-test code; deprecated shims removed |
| Security | PASS | No hardcoded secrets; input validation present; no panics on bad data |
| Knowledge stewardship | PASS | All implementation agent reports contain Queried/Stored entries |

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: PASS

**Evidence**:
- `crates/unimatrix-engine/src/graph.rs`: `RelationType` has exactly 5 variants (Supersedes, Contradicts, Supports, CoAccess, Prerequisite); `as_str()` and `from_str()` present; round-trip confirmed by `test_relation_type_roundtrip_all_variants` and `test_relation_type_prerequisite_roundtrips`. `RelationEdge` carries all required fields. `TypedRelationGraph` wraps `StableGraph<u64, RelationEdge>` with `HashMap<u64, NodeIndex>` node map.
- `build_typed_relation_graph()`: multi-pass build with `bootstrap_only=true` edges structurally excluded in pass 1. `edges_of_type()` is the sole filter boundary — no raw `.edges_directed()` calls in `graph_penalty`, `find_terminal_active`, or private helpers.
- `crates/unimatrix-store/src/db.rs`: GRAPH_EDGES DDL matches spec exactly — all 10 columns including `metadata TEXT DEFAULT NULL`, UNIQUE(source_id, target_id, relation_type) constraint, 3 indexes.
- `crates/unimatrix-store/src/migration.rs`: v12→v13 block present with all steps: CREATE TABLE, Supersedes bootstrap INSERT, CoAccess bootstrap INSERT with COALESCE weight formula, schema_version UPDATE. `CURRENT_SCHEMA_VERSION = 13`, `CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3`.
- `crates/unimatrix-store/src/analytics.rs`: `AnalyticsWrite::GraphEdge` variant present; drain arm has `weight.is_finite()` guard; INSERT OR IGNORE; `bootstrap_only` cast to `i64`. `variant_name()` returns `"GraphEdge"`.
- `crates/unimatrix-store/src/read.rs`: `GraphEdgeRow` struct has all 8 fields; `query_graph_edges()` uses `read_pool`; `bootstrap_only` mapped via `i64 != 0`.
- `crates/unimatrix-server/src/services/typed_graph.rs`: `TypedGraphState` holds `typed_graph: TypedRelationGraph`, `all_entries`, `use_fallback`. Pre-built graph pattern (no per-query rebuild). `CycleDetected` handled by setting `use_fallback=true`.
- `crates/unimatrix-server/src/background.rs`: Tick sequence — GRAPH_EDGES compaction after `maintenance_tick`, TypedGraphState rebuild after compaction, contradiction scan last. Compaction uses direct `write_pool` (not analytics queue).

### Check 2: Architecture Compliance

**Status**: PASS

**Evidence**:
- All 6 component boundaries from ARCHITECTURE.md implemented as separate files: engine-types (graph.rs), store-schema (db.rs), store-migration (migration.rs), store-analytics (analytics.rs), server-state (typed_graph.rs), background-tick (background.rs).
- ADR-001 decisions followed: `edges_of_type()` as sole filter boundary (SR-01); no per-query rebuild (FR-22, C-14); direct write_pool for GRAPH_EDGES compaction.
- `SupersessionState` → `TypedGraphState` rename complete. Grep over all crates confirms zero occurrences of `SupersessionGraph`, `SupersessionState`, `SupersessionStateHandle`, or `build_supersession_graph` in production code. Only doc-comment references remain (in graph.rs line 149 and typed_graph.rs line 3), which is expected.
- ADR-001 stored as Unimatrix entry #2416; ADR-004 (entry #1604) deprecated. AC-16 satisfied.

**Minor concern (WARN, unchanged from previous report)**: FR-24 tick sequence specifies: (1) maintenance_tick, (2) GRAPH_EDGES compaction, (3) VECTOR_MAP compaction, (4) TypedGraphState rebuild, (5) contradiction scan. VECTOR_MAP compaction is embedded inside `maintenance_tick → run_maintenance`, not as a named separate step between GRAPH_EDGES compaction and TypedGraphState rebuild. The semantic ordering contract — "compaction before rebuild" — holds. Not escalated.

### Check 3: Interface Implementation

**Status**: PASS

**Evidence**:
- `SearchService::new()` accepts `typed_graph_handle: TypedGraphStateHandle`; search path acquires read lock, uses pre-built `TypedRelationGraph`, no per-query rebuild.
- `ServiceLayer` → `TypedGraphState::new_handle()`, threads handle to background tick and search service.
- `store::GraphEdgeRow` exported from `unimatrix-store/src/lib.rs` as `pub use read::{GraphEdgeRow, ...}`.
- Dual `GraphEdgeRow` types (store and engine) correctly mapped in `typed_graph.rs::rebuild()`.
- All interface signatures from ARCHITECTURE.md §Integration Surface are present.

### Check 4: Test Case Alignment

**Status**: PASS

**Evidence**:
- `cargo test -p unimatrix-engine --lib -- graph`: **55 tests pass** — covers all 25+ legacy penalty cases (ORPHAN, DEAD_END, PARTIAL_SUPERSESSION, CLEAN_REPLACEMENT, hop-decay, cycle detection) plus new typed-graph tests: `edges_of_type` filter, bootstrap exclusion, mixed-edge-type isolation, unknown RelationType skip, unmapped node skip, Prerequisite round-trip, Supersedes-not-doubled.
- `cargo test -p unimatrix-store --lib -- graph analytics`: **18 store tests pass** — GRAPH_EDGES DDL, UNIQUE constraint, indexes, `query_graph_edges()`, weight validation guard, drain idempotency.
- `cargo test -p unimatrix-store --test migration_v12_to_v13 --features test-support`: **12 migration integration tests pass** — Supersedes bootstrap, CoAccess threshold and weight normalization (R-15), empty co_access (R-06), all-below-threshold, no Contradicts bootstrapped (AC-08), idempotency (R-08), promotion path (AC-21), edge direction, bootstrap_only=0 enforcement.
- `cargo test -p unimatrix-server --lib -- typed_graph`: **15 server tests pass** — cold-start fallback, write-then-read, poison recovery, pre-built graph pattern, handle swap atomicity, search hot path.
- All critical risk scenarios (R-01 through R-15) have corresponding tests.

### Check 5: Code Quality

**Status**: PASS

**Build**: `cargo build --workspace` completes with 0 errors, 6 warnings (pre-existing, from non-crt-021 code).

**Deprecated shims removed (AC-01 resolved)**: `SupersessionGraph` type alias and `build_supersession_graph` wrapper are no longer present in `graph.rs`. A grep over all crates for `SupersessionGraph|build_supersession_graph` returns only one doc-comment line (`/// Typed relationship graph. Replaces \`SupersessionGraph\`.`). AC-01 is fully satisfied.

**sqlx-data.json (AC-19/NF-08 — false positive resolved)**: This codebase uses `sqlx::query()` runtime-checked queries exclusively. A grep over all crates for `sqlx::query!`, `sqlx::query_as!`, and `sqlx::query_scalar!` (compile-time macros) returns zero results — only the comment in `analytics.rs` line 368 referencing the deliberate runtime-only choice. No `sqlx-data.json` file is required or applicable. The NF-08/AC-19 requirement for `sqlx-data.json` was predicated on compile-time query macro usage that does not exist in this codebase. This is correctly classified as a false positive from the previous gate run.

**No stubs/placeholders**: No `todo!()`, `unimplemented!()`, `TODO`, or `FIXME` found in crt-021 implementation files.

**No `.unwrap()` in new non-test code**: The `.unwrap()` calls in `analytics.rs` lines 1129-1136 are inside a `#[tokio::test]` function. The `.unwrap()` at `background.rs` line 672 is guarded by `is_some()` and was present before crt-021 (confirmed via git history). No new unguarded `.unwrap()` was introduced by crt-021.

**File lengths**: `graph.rs` is 587 lines (the sole new file introduced by crt-021). Files modified by crt-021 that exceed 500 lines (`analytics.rs` 1295, `migration.rs` 1302, `read.rs` 1420, `background.rs` 2757) were all already well over 500 lines before crt-021 (confirmed via git history at the nxs-011 commit: 929, 1174, 1235, 2345 lines respectively). crt-021 added incremental code to pre-existing large files; it did not create them. No new files introduced by crt-021 exceed 500 lines.

**Pre-existing doctest failure**: `cargo test --workspace` reveals one doctest failure in `crates/unimatrix-server/src/infra/config.rs` — a `~/.unimatrix/config.toml` path example in a doc comment that the doctest runner tries to parse as Rust code. This was introduced by dsn-001 (#307), not by crt-021 (confirmed via `git log --follow`). All lib tests pass: 2549 tests, 0 failures, 18 ignored.

### Check 6: Security

**Status**: PASS

**Evidence**:
- No hardcoded secrets, API keys, or credentials in any crt-021 implementation file.
- Input validation at system boundaries: `analytics.rs` rejects non-finite weights (`weight.is_finite()` guard at drain time); all SQLite queries use parameterized `sqlx::query()` with `.bind()` — no string interpolation; `from_str()` for `RelationType` returns `None` for unknown strings (no panic).
- No path traversal vulnerabilities — no file path operations in crt-021 code.
- No command injection — no shell/process invocations in crt-021 code.
- Serialization/deserialization: unknown `relation_type` strings are logged as warnings and the edge is skipped (no silent misclassification, no panic), confirmed by `test_build_typed_graph_skips_unknown_relation_type`.
- No new dependencies introduced by crt-021.

### Check 7: Knowledge Stewardship

**Status**: PASS

**Evidence**: All implementation agent reports contain `## Knowledge Stewardship` sections:
- `crt-021-agent-3-engine-types-report.md`: Queried `/uni-query-patterns`; Stored ADR entry #2416.
- `crt-021-agent-4-store-schema-report.md`: Queried; Stored.
- `crt-021-agent-5-store-migration-report.md`: Queried; Stored / "nothing novel" with reason.
- `crt-021-agent-7-server-state-report.md`: Queried; Stored.
- `crt-021-agent-8-background-tick-report.md`: Queried; "nothing novel to store" with specific reason (patterns already captured in entries #1560, #732).

---

## Rework Required

None.

---

## Notes on Resolved Issues from Previous Gate

**Issue 1 (sqlx-data.json)**: Confirmed false positive. The codebase uses `sqlx::query()` runtime-checked queries only — no compile-time `query!()` macros exist. `sqlx-data.json` is not required.

**Issue 2 (deprecated SupersessionGraph shims)**: Confirmed resolved. The `SupersessionGraph` type alias and `build_supersession_graph` wrapper have been removed from `graph.rs`. Verified by grep: only doc-comment references remain.

## Notes on WARNs Not Escalated

**Tick sequence ordering (FR-24)**: VECTOR_MAP compaction is embedded inside `maintenance_tick → run_maintenance` rather than being a named separate step between GRAPH_EDGES compaction and TypedGraphState rebuild. The semantic ordering contract holds. Not a behavioral defect.

**Dual GraphEdgeRow types**: Two `GraphEdgeRow` structs (one in `unimatrix-store/src/read.rs`, one in `unimatrix-engine/src/graph.rs`) serve distinct crate-boundary purposes and are explicitly mapped in `typed_graph.rs::rebuild()`. Documented design decision.

**Pre-existing doctest failure**: `config.rs` doctest failure from dsn-001 is pre-existing and unrelated to crt-021.

## Knowledge Stewardship

- Stored: entry #2452 "Gate 3b false positive: sqlx-data.json not required when codebase uses runtime-only sqlx::query()" via /uni-store-lesson
