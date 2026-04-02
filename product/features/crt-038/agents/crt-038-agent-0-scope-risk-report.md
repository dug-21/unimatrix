# Agent Report: crt-038-agent-0-scope-risk

**Mode**: scope-risk
**Feature**: crt-038 (conf-boost-c Formula and NLI Dead-Code Removal)

## Output

- `/workspaces/unimatrix/product/features/crt-038/SCOPE-RISK-ASSESSMENT.md` (35 lines)

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 3 (SR-01, SR-02, SR-06) |
| Med | 2 (SR-03, SR-04) |
| Low | 2 (SR-05, SR-07) |

## Top 3 Risks for Architect/Spec Writer

1. **SR-01 (High/High)**: The `effective(false)` re-normalization path produces w_sim'≈0.588, w_conf'≈0.412 instead of the intended 0.50/0.35. The AC-02 short-circuit is the resolution, but the spec must make it unconditional and prior to any eval run.

2. **SR-06 (High/Med)**: `run_graph_inference_tick` in `nli_detection_tick.rs` shares helpers with the three functions being deleted from `nli_detection.rs`. The spec must enumerate retained symbols explicitly — this is the primary compilation-breakage risk in the removal.

3. **SR-02 (High/Med)**: Eval run ordering matters. If AC-12 is run before AC-02 is implemented, the gate result is invalid because the scoring path differs from the researched baseline. The spec must enforce AC-02 before eval.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found entries #3579, #2758, #1203, #2577, #3935 (gate-3b test omission, cascading rework patterns; applicable to dead-code removal test cleanup risk SR-03)
- Queried: `/uni-knowledge-search` for "risk pattern scoring formula weight change" — found entry #4003 (w_nli=0.0 re-normalization, directly informs SR-01/SR-02) and #2985 (differential test design, informs SR-03)
- Queried: `/uni-knowledge-search` for "NLI scoring dead code removal rework" — found entries #2970, #3985, #3986 (NLI removal audit history)
- Stored: nothing novel to store — SR-01/SR-02 pattern already captured in entry #4003; no new cross-feature pattern visible from this single feature's scope
