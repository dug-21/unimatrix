# Agent Report: crt-048-agent-0-scope-risk

## Output

- Produced: `product/features/crt-048/SCOPE-RISK-ASSESSMENT.md`
- Risk count: 7 risks total (SR-01 through SR-07)
  - High: 1 (SR-03)
  - Medium: 6 (SR-01, SR-02, SR-04, SR-05, SR-06, SR-07)
  - Low: 0

## Top 3 Risks for Architect/Spec Writer

1. **SR-03 (High)** — `DEFAULT_STALENESS_THRESHOLD_SECS` ambiguity: Goal 7 says "remove if unused" but Implementation Notes require retention for `run_maintenance()`. Spec must express this as a conditional AC, not a prose note.
2. **SR-06 (Med, High likelihood)** — ~12 `StatusReport` fixture sites in `mcp/response/mod.rs` will cause compile errors if not all removed atomically. Enumerate exact sites before pseudocode.
3. **SR-01 (Med)** — New weights (0.46/0.31/0.23) sum to 1.00 in decimal but f64 representation may not be exact; `lambda_weight_sum_invariant` test must use epsilon tolerance.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection coherence lambda" — results were general gate lessons (#4177, #4147, #4142), not specific to Lambda/coherence removal
- Queried: `/uni-knowledge-search` for "risk pattern" — no Lambda-specific risk patterns found
- Queried: `/uni-knowledge-search` for "struct field removal breaking change API serialization" — pattern #923 (serde alias for field renames) and #646 (backward-compatible config) are adjacent but do not cover clean-removal of public JSON fields
- Queried: `/uni-knowledge-search` for "weight constant normalization floating point sum invariant" — pattern #3206 (FusionWeights dual exemption from sum-check) is relevant: shows prior art for weight struct exemptions
- Stored: nothing novel to store — SR-03 (conditional constant retention when a goal says "remove if unused") is feature-specific to this removal pattern; not yet seen across 2+ features
