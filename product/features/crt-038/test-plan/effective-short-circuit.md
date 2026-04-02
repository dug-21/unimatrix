# Test Plan: FusionWeights::effective() Short-Circuit

**Component**: `crates/unimatrix-server/src/services/search.rs`
**AC Coverage**: AC-02
**Risk Coverage**: R-01, R-09
**Wave**: Wave 1 (parallel with config-defaults)

---

## Unit Test Expectations

### New Tests (must be added — AC-02)

Three unit tests are required in `search.rs` inside the existing `#[cfg(test)]` module.
All three are in `mod tests` within `search.rs`.

---

#### `test_effective_short_circuit_w_nli_zero_nli_available_false`

**Covers**: R-01 scenario 1 — effective(false) with w_nli=0.0 must return weights unchanged.

**Arrange**:
```rust
let fw = FusionWeights {
    w_sim: 0.50,
    w_nli: 0.00,
    w_conf: 0.35,
    w_coac: 0.00,
    w_util: 0.00,
    w_prov: 0.00,
    w_phase_histogram: 0.02,
    w_phase_explicit: 0.05,
};
```

**Act**:
```rust
let result = fw.effective(false);
```

**Assert** (field-by-field equality, not approximate):
```rust
assert_eq!(result.w_sim,             0.50, "w_sim must not be re-normalized");
assert_eq!(result.w_nli,             0.00, "w_nli must remain 0.0");
assert_eq!(result.w_conf,            0.35, "w_conf must not be re-normalized");
assert_eq!(result.w_coac,            0.00);
assert_eq!(result.w_util,            0.00);
assert_eq!(result.w_prov,            0.00);
assert_eq!(result.w_phase_histogram, 0.02);
assert_eq!(result.w_phase_explicit,  0.05);
```

**Critical invariant**: Before this fix, `effective(false)` with these weights produced `w_sim'≈0.588` and `w_conf'≈0.412`. The test must assert EXACT equality, not just approximate — asserting `result.w_sim != 0.588` would be insufficient.

---

#### `test_effective_short_circuit_w_nli_zero_nli_available_true`

**Covers**: R-01 scenario 2 — effective(true) with w_nli=0.0 also returns weights unchanged (existing fast-path also works, but the short-circuit fires first).

**Arrange**: Same `FusionWeights` as above.

**Act**:
```rust
let result = fw.effective(true);
```

**Assert**: Identical field values as above. Both `effective(true)` and `effective(false)` must return unchanged weights when `w_nli == 0.0`. The short-circuit fires before the `nli_available` branch, so both paths hit it.

---

#### `test_effective_renormalization_still_fires_when_w_nli_positive`

**Covers**: R-01 scenario 3 — the short-circuit must NOT suppress re-normalization when `w_nli > 0.0`.

**Purpose**: Guards against an over-broad short-circuit that returns `*self` unconditionally (ignoring `w_nli`). The positive-weight re-normalization path must be preserved.

**Arrange**:
```rust
let fw = FusionWeights {
    w_sim: 0.25,
    w_nli: 0.20,   // positive — short-circuit must not fire
    w_conf: 0.15,
    w_coac: 0.00,
    w_util: 0.05,
    w_prov: 0.05,
    w_phase_histogram: 0.02,
    w_phase_explicit:  0.05,
};
```

**Act**:
```rust
let result = fw.effective(false);  // nli_available=false → triggers re-normalization
```

**Assert**: `w_nli` is zeroed and remaining weights are scaled up. The denominator for the five non-NLI core weights is `0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.50`.

Expected re-normalized values:
- `result.w_nli  == 0.00` (zeroed)
- `result.w_sim  ≈ 0.25 / 0.50 = 0.50` (within f64 precision)
- `result.w_conf ≈ 0.15 / 0.50 = 0.30`
- `result.w_util ≈ 0.05 / 0.50 = 0.10`
- `result.w_prov ≈ 0.05 / 0.50 = 0.10`
- `result.w_coac ≈ 0.00 / 0.50 = 0.00`

Use approximate equality (e.g., `(result.w_sim - 0.50).abs() < 1e-10`) since this is floating-point division.

Assert that `result.w_sim` differs from `fw.w_sim` — confirming re-normalization occurred, not a short-circuit.

---

### Existing Test (must be updated — AC-01, R-09)

#### `test_fusion_weights_default_sum_unchanged_by_crt030`

**Current state**: The assertion message references "crt-030". The expected sum of 0.92 is unchanged (`0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 + 0.02 + 0.05 = 0.92`).

**Required change**: Update only the assertion message string to reference crt-038 (e.g., `"sum changed from crt-038 formula defaults; expected 0.92"`). The expected value `0.92` is unchanged.

**Verification**: One-line string change; confirm in diff review.

---

## Integration Test Expectations

The formula change is not directly observable through MCP JSON-RPC responses (ranking differences are not deterministically assertable without a fixed query corpus). The infra-001 integration suites validate that:

- `context_search` continues to return results in valid format (tools suite)
- Search and briefing multi-step flows complete without error (lifecycle suite)
- Edge cases (empty DB, large payloads) do not cause regression (edge_cases suite)

No new integration tests are needed for this component. The formula correctness is validated by:
1. The three unit tests above (structural correctness of effective())
2. AC-12 eval gate (MRR at scale)

---

## Edge Cases Requiring Test Coverage

### All-zero weights pathological case

After the short-circuit is added, the all-zero denominator guard inside the re-normalization path is only reachable when `w_nli > 0.0` AND `nli_available=false` AND all other five core weights are also zero. Verify this guard is still reachable in the existing tests — it must not have been accidentally eliminated by the short-circuit placement.

**Check**: Confirm an existing test covers `effective(false)` with `w_nli > 0.0` and all-zero remaining weights (all-zero denominator → guard fires). If no such test exists, add one. Scan existing tests for this scenario before Stage 3b.

### f64 equality for w_nli == 0.0

The short-circuit uses exact f64 equality (`self.w_nli == 0.0`). This is safe because `default_w_nli()` returns a constant literal `0.0`. Verify no code path computes `w_nli` via arithmetic that could produce a near-zero non-literal (e.g., `0.35 - 0.35` can produce `~1e-17`).

**Assertion**: In the three new tests, `w_nli` is constructed as a literal `0.00`. This directly tests the production code path (default config → literal zero).

---

## Assertions Summary

| Test | Assertion Type | Key Values |
|------|---------------|------------|
| `test_effective_short_circuit_w_nli_zero_nli_available_false` | Exact equality (==) | All fields match input exactly |
| `test_effective_short_circuit_w_nli_zero_nli_available_true` | Exact equality (==) | All fields match input exactly |
| `test_effective_renormalization_still_fires_when_w_nli_positive` | Approx equality (<1e-10) | w_nli=0.0, others re-normalized |
| `test_fusion_weights_default_sum_unchanged_by_crt030` | Exact equality (==) | sum=0.92, message references crt-038 |

---

## Failure Modes

**Short-circuit is placed after the `nli_available` branch (not before)**: `test_effective_short_circuit_w_nli_zero_nli_available_false` fails because `effective(false)` still re-normalizes. This is R-01: the guard must be the FIRST branch in the function.

**Short-circuit uses `nli_available` condition instead of `w_nli == 0.0`**: `test_effective_short_circuit_w_nli_zero_nli_available_false` passes (both return unchanged) but `test_effective_renormalization_still_fires_when_w_nli_positive` fails (positive w_nli, nli_available=false would short-circuit instead of re-normalizing).

**Assertion message not updated**: `test_fusion_weights_default_sum_unchanged_by_crt030` passes functionally but the message string still says "crt-030". This is R-09 — grep for the string in diff review.
