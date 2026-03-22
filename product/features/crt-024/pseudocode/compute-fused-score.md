# crt-024: compute_fused_score Pure Function — Pseudocode

## Purpose

`compute_fused_score` is a pure function that applies the six-term weighted linear combination
to a set of normalized signal inputs. It is extracted as a standalone `pub(crate)` function
(ADR-004) so it can be unit-tested with known inputs without constructing a `SearchService`,
spawning tasks, or requiring a database.

This function has no side effects, no I/O, no async, no locks. It is the canonical arithmetic
kernel of the entire crt-024 feature.

Location: `crates/unimatrix-server/src/services/search.rs`, after the `FusionWeights` impl block.

---

## Function Signature

```
/// Compute the fused ranking score from normalized signal inputs and weights.
///
/// Pure function: no I/O, no async, no locks, no side effects.
///
/// Preconditions (caller's responsibility, enforced by construction):
///   - All inputs in [0.0, 1.0]
///   - weights.w_* fields individually in [0.0, 1.0]
///   - sum of weights <= 1.0 (after effective() is applied for NLI absence)
///
/// Returns a value in [0.0, 1.0] by construction under the above preconditions.
///
/// `status_penalty` is NOT applied here. Apply it at the call site:
///   final_score = compute_fused_score(&inputs, &weights) * status_penalty
///
/// WA-2 extension: add `w_phase * inputs.phase_boost_norm` term when WA-2 is implemented.
pub(crate) fn compute_fused_score(
    inputs: &FusedScoreInputs,
    weights: &FusionWeights,
) -> f64
```

---

## Function Body

```
pub(crate) fn compute_fused_score(
    inputs: &FusedScoreInputs,
    weights: &FusionWeights,
) -> f64 {
    weights.w_sim  * inputs.similarity
  + weights.w_nli  * inputs.nli_entailment
  + weights.w_conf * inputs.confidence
  + weights.w_coac * inputs.coac_norm
  + weights.w_util * inputs.util_norm
  + weights.w_prov * inputs.prov_norm
}
```

That is the entire function. No conditionals, no guards, no early returns. The simplicity is
intentional and load-bearing: ADR-004 requires this function to be a transparent accumulator
so that W3-1 can replace the weights by training, and so that test assertions can verify
formula correctness with exact arithmetic.

---

## What This Function Does NOT Do

- Does NOT apply `status_penalty` (applied at call site: `fused * penalty`)
- Does NOT call `FusionWeights::effective()` (called before passing weights in)
- Does NOT guard for zero weights (zero weights contribute 0.0 cleanly)
- Does NOT clamp the result to [0, 1] (range guaranteed by preconditions)
- Does NOT log or record anything
- Does NOT access any service state

---

## Call Site Pattern (from scoring loop in SearchService)

The caller constructs `FusedScoreInputs`, calls `weights.effective(nli_available)` to get the
appropriate weight set, then calls `compute_fused_score`, then multiplies by the penalty:

```
let effective_weights = self.fusion_weights.effective(nli_available);

for (i, (entry, sim)) in candidates.iter().enumerate() {
    // ... construct FusedScoreInputs (see search-service.md for full detail) ...

    let fused = compute_fused_score(&inputs, &effective_weights);
    let penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0);
    let final_score = fused * penalty;

    scored.push((entry.clone(), *sim, final_score));
}
```

The `effective_weights` computation happens ONCE before the loop, not per-candidate, because
NLI availability does not change between candidates in the same search call.

---

## Numerical Verification (from ADR-003)

These exact values must be reproducible by the function. They are the acceptance criteria
numerical checks from ARCHITECTURE.md and SPECIFICATION.md.

### AC-11 regression check

Inputs (equal sim=0.5, conf=0.5, util neutral=0.5, no co-access, no provenance):
- Entry A: sim=0.5, nli=0.9, conf=0.5, coac_norm=0.0, util_norm=0.5, prov_norm=0.0
- Entry B: sim=0.5, nli=0.3, conf=0.5, coac_norm=1.0, util_norm=0.5, prov_norm=0.0

Weights: defaults (w_sim=0.25, w_nli=0.35, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05)

```
score_A = 0.25*0.5 + 0.35*0.9 + 0.15*0.5 + 0.10*0.0 + 0.05*0.5 + 0.05*0.0
        = 0.125 + 0.315 + 0.075 + 0.0 + 0.025 + 0.0
        = 0.540

score_B = 0.25*0.5 + 0.35*0.3 + 0.15*0.5 + 0.10*1.0 + 0.05*0.5 + 0.05*0.0
        = 0.125 + 0.105 + 0.075 + 0.100 + 0.025 + 0.0
        = 0.430
```

Assert: score_A (0.540) > score_B (0.430). NLI-high beats co-access-high.

### AC-05 correctness check

sim=0.8, nli=0.7, conf=0.6, coac_raw=0.015 → coac_norm=0.5, util_norm=0.5, prov_norm=1.0
weights: w_sim=0.30, w_nli=0.30, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05

```
expected = 0.30*0.8 + 0.30*0.7 + 0.15*0.6 + 0.10*0.5 + 0.05*0.5 + 0.05*1.0
         = 0.24 + 0.21 + 0.09 + 0.05 + 0.025 + 0.05
         = 0.665
```

Assert: |compute_fused_score(inputs, weights) - 0.665| < 1e-9.

### Constraint 10 check (ADR-003)

sim dominant over conf at defaults, no NLI, no co-access:
- Entry A: sim=0.9, conf=0.3, nli=0.0, coac=0.0, util=0.5, prov=0.0 → 0.25*0.9 + 0.15*0.3 = 0.270
- Entry B: sim=0.5, conf=0.9, nli=0.0, coac=0.0, util=0.5, prov=0.0 → 0.25*0.5 + 0.15*0.9 = 0.260

Assert score_A > score_B.

---

## Error Handling

This function cannot fail. It performs only f64 arithmetic on values that, by precondition, are
finite and in [0.0, 1.0]. The result is deterministic for identical inputs (NFR-03).

The only edge cases in the arithmetic domain:
- All inputs = 0.0 → returns 0.0 (not negative, not NaN)
- All inputs = 1.0, all weights at max valid sum 1.0 → returns exactly 1.0
- NaN input values: if a caller violates the precondition and passes NaN, the result is NaN.
  This is not guarded here — callers must normalize inputs correctly. The `prov_norm` zero-guard
  in the scoring loop (search-service.md) is the critical guard preventing NaN entry.

---

## Key Test Scenarios

### T-CF-01: AC-05 known-value correctness

Inputs and weights from the AC-05 numerical check above. Assert |result - 0.665| < 1e-9.
This is the primary correctness regression test for the formula.

### T-CF-02: AC-11 NLI-high beats co-access-high

Inputs and weights from the AC-11 numerical check above. Assert score_A > score_B.
This test must be present and must reference "AC-11" in a comment for traceability.

### T-CF-03: Constraint 10 — sim dominant at defaults, NLI absent

Construct effective weights with `effective(false)` applied to defaults.
Entry A (sim=0.9, conf=0.3, rest neutral), Entry B (sim=0.5, conf=0.9, rest neutral).
Assert score_A > score_B. (ADR-003 Constraint 10 numerical verification)

### T-CF-04: status_penalty at call site, not inside

Compute `compute_fused_score` with known inputs. Assert result does NOT change when a separate
penalty would apply. Apply penalty manually: `result * ORPHAN_PENALTY`. Assert both products
work correctly. This test documents that penalty is external.

### T-CF-05: all-zero inputs → 0.0 output (EC-01)

Inputs: all fields 0.0. Weights: defaults. Assert result == 0.0. Not negative, not NaN.

### T-CF-06: all-one inputs, weights sum to 1.0 → at most 1.0 output

Inputs: all fields 1.0. Weights: w_sim=0.4, w_nli=0.35, w_conf=0.15, w_coac=0.10, others 0.0
(sum = 1.0). Assert result == 1.0.

### T-CF-07: single-term formula (one weight non-zero)

Weights: w_sim=1.0, all others 0.0. Inputs: sim=0.75, all others anything.
Assert result == 0.75. Confirms the formula has no cross-term contamination.

### T-CF-08: NLI disabled — w_nli=0.0 contributes nothing (R-09)

Weights: w_nli=0.0, others valid. Inputs: nli_entailment=0.9.
Assert result equals the same computation with nli_entailment=0.0 (w_nli*anything == 0.0).

### T-CF-09: result is_finite for any valid inputs (R-03, SeR-02)

Property-style test: generate 20+ candidate input vectors with values in [0.0, 1.0] and weights
summing to <= 1.0. Assert all results are `is_finite()` (no NaN, no infinity).

### T-CF-10: utility_norm at all three boundary values (R-01, R-11)

Test the normalization that produces util_norm separately (at the call site in the loop), then
feed the normalized value into compute_fused_score:
- Ineffective: raw_delta = -0.05 → util_norm = 0.0 → fused score contribution = w_util * 0.0
- Neutral: raw_delta = 0.0 → util_norm = 0.5
- Effective: raw_delta = +0.05 → util_norm = 1.0
Assert fused score is >= 0.0 in all three cases (score range guarantee, NFR-02, R-11).

### T-CF-11: prov_norm zero-guard (R-03)

Test the normalization that produces prov_norm: when PROVENANCE_BOOST == 0.0, prov_norm must be
0.0 (not NaN from 0.0/0.0). Pass prov_norm=0.0 into compute_fused_score. Assert result is finite.
