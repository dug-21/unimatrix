# Agent Report: col-031-agent-0-scope-risk

## Output

- `/workspaces/unimatrix/product/features/col-031/SCOPE-RISK-ASSESSMENT.md` (36 lines)

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 3 (SR-01, SR-03, SR-06) |
| Med | 3 (SR-02, SR-04, SR-07) |
| Low | 1 (SR-05) |

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-01 (High/Med)** — `json_each` integer-array SQL expansion is unverified. Silent zero-row result if element typing or cast form is wrong. Must be pinned to confirmed working example before store query is finalized.

2. **SR-03 (High/Med)** — Eval harness fix (`extract.rs` + `current_phase`) and scoring activation are not independently shippable. If eval fix ships late or is treated as a separate wave, AC-12 becomes a vacuous gate. Spec must enforce hard ordering.

3. **SR-06 (High/Low)** — Third `Arc<RwLock<_>>` on the hot search path. Lock acquisition order across all three handles (TypedGraph, Effectiveness, PhaseFreqTable) must be formally documented in the architecture to prevent deadlock in background tick.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned gate failures — found #3579, #3580 (mandatory test omission, 500-line violations); for scoring rework — found #3208 (weight calibration lesson, directly applicable to SR-02); for risk patterns — found #3207 (compute_fused_score extension pattern), #2985 (differential test extreme inputs).
- Stored: entry #3678 "json_each expansion on integer JSON arrays requires explicit cast verification" via `/uni-store-pattern` — novel pattern visible across any feature using `result_entry_ids` JSON expansion, not previously stored.
