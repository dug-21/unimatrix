# Pseudocode: config-production

## Component: `src/infra/config.rs` — Production Code Sites

### Purpose

Update the two compiled-default definition sites for `w_coac` from `0.10` to `0.0`,
and update three inline doc comments that encode the old sum figure.

### Invariants to Preserve

- `w_coac` field definition (`pub w_coac: f64`) — unchanged
- `#[serde(default = "default_w_coac")]` attribute — unchanged
- `validate()` range check on `w_coac` — unchanged
- `CO_ACCESS_STALENESS_SECONDS` constant — unchanged

---

## Site 1: `default_w_coac()` function (approx. line 621–623)

### Before

```
fn default_w_coac() -> f64 {
    0.10
}
```

### After

```
fn default_w_coac() -> f64 {
    0.0
}
```

**Invariant**: No other change to this function. Return type `f64` unchanged.

---

## Site 2: `InferenceConfig::default()` struct literal (approx. line 549)

### Before

```rust
InferenceConfig {
    // ... other fields ...
    w_coac: 0.10,
    // ... other fields ...
}
```

### After

```rust
InferenceConfig {
    // ... other fields ...
    w_coac: 0.0,
    // ... other fields ...
}
```

**Invariant**: Only `w_coac` value changes. All other field values in the struct literal unchanged.

---

## Site 3: Field doc comment on `w_coac` field (approx. line 358)

### Before

```
/// ... Default: 0.10 ...
pub w_coac: f64,
```

### After

```
/// ... Default: 0.0 ...
pub w_coac: f64,
```

**Invariant**: Only the value in the comment changes. Field definition `pub w_coac: f64` unchanged.

---

## Site 4: `w_prov` field doc comment (approx. line 367)

### Before

```
/// ... Defaults sum to 0.95 ...
```

### After

```
/// ... Defaults sum to 0.85 ...
```

---

## Site 5: `w_phase_explicit` field doc comment (approx. line 381)

### Before

```
/// ... Total weight sum with defaults: 0.95 + 0.02 + 0.05 = 1.02 ...
```

### After

```
/// ... Total weight sum with defaults: 0.85 + 0.02 + 0.05 = 0.92 ...
```

---

## Error Handling

No error handling needed. These are value/comment changes only.

## Key Test Scenarios

- Deserialize empty `[inference]` TOML block → assert `w_coac == 0.0` (exercises `default_w_coac()`)
- Construct `InferenceConfig::default()` → assert `w_coac == 0.0` (exercises struct literal)
- Both assertions must pass in the same test run (R-01 coverage)
