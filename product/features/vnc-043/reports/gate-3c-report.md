# Gate 3c Report: vnc-043

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-07-05
> Result: PASS

context_graph subgraph — Class-1 doc fix + live depth-1 read via `subgraph_via_db` reuse (GH #903).
Validated against committed HEAD `61b7440b`.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof (R-01..R-11) | PASS | All 11 risks map to executed, passing tests; RISK-COVERAGE-REPORT complete |
| Test coverage completeness | PASS | Every Phase-2 risk scenario exercised; unit + wire; R-06 sweep clean |
| Specification compliance (AC-01..AC-15) | PASS | All 15 ACs have named passing verification |
| Architecture compliance (ADRs) | PASS | Dispatch, ordering, doc surfaces match ADR-001/002/003 vnc-043; no wire/struct/hot-path change |
| Integration smoke + suites | PASS | smoke 28/0; protocol/tools/lifecycle/edge_cases ran; counts in report |
| xfail hygiene | PASS | All xfails pre-existing w/ GH refs; none added/removed; python diff purely additive (0 deletions) |
| xpass triage | PASS | 1 xpass is pre-existing non-strict tick-timing marker, feature-unrelated |
| R-06 depth>1 fixed-order sweep | PASS | Swept; zero fixed-order assertions; no coverage weakened |
| Knowledge stewardship | PASS | RISK-COVERAGE-REPORT / strategy carry Queried + nothing-novel entries |
| Code hygiene (anti-stub / unwrap) | PASS | No todo/unimplemented/FIXME; no bare unwrap in handler/via_db |

Independent verification performed at this gate (not just report trust):
- Ran `cargo test -p unimatrix-server --lib` filtered to subgraph + doc tests → **56 passed, 0 failed**.
- Ran `test_graphparams_schemars_docs_state_subgraph_applies` → **1 passed**.
- Inspected committed source: dispatch placement, dual-path sort, four doc edit points, diff scope.

## Detailed Findings

### Risk mitigation proof (R-01..R-11)
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md maps each of R-01..R-11 to named tests, all PASS. Spot-verified
the load-bearing structural risks against source:
- **R-01 / R-08 (dispatch placement, lock-free depth-1)**: `graph_read_subgraph.rs:171` — `if max_depth == 1 { return subgraph_via_db(...) }` exact-match, computed after all filter args (`petgraph_dirs`, `edge_types`, `resolve_supersessions` at :162) and **before** the `typed_graph_state.read()` snapshot at :190. Depth-1 takes zero `TypedGraphState` lock. Matches ADR-001 and the RISK-COVERAGE R-08 structural review.
- **R-02 (cold-start fallback)**: depth>1 `use_fallback` branch (`:197`) untouched below the depth-1 early return; `test_bfs_cold_start_empty_result` present + passing.
- **R-06 (uniform ordering both paths)**: single `sort_subgraph_output` helper (`:619`) called from BOTH the warm-BFS assembly (`:391`) and `subgraph_via_db` (`:593`). Keys = nodes by `id`, edges by `(source_id, target_id, relation_type)` — matches ADR-003 / FR-9.
- **R-07 (four-point doc drift)**: verified all four edit points landed (see AC-13 below).

### Test coverage completeness
**Status**: PASS
**Evidence**: All 14 named unit tests exist in `graph_read_subgraph_bfs_tests.rs`; targeted run = 56 passed.
R-04 dedup/dangling/metadata-cap, R-05 hydration parity, R-03 SET parity, R-09 truncation both-ways,
R-10 freshness both-ways, R-11 label invariant all present. Integration adds 3 wire tests (DoD write-then-read,
realistic-fanin truncation, ordering determinism).

### Specification compliance (AC-01..AC-15)
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT §Acceptance Criteria Verification lists all 15 ACs PASS with named tests.
Cross-checked ACs against source: AC-01/07/11 (freshness), AC-10 (no wire/struct change — diff touches only
handler dispatch + doc text + tests), AC-13/09 (doc surfaces), AC-14 (ordering), AC-15 (truncation).

### Architecture compliance (ADRs)
**Status**: PASS
**Evidence**: Diff scope (`git diff 0d2ebbd0..HEAD`) = `graph_read.rs` (+5, schemars docs), `graph_read_subgraph.rs`
(+62, dispatch + sort helper), `tools.rs` (+37, twin description literals + tests), test files. No `GraphParams`/
`SubgraphResponse`/`RelationEdge` shape change. No new `subgraph_sql` helper — reuses `subgraph_via_db` per ADR-001.
Twin-literal + byte-equality-guard pattern kept per ADR-002 (`test_graph_tool_attr_description_matches_const` green).

### Integration smoke + suites
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT §Integration Tests — smoke 28 passed / 0 failed (mandatory gate); protocol 13,
tools 198 (1 pre-existing xfail GH#405), lifecycle 85 (6 pre-existing xfails), edge_cases 23 (1 pre-existing xfail GH#111).
Integration counts present in the report. Suite results are report-attested; corroborated at this gate by the
purely-additive python diff (see xfail hygiene) and green unit layer.

### xfail hygiene + xpass triage
**Status**: PASS
**Evidence**: `git diff 0d2ebbd0..HEAD -- product/test/infra-001/` = **0 removed non-blank lines, 0 xfail/xpass markers
touched** — confirms only-additive (3 new tests in test_lifecycle.py/test_tools.py), no integration test deleted or
commented out, no xfail added or removed. All xfails pre-existing with GH refs (#111/#405/#406/tick-timing). The 1 xpass
is a pre-existing `strict=False` tick-timing marker; the feature changes no tick path (depth>1 stays cached, depth-1 is
live and lock-free), so it is genuinely feature-unrelated and correctly left as-is.

### R-06 depth>1 fixed-order sweep
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT §R-06 sweep covers the three named sources; all set-based assertions, no index
pin the uniform sort could flip. Confirmed by 0 removed lines in the diff — no existing coverage weakened; new
determinism tests added at unit + wire level.

### Knowledge stewardship (test-phase agent)
**Status**: PASS
**Evidence**: RISK-TEST-STRATEGY.md and RISK-COVERAGE-REPORT carry `## Knowledge Stewardship` with `Queried:`
(context_search — #5396 drift-guard, #4474 execution-path-asymmetry, #4473 warn+continue) and a
"nothing novel to store" entry with reason (patterns already recorded).

### Code hygiene
**Status**: PASS
**Evidence**: No `todo!`/`unimplemented!`/`TODO`/`FIXME` in changed non-test source. No bare `.unwrap()` in the
`handle_subgraph`/`subgraph_via_db` region (lock poison handled via `unwrap_or_else(|e| e.into_inner())`). File
`graph_read_subgraph.rs` within the 500-line responsibility split (module is single-purpose).

## Rework Required

None.

## Scope Concerns

None.
