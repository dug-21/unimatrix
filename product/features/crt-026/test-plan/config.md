# crt-026: Test Plan — Component 6: InferenceConfig — New Weight Fields

**File under test**: `crates/unimatrix-server/src/infra/config.rs`
**Test module**: `#[cfg(test)] mod tests` at bottom of `config.rs` (or integrated with
existing config tests in the same file)

---

## AC Coverage

| AC-ID | Test |
|-------|------|
| AC-09 | `test_phase_explicit_norm_placeholder_fields_present` (shared with fused-score.md) |
| AC-09 | `test_inference_config_default_phase_weights` |
| AC-09 | `test_config_validation_rejects_out_of_range_phase_weights` |

Risk coverage: R-11 (range validation), R-07 (placeholder fields), R-12 (struct defaults).

---

## Scope

Component 6 adds two fields to `InferenceConfig` using the existing `default_w_*` serde
pattern, and adds per-field `[0.0, 1.0]` range checks to `InferenceConfig::validate()`.
Tests here verify:

1. Default values are correct (`w_phase_explicit=0.0, w_phase_histogram=0.02`)
2. Fields are serializable/deserializable (serde round-trip)
3. `validate()` accepts valid values and rejects out-of-range values
4. The existing six-weight sum check is NOT modified

---

## Tests

### T-CFG-01: `test_inference_config_default_phase_weights`
**AC-09 | R-07, R-11**

**Arrange/Act**:
```rust
let cfg = InferenceConfig::default();
```

**Assert**:
```rust
assert_eq!(
    cfg.w_phase_histogram, 0.02,
    "w_phase_histogram default must be 0.02 (ASS-028 calibrated value, ADR-004)"
);
assert_eq!(
    cfg.w_phase_explicit, 0.0,
    "w_phase_explicit default must be 0.0 (W3-1 placeholder, ADR-003)"
);
```

**Module**: `infra/config.rs` `#[cfg(test)] mod tests`

---

### T-CFG-02: `test_config_validation_rejects_out_of_range_phase_weights`
**R-11**

**Arrange**:
```rust
let mut cfg = InferenceConfig::default();
```

**Act/Assert — w_phase_histogram too high**:
```rust
cfg.w_phase_histogram = 1.5;
let result = cfg.validate();
assert!(
    result.is_err(),
    "w_phase_histogram = 1.5 must fail validate() (above [0.0, 1.0] range)"
);
```

**Act/Assert — w_phase_explicit negative**:
```rust
cfg = InferenceConfig::default();
cfg.w_phase_explicit = -0.1;
let result = cfg.validate();
assert!(
    result.is_err(),
    "w_phase_explicit = -0.1 must fail validate() (below 0.0)"
);
```

**Act/Assert — valid boundary values pass**:
```rust
cfg = InferenceConfig::default();
cfg.w_phase_histogram = 0.0;
cfg.w_phase_explicit = 0.0;
assert!(cfg.validate().is_ok(),
    "w_phase_histogram=0.0, w_phase_explicit=0.0 must pass validate()");

cfg.w_phase_histogram = 1.0;
cfg.w_phase_explicit = 1.0;
assert!(cfg.validate().is_ok(),
    "w_phase_histogram=1.0, w_phase_explicit=1.0 must pass validate()");
```

**Act/Assert — default values pass**:
```rust
let default_cfg = InferenceConfig::default();
assert!(default_cfg.validate().is_ok(),
    "default InferenceConfig (w_phase_histogram=0.02, w_phase_explicit=0.0) \
     must pass validate()");
```

**Module**: `infra/config.rs` `#[cfg(test)] mod tests`

---

### T-CFG-03: `test_inference_config_six_weight_sum_unchanged_by_phase_fields`
**R-11 (sum-invariant), ADR-004**

**Arrange/Act**:
```rust
let cfg = InferenceConfig::default();
let six_weight_sum = cfg.w_sim + cfg.w_nli + cfg.w_conf
                   + cfg.w_coac + cfg.w_util + cfg.w_prov;
let total_with_phase = six_weight_sum + cfg.w_phase_histogram + cfg.w_phase_explicit;
```

**Assert**:
```rust
assert!(
    (six_weight_sum - 0.95).abs() < f64::EPSILON,
    "sum of six original weights must still be 0.95; got {six_weight_sum}"
);
assert!(
    (total_with_phase - 0.97).abs() < f64::EPSILON,
    "total including phase weights must be 0.97; got {total_with_phase}"
);
// Verify the six-weight sum check in validate() does NOT include phase fields
// (ADR-004: phase fields are additive, outside the <= 1.0 constraint)
assert!(cfg.validate().is_ok(),
    "default config with sum=0.97 must pass validate() (six-weight check uses only original six)");
```

**Module**: `infra/config.rs` `#[cfg(test)] mod tests`

---

### T-CFG-04: `test_inference_config_serde_round_trip_phase_fields`
**AC-09 (serde pattern)**

**Arrange**:
```rust
// Simulate TOML deserialization: config with explicit phase weight values
let toml_str = r#"
[inference]
w_phase_histogram = 0.03
w_phase_explicit = 0.0
"#;
```

**Act**:
```rust
#[derive(serde::Deserialize)]
struct TestConfig {
    #[serde(default)]
    inference: InferenceConfig,
}
let config: TestConfig = toml::from_str(toml_str).expect("valid TOML");
```

**Assert**:
```rust
assert!(
    (config.inference.w_phase_histogram - 0.03).abs() < f64::EPSILON,
    "w_phase_histogram must deserialize from TOML; got {}",
    config.inference.w_phase_histogram
);
```

**Module**: `infra/config.rs` `#[cfg(test)] mod tests`

**Notes**: The `#[serde(default = "default_w_phase_histogram")]` attribute pattern
requires `toml` or `serde_json` in dev-dependencies for this test. If the existing
config tests already exercise serde deserialization, extend those tests rather than
creating a new round-trip test.

---

### T-CFG-05: `test_inference_config_missing_phase_fields_use_defaults`
**AC-09 | FM-04 (backward compatibility)**

**Arrange**:
```rust
// Existing config file without new fields — omit w_phase_* keys entirely
let toml_str = r#"
[inference]
w_sim = 0.25
"#;
```

**Act**:
```rust
let config: TestConfig = toml::from_str(toml_str).expect("should not fail with missing fields");
```

**Assert**:
```rust
assert_eq!(config.inference.w_phase_histogram, 0.02,
    "missing w_phase_histogram must default to 0.02");
assert_eq!(config.inference.w_phase_explicit, 0.0,
    "missing w_phase_explicit must default to 0.0");
```

**Notes**: Validates `#[serde(default)]` backward compatibility. Operators upgrading
without modifying their config file must not experience a startup failure (FM-04).

**Module**: `infra/config.rs` `#[cfg(test)] mod tests`

---

## Code-Review Assertions

**R-12 (struct literal construction)**: `InferenceConfig::default()` struct literal in
`Default::default()` must include `w_phase_explicit: 0.0` and `w_phase_histogram: 0.02`
explicitly (or use `..Self::default()` if the `serde(default)` functions handle it). Verify
by reading the `Default::default()` implementation after Stage 3b.

**R-07 (ADR-003 comment)**: The `w_phase_explicit` field in `InferenceConfig` must have a
doc-comment or inline comment citing ADR-003 and noting this is a W3-1 placeholder. This
prevents future removal as "unused config key."
