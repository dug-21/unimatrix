# Pseudocode: config-tests

## Component: `src/infra/config.rs` — Test Code Sites

### Purpose

Update the test helper and one default-assertion test to reflect the new default of `0.0`.
Update one inline comment that references the old sum figure.

### Invariants to Preserve

- `test_inference_config_validate_accepts_sum_exactly_one` — must NOT change (intentional fixture with `w_coac: 0.10`)
- All per-field rejection tests that use `make_weight_config()` — pass naturally; no changes needed
- Sum assertion `<= 0.95 + 1e-9` in `test_inference_config_default_weights_sum_within_headroom` — passes at 0.85; no change needed

---

## Site 1: `make_weight_config()` helper (approx. line 4729)

### Before

```rust
fn make_weight_config() -> InferenceConfig {
    InferenceConfig {
        // ...
        w_coac: 0.10,
        // ...
    }
}
```

### After

```rust
fn make_weight_config() -> InferenceConfig {
    InferenceConfig {
        // ...
        w_coac: 0.0,
        // ...
    }
}
```

**Invariant**: Only `w_coac` changes. All other fields in this helper unchanged.

---

## Site 2: `test_inference_config_weight_defaults_when_absent` assertion (approx. line 4754–4756)

### Before

```rust
assert!(
    (inf.w_coac - 0.10).abs() < 1e-9,
    "w_coac default must be 0.10"
);
```

### After

```rust
assert!(
    inf.w_coac.abs() < 1e-9,
    "w_coac default must be 0.0"
);
```

**Note**: `inf.w_coac.abs() < 1e-9` is equivalent to `(inf.w_coac - 0.0).abs() < 1e-9`.
Using `.abs()` directly is the idiomatic form for asserting a value equals zero.

**Invariant**: The sum assertion in the same test (`<= 0.95 + 1e-9`) does NOT change — 0.85 ≤ 0.95 passes naturally.

---

## Site 3: Inline comment in `test_inference_config_partial_toml_gets_defaults_not_error` (approx. line 4883)

### Before

```rust
// Total sum: 0.40 + 0.25 + 0.15 + 0.10 + 0.05 + 0.05 = 1.00
```

### After

```rust
// Total sum: 0.40 + 0.25 + 0.15 + 0.00 + 0.05 + 0.05 = 0.90
```

**Invariant**: The test assertion itself (`<= 1.0 + 1e-9`) does NOT change — it still passes at 0.90.

---

## Error Handling

No error handling needed. These are value/comment changes in tests only.

## Key Test Scenarios

- After change: grep config.rs for `w_coac.*0\.10` in default-assertion contexts → zero matches (R-02)
- `test_inference_config_validate_accepts_sum_exactly_one` still contains `w_coac: 0.10` → count unchanged (R-03)
- `make_weight_config()` body has `w_coac: 0.0` (AC-15)
