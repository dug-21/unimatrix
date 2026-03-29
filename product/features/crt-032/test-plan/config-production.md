# Test Plan: config-production

## Component: `src/infra/config.rs` — Production Code Sites

### Risks Covered

- R-01 (Critical): Both default definition sites updated
- R-04 (Medium): Doc comments updated

---

## Unit Test Expectations

### Test: `test_inference_config_weight_defaults_when_absent` — MUST CHANGE

**Risk**: R-01, R-02
**What it tests**: Deserialize an empty `[inference]` TOML block; verify `default_w_coac()` is called and returns `0.0`.

**Before**:
```rust
assert!((inf.w_coac - 0.10).abs() < 1e-9, "w_coac default must be 0.10");
```

**After**:
```rust
assert!(inf.w_coac.abs() < 1e-9, "w_coac default must be 0.0");
```

**Pass condition**: `inf.w_coac` is `0.0` (within 1e-9 tolerance).
**Sum assertion**: `<= 0.95 + 1e-9` — passes naturally at 0.85. No change needed.

---

### Test: `test_inference_config_default_weights_sum_within_headroom` — PASSES NATURALLY

**Risk**: R-01 (Default::default() path)
**What it tests**: `InferenceConfig::default()` sum invariant.

**Expected result**: `sum = 0.85 ≤ 0.95`. Passes without change.
**Pass condition**: Test continues passing after `w_coac: 0.0` in struct literal.

---

### Doc Comment Verification (R-04)

After delivery, grep assertions:

| Check | Pattern | Expected |
|-------|---------|---------|
| w_coac field doc | `Default: 0\.10` in config.rs near `w_coac` field | Zero matches |
| w_prov field doc | `Defaults sum to 0\.95` in config.rs | Zero matches |
| w_phase_explicit doc | `0\.95 \+ 0\.02 \+ 0\.05 = 1\.02` in config.rs | Zero matches |
| w_coac field doc new | `Default: 0\.0` near `w_coac` field | Present |
| w_prov field doc new | `Defaults sum to 0\.85` | Present |
| w_phase_explicit doc new | `0\.85 \+ 0\.02 \+ 0\.05 = 0\.92` | Present |

---

## Edge Cases

- `InferenceConfig` constructed both via TOML deserialization AND via `Default::default()` — both must yield `w_coac == 0.0` (covered by two separate tests)
- `w_coac: 0.0` passes `validate()` — verified by `test_inference_config_validate_accepts_all_zeros` (no change needed)
- Partial TOML with only `w_nli = 0.40` → remaining fields use defaults → `w_coac == 0.0` → sum = 0.90 ≤ 1.0 (covered by partial-TOML test)
