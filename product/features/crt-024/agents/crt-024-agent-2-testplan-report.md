# Agent Report: crt-024-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Feature**: crt-024 (Ranking Signal Fusion — WA-0)
**Status**: Complete

## Output Files

- `/workspaces/unimatrix/product/features/crt-024/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-024/test-plan/inference-config.md`
- `/workspaces/unimatrix/product/features/crt-024/test-plan/score-structs.md`
- `/workspaces/unimatrix/product/features/crt-024/test-plan/compute-fused-score.md`
- `/workspaces/unimatrix/product/features/crt-024/test-plan/search-service.md`

## Risk Coverage Mapping

| Risk ID | Priority | Test Plan File | Coverage |
|---------|----------|----------------|----------|
| R-01 (util_norm shift-and-scale) | Critical | compute-fused-score | 3 boundary tests + fused score non-negative |
| R-02 (zero-denominator guard) | High | score-structs | 3 tests: zero-denom, single nonzero, complement |
| R-03 (PROVENANCE_BOOST guard) | High | compute-fused-score | 3 tests: zero, boosted, unboosted + is_finite property |
| R-04 (regression test churn) | Critical | search-service | Pre-merge audit + per-test update requirement |
| R-05 (apply_nli_sort migration) | Critical | search-service | 5 named successor tests for all 5 apply_nli_sort behaviors |
| R-06 (W3-1 training signal) | High | compute-fused-score + score-structs | AC-11 + Constraint 9 + Constraint 10 as named tests |
| R-07 (boost_map sequencing) | High | search-service (integration) | test_lifecycle.py new test + code review gate |
| R-08 (constant duplication) | High | search-service | coac_norm boundary test using imported constant + grep gate |
| R-09 (spurious re-normalization) | High | score-structs | NLI-active unchanged + headroom preserved |
| R-10 (try_nli_rerank return type) | High | search-service | Compile gate + 3 retained fallback tests + 1 new success test |
| R-11 (util_delta negative range) | High | compute-fused-score | Ineffective entry: util_norm=0.0, fused>=0.0 |
| R-12 (validation bypass) | Med | inference-config | Direct InferenceConfig construction tests (no parse bypass) |
| R-13 (backward compat) | Med | inference-config | Partial TOML deserialization + missing-fields default test |
| R-14 (status_penalty inside formula) | Med | compute-fused-score | Compile-time struct shape + penalty-as-multiplier test |
| R-15 (NliScores index alignment) | Med | search-service | Index alignment test + length mismatch handling |
| R-16 (struct extensibility) | Low | score-structs | Named-field compilation test |
| R-NEW (EvalServiceLayer wiring) | High | search-service | 3 tests: sim-only profile, default profile, differential |

## Integration Suite Plan

**Suites to run**: `smoke` (gate), `tools`, `lifecycle`, `confidence`, `edge_cases`
**Not needed**: `protocol`, `contradiction`, `security`, `volume`

**New integration tests**:
1. `product/test/infra-001/suites/test_lifecycle.py` — `test_search_coac_signal_reaches_scorer`
   (R-07: co-access reaches fused scorer end-to-end through MCP)
2. `product/test/infra-001/suites/test_tools.py` — `test_search_nli_absent_uses_renormalized_weights`
   (R-02/AC-06: NLI-absent path returns finite scores in [0,1])

## Open Questions

1. **NaN handling in NliScores.entailment**: The existing `test_nli_sort_nan_entailment_treated_as_equal`
   test shows NaN was handled in `apply_nli_sort`. After removal, the scoring loop must handle NaN
   before constructing `FusedScoreInputs`. The implementation plan should specify whether NaN is
   mapped to 0.0 at the cast site (`nli_scores[i].entailment as f64`) or detected via `.is_nan()`.
   This affects `test_fused_score_nan_nli_defaults_to_zero` in search-service.md.

2. **EC-05 coac_norm clamping**: R-05 (EC-05) raises the question of whether `coac_norm` should be
   clamped with `.min(1.0)`. The spec does not explicitly require it. The test plan notes this as
   a "consider" item. The implementer should decide and the test plan will be extended if clamping
   is added.

3. **EvalServiceLayer location**: The test plan assumes `EvalServiceLayer` is accessible for unit
   testing (can be constructed with a custom `InferenceConfig`). If `EvalServiceLayer` is a separate
   crate or binary-only, the R-NEW tests may need to be structured differently (e.g., testing
   `SearchService::new()` directly with a custom config and asserting that scoring uses it).

4. **rerank_score in fallback path**: The architecture says "fused formula in NLI-active path does
   not call rerank_score; it computes each term directly." The NLI-absent fallback path may still
   use `rerank_score`. The implementer should clarify whether the NLI-absent path uses
   `FusionWeights::effective(false)` + `compute_fused_score` (preferred, consistent) or falls back
   to `rerank_score` for compatibility. The test plan covers both cases but the migration test
   naming assumes the former.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `crt-024 architectural decisions` (category: decision, topic: crt-024) — found ADRs #2969-#2972 (all four crt-024 ADRs); fully informative
- Queried: `/uni-knowledge-search` for `search ranking scoring test patterns edge cases` — found entry #724 (Behavior-Based Ranking Tests: assert ordering not scores) and prior ADR #703; both informed R-04/R-05 planning
- Queried: `/uni-knowledge-search` for `zero denominator guard NaN propagation normalization division pattern` — no relevant entries found
- Stored: nothing novel to store — the zero-denominator guard pattern (PROVENANCE_BOOST, nli_absent_sum guards) is observed here for the first time across features. Recommend storing after Stage 3c if Stage 3b implementation confirms the specific guard approach used, as it would generalize to any future feature with optional normalization denominators.
