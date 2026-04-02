# Component: Config Default Weight Constants

**Wave**: 1 (parallel with effective-short-circuit)
**File**: `crates/unimatrix-server/src/infra/config.rs`
**ACs**: AC-01
**Risks**: R-09 (Med)

---

## Purpose

Change six `default_w_*()` backing functions and `default_nli_enabled()` to produce the
conf-boost-c profile. These are Serde default value functions — they are called when
`InferenceConfig` is deserialized without a config file (i.e., the production deployment).
No config file field names, types, or validation logic changes.

---

## Modified Functions

All seven functions are in `config.rs` in the block labeled
"Fusion weight default value functions (crt-024, ADR-003)" (lines 665–703)
and the NLI defaults block (lines 635–663).

### default_nli_enabled() — line 637

```
// Before:
fn default_nli_enabled() -> bool {
    false   // already false in current codebase
}

// No change needed — current value already matches target.
// Verify: confirm line 638 returns false before marking AC-01 complete.
```

Note: The IMPLEMENTATION-BRIEF.md and ARCHITECTURE.md list `default_nli_enabled` as
changing from `true` to `false`. The actual source at line 637-639 already returns
`false`. Delivery must verify the current value before treating this as a change.
If the live value is `true`, change it to `false`. If already `false`, no edit needed
but the test update still applies.

### default_w_sim() — line 669

```
// Before:
fn default_w_sim() -> f64 {
    0.25
}

// After:
fn default_w_sim() -> f64 {
    0.50
}
```

### default_w_nli() — line 673

```
// Before:
fn default_w_nli() -> f64 {
    0.35
}

// After:
fn default_w_nli() -> f64 {
    0.00
}
```

### default_w_conf() — line 677

```
// Before:
fn default_w_conf() -> f64 {
    0.15
}

// After:
fn default_w_conf() -> f64 {
    0.35
}
```

### default_w_util() — line 685

```
// Before:
fn default_w_util() -> f64 {
    0.05
}

// After:
fn default_w_util() -> f64 {
    0.00
}
```

### default_w_prov() — line 689

```
// Before:
fn default_w_prov() -> f64 {
    0.05
}

// After:
fn default_w_prov() -> f64 {
    0.00
}
```

### default_w_coac() — line 681 (unchanged)

```
// Retained unchanged at 0.0:
fn default_w_coac() -> f64 {
    0.0
}
```

### default_w_phase_histogram() and default_w_phase_explicit() (unchanged)

These are additive terms outside the six-weight sum constraint. Both are unchanged:
`default_w_phase_histogram()` returns `0.02`, `default_w_phase_explicit()` returns `0.05`.

---

## Weight Sum Verification

After the changes:
```
Six-weight sum: 0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 = 0.85
Total with additive terms: 0.85 + 0.02 + 0.05 = 0.92
InferenceConfig::validate() requires sum <= 1.0 => 0.85 <= 1.0 PASSES
test_inference_config_default_weights_sum_within_headroom asserts sum <= 0.95 => PASSES
```

---

## Data Flow

```
No config file present (production deployment):
  serde calls default_w_sim()  => 0.50
  serde calls default_w_nli()  => 0.00
  serde calls default_w_conf() => 0.35
  serde calls default_w_util() => 0.00
  serde calls default_w_prov() => 0.00
  serde calls default_nli_enabled() => false
       |
       v
  InferenceConfig { w_sim=0.50, w_nli=0.00, w_conf=0.35, ... }
       |
       v
  FusionWeights::from_config(&inference_config)
       |
       v
  FusionWeights { w_sim=0.50, w_nli=0.00, w_conf=0.35, ... }

Config file present with explicit w_sim override (operator):
  serde reads w_sim from TOML => operator value (0.50 default not applied)
  All other absent fields use defaults as above
```

No code paths other than `default_*()` functions change. No struct field additions,
removals, or renames. The `#[serde(default = "default_w_sim")]` attribute on
`InferenceConfig.w_sim` (and each other field) is unchanged.

---

## Updated Tests

### test_inference_config_weight_defaults_when_absent

**Action**: Update expected assertion values.

```
// Before — asserts old defaults:
assert_eq!(config.w_sim,  0.25);
assert_eq!(config.w_nli,  0.35);
assert_eq!(config.w_conf, 0.15);
assert_eq!(config.w_util, 0.05);
assert_eq!(config.w_prov, 0.05);
assert_eq!(config.nli_enabled, true);

// After — asserts new defaults:
assert_eq!(config.w_sim,  0.50);
assert_eq!(config.w_nli,  0.00);
assert_eq!(config.w_conf, 0.35);
assert_eq!(config.w_util, 0.00);
assert_eq!(config.w_prov, 0.00);
assert_eq!(config.nli_enabled, false);
```

The test construction (empty TOML / no-file deserialization) is unchanged. Only the
expected values in the assertions change.

### test_inference_config_default_weights_sum_within_headroom

**Action**: Verify or update.

```
// Current test likely asserts:
let sum = config.w_sim + config.w_nli + config.w_conf + config.w_coac
          + config.w_util + config.w_prov;
assert!(sum <= 0.95, "sum {} exceeds headroom", sum);

// With new defaults: sum = 0.50 + 0.00 + 0.35 + 0.00 + 0.00 + 0.00 = 0.85
// 0.85 <= 0.95 => assertion still passes.
// Update ONLY if the test asserts an exact old value (e.g., == 0.85).
// If it only asserts <= 0.95 or similar headroom check, no change needed.
```

Delivery must read the current assertion body and confirm whether an exact-value
assertion exists before deciding whether to edit this test.

---

## Error Handling

These are pure value-returning functions; no error handling. The six-weight sum
invariant is enforced at startup by `InferenceConfig::validate()` (separate function,
unchanged). No changes to `validate()` are required — the new sum (0.85) satisfies the
existing `sum <= 1.0` constraint.

---

## Key Test Scenarios Summary

| Scenario | Input | Expected |
|----------|-------|----------|
| No config file | serde default | w_sim=0.50, w_nli=0.00, w_conf=0.35, w_util=0.00, w_prov=0.00, nli_enabled=false |
| Config file with w_sim=0.30 | TOML override | w_sim=0.30, others still default |
| validate() passes | default sum | 0.85 <= 1.0 |
| headroom test | default sum | 0.85 <= 0.95 |
| Total with additive terms | all defaults | 0.50+0.35+0.02+0.05 = 0.92 |
