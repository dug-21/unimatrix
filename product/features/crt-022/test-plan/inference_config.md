# InferenceConfig — Unit Test Plan

**Component**: `crates/unimatrix-server/src/infra/config.rs` (modified — adds `InferenceConfig`)
**Risks addressed**: R-07
**AC addressed**: AC-09, AC-11 (tests 5–8)

Place tests in the existing `#[cfg(test)] mod tests` block in `config.rs`. Follow the
established pattern (see existing `config.rs` tests): construct the config struct directly,
call `validate()` or `InferenceConfig::validate()`, assert the result.

Note: `InferenceConfig::validate()` is a standalone method (not `validate_config`). It does
not require `ContentScanner::global()` warmup — it only checks the numeric range.

---

## §validate-unit-tests — Boundary Value Tests (AC-09, AC-11 tests 5–8)

### test_inference_config_valid_lower_bound (AC-11 #5)
```
// Arrange: InferenceConfig { rayon_pool_size: 1 }
// Act: config.validate()
// Assert: Ok(())
```
Lower bound is inclusive. A pool of 1 thread is valid (serialises all inference;
operators accept this on resource-constrained deployments, per ARCHITECTURE.md §pool-sizing).

### test_inference_config_valid_upper_bound (AC-11 #6)
```
// Arrange: InferenceConfig { rayon_pool_size: 64 }
// Act: config.validate()
// Assert: Ok(())
```
Upper bound is inclusive. Value 64 must not be rejected.

### test_inference_config_rejects_zero (AC-11 #7)
```
// Arrange: InferenceConfig { rayon_pool_size: 0 }
// Act: config.validate()
// Assert: Err(e) where e is a ConfigError variant naming rayon_pool_size
// Assert: error message contains "rayon_pool_size" and "0"
// Assert: error message states valid range or minimum value
```
A value of 0 causes `ThreadPoolBuilder` undefined behaviour. Must abort startup
with a structured error (AC-09, R-07).

### test_inference_config_rejects_sixty_five (AC-11 #8)
```
// Arrange: InferenceConfig { rayon_pool_size: 65 }
// Act: config.validate()
// Assert: Err(e) where e is a ConfigError variant
// Assert: error message contains "65" or "64" (upper bound)
```
Values above 64 create an oversized pool consuming excessive CPU without operator
awareness. Must abort startup with a structured error.

---

## §boundary-extras — Additional Boundary Values (R-07)

### test_inference_config_valid_eight (R-07 scenario 3 — mid-range value)
```
// Arrange: InferenceConfig { rayon_pool_size: 8 }
// Act: config.validate()
// Assert: Ok(())
```
Mid-range value that matches the formula ceiling `max(4).min(8)`.

### test_inference_config_valid_four (R-07 — floor value)
```
// Arrange: InferenceConfig { rayon_pool_size: 4 }
// Act: config.validate()
// Assert: Ok(())
```
The ADR-003 floor value must be valid.

---

## §default — Default Formula (R-07 scenario 5, AC-09)

### test_inference_config_default_formula_in_range
```
// Arrange: config = InferenceConfig::default()
// Assert: config.rayon_pool_size >= 4
// Assert: config.rayon_pool_size <= 8
// Assert: config.validate() == Ok(())
```
Verifies that `Default::default()` always produces a value that passes validation.
The formula `(num_cpus::get() / 2).max(4).min(8)` is bounded; this test confirms
the compiled-in default is never an invalid value.

### test_inference_config_absent_section_uses_default
```
// Arrange: toml_str = "" (no [inference] section)
// Act: parse UnimatrixConfig from toml_str (use existing parse_config_str helper)
// Assert: config.inference.rayon_pool_size is in [4, 8]
// Assert: config.inference.validate() == Ok(())
```
Absent `[inference]` section produces the ADR-003 default. Serde `#[serde(default)]`
on both `UnimatrixConfig.inference` and `InferenceConfig` handles this.

---

## §serde — Deserialization

### test_inference_config_parses_from_toml
```
// toml_str = "[inference]\nrayon_pool_size = 6\n"
// Act: parse into UnimatrixConfig
// Assert: config.inference.rayon_pool_size == 6
// Assert: config.inference.validate() == Ok(())
```

### test_inference_config_unknown_field_ignored
```
// toml_str = "[inference]\nrayon_pool_size = 4\nunknown_field = true\n"
// Act: parse into UnimatrixConfig
// Assert: no error (serde should ignore unknown fields for this struct, or deny_unknown_fields
//         is not set — follow the pattern of other config structs)
```
Follows the existing pattern for all other config sections.

---

## §error-message — ConfigError for InferenceConfig

### test_inference_config_error_message_names_field
```
// Arrange: InferenceConfig { rayon_pool_size: 0 }
// Act: let err = config.validate().unwrap_err()
// Assert: err.to_string() contains "rayon_pool_size"
// Assert: err.to_string() contains "[inference]"
// Assert: err.to_string() contains the field value or the valid range
```
Actionable error messages are a requirement (AC-09): the operator must be able to
identify the offending field from the error output alone.

---

## §startup-error — Startup Abort Path (R-07 scenario 6)

This scenario requires an integration test (server binary, bad config) — see OVERVIEW.md
§new-integration-tests. Unit tests cannot exercise the `main.rs` startup path.

Document the expected behaviour here for completeness:

- Config validation runs in `main.rs` before `RayonPool::new` is called.
- `validate()` returns `Err(ConfigError::InferencePoolSizeOutOfRange { ... })`.
- `main.rs` prints the error and exits with non-zero status.
- No MCP server socket is opened; no connection is accepted.
- The error message contains `[inference]`, `rayon_pool_size`, and the offending value.

---

## §config-integration — InferenceConfig in UnimatrixConfig

### test_unimatrix_config_has_inference_field
```
// Arrange: UnimatrixConfig::default()
// Assert: config.inference is present (compiles; field accessible)
// Assert: config.inference.rayon_pool_size is in [4, 8]
```
Structural test: `inference: InferenceConfig` field is wired into `UnimatrixConfig`.

### test_unimatrix_config_inference_serde_default
```
// Arrange: toml_str with [profile], [knowledge] but no [inference]
// Assert: deserialized config.inference.rayon_pool_size is in [4, 8]
```
`#[serde(default)]` on `inference: InferenceConfig` must apply when the section is absent.

---

## Test Placement

```rust
// In crates/unimatrix-server/src/infra/config.rs
// Inside the existing #[cfg(test)] mod tests { ... } block
// Below the existing test groups, in a new subsection:

// -----------------------------------------------------------------------
// [inference] InferenceConfig validation tests
// -----------------------------------------------------------------------
```

No new test file is needed. Extend the existing `config.rs` test module, following
the established style (direct struct construction, no filesystem operations).
