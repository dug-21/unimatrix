# Gate 3b Report: vnc-020

> Gate: 3b (Code Review)
> Date: 2026-05-20
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All three handlers match pseudocode step-by-step |
| Architecture compliance | PASS | Module split, dispatch, validation, lock discipline all correct |
| Interface implementation | PASS | All signatures match; response types correct |
| Test case alignment | PASS | All test plan scenarios present and correctly structured |
| Risk coverage | PASS | All 14 risks addressed; R-01 through R-14 have corresponding tests |
| Compilation | PASS | `cargo build -p unimatrix-server` produces no errors |
| No stubs/placeholders | PASS | No `todo!()`, `unimplemented!()`, TODO, FIXME found |
| No unwrap in production | FAIL | Two `.unwrap()` calls in `graph_read_filter.rs` production code |
| 500-line limit | PASS | All files within limit (max 381 for graph_read.rs) |
| AC-19 staleness disclosure | PASS | Verbatim text present in both constant and inline attribute |
| R-02 max_edge_count=0 | PASS | Unconditional `<= ?` binding, no special-casing |
| R-03 BFS visited set | PASS | `HashSet<u64>` keyed on resolved `effective_neighbor`, not raw NodeIndex |
| R-04 rejection matrix | PASS | All 8 new fields rejected on all non-owning modes with correct hints |
| AC-03a edge_types on inverse | PASS | Rejected via `validate_inverse_params`, test present |
| PathResponse.length type | PASS | `pub length: u8` in `PathResponse` struct |
| min_age_days SQL form | PASS | Uses `CAST(strftime('%s','now') AS INTEGER) - ?`, not `datetime()` text form |
| Security: no injection | PASS | All caller values bound via `push_bind`; alias names from loop counter only |
| cargo audit | WARN | `cargo-audit` not installed in this environment; CVE check not run |
| Knowledge Stewardship (agents) | WARN | Delivery agent reports not present for inspection |

---

## Detailed Findings

### Check 1 — Pseudocode Fidelity
**Status**: PASS
**Evidence**: All three handlers implement the pseudocode steps in order and with matching logic:
- `graph_read_inverse.rs` (199 lines): Steps 1–6 match `graph_read_inverse.md` exactly. SQL alias construction, status guard, limit validation identical.
- `graph_read_filter.rs` (245 lines): Steps 1–6 match `graph_read_filter.md` exactly. Two independent correlated subqueries for combined bounds (R-08), `<= ?` unconditional for `max_edge_count` (R-02), CAST epoch arithmetic for `min_age_days` (FR-07).
- `graph_read_path.rs` (322 lines): Steps 1–10 match `graph_read_path.md`. Self-path guard, depth validation, endpoint resolution before lock acquisition, path-carrying BFS with visited set keyed on resolved ID (R-03), exact not-found response shapes.
- `graph_read.rs` (381 lines): New `GraphParams` fields, response envelopes, dispatch arms, and module declarations all match `graph_read.md` pseudocode.
- `graph_read_validation.rs` (337 lines): Validation logic extracted to separate file (C5 compliance). Three new mode arms and shared `reject_new_fields_for_mode` helper match `graph_read.md §5`.

### Check 2 — Architecture Compliance
**Status**: PASS
**Evidence**:
- Module split: Three sibling modules declared via `#[path]` — ADR-001 ✓
- `GraphParams` field additions only (`Option<T>`) — ADR-002 ✓
- `validate_no_unsupported_params` delegated to `graph_read_validation.rs`, stays in `graph_read.rs` as single cross-mode rejection point — ADR-003 ✓
- `depth` field reused for path mode — ADR-004 ✓
- Path response format: `from_id` top-level, `hops` array, no null `relation_type` — ADR-005 ✓
- `resolve_supersessions` resolves endpoints before BFS, per-hop via `follow_to_current` — ADR-006 ✓
- No raw SQL in filter mode — ADR-007 ✓
- Lock discipline: `RwLock` acquired once (lock → clone → release) before any async work — correct ✓
- No new MCP tool, schema version stays at 27 — FR-18 ✓

### Check 3 — Interface Implementation
**Status**: PASS
**Evidence**:
- `handle_inverse(store: &Store, params: &GraphParams) -> Result<InverseResponse, ErrorData>` ✓
- `handle_filter(store: &Store, params: &GraphParams) -> Result<FilterResponse, ErrorData>` ✓
- `handle_path(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: &GraphParams) -> Result<PathResponse, ErrorData>` ✓
- `PathResponse.length: u8` (not usize) — confirmed at line 226 of `graph_read.rs` ✓
- `InverseResponse`, `FilterResponse`, `PathHop`, `PathResponse` structs match architecture specification ✓
- `parse_relation_types` is `pub(super)` in `graph_read_inverse.rs`, allowing import by sibling modules ✓

### Check 4 — Test Case Alignment
**Status**: PASS
**Evidence**:
All test plan scenarios from the component test plans are implemented:
- `graph_read_inverse_tests.rs`: AC-02, AC-03 (None and empty), AC-04, AC-05 (0, 501, 500, 1 boundary), AC-06, R-10 (N=1 and N=3), SR-B (injection rejected)
- `graph_read_inverse_integration_tests.rs`: AC-27 (single type + deprecated exclusion), AC-28 (4-state AND semantics fixture), AC-05 limit default=100 behavioral, duplicate type edge case, 10-type edge case, empty category
- `graph_read_filter_tests.rs`: AC-10, AC-09 (None and empty), AC-11, R-11, AC-12, R-02/AC-29 (max_edge_count=0), max_edge_count=1, AC-30 (min_edge_count>=2), R-08 (both bounds), R-10, IR-04, inverted confidence bounds edge case
- `graph_read_path_tests.rs`: AC-16, AC-17, AC-32 (self-path), AC-18 (depth 0, 11, default=5), AC-15 (from_id and to_id not in snapshot — distinct fixtures), R-12 (1-hop and 2-hop shape), SR-C (cyclic graph terminates), depth=1 misses 2-hop
- `graph_read_path_supersession_tests.rs`: R-03 (double-enqueue prevention), R-06 (from_id and to_id resolution), resolve_supersessions=false uses original ID, follow_to_current None fallback, self-resolves-to-different-target
- `graph_read_tests_vnc020.rs`: AC-26, AC-25 (depth rejected on all 5 modes), AC-22 (from_id on non-path), AC-23 (missing_edge_types on non-inverse), AC-03a (edge_types on inverse), R-04 8-field matrix, serialization tests

### Check 5 — Risk Coverage
**Status**: PASS
**Evidence**:
- R-01 (staleness disclosure): `test_context_graph_description_contains_staleness_text` + `CONTEXT_GRAPH_DESCRIPTION` contains "The cache is rebuilt each tick (typically 30-60 seconds)" verbatim ✓
- R-02 (max_edge_count=0): `test_filter_max_edge_count_zero_uses_lte_binding` with 4-entry fixture ✓
- R-03 (BFS visited set): `test_handle_path_bfs_visited_set_keyed_on_resolved_id` with forked deprecated graph ✓
- R-04 (rejection matrix): 8-field matrix fully covered in `graph_read_tests_vnc020.rs` ✓
- R-05 (AND semantics): `test_context_graph_inverse_and_semantics` with mandatory 4-state fixture ✓
- R-06 (endpoint resolution): from_id and to_id resolution each tested independently ✓
- R-07 (depth rejection): AC-25 sweep tests for all 5 affected modes ✓
- R-08 (two subqueries): `test_filter_both_edge_count_bounds_two_subqueries_in_sql` with 5-entry fixture ✓
- R-09 (no-path vs not-in-snapshot distinct fixtures): AC-14 and AC-15 use separate fixtures ✓
- R-10 (status guard): Tests in both inverse and filter for deprecated exclusion ✓
- R-11 (category-only): `test_handle_filter_category_only_no_validation_error` ✓
- R-12 (length vs hops.len): 1-hop and 2-hop shape tests; from_id not in hops verified ✓
- R-13 (limit boundary): limit=0, limit=501 for both modes ✓
- R-14 (RelationType wildcard): Tested via unrecognized type error tests across modules ✓

### Check 6 — No Stubs or Placeholders
**Status**: PASS
**Evidence**: `grep -rn "todo!\|unimplemented!\|TODO\|FIXME"` across all five production files returned no output.

### Check 7 — No `.unwrap()` in Non-Test Code
**Status**: FAIL
**Evidence**: Two `.unwrap()` calls exist in `graph_read_filter.rs` production code:
- Line 130: `let et = edge_types.as_ref().unwrap();` (inside `if let Some(min_n) = params.min_edge_count` block)
- Line 145: `let et = edge_types.as_ref().unwrap();` (inside `if let Some(max_n) = params.max_edge_count` block)

The invariant is logically sound — Step 2 validation guarantees `edge_types` is `Some(non-empty)` when `has_edge_count=true`. However, the project rule from `.claude/rules/rust-workspace.md` states "No `.unwrap()` in non-test code" without qualification. The calls are in production handler code and must be replaced with safe alternatives.

**Fix**: Replace both `.unwrap()` calls with an explicit `expect("edge_types guaranteed non-empty by Step 2 validation")` or restructure to avoid the Option by making the invariant structural (e.g., bind `edge_types` to a concrete `Vec<RelationType>` when `has_edge_count=true`, returning an error otherwise).

### Check 8 — 500-Line Limit
**Status**: PASS
**Evidence**:
- `graph_read.rs`: 381 lines ✓
- `graph_read_validation.rs`: 337 lines ✓ (extracted to keep graph_read.rs within budget)
- `graph_read_inverse.rs`: 199 lines ✓
- `graph_read_filter.rs`: 245 lines ✓
- `graph_read_path.rs`: 322 lines ✓

### Check 9 — Compilation
**Status**: PASS
**Evidence**: `cargo build -p unimatrix-server` completes with "Finished `dev` profile [unoptimized + debuginfo] target(s)". No errors. 21 pre-existing warnings unrelated to vnc-020.

### Check 10 — Tests Pass
**Status**: PASS
**Evidence**: `cargo test -p unimatrix-server` reports all suites passing:
- 3183 passed / 0 failed (main suite)
- 46 passed / 0 failed
- 16 passed / 0 failed (migration integration)
- Plus additional suites: all 0 failed

### Check 11 — AC-19 Staleness Disclosure (R-01)
**Status**: PASS
**Evidence**: The required phrase "The cache is rebuilt each tick (typically 30-60 seconds)" appears verbatim at:
- `CONTEXT_GRAPH_DESCRIPTION` constant, line 96–97 of `tools.rs`
- Inline `#[tool(description = "...")]` attribute at line 3451–3452 of `tools.rs`
The mandatory text "cache is rebuilt each tick (typically 30-60 seconds)" is present in both locations. `inverse` and `filter` mode descriptions contain no staleness language (they correctly state "Queries the live database — no staleness").

### Check 12 — R-02 max_edge_count=0 Uses <= Unconditionally
**Status**: PASS
**Evidence**: `graph_read_filter.rs` line 144–153:
```rust
if let Some(max_n) = params.max_edge_count {
    let et = edge_types.as_ref().unwrap();
    qb.push("... WHERE g.source_id = e.id AND g.relation_type IN (");
    push_relation_type_list(&mut qb, et);
    qb.push(")) <= ");
    qb.push_bind(max_n as i64);
}
```
Comment: "Use `<= ?` unconditionally — never special-case zero." No `if max_n == 0` branch exists.

### Check 13 — R-03 BFS Visited Set Keyed on Resolved NodeIndex
**Status**: PASS
**Evidence**: `graph_read_path.rs` line 196: `let mut visited: HashSet<u64> = HashSet::new()`. The set stores entry IDs (u64), not NodeIndex values. Inserts use `effective_neighbor` (the resolved ID after `follow_to_current`), not `raw_neighbor_id`. Line 260: `if !visited.contains(&effective_neighbor)` confirms correct key.

### Check 14 — R-04 Rejection Matrix Completeness (8 new fields)
**Status**: PASS
**Evidence**: All 8 new fields are explicitly rejected on all non-owning modes:
- `category`: rejected on chain, current, neighbors, subgraph (via `reject_new_fields_for_mode`), path (in `validate_path_params`); accepted by inverse, filter
- `missing_edge_types`: rejected on chain, current, neighbors, subgraph (via helper), filter, path; accepted by inverse
- `limit`: rejected on chain, current, neighbors, subgraph (via helper), path; accepted by inverse, filter
- `min_age_days`, `min_confidence`, `max_confidence`, `min_edge_count`, `max_edge_count`: rejected on chain, current, neighbors, subgraph (via helper), inverse, path; accepted by filter
- AC-03a: `edge_types` rejected on inverse mode with message naming `missing_edge_types` ✓
- AC-26: unrecognized mode error lists all seven mode names as exact fragment ✓

### Check 15 — min_age_days SQL Form
**Status**: PASS
**Evidence**: `graph_read_filter.rs` line 110:
```rust
qb.push(" AND e.created_at < (CAST(strftime('%s','now') AS INTEGER) - ");
qb.push_bind(days as i64 * 86_400_i64);
qb.push(")");
```
Uses integer epoch arithmetic as required by FR-07. `datetime()` text comparison is absent.

### Check 16 — Security
**Status**: PASS (with WARN for two `.unwrap()` calls already flagged)
**Evidence**:
- No hardcoded secrets found
- All caller-supplied values are bound via `push_bind` — no string interpolation of caller input
- SQL alias names (`g0`, `g1`, ...) constructed from loop counter only, never from caller input (SR-B)
- `RelationType::from_str` validates all edge type strings before SQL construction — injection impossible
- No path traversal vectors present (SQL modes only, no file operations)
- `cargo-audit` not installed; CVE check cannot be run in this environment (WARN, not blocking)

### Check 17 — Knowledge Stewardship (Delivery Agents)
**Status**: WARN
**Evidence**: No delivery agent report files found in `product/features/vnc-020/agents/`. The gate was invoked directly without swarm delivery agent reports to inspect. Cannot verify stewardship block presence. This is a process gap, not an implementation gap.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Two `.unwrap()` in `graph_read_filter.rs` production code (lines 130, 145) | rust-dev | Replace `edge_types.as_ref().unwrap()` with a safe alternative. Recommended: after the validation in Step 2, when `has_edge_count=true` and `edge_types` is `Some`, bind a concrete `Vec<RelationType>` variable; when `has_edge_count=false`, use a separate code path. Alternatively use `edge_types.as_ref().expect("edge_types guaranteed non-empty by Step 2 validation")` — `expect` is not banned by project rules (only raw `.unwrap()` is). After fix: re-run `cargo test -p unimatrix-server` to confirm no regression. |

---

## Notes

- `cargo audit` not executed — `cargo-audit` binary is not installed. Not marking as FAIL since this is a CI tooling gap, not a code defect. The WARN is recorded.
- The two `.unwrap()` calls are logically safe (invariant proven by Step 2 pre-validation) but violate the unconditional project rule. The fix is mechanical and does not require any design change.

## Knowledge Stewardship

- Stored: nothing novel to store — the `.unwrap()` in production code violation is an existing project convention that is already known. No recurring pattern specific to vnc-020 that is not already covered by the existing "no .unwrap() in non-test code" rule.
