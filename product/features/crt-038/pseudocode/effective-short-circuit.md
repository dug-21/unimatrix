# Component: FusionWeights::effective() Short-Circuit

**Wave**: 1 (parallel with config-defaults)
**File**: `crates/unimatrix-server/src/services/search.rs`
**ACs**: AC-02
**Risks**: R-01 (Critical), R-02 (Critical)

---

## Purpose

Add a correctness guard to `FusionWeights::effective()` that short-circuits before the
re-normalization branch when `w_nli == 0.0`. Without this guard, calling `effective(false)`
with the new defaults (`w_nli=0.00, w_sim=0.50, w_conf=0.35`) would divide each weight by
the sum of non-NLI weights (0.85), producing `w_sim≈0.588, w_conf≈0.412` — a formula
that was never evaluated and has no empirical basis.

This is a correctness fix (ADR-001). The guard must execute before the `if nli_available`
branch, not after it.

---

## Modified Function: FusionWeights::effective()

**Location**: `search.rs` line 151
**Signature** (unchanged): `pub(crate) fn effective(&self, nli_available: bool) -> FusionWeights`

### Current structure (three paths)

```
effective(nli_available):
  if nli_available == true:
    return self unchanged          // fast path: NLI scores available
  // NLI absent:
  denom = w_sim + w_conf + w_coac + w_util + w_prov
  if denom == 0.0:
    warn and return all-zeros      // zero-denominator guard (pathological)
  return re-normalized weights     // redistribute w_nli budget across remaining signals
```

### New structure (four paths, guard inserted FIRST)

```
effective(nli_available):
  // SHORT-CIRCUIT: inserted as first branch (ADR-001, AC-02)
  // Safe to use exact f64 equality because w_nli is always set from a literal
  // constant in default_w_nli() or from operator TOML config (not computed arithmetic).
  if self.w_nli == 0.0:
    return FusionWeights { ..*self }   // copy all fields unchanged; no redistribution

  // Existing paths below are UNCHANGED — only reachable when w_nli > 0.0

  if nli_available == true:
    return FusionWeights { ..*self }   // NLI scores available: use weights as-is

  // NLI absent AND w_nli > 0.0: redistribute the NLI weight budget
  denom = self.w_sim + self.w_conf + self.w_coac + self.w_util + self.w_prov
  if denom == 0.0:
    tracing::warn("all non-NLI weights are 0.0; fused_score will be 0.0")
    return FusionWeights {
      w_sim: 0.0, w_nli: 0.0, w_conf: 0.0, w_coac: 0.0, w_util: 0.0, w_prov: 0.0,
      w_phase_histogram: self.w_phase_histogram,   // pass-through (additive, not re-normalized)
      w_phase_explicit:  self.w_phase_explicit,    // pass-through (additive, not re-normalized)
    }
  return FusionWeights {
    w_sim:             self.w_sim  / denom,
    w_nli:             0.0,
    w_conf:            self.w_conf / denom,
    w_coac:            self.w_coac / denom,
    w_util:            self.w_util / denom,
    w_prov:            self.w_prov / denom,
    w_phase_histogram: self.w_phase_histogram,   // pass-through (unchanged from current code)
    w_phase_explicit:  self.w_phase_explicit,    // pass-through (unchanged from current code)
  }
```

### Implementation note

The existing `nli_available=true` path and the zero-denominator guard are both
preserved exactly. The only new code is the three-line guard at the top.

The guard uses `FusionWeights { ..*self }` (struct copy via spread syntax) rather than
`return *self` because `FusionWeights` must implement `Copy` for `*self` to work.
Verify that `Copy` is derived on `FusionWeights`; if not, use the explicit field copy
form matching the existing pattern in the `nli_available=true` branch.

---

## Updated Doc Comment

The doc comment on `effective()` (currently lines 140–150) must be updated to document
the new short-circuit branch as the first case:

```
/// Return an effective weight set adjusted for NLI availability.
///
/// Short-circuit (w_nli == 0.0): returns self unchanged regardless of nli_available.
///   Re-normalization is semantically meaningful only when w_nli > 0.0 (redistributing
///   a real weight budget because NLI is absent). Re-normalizing zero is a correctness
///   error that silently inflates sim and conf (ADR-001, crt-038).
///
/// NLI active (nli_available = true, w_nli > 0.0): returns self unchanged.
///   The configured weights are used directly. No re-normalization.
///
/// NLI absent (nli_available = false, w_nli > 0.0): sets w_nli = 0.0, re-normalizes
///   the remaining five weights by dividing each by their sum.
///   This preserves the relative signal dominance ordering (Constraint 9, ADR-003).
///
/// Zero-denominator guard (R-02): if all five non-NLI weights are 0.0
///   (pathological but reachable config), returns all-zeros without panic.
```

---

## Modified Test: test_fusion_weights_default_sum_unchanged_by_crt030

**Location**: `search.rs` line 4826 (inside `#[cfg(test)]` module)
**Change**: Update the assertion message only. Expected sum value (0.92) is unchanged.

```
Current message:
  "FusionWeights default sum must be 0.92 (crt-032: w_coac zeroed); got {total}"

New message:
  "FusionWeights default sum must be 0.92 (crt-038: conf-boost-c defaults); got {total}"
```

The arithmetic comment block above the assert must also be updated:
```
// crt-038: conf-boost-c defaults: w_sim=0.50, w_nli=0.00, w_conf=0.35,
// w_coac=0.00, w_util=0.00, w_prov=0.00, w_phase_histogram=0.02, w_phase_explicit=0.05
// Sum: 0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 + 0.02 + 0.05 = 0.92
```

---

## New Tests (three required — AC-02, R-01)

All three tests belong in the same `#[cfg(test)]` module as the existing
`test_fusion_weights_default_sum_unchanged_by_crt030` test.

### Test 1: test_effective_short_circuit_w_nli_zero_nli_available_false

```
setup:
  fw = FusionWeights {
    w_sim: 0.50, w_nli: 0.00, w_conf: 0.35,
    w_coac: 0.00, w_util: 0.00, w_prov: 0.00,
    w_phase_histogram: 0.02, w_phase_explicit: 0.05,
  }
act:
  result = fw.effective(false)
assert:
  result.w_sim  == 0.50   (not 0.50/0.85 ≈ 0.588)
  result.w_nli  == 0.00
  result.w_conf == 0.35   (not 0.35/0.85 ≈ 0.412)
  result.w_coac == 0.00
  result.w_util == 0.00
  result.w_prov == 0.00
  result.w_phase_histogram == 0.02
  result.w_phase_explicit  == 0.05
  // Use abs() < 1e-9 for each f64 comparison
```

Purpose: Verifies the short-circuit fires on `effective(false)` with `w_nli=0.0`.
Without the short-circuit, re-normalization would produce 0.588/0.412.

### Test 2: test_effective_short_circuit_w_nli_zero_nli_available_true

```
setup:
  fw = FusionWeights {
    w_sim: 0.50, w_nli: 0.00, w_conf: 0.35,
    w_coac: 0.00, w_util: 0.00, w_prov: 0.00,
    w_phase_histogram: 0.02, w_phase_explicit: 0.05,
  }
act:
  result = fw.effective(true)
assert:
  result.w_sim  == 0.50
  result.w_nli  == 0.00
  result.w_conf == 0.35
  // all fields equal input exactly
  // Use abs() < 1e-9 for each f64 comparison
```

Purpose: Confirms that when `w_nli=0.0`, `effective(true)` also returns unchanged
(the short-circuit fires before the `nli_available=true` fast path).

### Test 3: test_effective_renormalization_still_fires_when_w_nli_positive

```
setup:
  fw = FusionWeights {
    w_sim: 0.25, w_nli: 0.20, w_conf: 0.15,
    w_coac: 0.00, w_util: 0.05, w_prov: 0.05,
    w_phase_histogram: 0.02, w_phase_explicit: 0.05,
  }
  // Non-NLI sum = 0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.50
act:
  result = fw.effective(false)
assert:
  // Short-circuit must NOT fire (w_nli=0.20 > 0.0)
  // Re-normalization must occur: each non-NLI weight divided by 0.50
  (result.w_sim  - 0.50).abs()  < 1e-9   // 0.25 / 0.50
  result.w_nli  == 0.00                  // zeroed
  (result.w_conf - 0.30).abs()  < 1e-9  // 0.15 / 0.50
  result.w_coac == 0.00                 // 0.00 / 0.50
  (result.w_util - 0.10).abs()  < 1e-9  // 0.05 / 0.50
  (result.w_prov - 0.10).abs()  < 1e-9  // 0.05 / 0.50
  result.w_phase_histogram == fw.w_phase_histogram  // pass-through
  result.w_phase_explicit  == fw.w_phase_explicit   // pass-through
```

Purpose: Guards against the short-circuit accidentally suppressing the re-normalization
path for `w_nli > 0.0`. Required by RISK-TEST-STRATEGY R-01 scenario 3.

---

## Error Handling

This function does not return a `Result`. The zero-denominator guard (existing code)
emits a `tracing::warn!` and returns all-zeros for the pathological all-zero weights
case. This guard is unreachable through the new short-circuit path (if `w_nli == 0.0`,
the short-circuit fires first; the zero-denominator guard only applies when
`w_nli > 0.0` and all remaining weights are also 0.0).

---

## Key Test Scenarios Summary

| Scenario | w_nli | nli_available | Expected behavior |
|----------|-------|---------------|-------------------|
| Short-circuit: false path | 0.0 | false | Return unchanged (new behavior) |
| Short-circuit: true path | 0.0 | true | Return unchanged (via short-circuit) |
| Re-normalization preserved | 0.20 | false | Re-normalize (guard must not fire) |
| Existing: NLI active | 0.35 | true | Return unchanged (existing fast path) |
| Existing: all-zero pathological | 0.35 | false | Zero-denominator guard fires |
