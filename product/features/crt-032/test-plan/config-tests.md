# Test Plan: config-tests

## Component: `src/infra/config.rs` — Test Code Sites

### Risks Covered

- R-02 (Critical): No test asserts default is 0.10 after delivery
- R-03 (High): Intentional fixture `test_inference_config_validate_accepts_sum_exactly_one` unchanged
- R-07 (Low): Partial-TOML comment updated

---

## Unit Test Expectations

### Test: `make_weight_config()` helper — MUST CHANGE

**Risk**: R-02 (the helper sets "the defaults"; used in per-field rejection tests)
**Change**: `w_coac: 0.10` → `w_coac: 0.0`
**Impact**: Per-field rejection tests using this helper pass naturally — they mutate one field and check error messages; none compute a sum.
**Pass condition**: Helper body has `w_coac: 0.0`

---

### Test: `test_inference_config_validate_accepts_sum_exactly_one` — MUST NOT CHANGE

**Risk**: R-03 (intentional fixture)
**This test** sets `w_coac: 0.10` explicitly to construct a sum-exactly-1.0 scenario.
**Pass condition**: Test unchanged; `w_coac: 0.10` still present in this test body.
**Verification**: Count `FusionWeights`/`InferenceConfig` literals with `w_coac: 0.10` in this test — count = 1 before and after delivery.

---

### Test: `test_inference_config_partial_toml_gets_defaults_not_error` — COMMENT ONLY CHANGE

**Risk**: R-07
**Change**: Inline comment only. The test assertion is unchanged.

**Before comment**:
```rust
// Total sum: 0.40 + 0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 1.00
```

**After comment**:
```rust
// Total sum: 0.40 + 0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.90
```

**Pass condition**: Comment references `0.0` for w_coac and total of `0.90`.

---

### Fixture Scan: No Default Assertions at 0.10 (R-02)

After delivery, verify:
```
grep "w_coac.*0\.10" config.rs
```

Expected matches:
- `test_inference_config_validate_accepts_sum_exactly_one` body: 1 match (intentional fixture — acceptable)
- Default function body: 0 matches (was changed)
- Default struct literal: 0 matches (was changed)
- Default assertion message: 0 matches (was changed)
- `make_weight_config()`: 0 matches (was changed)

No match should appear in a "default must be 0.10" assertion context.

---

## Edge Cases

- `make_weight_config()` is called in per-field rejection tests (6 call sites). After `w_coac: 0.0`, these tests:
  - Mutate one field to out-of-range value
  - Call `config.validate()`
  - Assert specific error message for the mutated field
  - None reference `w_coac` value — all pass naturally.
