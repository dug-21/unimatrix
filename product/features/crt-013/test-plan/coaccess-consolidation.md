# Test Plan: Co-Access Signal Consolidation

## Unit Tests

### Confidence Invariant (R-01)

| Test | Assertion | Risk |
|------|-----------|------|
| weight_sum_stored_invariant (existing) | W_BASE + ... + W_TRUST == 0.92 | R-01 |
| weight_sum_invariant_f64 (modified) | stored_sum == 0.92 (remove W_COAC assertions) | R-01 |

### Compilation Verification (R-01, R-02, R-10)

| Test | Assertion | Risk |
|------|-----------|------|
| cargo build --workspace | Zero errors | R-01, R-02, R-10 |
| cargo test --workspace | All remaining tests pass | R-01 |

### Grep Verification (AC-01, AC-02)

| Check | Command | Expected |
|-------|---------|----------|
| AC-01 | `grep -r "episodic" --include="*.rs" crates/` | Zero hits |
| AC-02 | `grep -r "co_access_affinity\|W_COAC" --include="*.rs" crates/` | Zero hits |

## Tests Removed

9 tests from confidence.rs:
- weight_sum_effective_invariant
- co_access_affinity_zero_partners
- co_access_affinity_max_partners_max_confidence
- co_access_affinity_large_partner_count_saturated
- co_access_affinity_zero_confidence
- co_access_affinity_negative_confidence
- co_access_affinity_effective_sum_clamped
- co_access_affinity_partial_partners
- co_access_affinity_returns_f64

5 tests from episodic.rs (entire module deleted):
- construction
- single_result_no_adjustment
- no_affinity_no_adjustment
- stub_returns_zero
- empty_results

Partial modification to weight_sum_invariant_f64: remove W_COAC lines.

## Edge Cases

- No edge cases specific to deletion. The compiler is the primary verification.
- Remaining confidence tests verify no behavioral change.
