# Agent Report: col-030-agent-3-risk

## Output

- Produced: `/workspaces/unimatrix/product/features/col-030/RISK-TEST-STRATEGY.md`
- Mode: architecture-risk
- Artifacts read: SCOPE.md, SCOPE-RISK-ASSESSMENT.md, ARCHITECTURE.md, ADR-001 through ADR-005, SPECIFICATION.md
- Source code inspected: `graph.rs` (587 lines), `graph_tests.rs` (1068 lines), `lib.rs`, `search.rs` (lines 870–940, 600–622)

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 |
| High     | 7 |
| Medium   | 5 |
| Total    | 13 |

## New High-Severity Risks Identified

### R-01 (Critical): graph_tests.rs line count
`graph_tests.rs` is already 1068 lines — more than double the 500-line limit. The architecture and specification are silent on this. Adding 6+ `suppress_contradicts` unit tests to this file would push it to ~1150 lines, guaranteeing a gate-3b rejection per lesson #3580. The implementation brief must designate `graph_tests.rs` as a split-candidate and require new tests to live in a separate `graph_suppression_tests.rs` or inline under `#[cfg(test)]` in `graph_suppression.rs`.

### R-02 (High): graph_suppression.rs visibility trap
ADR-001 and entry #3602 confirm: items re-exported via `pub use` in a parent module must be declared `pub` (not `pub(super)`) in the submodule. If the implementation agent writes `pub(super) fn suppress_contradicts`, the re-export compiles but is inaccessible outside `graph.rs`, causing an `E0364` error at the `search.rs` import site. The implementation brief must state `pub fn suppress_contradicts` explicitly.

### R-03 (High): final_scores immutable binding shadow
`final_scores` is declared `let` (immutable) at line 893. ADR-004 notes "the implementation agent must shadow it with `let final_scores = new_fs;` at Step 10b output." This is a non-obvious constraint that is easy to miss. If forgotten, Step 11's zip reads from the pre-suppression `final_scores`, silently pairing suppressed entries' scores with the surviving entries.

## Scope Risk Traceability

All 8 SR-XX scope risks traced. SR-03, SR-06, SR-08 resolved at architecture level (no residual architecture risks). SR-01, SR-02, SR-04, SR-05, SR-07 have residual delivery risks R-04 through R-13.

## Key Recommendation for Implementation Brief

The implementation brief must explicitly call out:
1. `graph_tests.rs` split — new tests cannot go in the existing file
2. `pub fn suppress_contradicts` (not `pub(super)`)
3. `let final_scores = new_fs;` shadow (not mutation of existing binding)
4. FR-14 positive integration test is a mandatory gate separate from the eval gate
5. FR-15: no `create_graph_edges_table` in new tests

## Knowledge Stewardship
- Queried: /uni-knowledge-search for `lesson-learned failures gate rejection` — entries #3579, #3580 directly applicable (test omission and file-size gate-3b patterns)
- Queried: /uni-knowledge-search for `parallel Vec alignment` — entry #3616 confirmed as col-030 pattern
- Queried: /uni-knowledge-search for `module re-export pub use sibling` — entry #3602 informs R-02
- Stored: nothing novel to store — R-01 (graph_tests.rs split-candidate) is col-030-specific; general patterns already in entries #3579, #3580, #3602
