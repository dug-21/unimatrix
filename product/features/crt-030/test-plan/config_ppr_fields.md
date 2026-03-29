# Test Plan: config.rs PPR Fields

## Component

`crates/unimatrix-server/src/infra/config.rs`

Five new fields in `InferenceConfig`:
- `ppr_alpha: f64` — default 0.85, range (0.0, 1.0) exclusive
- `ppr_iterations: usize` — default 20, range [1, 100] inclusive
- `ppr_inclusion_threshold: f64` — default 0.05, range (0.0, 1.0) exclusive
- `ppr_blend_weight: f64` — default 0.15, range [0.0, 1.0] inclusive
- `ppr_max_expand: usize` — default 50, range [1, 500] inclusive

Tests live in the `#[cfg(test)]` mod of `config.rs`, following the existing pattern.
All tests use `#[test]` (sync — no async config parsing).

---

## AC-09: Default Values, Serde Round-Trip, SearchService Wiring

### `test_inference_config_ppr_defaults`
Arrange: `InferenceConfig::default()`.
Assert: `cfg.ppr_alpha == 0.85`
Assert: `cfg.ppr_iterations == 20`
Assert: `cfg.ppr_inclusion_threshold == 0.05`
Assert: `cfg.ppr_blend_weight == 0.15`
Assert: `cfg.ppr_max_expand == 50`

### `test_inference_config_ppr_serde_round_trip`
Arrange: Serialize `InferenceConfig::default()` to TOML, then deserialize back.
Assert: deserialized values match the defaults for all five PPR fields.
Pattern: follows the TOML round-trip test established by entry #3662 (cited in ACCEPTANCE-MAP).

### `test_inference_config_ppr_serde_absent_fields_use_defaults`
Arrange: Deserialize an `InferenceConfig` from a TOML string that contains NO PPR fields (simulates
a config file written before crt-030 was deployed).
Assert: all five PPR fields take their default values (serde `default` attribute is active).
Rationale: This is the zero-downtime upgrade path — existing configs must not fail to parse.

### `test_inference_config_ppr_serde_explicit_override`
Arrange: Deserialize from TOML string with all five PPR fields explicitly set to non-default values:
`ppr_alpha = 0.9`, `ppr_iterations = 10`, `ppr_inclusion_threshold = 0.1`, `ppr_blend_weight = 0.2`,
`ppr_max_expand = 25`.
Assert: each field reads back the explicit value, not the default.

### `test_search_service_receives_ppr_fields`
Arrange: Construct a `SearchService` from `InferenceConfig::default()`.
Assert: the service's internal config (or the fields passed to it at construction) match the five
PPR defaults. This test verifies the wiring between `config.rs` and `SearchService::new()`.
Implementation note: if `SearchService` stores config fields directly (not the full `InferenceConfig`),
assert each field individually.

---

## AC-10 / R-06: Validation — Rejection of Out-of-Range Values

Each test below creates an `InferenceConfig` with one invalid field, calls `cfg.validate()`,
and asserts the result is `Err(ConfigError::NliFieldOutOfRange)` (or equivalent) naming the
specific field.

### `ppr_alpha` validation (exclusive range `(0.0, 1.0)`)

#### `test_ppr_alpha_zero_rejected`
Input: `ppr_alpha = 0.0`. Assert: `validate()` returns `Err` with message/variant referencing `"ppr_alpha"`.

#### `test_ppr_alpha_one_rejected`
Input: `ppr_alpha = 1.0`. Assert: `Err`.

#### `test_ppr_alpha_valid_boundary_low`
Input: `ppr_alpha = f64::EPSILON`. Assert: `validate()` returns `Ok(())`.

#### `test_ppr_alpha_valid_boundary_high`
Input: `ppr_alpha = 1.0 - f64::EPSILON`. Assert: `Ok(())`.

#### `test_ppr_alpha_typical_value`
Input: `ppr_alpha = 0.85` (default). Assert: `Ok(())`.

---

### `ppr_iterations` validation (inclusive range `[1, 100]`)

#### `test_ppr_iterations_zero_rejected`
Input: `ppr_iterations = 0`. Assert: `Err` referencing `"ppr_iterations"`.

#### `test_ppr_iterations_101_rejected`
Input: `ppr_iterations = 101`. Assert: `Err`.

#### `test_ppr_iterations_valid_min`
Input: `ppr_iterations = 1`. Assert: `Ok(())`.

#### `test_ppr_iterations_valid_max`
Input: `ppr_iterations = 100`. Assert: `Ok(())`.

#### `test_ppr_iterations_default_valid`
Input: `ppr_iterations = 20`. Assert: `Ok(())`.

---

### `ppr_inclusion_threshold` validation (exclusive range `(0.0, 1.0)`)

#### `test_ppr_inclusion_threshold_zero_rejected` (R-06 lower bound)
Input: `ppr_inclusion_threshold = 0.0`. Assert: `Err` referencing `"ppr_inclusion_threshold"`.
Rationale: The spec uses `>` (strictly greater than) for threshold comparison; a threshold of exactly
0.0 would include every non-zero PPR score, which is likely operator error.

#### `test_ppr_inclusion_threshold_one_rejected`
Input: `ppr_inclusion_threshold = 1.0`. Assert: `Err`.

#### `test_ppr_inclusion_threshold_valid_boundary_low`
Input: `ppr_inclusion_threshold = f64::EPSILON`. Assert: `Ok(())`.

#### `test_ppr_inclusion_threshold_default_valid`
Input: `ppr_inclusion_threshold = 0.05`. Assert: `Ok(())`.

---

### `ppr_blend_weight` validation (inclusive range `[0.0, 1.0]`)

#### `test_ppr_blend_weight_negative_rejected`
Input: `ppr_blend_weight = -0.001`. Assert: `Err` referencing `"ppr_blend_weight"`.

#### `test_ppr_blend_weight_above_one_rejected`
Input: `ppr_blend_weight = 1.001`. Assert: `Err`.

#### `test_ppr_blend_weight_zero_valid` (R-03 boundary — must be accepted)
Input: `ppr_blend_weight = 0.0`. Assert: `Ok(())`.
Note: The behavior when 0.0 is used is tested separately in `search_step_6d.md`.

#### `test_ppr_blend_weight_one_valid` (R-11 boundary — must be accepted)
Input: `ppr_blend_weight = 1.0`. Assert: `Ok(())`.

#### `test_ppr_blend_weight_default_valid`
Input: `ppr_blend_weight = 0.15`. Assert: `Ok(())`.

---

### `ppr_max_expand` validation (inclusive range `[1, 500]`)

#### `test_ppr_max_expand_zero_rejected`
Input: `ppr_max_expand = 0`. Assert: `Err` referencing `"ppr_max_expand"`.

#### `test_ppr_max_expand_501_rejected`
Input: `ppr_max_expand = 501`. Assert: `Err`.

#### `test_ppr_max_expand_valid_min`
Input: `ppr_max_expand = 1`. Assert: `Ok(())`.

#### `test_ppr_max_expand_valid_max`
Input: `ppr_max_expand = 500`. Assert: `Ok(())`.

#### `test_ppr_max_expand_default_valid`
Input: `ppr_max_expand = 50`. Assert: `Ok(())`.

---

## Validation Error Specificity

### `test_ppr_validation_error_names_field`
Arrange: Set one invalid field (e.g., `ppr_alpha = 0.0`).
Act: call `validate()`.
Assert: the `ConfigError` value or its `Display` output contains the string `"ppr_alpha"`.
Rationale: AC-10 requires the error names the specific field. Repeat for each field name to confirm
the validation messages are field-specific, not generic.

---

## Global+Project Config Merge

### `test_ppr_fields_merged_from_project_config`
Arrange: Construct a "global" `InferenceConfig` with `ppr_alpha = 0.80` and a "project" config
that overrides only `ppr_alpha = 0.90`.
Act: apply the config merge logic.
Assert: merged config has `ppr_alpha = 0.90` and all other PPR fields at global defaults.
Note: This test requires access to the merge function used in `config.rs`. If the merge is
implemented as `Config::merge(global, project)`, call that directly.

---

## S-03 (Security): Config Validation as Startup Gate

All validation tests above verify `validate()` returns an error for out-of-range values. The
security implication (from S-03 in the risk strategy) is that these errors must prevent server
startup. While the server startup test is an integration concern, the unit tests confirm the
validation contract that the startup path relies on.

No additional security-specific unit tests are needed for config fields beyond the validation
tests above.
