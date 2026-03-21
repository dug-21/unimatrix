# Test Plan: ScoreWeights / FusedScoreInputs (crt-024)

**File under test**: `crates/unimatrix-server/src/services/search.rs` (structs and `effective()`)
**Test location**: `#[cfg(test)] mod tests` block in the same file
**Risks addressed**: R-02, R-09, R-16; AC-06, AC-13

---

## Component Summary

Two structs:
- `FusedScoreInputs` — six normalized signal fields, all `f64` in [0, 1]; feature vector for W3-1
- `FusionWeights` (or `ScoreWeights`) — six weight fields from `InferenceConfig`; includes
  `effective(nli_available: bool) -> FusionWeights`

`effective()` is the only non-trivial logic in this component. It either returns weights as-is
(NLI active) or re-normalizes the five non-NLI weights to sum to 1.0 (NLI absent).

---

## Unit Test Expectations

### AC-06: NLI-Absent Re-normalization — Five-Weight Denominator (R-02)

**Test name**: `test_fusion_weights_effective_nli_absent_renormalizes_five_weights`

Set weights: `w_sim=0.25, w_nli=0.35, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05`.
Call `effective(nli_available: false)`. Assert:
- `eff.w_nli == 0.0`
- `eff.w_sim + eff.w_conf + eff.w_coac + eff.w_util + eff.w_prov` is within f64 epsilon of 1.0
- `eff.w_sim` ≈ 0.25 / 0.60 ≈ 0.4167 (within 1e-6)
- `eff.w_conf` ≈ 0.15 / 0.60 = 0.2500 (within 1e-6)
- `eff.w_coac` ≈ 0.10 / 0.60 ≈ 0.1667 (within 1e-6)

```rust
#[test]
fn test_fusion_weights_effective_nli_absent_renormalizes_five_weights() {
    let weights = FusionWeights { w_sim: 0.25, w_nli: 0.35, w_conf: 0.15,
                                   w_coac: 0.10, w_util: 0.05, w_prov: 0.05 };
    let eff = weights.effective(false);
    assert!((eff.w_nli - 0.0).abs() < 1e-9, "w_nli must be 0.0 when NLI absent");
    let sum = eff.w_sim + eff.w_conf + eff.w_coac + eff.w_util + eff.w_prov;
    assert!((sum - 1.0).abs() < 1e-9, "re-normalized weights must sum to 1.0, got {sum}");
    assert!((eff.w_sim - (0.25 / 0.60)).abs() < 1e-6, "w_sim must be re-normalized");
}
```

**Rationale**: AC-06, R-02 — confirms the denominator is all five non-NLI weights (not a
hardcoded three-weight subset, SR-03 resolution).

---

### R-02: Zero-Denominator Guard — All Non-NLI Weights Zero

**Test name**: `test_fusion_weights_effective_zero_denominator_returns_zeros_without_panic`

Set weights: `w_sim=0, w_nli=0.5, w_conf=0, w_coac=0, w_util=0, w_prov=0`.
Call `effective(nli_available: false)`. Assert:
- Does not panic
- Returns a `FusionWeights` where all fields are 0.0

```rust
#[test]
fn test_fusion_weights_effective_zero_denominator_returns_zeros_without_panic() {
    let weights = FusionWeights { w_sim: 0.0, w_nli: 0.5, w_conf: 0.0,
                                   w_coac: 0.0, w_util: 0.0, w_prov: 0.0 };
    let eff = weights.effective(false);
    // Must not panic; all effective weights must be 0.0
    assert_eq!(eff.w_sim,  0.0, "w_sim must be 0.0 on zero-denominator guard");
    assert_eq!(eff.w_conf, 0.0);
    assert_eq!(eff.w_coac, 0.0);
    assert_eq!(eff.w_util, 0.0);
    assert_eq!(eff.w_prov, 0.0);
}
```

**Rationale**: R-02 — pathological but reachable config. Dividing by 0.0 produces NaN/infinity
which silently corrupts all scores. Guard must be in `effective()`, not in the scoring loop.

---

### R-02b: Single Non-Zero Remaining Weight

**Test name**: `test_fusion_weights_effective_single_nonzero_weight_nli_absent`

Set weights: `w_sim=0.5, w_nli=0.5, w_conf=0, w_coac=0, w_util=0, w_prov=0`.
Call `effective(nli_available: false)`. Assert:
- `eff.w_nli == 0.0`
- `eff.w_sim == 1.0` (only non-zero weight gets full weight)
- All others remain 0.0

**Rationale**: R-02, Scenario 2 — confirms single-weight normalization arithmetic.

---

### AC-13: NLI-Active Path — No Re-normalization (R-09)

**Test name**: `test_fusion_weights_effective_nli_active_unchanged`

Set weights: `w_sim=0.25, w_nli=0.35, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05`
(sum = 0.95, valid with headroom).
Call `effective(nli_available: true)`. Assert:
- `eff.w_nli == 0.35` (unchanged)
- `eff.w_sim == 0.25` (unchanged)
- `eff.w_conf == 0.15` (unchanged)
- All six fields equal their input values exactly

```rust
#[test]
fn test_fusion_weights_effective_nli_active_unchanged() {
    let weights = FusionWeights { w_sim: 0.25, w_nli: 0.35, w_conf: 0.15,
                                   w_coac: 0.10, w_util: 0.05, w_prov: 0.05 };
    let eff = weights.effective(true);
    assert!((eff.w_nli  - 0.35).abs() < 1e-9, "w_nli must be unchanged when NLI active");
    assert!((eff.w_sim  - 0.25).abs() < 1e-9, "w_sim must be unchanged when NLI active");
    assert!((eff.w_conf - 0.15).abs() < 1e-9);
    assert!((eff.w_coac - 0.10).abs() < 1e-9);
    assert!((eff.w_util - 0.05).abs() < 1e-9);
    assert!((eff.w_prov - 0.05).abs() < 1e-9);
}
```

**Rationale**: AC-13, R-09 — re-normalization is strictly limited to the NLI-absent path.
If `effective(true)` modifies weights, configured headroom is silently consumed and AC-13 fails.

---

### AC-13b: Valid Sum < 1.0 — NLI Active Headroom Preserved

**Test name**: `test_fusion_weights_effective_nli_active_headroom_weight_preserved`

Set weights summing to 0.90 (valid, with 0.10 headroom).
Call `effective(nli_available: true)`. Assert returned sum ≈ 0.90, not 1.0.
Fused score over all-ones inputs must be ≤ 0.90.

**Rationale**: AC-13 — WA-2 reserves headroom via the 0.05 default gap. If `effective(true)` were
to re-normalize to 1.0, that headroom would be consumed silently.

---

### R-09 Negative: NLI Absent — Re-normalization Fires

**Test name**: `test_fusion_weights_effective_nli_absent_sum_is_one`

This is the complement to AC-13. Confirm re-normalization produces sum == 1.0 (not the original
0.60 denominator) when NLI is absent.

```rust
let eff = weights.effective(false);
let sum = eff.w_sim + eff.w_conf + eff.w_coac + eff.w_util + eff.w_prov;
assert!((sum - 1.0).abs() < 1e-9, "NLI-absent: effective weights must sum to 1.0");
```

---

### R-16: Struct Extensibility (WA-2 Contract)

**Test name**: `test_fused_score_inputs_struct_accessible_by_field_name`

Construct `FusedScoreInputs` with all named fields including `phase_boost_norm` if it is present,
or confirm the struct has exactly the six expected fields via field-access compilation.

This is a compile-time test: if the struct has positional fields or uses a tuple, WA-2's
"add one field" extension becomes a breaking change on all existing construction sites.

```rust
#[test]
fn test_fused_score_inputs_struct_accessible_by_field_name() {
    let inputs = FusedScoreInputs {
        similarity:      0.8,
        nli_entailment:  0.7,
        confidence:      0.6,
        coac_norm:       0.5,
        util_norm:       0.5,
        prov_norm:       0.0,
    };
    // If this compiles, the struct uses named fields — WA-2 can add one without breaking this site
    assert!((inputs.similarity - 0.8).abs() < 1e-9);
}
```

**Rationale**: R-16 — named-field structs allow WA-2 to add `phase_boost_norm` using `..existing`
syntax. Tuple structs would require updating every construction site.

---

## ADR-003 Constraint Verification (R-06)

These three tests are required as named unit tests (not just ADR prose):

**Test name**: `test_constraint_9_nli_disabled_sim_dominant_over_conf`

NLI disabled re-normalization (denominator = 0.60):
- `w_sim' = 0.4167, w_conf' = 0.2500`
- Entry A: sim=0.9, conf=0.3 → re-normalized score ≈ 0.450
- Entry B: sim=0.5, conf=0.9 → re-normalized score ≈ 0.434
- Assert score(A) > score(B) (sim dominant over conf in NLI-absent ranking)

**Test name**: `test_constraint_10_sim_dominant_no_nli_no_coac`

Default weights, NLI disabled, no co-access:
- Entry A: sim=0.9, conf=0.3 (score at default w_sim=0.25, w_conf=0.15: 0.270)
- Entry B: sim=0.5, conf=0.9 (score: 0.260)
- Assert score(A) > score(B)

(Note: these involve both `FusionWeights::effective` and `compute_fused_score` — they straddle the
component boundary and should be placed in the `compute-fused-score` test suite where both can be
called together.)
