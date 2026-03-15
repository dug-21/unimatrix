# Risk Coverage Report: crt-014 — Topology-Aware Supersession

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `graph_penalty` priority rule ordering wrong | `graph::tests::penalty_range_all_scenarios`, `orphan_softer_than_clean_replacement`, `two_hop_harsher_than_one_hop`, `partial_supersession_softer_than_clean`, `dead_end_softer_than_orphan`, `decay_formula_depth_{1,2,5,10}` | PASS | Full |
| R-02 | Cycle detection misses valid cycle | `graph::tests::cycle_two_node_detected`, `cycle_three_node_detected`, `cycle_self_referential_detected`, `valid_dag_depth_{1,2,3}`, `empty_entry_slice_is_valid_dag`, `single_entry_no_supersedes` | PASS | Full |
| R-03 | `find_terminal_active` returns wrong node | `graph::tests::terminal_active_three_hop_chain`, `terminal_active_depth_one_chain`, `terminal_active_no_reachable`, `terminal_active_superseded_intermediate_skipped`, `terminal_active_absent_node`, `terminal_active_starting_node_is_active` | PASS | Full |
| R-04 | Edge direction reversed in graph construction | `graph::tests::edge_direction_pred_to_successor` | PASS | Full |
| R-05 | Test migration window — coverage gap | Behavioral ordering tests in `graph.rs` land in same commit as removal of 4 `confidence.rs` constant tests; `orphan_softer_than_clean_replacement`, `two_hop_harsher_than_one_hop`, `partial_supersession_softer_than_clean` | PASS | Full |
| R-06 | search.rs injects wrong successor after multi-hop | `test_search_multihop_injects_terminal_active` (infra-001 lifecycle) | PASS | Full |
| R-07 | `MAX_TRAVERSAL_DEPTH` not enforced | `graph::tests::terminal_active_depth_cap`, `terminal_active_depth_boundary` | PASS | Full |
| R-08 | Cycle fallback applied to wrong scope | Code review: guard `entry.superseded_by.is_some() \|\| entry.status == Deprecated` confirmed; unit test `search.rs::tests::cycle_fallback_uses_fallback_penalty` | PASS | Full |
| R-09 | Dangling `supersedes` reference panics | `graph::tests::dangling_supersedes_ref_is_skipped` | PASS | Full |
| R-10 | Graph construction blocks async executor | Code review: `build_supersession_graph` called at line 294 inside `spawn_blocking` closure (lines 274–291); confirmed by grep | PASS | Full |
| R-11 | Dead import causes compile error or warning | AC-14 grep: zero non-comment/non-test hits for `DEPRECATED_PENALTY\|SUPERSEDED_PENALTY`; `cargo build --workspace` exits 0 | PASS | Full |
| R-12 | Penalty hop decay formula out-of-range | `graph::tests::decay_formula_depth_{1,2,5,10}`, `decay_never_exceeds_clean_replacement` | PASS | Full |
| R-13 | petgraph feature set conflict | `cargo build --workspace` exits 0; `stable_graph` only in Cargo.toml | PASS | Full |
| IR-01 | `QueryFilter::default()` returns Active only | Code: search.rs lines 276–287 query each status explicitly (`Active`, `Deprecated`, `Proposed`, `Quarantined`); unit test confirms graph includes deprecated nodes | PASS | Full |
| IR-02 | Unified penalty guard condition | Code review: `entry.superseded_by.is_some() \|\| entry.status == Deprecated`; `test_search_deprecated_entry_visible_with_topology_penalty` exercises deprecated path | PASS | Full |
| IR-03 | `graph_penalty` called for non-penalized entries | Code review: guard condition skips Active entries with `superseded_by.is_none()` | PASS | Full |
| IR-04 | `thiserror` availability | `cargo build --workspace` exits 0; `GraphError` compiles | PASS | Full |

---

## Test Results

### Unit Tests

| Result | Count |
|--------|-------|
| Passed | 2507 |
| Failed | 0 |
| Ignored | 18 |

**Graph module unit tests (34 tests — all new for crt-014):**

| Test | Covers |
|------|--------|
| `all_active_no_penalty` | IR-03 |
| `cycle_self_referential_detected` | R-02 |
| `cycle_three_node_detected` | R-02 |
| `cycle_two_node_detected` | R-02 |
| `dangling_supersedes_ref_is_skipped` | R-09, AC-17 |
| `dead_end_softer_than_orphan` | R-01 |
| `decay_formula_depth_1` | R-12, AC-07 |
| `decay_formula_depth_2` | R-12, AC-07 |
| `decay_formula_depth_10_clamped` | R-12 |
| `decay_formula_depth_5_clamped` | R-12 |
| `decay_never_exceeds_clean_replacement` | R-12 |
| `edge_direction_pred_to_successor` | R-04, AC-04 |
| `empty_entry_slice_is_valid_dag` | R-02, edge case |
| `fallback_softer_than_clean` | R-05 |
| `graph_penalty_entry_not_in_slice` | edge case |
| `node_id_zero_not_in_graph` | edge case |
| `orphan_softer_than_clean_replacement` | R-01, R-05, AC-06 |
| `partial_supersession_softer_than_clean` | R-01, R-05, AC-08 |
| `penalty_absent_node_returns_one` | AC-05 |
| `penalty_range_all_scenarios` | R-01, AC-05 |
| `single_entry_no_supersedes` | R-02, edge case |
| `terminal_active_absent_node` | R-03, AC-10 |
| `terminal_active_depth_boundary` | R-07, AC-11 |
| `terminal_active_depth_cap` | R-07, AC-11 |
| `terminal_active_depth_one_chain` | R-03 |
| `terminal_active_no_reachable` | R-03, AC-10 |
| `terminal_active_starting_node_is_active` | R-03, edge case |
| `terminal_active_superseded_intermediate_skipped` | R-03 |
| `terminal_active_three_hop_chain` | R-03, AC-09 |
| `two_hop_harsher_than_one_hop` | R-01, R-05, AC-07 |
| `two_successors_one_active_one_deprecated` | R-01, edge case |
| `valid_dag_depth_1` | R-02, AC-04 |
| `valid_dag_depth_2` | R-02, AC-04 |
| `valid_dag_depth_3` | R-02, AC-04 |

### Integration Tests

**Smoke suite (mandatory gate):** 18 passed, 1 xfailed (pre-existing GH#111), 0 failed.

**Lifecycle suite (after new tests):** 22 passed, 2 xfailed (pre-existing), 0 failed.

**Tools suite:** 67 passed, 5 xfailed (pre-existing GH#233, GH#238), 0 failed.

**Combined lifecycle + tools:** 89 passed, 6 xfailed, 0 failed.

| New Test | Covers | Result |
|----------|--------|--------|
| `test_search_multihop_injects_terminal_active` | AC-13, R-06 | PASS |
| `test_search_deprecated_entry_visible_with_topology_penalty` | AC-12, IR-02 | PASS |

**Test iterations during development:**
- Initial run of both new tests: 2 failed due to test assertion errors (wrong status string `"superseded"` vs actual `"deprecated"`, and insufficient HNSW population with 2 entries).
- Triage: both were bad test assertions — wrong assumption about status string set by `context_correct` (it sets `Deprecated`, not a separate `Superseded` variant) and HNSW needing minimum population for recall.
- Fix: corrected status assertion; added 5 baseline entries for HNSW recall in topology penalty test.
- Final run: 2 passed.

---

## AC-14 Verification

```
$ grep -rn "DEPRECATED_PENALTY\|SUPERSEDED_PENALTY" crates/ --include="*.rs"
```

Results — all non-production hits:
- `search.rs:918`: comment `// (crt-014: DEPRECATED_PENALTY replaced by ...)`
- `search.rs:950`: comment `// (crt-014: SUPERSEDED_PENALTY replaced by ...)`
- `search.rs:1186`: test assertion string `"penalty must not equal old SUPERSEDED_PENALTY (0.5)"`
- `search.rs:1190`: test assertion string `"penalty must not equal old DEPRECATED_PENALTY (0.7)"`

Zero production declarations. Zero import statements. **AC-14: PASS.**

## AC-18 Verification

```
$ cargo build --workspace 2>&1 | grep "^error" | wc -l
0
```

Build warnings: 9 total (pre-existing: `unused import: super::disteez::*`, `unused import: self`, `unused import: CYCLE_STOP_EVENT` — all pre-existing, none related to crt-014 changes). **AC-18: PASS.**

---

## Gaps

None. All 13 risks (R-01 through R-13) and all 4 integration risks (IR-01 through IR-04) have test coverage.

**AC-16 note (cycle fallback — unit-test-only):** As documented in `test-plan/OVERVIEW.md`, AC-16 cannot be verified through the MCP interface because no tool allows creating a supersession cycle. Coverage is provided by `search.rs` unit test `cycle_fallback_uses_fallback_penalty` which directly tests the fallback path with injected `Err(CycleDetected)`. This is not a gap — it was documented as intentional in the test plan.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `cargo build --workspace` exits 0; petgraph `stable_graph` feature present in `unimatrix-engine/Cargo.toml` |
| AC-02 | PASS | `pub mod graph` in `lib.rs`; `cargo doc --package unimatrix-engine` succeeds |
| AC-03 | PASS | `graph::tests::cycle_two_node_detected`, `cycle_three_node_detected`, `cycle_self_referential_detected` all pass |
| AC-04 | PASS | `graph::tests::valid_dag_depth_{1,2,3}` pass; `edge_direction_pred_to_successor` confirms A→B edge direction |
| AC-05 | PASS | `graph::tests::penalty_range_all_scenarios` asserts `0.0 < result < 1.0` for all 6 topology scenarios |
| AC-06 | PASS | `graph::tests::orphan_softer_than_clean_replacement`: `ORPHAN_PENALTY (0.75) > CLEAN_REPLACEMENT_PENALTY (0.40)` |
| AC-07 | PASS | `graph::tests::two_hop_harsher_than_one_hop`: depth-2 penalty (0.24) < depth-1 penalty (0.40) |
| AC-08 | PASS | `graph::tests::partial_supersession_softer_than_clean`: `PARTIAL_SUPERSESSION_PENALTY (0.60) > CLEAN_REPLACEMENT_PENALTY (0.40)` |
| AC-09 | PASS | `graph::tests::terminal_active_three_hop_chain`: A→B→C returns `Some(C.id)` |
| AC-10 | PASS | `graph::tests::terminal_active_no_reachable`: chain terminating at Deprecated returns `None` |
| AC-11 | PASS | `graph::tests::terminal_active_depth_cap`: chain of 11 returns `None`; `terminal_active_depth_boundary`: chain of 10 returns `Some` |
| AC-12 | PASS | `test_search_deprecated_entry_visible_with_topology_penalty`: deprecated orphan visible in search, active entries rank above it; grep confirms `DEPRECATED_PENALTY` absent from search.rs imports |
| AC-13 | PASS | `test_search_multihop_injects_terminal_active`: A→B→C chain via `context_correct`; search finds C (id_c) in results |
| AC-14 | PASS | Grep returns zero production hits for `DEPRECATED_PENALTY` or `SUPERSEDED_PENALTY` |
| AC-15 | PASS | `confidence.rs` tests `deprecated_penalty_value`, `superseded_penalty_value`, `superseded_penalty_harsher_than_deprecated`, `penalties_independent_of_confidence_formula` absent; behavioral ordering tests `orphan_softer_than_clean_replacement`, `two_hop_harsher_than_one_hop`, `partial_supersession_softer_than_clean` present in `graph.rs` |
| AC-16 | PASS | Unit-only per test plan: `search.rs::tests::cycle_fallback_uses_fallback_penalty` verifies `CycleDetected` triggers `FALLBACK_PENALTY`; MCP interface cannot inject cycles (documented) |
| AC-17 | PASS | `graph::tests::dangling_supersedes_ref_is_skipped`: entry with `supersedes=Some(9999)` → `Ok(graph)`, no panic; graph has 1 node |
| AC-18 | PASS | `cargo build --workspace` exits 0; zero new warnings attributable to crt-014 |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for testing procedures — results returned (#487, #487 workspace test procedure, #296 service extraction, #750 pipeline validation, #553 worktree isolation) — none directly applicable to crt-014 graph testing.
- Stored: nothing novel to store — the graph-testing patterns used (HNSW minimum population for recall, behavioral ordering assertions, `context_correct` status behavior) are implementation details discoverable from the code. The triage pattern (bad test assertion vs pre-existing bug) follows the documented USAGE-PROTOCOL.md decision tree exactly.
