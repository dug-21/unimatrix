# Gate 3b Rework1 Report: vnc-020

> Gate: 3b (Code Review — rework iteration 1)
> Date: 2026-05-20
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| .unwrap() fix verified — graph_read_filter.rs | PASS | Lines 130, 145 now use `.expect(...)` with rationale |
| .unwrap() fix verified — graph_read_inverse.rs | PASS | No .unwrap() in production code |
| .unwrap() fix verified — graph_read_path.rs | PASS | No .unwrap() in production code; `unwrap_or` usage is correct |
| Pseudocode fidelity | PASS | All handlers match pseudocode specifications exactly |
| Architecture compliance | PASS | Component boundaries, ADR decisions, lock discipline all followed |
| Interface implementation | PASS | All function signatures match ARCHITECTURE.md §Integration Surface |
| Test case alignment | PASS | All AC test scenarios covered across unit + integration test files |
| Code compiles | PASS | `cargo build -p unimatrix-server` — 0 errors, 21 warnings (all pre-existing) |
| No stubs or placeholders | PASS | No todo!(), unimplemented!(), TODO, FIXME found |
| No .unwrap() in production code | PASS | Confirmed by grep — zero matches across all three new modules |
| File size limits | PASS | All files under 500 lines (graph_read.rs 381, validation 337, inverse 199, filter 245, path 322) |
| Security — SQL injection surface | PASS | All values bound via push_bind; RelationType::from_str validates before SQL |
| Staleness disclosure in tools.rs | PASS | Exact text present at lines 96-102: "path mode uses the in-memory graph cache..." |
| Knowledge stewardship | PASS | Agent reports not directly in scope for this gate; previous gate report confirmed |

## Detailed Findings

### .unwrap() Fix — Primary Rework Target

**Status**: PASS

**Evidence**: Grep of all three production modules returns no output — zero `.unwrap()` calls present.

The two previously failing lines in `graph_read_filter.rs` (130, 145) have been replaced with:
```
.expect("edge_types guaranteed non-empty by Step 2 validation")
```

This is the correct approach: the `.expect()` message documents the invariant established at Step 2 (`has_edge_count` guard), making the panic semantically impossible in the valid execution path. The same pattern appears at line 145 for `max_edge_count`. Both uses are in-line with the project rule that `.expect()` with a clear invariant message is acceptable where the precondition is structurally enforced.

In `graph_read_path.rs`, `unwrap_or` is used (not `unwrap()`) at lines 122, 128, 231 as fallback for `follow_to_current` returning `None` — these are correct and intentional per ADR-006.

### Compilation

**Status**: PASS

**Evidence**: `cargo build -p unimatrix-server` exits clean with 0 errors. 21 warnings present, all pre-existing (not introduced by vnc-020).

### Test Results

**Status**: PASS

**Evidence**: All test suites pass with 0 failures across all crates. The unimatrix-server suite (3183 tests) and integration test suites (16 + 46 tests) all pass.

### Pseudocode Fidelity

**Status**: PASS

**Evidence**:
- `handle_inverse` follows the 6-step flow exactly: validate category → validate missing_edge_types → validate limit → build parameterized antijoin SQL → execute → return InverseResponse.
- `handle_filter` follows the 6-step flow: validate category → validate edge_types conditional → validate limit → build correlated subquery → execute → return FilterResponse.
- `handle_path` follows the 10-step BFS flow: validate from_id → validate to_id → self-path guard → validate depth → validate edge_types → resolve supersession endpoints → acquire graph snapshot → snapshot guards → BFS traversal with path-carrying frontier → return PathResponse.
- `validate_no_unsupported_params` delegation to `graph_read_validation.rs` is correct; all mode arms present.

### Architecture Compliance

**Status**: PASS

**Evidence**:
- ADR-001: Three sibling modules correctly split. `validate_no_unsupported_params` stays in `graph_read.rs` and delegates to `graph_read_validation.rs` (itself a separate file to respect C5).
- ADR-002: 8 new `GraphParams` fields are all `Option<T>`, backward-compatible.
- ADR-003: AND semantics confirmed in `handle_inverse` — one LEFT JOIN per type, all NULL checks ANDed.
- ADR-004: `depth` field reused for path mode default=5 range [1,10].
- ADR-005: PathResponse has `from_id` as top-level field; hops array excludes `from_id`; `length == hops.len()`.
- ADR-006: `resolve_supersessions` resolves endpoints before BFS; per-hop resolution uses `follow_to_current` (reused from `graph_read_neighbors`).
- ADR-007: No raw SQL from callers — all filter clauses use `push_bind`.
- RwLock discipline: lock acquired once, graph cloned, lock released before any async work.
- `CategoryAllowlist` poison recovery pattern (`unwrap_or_else(|e| e.into_inner())`) used correctly at line 137 of `graph_read_path.rs`.

### Interface Implementation

**Status**: PASS

**Evidence**: All function signatures match ARCHITECTURE.md §Integration Surface exactly:
- `handle_inverse(store: &Store, params: &GraphParams) -> Result<InverseResponse, ErrorData>` — confirmed.
- `handle_filter(store: &Store, params: &GraphParams) -> Result<FilterResponse, ErrorData>` — confirmed.
- `handle_path(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: &GraphParams) -> Result<PathResponse, ErrorData>` — confirmed.
- `pub(super)` visibility on all handlers — confirmed.
- Response structs match wire format specification exactly.

### Test Case Alignment with Component Test Plans

**Status**: PASS

**Evidence**:

**inverse module** (graph_read_inverse_tests.rs + graph_read_inverse_integration_tests.rs):
- AC-02: unrecognized edge type error names the value and lists all 16 types — tested
- AC-03: missing_edge_types absent/empty errors — tested (both None and empty vec)
- AC-03a: edge_types rejected on inverse mode — tested in vnc020 validation tests
- AC-04: category absent error — tested
- AC-05: limit boundary (0, 501, 500, 1) — tested; behavioral default=100 tested
- AC-06: total_returned == entries.len() — tested
- AC-27: single-type integration with deprecated+active+with-edge fixture — tested
- AC-28: AND semantics 4-state fixture — tested
- R-10: status=0 guard (N=1 and N=3 LEFT JOINs) — tested
- SR-B: SQL injection via crafted type string — tested
- IR-01: 10 LEFT JOINs, duplicate types — tested

**filter module** (graph_read_filter_tests.rs):
- AC-09: edge_types required with edge count constraints — tested (None and empty vec)
- AC-10: category absent/empty errors — tested
- AC-11: limit boundary — tested
- AC-12: total_returned invariant — tested
- AC-29: max_edge_count=0 boundary (4-entry fixture) — tested
- AC-30: min_edge_count >= 2 — tested
- R-02: max_edge_count=0 uses `<= ?` binding — tested behaviorally
- R-08: both bounds produce two independent subqueries — tested behaviorally (5-entry fixture)
- R-10: deprecated entries excluded — tested
- R-11: category-only query valid — tested
- IR-04: multi-type edge_types IN clause — tested

**path module** (graph_read_path_tests.rs + graph_read_path_supersession_tests.rs):
- AC-15: from_id not in snapshot → found: false not Err — tested
- AC-16: from_id required — tested
- AC-17: to_id required — tested
- AC-18: depth default=5, depth=0 rejected, depth=11 rejected — tested
- AC-20/AC-21: endpoint resolution reflected in response (from_id and to_id) — tested
- AC-32: self-path returns found: false — tested
- R-03: visited set keyed on resolved ID (double-enqueue prevention) — tested with forked deprecated graph
- R-06: endpoint resolution in response — tested for both from_id and to_id
- R-09: snapshot absence distinct from no-path (two separate fixtures) — tested
- R-12: path response shape (1-hop, 2-hop, from_id not in hops) — tested
- SR-C: BFS cycle termination — tested

**validation** (graph_read_tests_vnc020.rs):
- AC-22: from_id rejected on chain/current/neighbors/subgraph/filter — tested
- AC-23: missing_edge_types rejected on all non-inverse modes — tested
- AC-24: filter-only params on wrong modes — tested (8 fields × minimum one mode)
- AC-25: depth rejected on 5 modes (chain/current/subgraph/inverse/filter) — tested; regression for neighbors and path acceptance
- AC-26: unrecognized mode lists all 7 modes — tested with exact fragment check
- R-04: 8-field rejection matrix — all 8 new fields have at least one wrong-mode rejection test

### Security

**Status**: PASS

**Evidence**:
- No hardcoded secrets or credentials.
- All SQL values bound via `push_bind` — no string interpolation of caller values.
- `RelationType::from_str` validates all edge type strings before SQL construction in both `inverse` and `filter` modes.
- SQL alias names (`g0`, `g1`, ...) derived from loop counter only, never from caller input.
- Fuzz-style boundary inputs (extreme `u32`, `f64::INFINITY`) handled by typed Rust parameters.
- SQL injection test for crafted `missing_edge_types` value present and passing.
- Input validation at MCP boundary via `validate_no_unsupported_params` before handler dispatch.

### Tools.rs Staleness Disclosure (AC-19 / R-01)

**Status**: PASS

**Evidence**: tools.rs lines 96-102 contain the required text:

> "path mode uses the in-memory graph cache for BFS traversal. The cache is rebuilt each tick (typically 30-60 seconds). Edges written within the current tick interval may not appear in the result. This is the same staleness contract as neighbors mode at depth>1 and subgraph mode. If from_id or to_id is not present in the current graph snapshot, the result is { found: false } — not an error."

Additionally, lines 82 and 90 explicitly state "Queries the live database — no staleness" for inverse and filter modes respectively, satisfying SR-01.

### File Size Compliance (C5 / IR-03)

**Status**: PASS

All files are well within the 500-line limit:
- graph_read.rs: 381 lines
- graph_read_validation.rs: 337 lines
- graph_read_inverse.rs: 199 lines
- graph_read_filter.rs: 245 lines
- graph_read_path.rs: 322 lines

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store — the `.expect()` with invariant message as a replacement for `.unwrap()` in structurally-enforced preconditions is a well-established project pattern. The specific gate failure (`.unwrap()` in production code) is already captured as lesson #4473 (warn+continue anti-pattern) and the general rule is in rust-workspace.md. No new lesson entry is warranted from this rework iteration.
