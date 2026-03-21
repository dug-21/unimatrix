# Scope Risk Assessment: crt-024

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Default weights are W3-1's initialization point — underweighted NLI at defaults means W3-1 trains on a world where NLI barely mattered, permanently biasing the learned function | High | Med | Architect must derive defaults from signal-role reasoning and verify numerically; document in ADR with expected ranking behavior at defaults |
| SR-02 | Six-term formula adds `w_util` and `w_prov` that the product vision's four-term formula omits; semantic divergence between SCOPE.md and PRODUCT-VISION.md creates a config surface mismatch (entry #2298 pattern) | High | High | Architect must canonicalize the six-term formula as the implementation target; ADR must note the vision formula was illustrative, not exhaustive |
| SR-03 | NLI re-normalization path when `w_nli=0`: AC-06 divides each remaining weight by `(w_sim + w_conf + w_coac)` but the formula has six terms; if `w_util` and `w_prov` are non-zero, the denominator is wrong | Med | High | Spec must define re-normalization denominator explicitly as sum of all non-zero remaining weights, not a hardcoded subset |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | WA-2 depends on inserting `w_phase * phase_boost` as an additional term in the same formula; if WA-0 implements the formula as a fixed-arity function, WA-2 becomes a formula change not an extension | Med | Med | Architect should design the formula as a variable-term accumulator or expose a clear extension point; document the WA-2 contract |
| SR-05 | `apply_nli_sort` is `pub(crate)` with direct unit tests from crt-023; its fate (remove vs. retain as helper) is unresolved in the scope — test coverage gap if deleted without migration plan | Low | High | Scope explicitly leaves this to architect (Open Question 1); decision must be made before implementation begins |
| SR-06 | Config validation checks sum of all six weights > 1.0, but WA-2 adds a seventh (`w_phase`) later; operators who tuned six weights to sum = 0.95 will silently exceed 1.0 after WA-2 | Med | Med | Reserve explicit headroom in defaults (SCOPE §Proposed Approach: "leave 0.05"); document in config comments that WA-2 claims this headroom |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Co-access boost map is computed via `spawn_blocking` in current Step 8; fused formula requires the boost_map available before iterating candidates; data-flow order must be restructured (SCOPE Open Question 3) | Med | High | Architect must verify boost_map fetch is completed before the single-pass scorer iterates; treat as a sequencing constraint, not a perf optimization |
| SR-08 | `BriefingService` uses a separate `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` constant; WA-0 normalizes SearchService's boost by `MAX_CO_ACCESS_BOOST = 0.03`; if briefing is ever migrated to the fused formula, the normalization constant will be wrong | Low | Low | AC-14 explicitly excludes briefing; document that briefing has a different normalization constant that would need reconciling if it adopts the fused formula |
| SR-09 | `rerank_score` in `unimatrix-engine/src/confidence.rs` is used by the fallback path and existing tests; if the fused formula inlines `rerank_score`'s logic rather than calling it, behavioral divergence between fallback and NLI paths becomes harder to detect | Low | Med | Architect should use `rerank_score` as a building block inside the fused formula rather than duplicating its arithmetic |

## Assumptions

| Assumption | SCOPE.md Section | Risk if Wrong |
|-----------|-----------------|---------------|
| `utility_delta` is always in a known normalization range (`÷ UTILITY_BOOST`) | §Signal Ranges / §Proposed Approach | If `utility_delta` can be negative (penalty side), normalizing by `UTILITY_BOOST` produces a negative term, pulling fused score below zero |
| `PROVENANCE_BOOST` is a fixed scalar constant suitable as a denominator | §Signal Ranges | If provenance boost is ever parameterized, normalization constant becomes stale |
| GH #329 is not yet merged or its effect is limited | §GH #329 Context | If #329 introduced persistent behavioral changes, WA-0's regression test (AC-11) baseline may not represent the pre-crt-024 pipeline as described |

## Design Recommendations

- **SR-01, SR-02**: Architect ADR must define all six default weights with signal-role justification, reconcile the four-term vision formula vs. six-term implementation formula, and verify the NLI-disabled fallback produces rankings consistent with pre-crt-024 behavior (SCOPE Constraint 9).
- **SR-03**: Spec must update AC-06's re-normalization denominator to `sum of weights for all non-zero signals`, explicitly listing all six terms.
- **SR-04**: Architect should explicitly design the WA-2 extension contract — either a variadic accumulator or documented "add one term, re-validate sum" pattern.
- **SR-07**: Architect must confirm boost_map prefetch placement in the restructured pipeline before the single-pass score loop; this is a correctness constraint, not optional.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection ranking scoring pipeline" -- found entry #724 (behavior-based ranking tests: assert ordering not scores) relevant to SR-01; no gate rejection outcomes for search pipeline features
- Queried: /uni-knowledge-search for "outcome rework search inference config" -- found entry #2701 (ADR-002 crt-023 NLI replaces rerank_score) as integration context; no rework outcomes
- Queried: /uni-knowledge-search for "risk pattern" (category:pattern) -- found entries #2298 (config key semantic divergence), #2730 (InferenceConfig struct extension pattern), #2964 (signal fusion pattern); all directly informed SR-02 and SR-03
- Stored: nothing novel to store -- SR-01 through SR-09 are feature-specific; no cross-feature pattern visible beyond what #2298 and #2964 already capture
