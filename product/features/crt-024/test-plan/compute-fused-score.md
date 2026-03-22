# Test Plan: compute_fused_score (crt-024)

**File under test**: `crates/unimatrix-server/src/services/search.rs`
**Function**: `pub(crate) fn compute_fused_score(inputs: &FusedScoreInputs, weights: &FusionWeights) -> f64`
**Test location**: `#[cfg(test)] mod tests` block in the same file
**Risks addressed**: R-01, R-03, R-06, R-11, R-14; AC-05, AC-09, AC-10, AC-11

---

## Component Summary

`compute_fused_score` is the pure function implementation of the six-term formula (ADR-004):

```
fused_score = w_sim*sim + w_nli*nli + w_conf*conf + w_coac*coac_norm
            + w_util*util_norm + w_prov*prov_norm
```

No async, no locks, no side effects. `status_penalty` is applied by the caller, not inside this
function (ADR-004 invariant). All normalization (util_norm shift-and-scale, prov_norm division
guard) is done by the caller before constructing `FusedScoreInputs`; the function only multiplies
and sums.

---

## Unit Test Expectations

### AC-05: Six-Term Formula Correctness

**Test name**: `test_compute_fused_score_six_term_correctness_ac05`

Controlled inputs (from AC-05 specification):
- `sim=0.8, nli=0.7, conf=0.6, coac_norm=0.5, util_norm=0.5, prov_norm=1.0`
- `weights=(w_sim=0.30, w_nli=0.30, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05)`
- Expected: `0.30*0.8 + 0.30*0.7 + 0.15*0.6 + 0.10*0.5 + 0.05*0.5 + 0.05*1.0`
  = `0.240 + 0.210 + 0.090 + 0.050 + 0.025 + 0.050 = 0.665`

```rust
#[test]
fn test_compute_fused_score_six_term_correctness_ac05() {
    let inputs = FusedScoreInputs {
        similarity: 0.8, nli_entailment: 0.7, confidence: 0.6,
        coac_norm: 0.5, util_norm: 0.5, prov_norm: 1.0,
    };
    let weights = FusionWeights {
        w_sim: 0.30, w_nli: 0.30, w_conf: 0.15,
        w_coac: 0.10, w_util: 0.05, w_prov: 0.05,
    };
    let score = compute_fused_score(&inputs, &weights);
    assert!((score - 0.665).abs() < 1e-9,
        "AC-05: expected 0.665, got {score}");
}
```

---

### AC-11: NLI-High Beats Co-Access-High Regression Test (R-04, R-05, R-06)

**Test name**: `test_compute_fused_score_nli_high_beats_coac_high_ac11`

This is the named regression test. Values from ADR-003 numerical verification.

- Entry A: `sim=0.8, nli=0.9, conf=0.65, coac_norm=0.0, util_norm=0.0, prov_norm=0.0`
  Expected score at defaults: `0.35*0.9 + 0.25*0.8 + 0.15*0.65 = 0.315 + 0.200 + 0.0975 = 0.540`
  (util and prov terms: `0.05*0.0 + 0.05*0.0 = 0.0`)
- Entry B: `sim=0.8, nli=0.3, conf=0.65, coac_norm=1.0, util_norm=0.0, prov_norm=0.0`
  Expected score at defaults: `0.35*0.3 + 0.25*0.8 + 0.15*0.65 + 0.10*1.0 = 0.105 + 0.200 + 0.0975 + 0.100 = 0.430`

Assert `score_a > score_b` and `(score_a - 0.540).abs() < 1e-9` and `(score_b - 0.430).abs() < 1e-9`.

```rust
// AC-11: NLI dominance regression — proves the defect that existed pre-crt-024.
// Pre-crt-024: co-access applied as additive afterthought in Step 8 (after NLI sort in Step 7),
// which allowed Entry B (coac=max) to overtake Entry A (nli=0.9) in the Step 8 re-sort.
// Formula: final = (1-cw)*sim + cw*conf + coac_boost → B could score higher than A.
// crt-024 fix: all signals in one weighted formula; NLI weight (0.35) > max co-access (0.10*1.0).
#[test]
fn test_compute_fused_score_nli_high_beats_coac_high_ac11() {
    let default_weights = FusionWeights {
        w_sim: 0.25, w_nli: 0.35, w_conf: 0.15,
        w_coac: 0.10, w_util: 0.05, w_prov: 0.05,
    };
    let entry_a = FusedScoreInputs {
        similarity: 0.8, nli_entailment: 0.9, confidence: 0.65,
        coac_norm: 0.0, util_norm: 0.0, prov_norm: 0.0,
    };
    let entry_b = FusedScoreInputs {
        similarity: 0.8, nli_entailment: 0.3, confidence: 0.65,
        coac_norm: 1.0, util_norm: 0.0, prov_norm: 0.0,
    };
    let score_a = compute_fused_score(&entry_a, &default_weights);
    let score_b = compute_fused_score(&entry_b, &default_weights);
    assert!((score_a - 0.540).abs() < 1e-9, "Entry A must score 0.540, got {score_a}");
    assert!((score_b - 0.430).abs() < 1e-9, "Entry B must score 0.430, got {score_b}");
    assert!(score_a > score_b,
        "AC-11: Entry A (nli=0.9, coac=0) must beat Entry B (nli=0.3, coac=max)");
}
```

---

### ADR-003 Constraint 9: NLI-Disabled, Sim Dominant (R-06)

**Test name**: `test_compute_fused_score_constraint9_nli_disabled_sim_dominant`

Use `FusionWeights::effective(false)` on defaults (denominator=0.60), then score:
- Entry A: sim=0.9, conf=0.3, all others=0.0 → expected ≈ 0.450
- Entry B: sim=0.5, conf=0.9, all others=0.0 → expected ≈ 0.434

Assert `score_a > score_b`. Assert re-normalized `w_sim' ≈ 0.4167` and `w_conf' ≈ 0.2500`.

**Rationale**: R-06 — W3-1 training baseline. If defaults or re-normalization are wrong, every
context_search call without NLI trains toward a degraded starting model.

---

### ADR-003 Constraint 10: Sim Dominant Over Conf at Full Defaults (R-06)

**Test name**: `test_compute_fused_score_constraint10_sim_dominant_at_defaults`

Default weights (NLI active, zero NLI scores, zero coac/util/prov):
- Entry A: sim=0.9, conf=0.3, nli=0.0 → score = 0.25*0.9 + 0.15*0.3 = 0.225 + 0.045 = 0.270
- Entry B: sim=0.5, conf=0.9, nli=0.0 → score = 0.25*0.5 + 0.15*0.9 = 0.125 + 0.135 = 0.260

Assert `score_a > score_b`.

---

### R-01: util_norm Boundary Values — Shift-and-Scale Formula

Three tests verifying the shift-and-scale normalization. Note: `util_norm` is computed at the
*call site* (in `SearchService`), not inside `compute_fused_score`. These tests exercise the
normalization helper directly, then feed the result to `compute_fused_score`.

**Test name**: `test_util_norm_ineffective_entry_maps_to_zero`

```rust
// utility_delta = -UTILITY_PENALTY = -0.05
// util_norm = (-0.05 + 0.05) / (0.05 + 0.05) = 0.0 / 0.10 = 0.0
let util_norm = (utility_delta + UTILITY_PENALTY) / (UTILITY_BOOST + UTILITY_PENALTY);
assert!((util_norm - 0.0).abs() < 1e-9, "Ineffective entry: util_norm must be 0.0");
```

**Test name**: `test_util_norm_neutral_entry_maps_to_half`

```rust
// utility_delta = 0.0 → util_norm = 0.05 / 0.10 = 0.5
assert!((util_norm - 0.5).abs() < 1e-9);
```

**Test name**: `test_util_norm_effective_entry_maps_to_one`

```rust
// utility_delta = +UTILITY_BOOST = +0.05 → util_norm = 0.10 / 0.10 = 1.0
assert!((util_norm - 1.0).abs() < 1e-9);
```

**Rationale**: R-01 (resolved) — the shift-and-scale formula is the canonical authoritative form
(FR-05). Retained as correctness regression guards. A reimplementation using plain division
(`utility_delta / UTILITY_BOOST`) would produce -1.0 for Ineffective entries, failing R-11.

---

### R-11: Ineffective Entry — Fused Score Never Negative

**Test name**: `test_compute_fused_score_ineffective_util_non_negative`

With `util_norm = 0.0` (shift-and-scale result for Ineffective entry) and all other inputs at 0.0,
`w_util = 0.05`:

```rust
let inputs = FusedScoreInputs { similarity: 0.0, nli_entailment: 0.0, confidence: 0.0,
                                  coac_norm: 0.0, util_norm: 0.0, prov_norm: 0.0 };
let weights = FusionWeights { w_sim: 0.25, w_nli: 0.35, w_conf: 0.15,
                               w_coac: 0.10, w_util: 0.05, w_prov: 0.05 };
let score = compute_fused_score(&inputs, &weights);
assert!(score >= 0.0, "fused_score must be >= 0.0 for Ineffective entry, got {score}");
assert!(score.is_finite(), "fused_score must be finite");
```

**Rationale**: R-11, NFR-02 — `util_norm = 0.0` (not -1.0) is what prevents the score from going
below zero. This test explicitly documents the consequence of using the shift-and-scale formula.

---

### R-03: PROVENANCE_BOOST Guard — prov_norm

**Test name**: `test_prov_norm_zero_denominator_returns_zero`

Simulate `PROVENANCE_BOOST = 0.0`:
```rust
let prov_norm = if PROVENANCE_BOOST == 0.0 { 0.0 } else { raw_boost / PROVENANCE_BOOST };
assert_eq!(prov_norm, 0.0, "prov_norm must be 0.0 when PROVENANCE_BOOST == 0.0");
```

**Test name**: `test_prov_norm_boosted_entry_equals_one`

With `PROVENANCE_BOOST = 0.02`, boosted entry has `raw_boost = 0.02`:
- `prov_norm = 0.02 / 0.02 = 1.0`

**Test name**: `test_prov_norm_unboosted_entry_equals_zero`

With `raw_boost = 0.0` and `PROVENANCE_BOOST = 0.02`:
- `prov_norm = 0.0 / 0.02 = 0.0`

**Test name**: `test_compute_fused_score_result_is_finite`

Property-style test: compute 100 scores with randomized valid inputs (all in [0,1]) and valid
weights (sum ≤ 1.0). Assert all results are finite (`score.is_finite()`).

```rust
// Generator: use deterministic pseudo-random permutation over [0.0, 0.1, 0.5, 0.9, 1.0]
// for each of the six input fields and six weight fields (ensuring sum ≤ 1.0).
// Assert: for all, compute_fused_score(&inputs, &weights).is_finite()
```

**Rationale**: R-03, SeR-02 — NaN from unchecked division propagates silently into sort order.
This property test is the general guard; the specific tests above are the precise guards.

---

### R-14: status_penalty Applied Outside compute_fused_score (ADR-004)

**Test name**: `test_compute_fused_score_does_not_accept_status_penalty`

`FusedScoreInputs` must NOT have a `status_penalty` field. `compute_fused_score` signature takes
only `&FusedScoreInputs` and `&FusionWeights`. Verify:

```rust
// Compile-time: if FusedScoreInputs had status_penalty, construction without it would fail.
// This construction must compile exactly as written — no status_penalty field.
let inputs = FusedScoreInputs {
    similarity: 0.8, nli_entailment: 0.7, confidence: 0.6,
    coac_norm: 0.5, util_norm: 0.5, prov_norm: 0.0,
};
// If status_penalty were in FusedScoreInputs, this struct literal would be incomplete and
// rustc would error. Compilation success is the assertion.
```

Then verify penalty applied by caller:
```rust
let fused = compute_fused_score(&inputs, &weights);
let final_score = fused * STATUS_PENALTY;
let expected_final = 0.665 * STATUS_PENALTY;
assert!((final_score - expected_final).abs() < 1e-9);
```

**Rationale**: R-14, ADR-004 — `compute_fused_score` must remain a pure signal combiner. If penalty
is inside the function, the formula is no longer the feature vector that W3-1 will learn.

---

### AC-09: Status Penalty as Multiplier

**Test name**: `test_status_penalty_applied_as_multiplier_after_fused_score`

- `fused_score = 0.8` (from compute_fused_score with known inputs)
- `DEPRECATED_PENALTY = 0.7` (unchanged constant)
- `final_score = 0.8 * 0.7 = 0.56`

Assert `|final_score - 0.56| < 1e-9`.

---

### AC-10: Utility and Provenance Inside Formula — Not Additive (R-14)

**Test name**: `test_compute_fused_score_util_contributes_exactly_w_util_times_util_norm`

Two identical inputs except `util_norm`: one with `util_norm=1.0`, one with `util_norm=0.0`.
The difference in fused scores must equal exactly `w_util * 1.0 - w_util * 0.0 = w_util`.

```rust
let inputs_a = FusedScoreInputs { ..., util_norm: 1.0 };
let inputs_b = FusedScoreInputs { ..., util_norm: 0.0 };
let diff = compute_fused_score(&inputs_a, &weights) - compute_fused_score(&inputs_b, &weights);
assert!((diff - weights.w_util).abs() < 1e-9,
    "util_norm difference must be exactly w_util={}, got diff={diff}", weights.w_util);
```

**Rationale**: AC-10 — confirms `utility_delta` is not applied as an additive afterthought. If
`compute_fused_score` omits the `w_util * util_norm` term and it is added outside the function,
this delta test fails.

---

### Range Guarantee (NFR-02)

**Test name**: `test_compute_fused_score_range_guarantee_all_inputs_max`

All inputs at 1.0, default weights (sum=0.95):
- Expected max = 0.95
- Assert `score <= 1.0` and `score >= 0.0` and `(score - 0.95).abs() < 1e-9`

**Test name**: `test_compute_fused_score_range_guarantee_all_inputs_zero`

All inputs at 0.0:
- Expected = 0.0
- Assert `score == 0.0` (not negative)

---

## Edge Cases

### EC-04: NLI Returns All-Zero Entailment

**Test name**: `test_compute_fused_score_all_zero_nli_degrades_to_five_signals`

`nli_entailment = 0.0`, all other inputs non-zero, NLI weights active (not re-normalized).
Assert score equals the five-signal sum (no NLI contribution, no panic).

### EC-05: coac_norm Slightly Above 1.0 (Floating-Point Epsilon)

**Test name**: `test_prov_norm_coac_norm_clamped_to_one`

If `coac_norm = 1.0 + 1e-15` (floating-point overshoot), the call site should use
`(raw / MAX_CO_ACCESS_BOOST).min(1.0)`. Document this as a consideration; if the implementation
clamps, test that `coac_norm = 1.0` after clamping.
