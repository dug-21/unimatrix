# Test Plan: InferenceConfig (crt-024)

**File under test**: `crates/unimatrix-server/src/infra/config.rs`
**Test location**: `#[cfg(test)] mod tests` block in the same file (existing pattern)
**Risks addressed**: R-12, R-13; AC-01, AC-02, AC-03, AC-12

---

## Component Summary

`InferenceConfig` gains six new `f64` weight fields (`w_sim`, `w_nli`, `w_conf`, `w_coac`,
`w_util`, `w_prov`) and extended validation in `validate()`. Tests follow the existing
`NliFieldOutOfRange`-style pattern: structured errors, named fields, no panics.

---

## Unit Test Expectations

### AC-01: Default Deserialization (R-13)

**Test name**: `test_inference_config_weight_defaults_when_absent`

Parse a `UnimatrixConfig` TOML string with no weight fields in `[inference]`. Assert:
- `config.inference.w_sim == 0.25`
- `config.inference.w_nli == 0.35`
- `config.inference.w_conf == 0.15`
- `config.inference.w_coac == 0.10`
- `config.inference.w_util == 0.05`
- `config.inference.w_prov == 0.05`
- `w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 0.95`

```rust
let toml = r#"[inference]
nli_enabled = false
"#;
let config: UnimatrixConfig = toml::from_str(toml).expect("must parse");
assert!((config.inference.w_sim - 0.25).abs() < 1e-9);
// ... assert all six; assert sum <= 0.95
```

**Rationale**: R-13 — operators who do not add the new fields must get defaults that sum ≤ 0.95.
Any missing `#[serde(default)]` causes deserialization failure, caught here.

---

### AC-01b: Default Sum ≤ 0.95 (R-13, R-06)

**Test name**: `test_inference_config_default_weights_sum_within_headroom`

Construct `InferenceConfig::default()`. Assert:
- `sum = w_sim + w_nli + w_conf + w_coac + w_util + w_prov`
- `sum <= 0.95` with f64 epsilon tolerance
- `sum > 0.0` (non-zero defaults produce non-zero scoring)

**Rationale**: ADR-003 numerical verification — default weights are W3-1's training initialization.
A defaults typo that changes the sum corrupts W3-1's cold-start baseline (R-06).

---

### AC-02: Weight Sum Rejection (R-12)

**Test name**: `test_inference_config_validate_rejects_sum_exceeding_one`

Construct `InferenceConfig` with `w_sim=0.5, w_nli=0.4, w_conf=0.15, w_coac=0.0, w_util=0.0, w_prov=0.0`
(sum = 1.05). Call `validate()`. Assert:
- Returns `Err`
- Error message contains all six field names (`w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`)
- Error message contains the computed sum (1.05 or close)

```rust
let mut cfg = InferenceConfig::default();
cfg.w_sim = 0.5;
cfg.w_nli = 0.4;
cfg.w_conf = 0.15;
let err = cfg.validate(Path::new("/tmp/c.toml")).expect_err("must reject sum > 1.0");
let msg = err.to_string();
assert!(msg.contains("w_sim"), "must name w_sim");
assert!(msg.contains("w_nli"), "must name w_nli");
// ... assert all six; assert sum value present
```

**Rationale**: AC-02 — operator must see exactly which fields to reduce. Unnamed errors require
guessing. The error variant must be `FusionWeightSumExceeded` (new variant).

---

### AC-02b: Weight Sum Exactly 1.0 is Valid (EC-02)

**Test name**: `test_inference_config_validate_accepts_sum_exactly_one`

Construct weights that sum to exactly 1.0 (e.g., `w_sim=0.30, w_nli=0.35, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05`).
Assert `validate()` returns `Ok(())`.

**Rationale**: EC-02 — sum=1.0 is a valid operator choice. Validation must not reject it.

---

### AC-03: Individual Field Range Rejection (R-12)

**12 tests** (one per field per boundary), following the `NliFieldOutOfRange` pattern.

**Test names**: `test_inference_config_validate_rejects_w_{field}_below_zero` and
`test_inference_config_validate_rejects_w_{field}_above_one` for each of the six fields.

For each field, two tests:
1. Set field to `-0.01`. Assert `validate()` returns `Err` with message naming the offending field.
2. Set field to `1.01`. Assert `validate()` returns `Err` with message naming the offending field.

```rust
fn make_weight_config() -> InferenceConfig {
    InferenceConfig { w_sim: 0.25, w_nli: 0.35, w_conf: 0.15,
                      w_coac: 0.10, w_util: 0.05, w_prov: 0.05, ..Default::default() }
}

#[test]
fn test_inference_config_validate_rejects_w_sim_below_zero() {
    let mut cfg = make_weight_config();
    cfg.w_sim = -0.01;
    let err = cfg.validate(Path::new("/tmp/c.toml")).expect_err("must reject negative w_sim");
    assert!(err.to_string().contains("w_sim"), "error must name w_sim");
}
```

**Rationale**: AC-03 — per-field range checks must use the same structured error pattern as
existing NLI validation (path, field, value, reason). Twelve tests ensure no field's check is
accidentally omitted.

---

### AC-12: Validation Pattern Consistency (R-12)

**Test name**: `test_inference_config_validate_uses_result_not_panic`

Construct an `InferenceConfig` with multiple invalid fields. Call `validate()` without
`.unwrap()`. Assert it returns `Err`, not panics. Assert the error is `ConfigError`-typed.

**Rationale**: AC-12 — `validate()` must return `Result`, not call `unwrap()`/`expect()` internally.
The compile-time signature enforces `Result` return; this test enforces no panic path under invalid input.

---

### R-13: Partial Config File (Backward Compatibility)

**Test name**: `test_inference_config_partial_toml_gets_defaults_not_error`

Parse a TOML string that sets only some of the six weight fields (e.g., only `w_nli = 0.40`).
Assert:
- Parse succeeds (no deserialization error)
- Set field (`w_nli`) equals the configured value (0.40)
- Unset fields equal their defaults (e.g., `w_sim == 0.25`)
- The total sum is consistent (0.40 + 0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 1.00 ≤ 1.0)

**Rationale**: R-13 — operators upgrade gradually. A config setting only two or three of the six
new fields must not fail to parse.

---

## Edge Cases

### EC-01: All Weights Zero (Valid Config)

**Test name**: `test_inference_config_validate_accepts_all_zeros`

All six weights = 0.0. Assert `validate()` returns `Ok(())`.
Note: scoring will produce all-zero fused scores — not an error, just degenerate.

### EC-02: Weight Sum Boundary (Already Covered Above)

sum = 1.0 → `Ok`. sum = 1.0 + epsilon → `Err`.

---

## Assertions Style Reference

Follow the config.rs test pattern:
- Parse via `toml::from_str::<UnimatrixConfig>(toml_str).expect("...")`
- Validate via `validate_config(&config, Path::new("/tmp/config.toml"))`
- Match error variant explicitly with `match &err { ConfigError::FusionWeightSumExceeded { ... } => { ... } other => panic!(...) }`
- Assert `.to_string()` on the error contains field names and values (operator diagnostics)
