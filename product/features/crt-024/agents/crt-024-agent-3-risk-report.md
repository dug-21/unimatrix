# Agent Report: crt-024-agent-3-risk

## Status: Complete

## Output
- Produced: `/workspaces/unimatrix/product/features/crt-024/RISK-TEST-STRATEGY.md`

## Risk Summary
| Priority | Count |
|----------|-------|
| Critical | 3 |
| High | 8 |
| Medium | 4 |
| Low | 1 |
| **Total** | **16** |

## Top Risks for Implementation Attention
1. **R-01 (Critical)**: `utility_delta` normalization formula — spec FR-05 and ARCHITECTURE.md disagree. Spec says `÷ UTILITY_BOOST` with clamp; architecture says shift-and-scale `(val + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY)`. Implementer must follow the architecture formula or score range guarantee breaks.
2. **R-04 (Critical)**: All pre-crt-024 score-value assertions must be updated, none deleted. AC-08 requires net test count increase. Use entry #751 procedure.
3. **R-05 (Critical)**: `apply_nli_sort` removal requires explicit one-to-one test migration. Every crt-023 behavior tested in `apply_nli_sort` tests needs a named successor in the fused scorer suite.
4. **R-11 (High)**: Negative `utility_delta` without shift formula produces negative fused score — breaks NFR-02 score range guarantee for all Ineffective/Noisy entries.
5. **R-07 (High)**: boost_map prefetch async sequencing — both `spawn_blocking` and NLI batch must fully resolve before scoring loop begins; no interleaved `.await` in the loop.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection ranking scoring pipeline" — entry #724 (Behavior-Based Ranking Tests) was the most relevant; no gate rejections for search pipeline features in history
- Queried: `/uni-knowledge-search` for "risk pattern" (category:pattern) — entry #2964 (signal fusion sequential sort override pattern) confirmed; entries #1042, #749 provided pure-function and calibration test patterns
- Queried: `/uni-knowledge-search` for "normalization pure function InferenceConfig" — entry #751 (Updating Golden Regression Values) directly informs R-04 coverage requirement
- Queried: `/uni-knowledge-search` for "co-access boost normalization" — entries #701, #702 confirm co-access weight history
- Stored: nothing novel to store — zero-denominator and NaN propagation patterns are first observed here; will store if they recur across a second feature
