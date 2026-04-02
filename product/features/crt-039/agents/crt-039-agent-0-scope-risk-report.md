# Agent Report: crt-039-agent-0-scope-risk

## Output
- SCOPE-RISK-ASSESSMENT.md written to: product/features/crt-039/SCOPE-RISK-ASSESSMENT.md
- 35 lines (limit: 100)

## Risk Summary

| Severity | Count |
|----------|-------|
| High     | 4 (SR-01, SR-03, SR-04, SR-05) |
| Medium   | 3 (SR-02, SR-06, SR-07) |
| Low      | 0 |

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-04** — Phase 1 `get_provider()` guard split. Highest structural risk: incomplete split allows Phase 8 to execute without valid NLI scores, producing silent Supports edge corruption. Spec must define control flow and require explicit tests for both branches.

2. **SR-05** — `test_run_graph_inference_tick_nli_not_ready_no_op` semantics change. The test's existing assertion (zero edges when NLI not ready) becomes incorrect after crt-039. If updated vacuously, CI passes but the regression guarantee is lost. Must become two targeted assertions.

3. **SR-01** — Phase 4b Informs edge volume. Previously this path never ran. `MAX_INFORMS_PER_TICK = 25` is the only stated control. Spec must confirm dedup pre-filter applies before the cap and the cap is a hard write limit.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" — found #3579 (Gate 3b test omission), #2758 (Gate 3c non-negotiable test names), #2577 (validate() boundary test gap). Relevant: SR-05 test semantics risk is consistent with the recurring pattern of behavior-change tests being rephrased to pass vacuously.
- Queried: /uni-knowledge-search for "outcome rework tick background inference" — found #3723 (tick completion log absence), #1616 (background tick dedup ordering). Both directly informed SR-06 and SR-01.
- Queried: /uni-knowledge-search for "risk pattern" — found #3624 (eval gate validates no-op only — false confidence on suppression/filter features). Directly informed SR-05.
- Queried: /uni-knowledge-search for "graph inference tick edge flood dedup candidate set separation" — found #3956 (ADR-003 crt-037: directional dedup for query_existing_informs_pairs). Confirms dedup mechanism exists but does not confirm it applies before cap in Phase 5.
- Stored: nothing novel to store — SR-05 (behavior-change test updated vacuously → CI passes, regression lost) is an instance of the already-documented pattern in #3579/#2758/#3624. Not a new cross-feature pattern.
