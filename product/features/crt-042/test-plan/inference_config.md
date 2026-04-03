# crt-042: Test Plan — `InferenceConfig` Additions

## Component Scope

Component 3 of 4. Three new fields added to `InferenceConfig` in
`crates/unimatrix-server/src/infra/config.rs`:

| Field | Type | Default | Valid Range |
|-------|------|---------|-------------|
| `ppr_expander_enabled` | `bool` | `false` | n/a |
| `expansion_depth` | `usize` | `2` | [1, 10] |
| `max_expansion_candidates` | `usize` | `200` | [1, 1000] |

All tests live in the existing `mod tests` block inside `config.rs`.
Follow the exact pattern of the existing PPR field tests (lines ~6161–6313 in `config.rs`).

**Critical risk**: R-08 (High Likelihood) — InferenceConfig hidden test sites. This is
historically the most likely failure mode (entries #4044, #2730, #3817). The grep check
below is non-negotiable.

---

## R-08: Hidden Test Sites — Mandatory Grep Check

**Risk covered**: R-08 (High)

Before Stage 3c can pass, the tester MUST execute:

```bash
grep -rn 'InferenceConfig {' crates/unimatrix-server/src/ --include='*.rs'
```

For every match found, verify one of:
1. All three new fields (`ppr_expander_enabled`, `expansion_depth`,
   `max_expansion_candidates`) are explicitly listed in the struct literal, OR
2. The literal ends with `..InferenceConfig::default()` or `..Default::default()`.

**Known sites to check** (from entry #4044 pattern — these are the sites that have hidden
existing literals):
- `config.rs` `impl Default for InferenceConfig` block (struct literal)
- `config.rs` `InferenceConfig::merged()` function (struct literal in return position)
- Every test that constructs `InferenceConfig { ... }` literally
- `assert_validate_fails_with_field` helper (uses `InferenceConfig { field: val, ..InferenceConfig::default() }`)
- Any `toml::from_str` test that asserts specific field values (may need a new assertion for the three new fields)

If any literal site is found that is missing the new fields and does not use spread syntax,
return to the implementation agent with: `R-08 FAIL: InferenceConfig { literal at {file}:{line}
missing expansion_depth/max_expansion_candidates/ppr_expander_enabled`.

---

## AC-17: Missing Fields Load Defaults

**Risk covered**: R-08 (partial)

Two variants: one using `InferenceConfig::default()`, one using TOML deserialization.

```rust
// test_inference_config_expander_fields_defaults
// Assert InferenceConfig::default() returns correct values for all three new fields.
#[test]
fn test_inference_config_expander_fields_defaults() {
    let cfg = InferenceConfig::default();
    assert_eq!(cfg.ppr_expander_enabled, false,
        "ppr_expander_enabled default must be false");
    assert_eq!(cfg.expansion_depth, 2,
        "expansion_depth default must be 2");
    assert_eq!(cfg.max_expansion_candidates, 200,
        "max_expansion_candidates default must be 200");
}

// test_inference_config_expander_fields_serde_defaults
// Assert that a TOML that omits all three fields produces the documented defaults.
// Pattern: InferenceConfig is deserializable directly from flat TOML (no [inference] header).
#[test]
fn test_inference_config_expander_fields_serde_defaults() {
    // Empty TOML — all fields absent.
    let cfg: InferenceConfig = toml::from_str("").unwrap();
    assert_eq!(cfg.ppr_expander_enabled, false,
        "absent ppr_expander_enabled must default to false via #[serde(default)]");
    assert_eq!(cfg.expansion_depth, 2,
        "absent expansion_depth must default to 2 via #[serde(default)]");
    assert_eq!(cfg.max_expansion_candidates, 200,
        "absent max_expansion_candidates must default to 200 via #[serde(default)]");
}

// test_unimatrix_config_expander_toml_omitted_produces_defaults
// Parse a full UnimatrixConfig from TOML that has [inference] but no expander fields.
// Pattern mirrors test_inference_config_ppr_defaults_when_absent.
#[test]
fn test_unimatrix_config_expander_toml_omitted_produces_defaults() {
    let toml_str = "[inference]\n";
    let config: UnimatrixConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.inference.ppr_expander_enabled, false);
    assert_eq!(config.inference.expansion_depth, 2);
    assert_eq!(config.inference.max_expansion_candidates, 200);
}
```

**Dual-site invariant check** (entry #3817): assert that serde default function values
match `Default::default()` values. The above tests implicitly verify this; additionally:

```rust
// test_inference_config_expander_serde_fn_matches_default
// Calls both serde deserialization and Default::default(); compares all three fields.
#[test]
fn test_inference_config_expander_serde_fn_matches_default() {
    let from_empty: InferenceConfig = toml::from_str("").unwrap();
    let from_default = InferenceConfig::default();
    // All three new fields must match between serde and Default paths.
    assert_eq!(from_empty.ppr_expander_enabled, from_default.ppr_expander_enabled,
        "ppr_expander_enabled: serde default fn must match Default::default()");
    assert_eq!(from_empty.expansion_depth, from_default.expansion_depth,
        "expansion_depth: serde default fn must match Default::default()");
    assert_eq!(from_empty.max_expansion_candidates, from_default.max_expansion_candidates,
        "max_expansion_candidates: serde default fn must match Default::default()");
}
```

---

## AC-18: expansion_depth = 0 Fails Validation

**Risk covered**: R-14 (Med), ADR-004

```rust
// test_validate_expansion_depth_zero_fails
// Pattern mirrors test_ppr_iterations_zero_rejected exactly.
#[test]
fn test_validate_expansion_depth_zero_fails() {
    let c = InferenceConfig {
        expansion_depth: 0,
        ..InferenceConfig::default()
    };
    assert_validate_fails_with_field(c, "expansion_depth");
}
```

**Critical**: `ppr_expander_enabled` is explicitly set to `false` via `..InferenceConfig::default()`
(default is false). This test must pass with the flag OFF — it is the direct proof of ADR-004
(unconditional validation, not conditional on the flag). If this test were to pass with
`expansion_depth = 0` when `ppr_expander_enabled = false`, it would repeat the NLI
conditional-validation trap documented in entry #3817.

---

## AC-19: expansion_depth = 11 Fails Validation

**Risk covered**: R-14 (Med), ADR-004

```rust
// test_validate_expansion_depth_eleven_fails
#[test]
fn test_validate_expansion_depth_eleven_fails() {
    let c = InferenceConfig {
        expansion_depth: 11,
        ..InferenceConfig::default()
    };
    assert_validate_fails_with_field(c, "expansion_depth");
}
```

**Boundary complement**: also assert that depth=10 is accepted:

```rust
// test_validate_expansion_depth_ten_passes
#[test]
fn test_validate_expansion_depth_ten_passes() {
    let c = InferenceConfig {
        expansion_depth: 10,
        ..InferenceConfig::default()
    };
    assert!(c.validate(Path::new("/fake")).is_ok(),
        "expansion_depth=10 (upper bound) must pass validation");
}

// test_validate_expansion_depth_one_passes
#[test]
fn test_validate_expansion_depth_one_passes() {
    let c = InferenceConfig {
        expansion_depth: 1,
        ..InferenceConfig::default()
    };
    assert!(c.validate(Path::new("/fake")).is_ok(),
        "expansion_depth=1 (lower bound) must pass validation");
}
```

---

## AC-20: max_expansion_candidates = 0 Fails Validation

**Risk covered**: R-14 (Med), ADR-004

```rust
// test_validate_max_expansion_candidates_zero_fails
#[test]
fn test_validate_max_expansion_candidates_zero_fails() {
    let c = InferenceConfig {
        max_expansion_candidates: 0,
        ..InferenceConfig::default()
    };
    assert_validate_fails_with_field(c, "max_expansion_candidates");
}
```

Again: `ppr_expander_enabled` is `false` via spread. This validates unconditional enforcement.

---

## AC-21: max_expansion_candidates = 1001 Fails Validation

**Risk covered**: R-14 (Med), ADR-004

```rust
// test_validate_max_expansion_candidates_1001_fails
#[test]
fn test_validate_max_expansion_candidates_1001_fails() {
    let c = InferenceConfig {
        max_expansion_candidates: 1001,
        ..InferenceConfig::default()
    };
    assert_validate_fails_with_field(c, "max_expansion_candidates");
}
```

**Boundary complement**: also assert boundaries are accepted:

```rust
// test_validate_max_expansion_candidates_one_passes
#[test]
fn test_validate_max_expansion_candidates_one_passes() {
    let c = InferenceConfig {
        max_expansion_candidates: 1,
        ..InferenceConfig::default()
    };
    assert!(c.validate(Path::new("/fake")).is_ok(),
        "max_expansion_candidates=1 (lower bound) must pass validation");
}

// test_validate_max_expansion_candidates_1000_passes
#[test]
fn test_validate_max_expansion_candidates_1000_passes() {
    let c = InferenceConfig {
        max_expansion_candidates: 1000,
        ..InferenceConfig::default()
    };
    assert!(c.validate(Path::new("/fake")).is_ok(),
        "max_expansion_candidates=1000 (upper bound) must pass validation");
}
```

---

## Error Message Quality Test

Following the existing `test_inference_config_error_message_names_field` pattern:

```rust
// test_validate_expansion_depth_error_names_field
// Assert that the validation error message includes the offending field name.
#[test]
fn test_validate_expansion_depth_error_names_field() {
    let c = InferenceConfig { expansion_depth: 0, ..InferenceConfig::default() };
    let err = c.validate(Path::new("/fake/config.toml")).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("expansion_depth"),
        "error message must name the offending field 'expansion_depth'; got: {msg}");
}

// test_validate_max_expansion_candidates_error_names_field
#[test]
fn test_validate_max_expansion_candidates_error_names_field() {
    let c = InferenceConfig { max_expansion_candidates: 0, ..InferenceConfig::default() };
    let err = c.validate(Path::new("/fake/config.toml")).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("max_expansion_candidates"),
        "error message must name the offending field; got: {msg}");
}
```

---

## Config Merge Test (InferenceConfig::merged)

**Risk covered**: R-08 (the merge function is a hidden site per entry #4044)

```rust
// test_inference_config_merged_propagates_expander_fields
// Assert that when project config overrides one expander field, the merge produces
// the project value for that field and the global value for the others.
// Pattern mirrors existing merge tests for ppr_alpha and ppr_max_expand.
#[test]
fn test_inference_config_merged_propagates_expander_fields() {
    let global = InferenceConfig {
        expansion_depth: 3,
        max_expansion_candidates: 100,
        ppr_expander_enabled: false,
        ..InferenceConfig::default()
    };
    let project = InferenceConfig {
        expansion_depth: 5,  // project overrides depth
        ..InferenceConfig::default()
    };
    let merged = InferenceConfig::merged(&global, &project);
    assert_eq!(merged.expansion_depth, 5,
        "project expansion_depth=5 must override global expansion_depth=3");
    assert_eq!(merged.max_expansion_candidates, 200,
        "project defaults must fall back to global: global=100, project=200(default), \
         but merge semantics: project != default → project wins. \
         If project == default, global wins. Verify merge pattern matches ppr_max_expand.");
    assert_eq!(merged.ppr_expander_enabled, false,
        "ppr_expander_enabled must propagate from project or global correctly");
}
```

**Note on merge semantics**: Consult the `ppr_max_expand` merge logic (around line 2503
in config.rs). The pattern is: if `project.inference.field != default.inference.field`,
use project value; else use global value. The new fields must follow this exactly.

---

## TOML Round-Trip Test

```rust
// test_inference_config_expander_toml_explicit_override
// Assert explicit override values are parsed correctly.
#[test]
fn test_inference_config_expander_toml_explicit_override() {
    let toml_str = "ppr_expander_enabled = true\n\
                    expansion_depth = 5\n\
                    max_expansion_candidates = 100\n";
    let cfg: InferenceConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.ppr_expander_enabled, true, "explicit ppr_expander_enabled");
    assert_eq!(cfg.expansion_depth, 5, "explicit expansion_depth");
    assert_eq!(cfg.max_expansion_candidates, 100, "explicit max_expansion_candidates");
}
```

---

## Test Count Summary

| Test Name | AC | Risk |
|-----------|-----|------|
| test_inference_config_expander_fields_defaults | AC-17 | R-08 |
| test_inference_config_expander_fields_serde_defaults | AC-17 | R-08 |
| test_unimatrix_config_expander_toml_omitted_produces_defaults | AC-17 | R-08 |
| test_inference_config_expander_serde_fn_matches_default | R-08 (dual-site) | R-08 |
| test_validate_expansion_depth_zero_fails | AC-18 | R-14 |
| test_validate_expansion_depth_eleven_fails | AC-19 | R-14 |
| test_validate_expansion_depth_ten_passes | AC-19 (boundary) | R-14 |
| test_validate_expansion_depth_one_passes | AC-18 (boundary) | R-14 |
| test_validate_max_expansion_candidates_zero_fails | AC-20 | R-14 |
| test_validate_max_expansion_candidates_1001_fails | AC-21 | R-14 |
| test_validate_max_expansion_candidates_one_passes | AC-20 (boundary) | R-14 |
| test_validate_max_expansion_candidates_1000_passes | AC-21 (boundary) | R-14 |
| test_validate_expansion_depth_error_names_field | R-08 (error msg) | R-08 |
| test_validate_max_expansion_candidates_error_names_field | R-08 (error msg) | R-08 |
| test_inference_config_merged_propagates_expander_fields | R-08 (merge site) | R-08 |
| test_inference_config_expander_toml_explicit_override | R-08 (TOML round-trip) | R-08 |
| Grep: `InferenceConfig {` scan (shell) | R-08 (hidden sites) | R-08 |

**Total**: 16 unit tests + 1 mandatory grep check for Component 3.

**Note**: AC-18/19 (validation at flag=false) are the most critical tests in this component.
They are the direct proof that the NLI conditional-validation trap (entry #3817) is not
repeated in crt-042.
