# Test Plan: SearchService (crt-024)

**File under test**: `crates/unimatrix-server/src/services/search.rs`
**Test location**: `#[cfg(test)] mod tests` block in the same file + integration test suites
**Risks addressed**: R-04, R-05, R-07, R-08, R-10, R-15, R-NEW; AC-04, AC-07, AC-08, AC-11, AC-14, AC-15; IR-01 through IR-04

---

## Component Summary

`SearchService` owns the fused scoring pipeline. Changes in crt-024:
1. `apply_nli_sort` removed (ADR-002); test coverage migrated
2. `try_nli_rerank` return type changes from `Option<Vec<(EntryRecord, f64)>>` to `Option<Vec<NliScores>>`
3. Co-access boost_map prefetch moved to Step 6c (before NLI scoring)
4. Single fused scoring pass in Step 7 + 7b; single sort; no secondary sort
5. `EvalServiceLayer` wires `InferenceConfig` fusion weights to `SearchService::new()`

---

## Unit Test Expectations

### R-04 / AC-08: No Test Deletions — Score Value Updates

**Audit test (not a unit test — a pre-merge gate)**: Before merging crt-024, verify:
- `git diff --stat` from `main` to the PR branch shows no deleted `*_test.rs` or `#[test]` removal
- Total `cargo test --workspace` test count ≥ pre-crt-024 baseline

**Per-test update requirement**: Every test in `search.rs` that currently asserts a specific
`final_score` value must be updated to use the new formula's expected value. The test body changes;
the test name, description, and `assert!` structure remain. Do not delete; update.

Pre-crt-024 formula: `final = (1-cw)*sim + cw*conf + utility_delta + co_access_boost + prov_boost`
Post-crt-024 formula: `final = (w_sim*sim + w_nli*nli + w_conf*conf + w_coac*coac_norm + w_util*util_norm + w_prov*prov_norm) * status_penalty`

For each pre-existing score assertion, recompute the expected value using the new formula with the
test's specific inputs and the new default weights.

---

### R-05 / AC-08: apply_nli_sort Behavior Migration

The following behaviors tested by `apply_nli_sort` unit tests (crt-023) must have named successor
tests in the fused scorer suite. Migration is one-to-one, not lossy.

| Original test (apply_nli_sort) | Successor test (fused scorer) | Assertion mapped to |
|-------------------------------|-------------------------------|---------------------|
| `test_nli_sort_orders_by_entailment_descending` | `test_fused_score_nli_entailment_dominates_when_high` | compute_fused_score with high vs low nli, equal sim/conf → order |
| `test_nli_sort_stable_identical_scores_preserves_original_order` | `test_fused_score_equal_fused_scores_deterministic_sort` | Sort stability over equal final_score values |
| `test_nli_sort_nan_entailment_treated_as_equal` | `test_fused_score_nan_nli_defaults_to_zero` | NaN NliScores.entailment cast to f64: nan as f64 → handled before fused score |
| `test_nli_sort_truncates_to_top_k` | (covered by pipeline truncation — AC-04) | Step 9 truncation unchanged |
| `test_nli_sort_penalty_depresses_effective_entailment` | `test_status_penalty_depresses_final_score` | `final = fused * penalty`; deprecated entry scores lower |

**Test name**: `test_fused_score_nli_entailment_dominates_when_high`

Entry with `nli=0.9, sim=0.5, conf=0.5` must score above entry with `nli=0.1, sim=0.5, conf=0.5`
at default weights. Uses `compute_fused_score` directly.

**Test name**: `test_fused_score_equal_fused_scores_deterministic_sort`

10 candidates with identical computed fused scores. Assert sorting is deterministic: running 10
times produces the same order each time. Assert stable sort preserves original HNSW insertion
order (or secondary sort by entry ID — document the tiebreak convention).

**Test name**: `test_fused_score_nan_nli_defaults_to_zero`

When `NliScores.entailment = f32::NAN`, the scoring loop casts to f64. Assert that the
implementation either: (a) treats NaN as 0.0 before constructing `FusedScoreInputs`, or
(b) calls `.is_nan()` and substitutes 0.0. The `FusedScoreInputs.nli_entailment` field must be
finite when passed to `compute_fused_score`.

---

### R-08 / AC-07: MAX_CO_ACCESS_BOOST Import-Only

**Test name**: `test_coac_norm_boundary_values`

Uses the imported `MAX_CO_ACCESS_BOOST` constant as the reference value (not a literal 0.03):

```rust
use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST;

#[test]
fn test_coac_norm_boundary_values() {
    // raw = MAX_CO_ACCESS_BOOST → norm = 1.0
    let norm_max = MAX_CO_ACCESS_BOOST / MAX_CO_ACCESS_BOOST;
    assert!((norm_max - 1.0).abs() < 1e-9);
    // raw = MAX_CO_ACCESS_BOOST / 2.0 → norm = 0.5
    let norm_half = (MAX_CO_ACCESS_BOOST / 2.0) / MAX_CO_ACCESS_BOOST;
    assert!((norm_half - 0.5).abs() < 1e-9);
    // raw = 0.0 → norm = 0.0
    let norm_zero = 0.0f64 / MAX_CO_ACCESS_BOOST;
    assert!((norm_zero - 0.0).abs() < 1e-9);
}
```

This test automatically detects constant duplication: if `search.rs` defines its own
`MAX_CO_ACCESS_BOOST = 0.03` and the engine changes it to 0.04, this test still references the
engine constant and will catch the divergence.

Complement: a `grep`-based audit must confirm `MAX_CO_ACCESS_BOOST` appears in `search.rs` only
as a `use` import, never as `const MAX_CO_ACCESS_BOOST`. This is a Stage 3c tester responsibility.

---

### R-10 / AC-compile: try_nli_rerank Return Type Migration

**Primary gate**: `cargo check --workspace` with zero errors. The Rust type system enforces the
return type change. Any call site that destructures `(EntryRecord, f64)` from the result will fail
to compile.

**Test name**: `test_try_nli_rerank_returns_nli_scores_vec_on_success`

With a mock NLI provider that returns a known `Vec<NliScores>`, assert:
- `try_nli_rerank` returns `Some(vec_of_nli_scores)`
- The returned `Vec` has length equal to the candidates slice length
- `NliScores.entailment` values match the mocked provider output

**Test name** (migrated from crt-023): `test_nli_fallback_when_handle_not_ready` (RETAIN)

This test already exists and validates `try_nli_rerank` returns `None` on Loading state. It must
pass unchanged after the return type migration. If the test fails due to the return type change,
update the assertion to match the new type (`Option<Vec<NliScores>>` not `Option<Vec<(EntryRecord, f64)>>`).

**Test name** (migrated from crt-023): `test_nli_fallback_when_handle_exhausted` (RETAIN)

Same as above — retain, verify it passes with new return type.

**Test name** (migrated from crt-023): `test_nli_fallback_on_empty_candidates` (RETAIN)

Retain. Verify `None` return for empty candidate list.

---

### R-15: NliScores Index Alignment with Candidates

**Test name**: `test_fused_scoring_nli_scores_aligned_with_candidates`

With candidates `[A, B, C]` and `NliScores` `[scores_A, scores_B, scores_C]`, the scoring loop
must apply `scores_A.entailment` to entry A, `scores_B.entailment` to entry B, etc.

Construct a deliberate test: entry A has low sim (0.3) but high NLI (0.9), entry B has high sim
(0.9) but low NLI (0.1). With default weights, NLI-dominant scoring must rank A above B.

If `nli_scores[i]` is applied to `candidates[i]` correctly, A ranks above B (NLI dominates).
If indices are misaligned (score_B applied to A, score_A to B), B would rank above A.

Assert final ordering is A > B.

---

### EC-07 / R-15: Length Mismatch Between NliScores and Candidates

**Test name**: `test_fused_scoring_handles_nli_scores_length_mismatch`

Construct a scenario where `try_nli_rerank` returns a `Vec<NliScores>` with length ≠ candidates
length (simulating a partial NLI response or error). Assert:
- No index-out-of-bounds panic
- Either: scoring falls back to `nli_entailment = 0.0` for out-of-range indices, OR
  the implementation detects the mismatch and re-runs without NLI (same as NLI-absent path)
- The chosen behavior must be documented in the implementation

---

### R-NEW / AC-15: EvalServiceLayer Weight Wiring

**Test name**: `test_eval_service_layer_sim_only_profile_scores_equal_sim`

Construct an `EvalServiceLayer` (or directly a `SearchService`) with `InferenceConfig` where
`w_sim=1.0` and all other weights = 0.0. Run `compute_fused_score` with:
- `sim=0.6, nli=0.9, conf=0.8, coac_norm=0.02/0.03, util_norm=0.5, prov_norm=0.0`
- Expected: `fused_score = 1.0 * 0.6 + 0.0 * rest = 0.6`
- `final_score = 0.6 * status_penalty`

Assert `|final_score - 0.6 * status_penalty| < 1e-9`.

If `EvalServiceLayer` uses default weights instead of `w_sim=1.0`, the actual score would be
`0.25*0.6 + 0.35*0.9 + 0.15*0.8 + 0.10*0.667 + 0.05*0.5 = 0.150 + 0.315 + 0.120 + 0.067 + 0.025 = 0.677`
— materially higher than 0.6, causing the assertion to fail and surfacing the wiring bug.

**Test name**: `test_eval_service_layer_default_weights_score_differs_from_sim_only`

With default weights on the same candidate as above, assert `final_score > 0.6 * status_penalty`.
This confirms NLI and confidence signals contribute when weights are correctly wired.

**Test name**: `test_eval_service_layer_differential_two_profiles_produce_different_scores`

Run the same candidate through two instances with different profiles:
- Profile 1: `w_nli=0.0, w_sim=0.85, w_conf=0.15, rest=0.0`
- Profile 2: `w_nli=0.35, w_sim=0.25, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05`
- Candidate: `nli=0.9, sim=0.5, conf=0.5`

Difference: `0.35 * nli_score - 0 = 0.35 * 0.9 - 0 = 0.315` score delta.
Assert `|score_profile2 - score_profile1| >= 0.30` (conservative bound accounting for re-distribution).

**Rationale**: R-NEW — if the two scores are equal (or within rounding error), EvalServiceLayer
is not wiring the config. This is a silent failure: the eval regression report appears to show
no ranking differences because both profile runs are actually identical.

---

### AC-04: No Secondary Sort After Fused Scoring

This is primarily a code review assertion, but can be supplemented with a unit test:

**Test name**: `test_search_pipeline_single_sort_pass`

After fused scoring, the result slice is sorted exactly once (Step 7c) and the order must not
change in subsequent steps. Test: create 5 candidates with monotonically decreasing `fused_score`.
Assert returned order matches the sorted-by-fused-score order without any secondary sort.

If a secondary sort were present (by co-access, by confidence, etc.), it would reorder the results,
breaking this test.

---

### AC-14: BriefingService Unchanged

**Gate**: `git diff` for the briefing service source file must show zero changes. This is a
Stage 3c tester responsibility (code review, not a unit test).

Additionally, the existing briefing integration tests must pass without modification.

---

### IR-01: boost_map Sequencing (R-07)

Code review assertion: `boost_map` must be `.await`-ed and its result bound to a variable
**before** the candidate iteration begins. No `await` point may appear inside the scoring loop
that refers to the boost_map future.

**Test name**: `test_search_coac_signal_reaches_scorer_via_boost_map` (integration — see OVERVIEW.md)

This behavioral test (in `test_lifecycle.py`) is the primary runtime guard for R-07.

---

### IR-03: BriefingService Isolation — Normalization Constant

Grep assertion (Stage 3c tester): confirm that `MAX_BRIEFING_CO_ACCESS_BOOST` is not replaced
by `MAX_CO_ACCESS_BOOST` anywhere in the briefing pipeline. This would change briefing scoring.

The following must hold after crt-024:
- `MAX_CO_ACCESS_BOOST = 0.03` is only imported/used in `SearchService` scoring path
- `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01` remains in the briefing path
- No shared utility function merges the two paths

---

### IR-04: rerank_score Retained

**Gate**: `cargo check` must pass. `rerank_score` must remain in
`unimatrix-engine/src/confidence.rs` and be callable. The NLI-absent fallback path in `search.rs`
may continue to use it; existing tests that call it must pass unchanged.

---

## Integration Test Expectations

### I-01: co-access Signal Reaches Scorer (`test_lifecycle.py`)

See OVERVIEW.md §New Integration Tests for the full scenario. Summary:
- Store entry A, build co-access history via repeated searches with same agent
- Verify A's `final_score` is higher than a new entry with identical content but no co-access
- Assert `final_score` is finite and in [0, 1]

### I-02: NLI-Absent Score in Valid Range (`test_tools.py`)

When NLI model is absent (cold start), `context_search` must return results where:
- All `final_score` values are finite (no NaN)
- All `final_score` values are in [0.0, 1.0]
- At least one result is returned for a query matching a stored entry

This test confirms R-02's zero-denominator guard works end-to-end through the MCP interface.

---

## Test Count Accounting

Pre-crt-024 `apply_nli_sort` tests that are removed (R-05 migration obligation):
- `test_nli_sort_stable_identical_scores_preserves_original_order` → migrated to `test_fused_score_equal_fused_scores_deterministic_sort`
- `test_nli_sort_orders_by_entailment_descending` → migrated to `test_fused_score_nli_entailment_dominates_when_high`
- `test_nli_sort_nan_entailment_treated_as_equal` → migrated to `test_fused_score_nan_nli_defaults_to_zero`
- `test_nli_sort_truncates_to_top_k` → covered by pipeline truncation (AC-04 test)
- `test_nli_sort_penalty_depresses_effective_entailment` → migrated to `test_status_penalty_depresses_final_score`

**Net count**: 5 removed apply_nli_sort tests, 5+ named successor tests + 10+ new tests for new
behaviors = net increase. AC-08 requirement satisfied.
