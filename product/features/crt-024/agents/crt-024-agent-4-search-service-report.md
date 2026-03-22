# Agent Report: crt-024-agent-4-search-service

**Feature**: crt-024 — Ranking Signal Fusion (WA-0)
**Agent**: crt-024-agent-4-search-service
**Wave**: 2
**Commit**: `384c4f7` — `impl(search-service): crt-024 W2 — fused scoring formula + pipeline rewrite (#335)`

---

## Deliverables

### Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/services/search.rs`
  — All three Wave 2 components: `FusedScoreInputs`, `FusionWeights`, `compute_fused_score`, pipeline rewrite, ~55 new tests

- `/workspaces/unimatrix/crates/unimatrix-server/src/services/mod.rs`
  — Added `FusionWeights` to exports; passed `FusionWeights::from_config(&inference_config)` to `SearchService::new()`

- `/workspaces/unimatrix/crates/unimatrix-server/src/services/briefing.rs`
  — Updated test-helper `SearchService::new()` call to pass `FusionWeights::from_config(&InferenceConfig::default())` as final argument; no pipeline changes

### Components Implemented

**Component 1 — `FusedScoreInputs` + `FusionWeights`**

- `FusedScoreInputs`: six `pub f64` fields (`similarity`, `nli_entailment`, `confidence`, `coac_norm`, `util_norm`, `prov_norm`)
- `FusionWeights`: six `pub f64` fields with `#[derive(Debug, Clone, Copy)]`
  - `FusionWeights::from_config(cfg: &InferenceConfig)`: reads `w_*` fields from config
  - `FusionWeights::effective(nli_available: bool)`: when NLI absent, zeros `w_nli` and re-normalizes remaining five weights; zero-denominator guard returns all-zeros
- `WA-2` extension point: `phase_boost_norm` field in `FusedScoreInputs`, `w_phase` in `FusionWeights`, both at 0.0 default

**Component 2 — `compute_fused_score`**

Pure `pub(crate) fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64` — six-term dot product, no I/O, no state. `status_penalty` applied at call site in the pipeline loop.

**Component 3 — Pipeline Rewrite**

- `apply_nli_sort` removed (ADR-002)
- `try_nli_rerank` signature changed to `async fn try_nli_rerank(...) -> Option<Vec<NliScores>>` — returns raw NLI scores with no sort/truncation
- Step 6c: `boost_map` prefetch moved before NLI scoring — fully `.await`-ed before scoring loop begins
- Step 7: NLI scoring → `effective_weights` computed → fused scoring loop per candidate → single sort by `final_score` → single `truncate(k)`
- Step 11: `ScoredEntry` construction uses fused `final_score`
- `MAX_CO_ACCESS_BOOST` imported from `unimatrix_engine::coaccess` — no local redefinition
- `rerank_score` import moved to `#[cfg(test)]` only (unused in production path, retained for existing tests)

---

## Test Results

**Pre-crt-024 baseline**: ~28 tests in search.rs (crt-023 wave)
**Post-crt-024**: 83 tests in search.rs

```
test result: ok. 1778 passed; 0 failed; 0 ignored; 0 measured
```

### Test Coverage by Plan Section

| Plan section | Tests written | Status |
|---|---|---|
| R-05: apply_nli_sort migration (5 tests) | `test_fused_score_nli_entailment_dominates_when_high`, `test_fused_score_equal_fused_scores_deterministic_sort`, `test_fused_score_nan_nli_defaults_to_zero`, `test_search_pipeline_single_sort_pass`, `test_status_penalty_depresses_final_score` | Pass |
| R-08: MAX_CO_ACCESS_BOOST import-only | `test_coac_norm_boundary_values` | Pass |
| R-10: try_nli_rerank return type | `test_try_nli_rerank_returns_nli_scores_vec_on_success`, `test_nli_fallback_when_handle_not_ready`, `test_nli_fallback_when_handle_exhausted`, `test_nli_fallback_on_empty_candidates` | Pass |
| R-15: NliScores index alignment | `test_fused_scoring_nli_scores_aligned_with_candidates`, `test_fused_scoring_handles_nli_scores_length_mismatch` | Pass |
| R-NEW: EvalServiceLayer weight wiring | `test_eval_service_layer_sim_only_profile_scores_equal_sim`, `test_eval_service_layer_default_weights_score_differs_from_sim_only`, `test_eval_service_layer_differential_two_profiles_produce_different_scores` | Pass |
| AC-11: NLI entailment regression proof | `test_compute_fused_score_nli_high_beats_coac_high_ac11` | Pass |
| FusionWeights unit tests | `test_fusion_weights_from_config_defaults`, `test_fusion_weights_effective_nli_absent_renormalization`, `test_fusion_weights_effective_nli_present_unchanged`, `test_fusion_weights_effective_zero_denominator_guard` | Pass |
| Signal normalization helpers | `test_util_norm_*`, `test_prov_norm_*`, `test_coac_norm_boundary_values` | Pass |

---

## Issues Encountered

### AC-11 Test Plan Math Error

`test-plan/compute-fused-score.md` specified expected value `0.540` for the AC-11 test with inputs `sim=0.8, conf=0.65`. This value was copied from IMPLEMENTATION-BRIEF "Key Numbers" which used `sim=0.5, conf=0.5`. The actual correct value for `(sim=0.8, nli=0.9, conf=0.65)` with default weights is `0.6125`.

Fix: test computes expected values dynamically (`let expected_a = 0.35 * 0.9 + 0.25 * 0.8 + 0.15 * 0.65`) — self-consistent with its own inputs. The AC-11 structural assertion (A > B) holds regardless.

### Differential Profiles Test Input Selection

`test_eval_service_layer_differential_two_profiles_produce_different_scores` with `sim=0.5, nli=0.9` produced only a 0.04 score difference, failing the `>= 0.20` threshold — even though wiring was correct. The partial redistribution of other weights in Profile 2 partially compensated for the gain from `w_nli`.

Fix: switched to `sim=0.1, nli=0.9` so Profile 1 (w_sim=0.85) gets a small sim contribution and Profile 2 (w_nli=0.35) dominates clearly. Diff became 0.280.

### Pre-existing Clippy Warnings

`too-many-arguments` on `SearchService::new`, a default-impl pattern warning in `briefing.rs`, and store crate issues were all present before this work. Not introduced here; not fixed here.

---

## Architecture Conformance

- ADR-002 (apply_nli_sort removal): applied — function deleted, behavior migrated to fused scorer tests
- ADR-003 (default weights w_nli=0.35 etc.): applied — defaults in `FusionWeights::from_config` match ADR-003
- ADR-004 (compute_fused_score as standalone pure function): applied — function is `pub(crate)`, no I/O
- BriefingService: zero pipeline changes; only test-helper call site updated for new `SearchService::new()` arity
- `MAX_CO_ACCESS_BOOST`: imported only, never redefined in search.rs

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` fused scoring, test plan math errors, differential test inputs — existing entry #2964 covers signal fusion pipeline patterns; no prior coverage of the two new gotchas below
- Stored: entry #2984 "Test plan expected values copied from a different input set silently fail when inputs differ" via `/uni-store-pattern`
- Stored: entry #2985 "Differential profile tests require extreme input values to expose weight-wiring bugs reliably" via `/uni-store-pattern`
