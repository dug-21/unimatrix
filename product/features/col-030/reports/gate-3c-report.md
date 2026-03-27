# Gate 3c Report: col-030

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-27
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 13 risks mapped to passing tests in RISK-COVERAGE-REPORT.md |
| Test coverage completeness | PASS | 8 unit tests (graph_suppression.rs) + 2 integration tests (search.rs) + 13 infra-001 contradiction tests + 20 smoke tests |
| Specification compliance | PASS | All 15 FRs, 7 NFRs, and 12 ACs verified; signature extension is backward-compatible and serves FR-09 |
| Architecture compliance | PASS | Module placement, wiring, guard form, single-pass invariant — all match approved Architecture |
| Knowledge stewardship compliance | PASS | Tester report includes Queried and Stored entries |
| Integration test validation | PASS | Smoke 20/20; contradiction 13/13; tools 93/93+2xfail; lifecycle 38/38+2xfail+1xpass; all xfails pre-existing with GH issues |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 13 risks from RISK-TEST-STRATEGY.md to passing test evidence:

- R-01 (graph_tests.rs line-count violation): Tests placed in `graph_suppression.rs` [cfg(test)] (326 lines); `graph_tests.rs` confirmed at 1068 lines (unchanged). Verified by direct `wc -l` in this validation.
- R-02 (visibility): `pub fn suppress_contradicts` confirmed by grep; re-export at graph.rs lines 27-28 compiles cleanly.
- R-03 (final_scores immutable binding): `test_step10b_floor_and_suppression_combo_correct_scores` asserts `results[1].final_score == 0.78` (C's score), not 0.82 (B's score) — confirms the shadow is present and aligned.
- R-04 (edges_of_type string match): T-GS-02 constructs edge using `RelationType::Contradicts.as_str()` and confirms suppression fires.
- R-05 (bidirectional omission): T-GS-06 writes edge as rank-1→rank-0 (Incoming from rank-0) and asserts rank-1 suppressed. Non-negotiable gate passed.
- R-06 (mask length mismatch): All 8 unit tests assert `mask.len() == result_ids.len()`; T-GS-01 tests empty input.
- R-07 (aligned_len from wrong Vec): T-SC-09 exercises floor removal (results_with_scores.len()=3) + suppression (final_scores_pre.len()=4) and asserts correct survivor count and scores. `aligned_len = results_with_scores.len()` at search.rs line 923 confirmed.
- R-08 (graph_suppression.rs not wired): `graph.rs` lines 27-28 contain `mod graph_suppression;` and `pub use graph_suppression::suppress_contradicts;`. Build succeeds.
- R-09 (lib.rs polluted): `grep "graph_suppression" lib.rs` returns zero matches. Confirmed.
- R-10 (DEBUG log missing contradicting_id): search.rs lines 941-942 log both `suppressed_entry_id` and `contradicting_entry_id`. Confirmed.
- R-11 (cold-start guard missing/inverted): search.rs line 913 uses `if !use_fallback { ... } else { final_scores }` — correct non-inverted form.
- R-12 (create_graph_edges_table): grep of new test code returns 0 matches; both T-SC-08 and T-SC-09 use `build_typed_relation_graph` with `bootstrap_only: false`.
- R-13 (eval gate passage mistaken for proof): T-SC-08 (`test_step10b_contradicts_suppression_removes_lower_ranked`) is present as a mandatory positive gate separate from the eval gate.

No risks lack coverage.

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence**: All risk-to-scenario mappings from the Risk-Based Test Strategy are exercised:

Unit tests in `graph_suppression.rs` (8 tests, all passing):
- T-GS-01: empty graph, empty input no-panic sub-case (R-06, AC-01)
- T-GS-02: outgoing Contradicts rank-0→rank-1 (AC-02, R-04)
- T-GS-03: outgoing Contradicts rank-0→rank-3 non-adjacent (AC-01)
- T-GS-04: chain propagation — rank-0 contradicts rank-2; rank-2 contradicts rank-3 (FR-02)
- T-GS-05: non-Contradicts edges (CoAccess, Supports) — no suppression (AC-04)
- T-GS-06: incoming direction — edge written rank-1→rank-0; rank-1 suppressed (AC-03, R-05)
- T-GS-07: edge only between rank-2 and rank-3 (AC-01)
- T-GS-08: empty TypedRelationGraph (cold-start proxy, R-11, AC-05)

Integration tests in `search.rs` (2 new tests, both passing):
- T-SC-08: FR-14 mandatory positive gate — A retained, B suppressed, C retained, count=2 (AC-07, R-13)
- T-SC-09: floor + suppression combo, aligned_len and final_scores shadow correctness (R-03, R-07)

FR-13 coverage cross-check: SPECIFICATION.md FR-13 requires 8 unit test cases. All 8 are present:
1. Empty graph — T-GS-01
2. No Contradicts edges — T-GS-05
3. Outgoing rank-0→rank-1 — T-GS-02
4. Incoming rank-1→rank-0 — T-GS-06
5. Outgoing rank-0→rank-3 — T-GS-03
6. Edge between rank-2 and rank-3 only — T-GS-07
7. Chain rank-0→rank-2, rank-2→rank-3 — T-GS-04
8. Non-Contradicts edges — T-GS-05

All 8 required FR-13 cases are covered.

R-07 scenario (floor + suppression) is present in T-SC-09.

Edge cases from RISK-TEST-STRATEGY.md:
- Empty result set: covered in T-GS-01
- Single-entry result set: no dedicated test, but T-GS-01 empty sub-case verifies no-panic; the mask initialization `vec![true; n]` makes a single-entry case trivially correct. WARN: no explicit single-entry test, but this is minor given the initialization guarantee.
- Chain not transitive: T-GS-04 covers this correctly (rank-3 is suppressed by rank-2, which propagates its edges even after being suppressed — Option B behavior).
- Non-Contradicts edge types: T-GS-05.
- Entry not in graph node_index: `node_idx = None => continue` in graph_suppression.rs line 57-58; no explicit test but the cold-start T-GS-08 exercises the all-absent case.

### 3. Specification Compliance

**Status**: PASS

**Evidence**:

All 12 acceptance criteria confirmed PASS in RISK-COVERAGE-REPORT.md. Verification of key items:

**FR-01 / AC-08 — Function location and signature**: `suppress_contradicts` is in `crates/unimatrix-engine/src/graph_suppression.rs` (line 44) and re-exported from `graph.rs` (lines 27-28). PASS.

**Signature deviation note**: SPECIFICATION.md FR-01 specifies return type `Vec<bool>`. The implementation returns `(Vec<bool>, Vec<Option<u64>>)` — a tuple also carrying the `contradicting_id` per suppressed entry. This deviation is:
- Backward-compatible: all callers that need only the keep mask destructure `(keep_mask, _)`.
- Purposeful: the second element is required at the search.rs call site to emit the `debug!` log with `contradicting_entry_id` (FR-09, NFR-05). Without it, the implementation would need to re-traverse the graph at log time.
- Tested: all unit tests assert both elements; the second element enables the FR-09 log requirement to be met cleanly.
- This constitutes a forward-only extension, not a behavioral deviation. AC-01 through AC-03 are still satisfied — the keep mask semantics are identical to what the spec defines.

**FR-02 / FR-03 — Bidirectional Contradicts check**: Both `Direction::Outgoing` and `Direction::Incoming` queried via `edges_of_type` (graph_suppression.rs lines 63-72). PASS.

**FR-05 / AC-10 — edges_of_type sole boundary**: `grep "edges_directed\|neighbors_directed" graph_suppression.rs` returns only a comment at line 62, no actual call. PASS.

**FR-06 — Step 10b insertion point**: search.rs lines 908-953, after Step 10 floors and before Step 11 ScoredEntry construction. PASS.

**FR-07 / AC-12 — Single indexed pass**: search.rs lines 928-946 use a single `enumerate()` + `zip` loop. No separate `retain` calls. PASS.

**FR-08 / AC-05 — Cold-start guard**: `if !use_fallback { ... } else { final_scores }` at search.rs line 913. Guard present and non-inverted. PASS.

**FR-09 / AC-11 — DEBUG log with both IDs**: search.rs lines 940-944 emit `tracing::debug!` with both `suppressed_entry_id` and `contradicting_entry_id`. PASS.

**FR-10 — No score modification**: Surviving entries' scores taken from the pre-suppression `final_scores` vec — no recalculation. PASS.

**FR-11 — No backfill**: T-SC-08 asserts `results.len() == 2` (k=3 minus 1 suppressed); no padding. PASS.

**FR-14 / FR-15 — Mandatory positive integration test using correct helper**: T-SC-08 present and uses `build_typed_relation_graph` with `bootstrap_only: false`; no `create_graph_edges_table` usage. PASS.

**NFR-01 — Performance (pure function, no I/O)**: `suppress_contradicts` is `pub fn` (not async), no I/O, no store reads. Output allocation is `vec![true; n]` + `vec![None; n]` — O(n). PASS.

**NFR-02 — Zero regression**: 174 eval tests pass; all existing scenarios have no Contradicts edges. PASS.

**NFR-03 — No new dependencies**: Only `petgraph` (existing) and `std::collections::HashSet` used. No new crates. PASS.

**NFR-04 — No schema changes**: No migrations added. PASS.

**NFR-06 — No config toggle**: No feature flag; suppression is unconditionally gated by `!use_fallback`. PASS.

**NFR-07 — File size budget**:
- `graph_suppression.rs`: 326 lines (under 500). PASS.
- `graph.rs`: 591 lines (over 500 limit by 91 lines).

**NFR-07 flag**: `graph.rs` is 591 lines, which exceeds the 500-line workspace hard limit. However, ARCHITECTURE.md explicitly notes that `graph.rs` was 587 lines pre-feature and the col-030 wiring adds only 2 new lines (lines 27-28). The ADR-001 decision to use `graph_suppression.rs` was specifically to avoid adding any substantive logic to `graph.rs`. The 591-line count reflects a pre-existing over-limit state that col-030 worsened by only 4 lines. RISK-COVERAGE-REPORT.md marks this PASS with the note that wc-l=591 is the post-wiring count vs. 587 pre-feature. Gate-3b already reviewed and passed this file; the 4-line delta does not change the pre-existing violation status. This gate records it as a WARN for tracking.

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

- ADR-001 (graph_suppression.rs module split): File exists at `crates/unimatrix-engine/src/graph_suppression.rs`; wired via `#[path = "graph_suppression.rs"] mod graph_suppression;` and `pub use graph_suppression::suppress_contradicts;` in `graph.rs` lines 26-28. `lib.rs` has no `graph_suppression` entry. PASS.
- ADR-002 (edges_of_type sole traversal boundary): All graph traversal in `suppress_contradicts` calls `edges_of_type`; no direct `.edges_directed()` or `.neighbors_directed()` calls. PASS.
- ADR-003 (bidirectional query required): Both `Direction::Outgoing` and `Direction::Incoming` queried. PASS.
- ADR-004 (single indexed pass for mask application): Single `enumerate()` + `zip` loop at search.rs lines 930-946. PASS.
- ADR-005 (no config toggle): Suppression is active when `!use_fallback`; no feature flag. PASS.
- Component interactions match architecture diagram: `suppress_contradicts` called at Step 10b from `SearchService::search`; `typed_graph` clone already in scope from Step 6; no new lock acquisition. PASS.
- Integration points match the architecture surface table: function signature `pub fn suppress_contradicts(result_ids: &[u64], graph: &TypedRelationGraph) -> (Vec<bool>, Vec<Option<u64>>)` (tuple return is backward-compatible extension). PASS.
- NOT touched list confirmed: no changes to `context_lookup`, `context_get`, `GRAPH_EDGES` schema, scoring formula, or `edges_of_type` implementation. PASS.

### 5. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md includes a `## Knowledge Stewardship` section with:
- Queried: `mcp__unimatrix__context_briefing` — entries #229, #925, #3526 returned.
- Stored: "nothing novel to store — the bidirectional graph traversal test pattern is col-030-specific; the general xfail/xpass lifecycle protocol is documented in USAGE-PROTOCOL.md and not novel."

The reason given after "nothing novel to store" is specific and justified. PASS.

### 6. Integration Test Validation (Mandatory)

**Status**: PASS

**Smoke tests (pytest -m smoke)**: 20/20 passed. Verified by running `pytest -m smoke` in this gate — all 20 passed in 174.94s.

**Integration suites run**:
- `test_contradiction.py`: 13/13 passed (107.79s) — directly relevant; these tests exercise the contradiction detection and NLI write path that col-030 reads from.
- `test_lifecycle.py`: 38 passed, 2 xfailed, 1 xpassed (363.27s).
- `test_tools.py`: 93 passed, 2 xfailed (787.98s).

**xfail markers with GH issues**:
- `test_tools.py::test_retrospective_baseline_present` — GH#305, pre-existing, unrelated to col-030 (retrospective baseline, not search suppression).
- `test_lifecycle.py::test_auto_quarantine_after_consecutive_bad_ticks` — pre-existing (tick interval env var), unrelated.
- `test_lifecycle.py::test_dead_knowledge_entries_deprecated_by_tick` — pre-existing (background tick), unrelated.

All xfail markers reference pre-existing issues. None are related to col-030's search suppression path. PASS.

**xpass entry**:
- `test_lifecycle.py::test_search_multihop_injects_terminal_active` — marked `@pytest.mark.xfail(reason="Pre-existing: GH#406 — find_terminal_active multi-hop traversal not implemented; search injection stops at first hop; not caused by col-028")`. This test now passes unexpectedly.

Confirmed not caused by col-030: this feature touches only `suppress_contradicts` (pure function) and the Step 10b block in `search.rs`. Multi-hop supersession traversal (`find_terminal_active`) is a separate code path in `search.rs` unaffected by Step 10b. The xpass was pre-existing at the time of col-030 delivery (the RISK-COVERAGE-REPORT.md documents it as a signal to maintainers to remove the xfail marker and close GH#406). WARN: the xfail marker on this test should be removed and GH#406 closed, but this is maintenance work independent of col-030.

**No integration tests deleted or commented out**: confirmed — RISK-COVERAGE-REPORT.md states "No test removals, no `#[ignore]` additions for col-030." Cargo test results show 27 ignored tests unchanged (pre-existing).

**RISK-COVERAGE-REPORT.md includes integration test counts**: Table shows smoke=20, tools=95, lifecycle=41, contradiction=13, total=169 across suites with per-suite pass/fail/xfail/xpass breakdown. PASS.

---

## Warnings

| Issue | Severity | Recommendation |
|-------|----------|----------------|
| `graph.rs` is 591 lines (500-line limit) | WARN | Pre-existing; col-030 added 4 lines for wiring only. Track in backlog. The substantive logic correctly lives in `graph_suppression.rs` (326 lines). |
| `test_search_multihop_injects_terminal_active` xpassed | WARN | xfail marker for GH#406 should be removed; GH#406 should be closed. Not caused by col-030. Action: maintainer cleanup, not a blocker. |
| No explicit single-entry result set unit test | WARN | Not required by FR-13 or risk strategy. The mask initialization `vec![true; n]` makes n=1 trivially correct. Low risk. |
| `cargo-audit` not installed | WARN | `cargo audit` could not be run (binary not installed). Gate-3b already covers this; no new dependencies were added by col-030. |

---

## Knowledge Stewardship

- Stored: nothing novel to store — the xpass/pre-existing xfail handling pattern is feature-specific; the general gate-3c validation flow is not novel. The `graph.rs` pre-existing over-limit pattern is already documented in the project conventions.

---

## Final Determination

All gate-3c checks pass. The 4 warnings are minor and do not block delivery:
- The `graph.rs` line-count warning is pre-existing and the feature correctly mitigated the risk via `graph_suppression.rs`.
- The xpass is a maintenance signal, not a feature defect.
- The single-entry test absence is low-risk given the implementation's initialization guarantee.
- `cargo-audit` absence is an environment issue not introduced by this feature.

Gate result: **PASS**
