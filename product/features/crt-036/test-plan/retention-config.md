# Test Plan: RetentionConfig

**Component:** `crates/unimatrix-server/src/infra/config.rs`
**Risks Covered:** R-10, R-15
**ACs Covered:** AC-10, AC-11, AC-12, AC-12b, AC-13

---

## Unit Test Expectations

### `test_retention_config_defaults_and_override` (AC-10)

**Arrange:** Prepare two TOML strings — one with no `[retention]` section, one with
explicit non-default values.

**Act:**
1. Parse the no-retention TOML via `RetentionConfig::default()` or full
   `UnimatrixConfig` parse.
2. Parse the override TOML.

**Assert:**
- Absent `[retention]` block: `activity_detail_retention_cycles == 50`,
  `audit_log_retention_days == 180`, `max_cycles_per_tick == 10`.
- Explicit values: each field reflects the supplied value, not the default.
- `RetentionConfig::default()` unit call returns the same three defaults.

**Note:** The absent-block case must use a real TOML deserialization path (e.g.,
`toml::from_str::<UnimatrixConfig>(...)`) not just `RetentionConfig::default()`.
Serde `#[serde(default)]` behavior on absent sections differs from Rust's `Default`
trait; both must be verified independently.

---

### `test_retention_config_validate_rejects_zero_retention_cycles` (AC-11)

**Arrange:** `RetentionConfig { activity_detail_retention_cycles: 0, audit_log_retention_days: 180, max_cycles_per_tick: 10 }`

**Act:** Call `config.validate(Path::new("config.toml"))`.

**Assert:**
- Returns `Err(_)`.
- Error message (via `Display` or `Debug`) contains `"activity_detail_retention_cycles"`.

**Also assert:** Upper bound `activity_detail_retention_cycles = 10001` returns `Err(_)`
containing `"activity_detail_retention_cycles"`.

---

### `test_retention_config_validate_rejects_zero_audit_days` (AC-12)

**Arrange:** `RetentionConfig { activity_detail_retention_cycles: 50, audit_log_retention_days: 0, max_cycles_per_tick: 10 }`

**Act:** Call `config.validate(...)`.

**Assert:**
- Returns `Err(_)`.
- Error message contains `"audit_log_retention_days"`.

**Also assert:** `audit_log_retention_days = 3651` returns `Err(_)` (upper bound).

---

### `test_retention_config_validate_rejects_invalid_max_cycles` (AC-12b)

**Arrange:**
- Case A: `max_cycles_per_tick: 0`
- Case B: `max_cycles_per_tick: 1001`

**Act:** Call `validate()` for each case.

**Assert:**
- Case A: `Err(_)`, message contains `"max_cycles_per_tick"`.
- Case B: `Err(_)`, message contains `"max_cycles_per_tick"`.

**Also assert:** Boundary values `(activity_detail_retention_cycles: 1, audit_log_retention_days: 1, max_cycles_per_tick: 1)` all pass `validate()` without error (lower-bound accepted).

---

## Manual Review Assertion (AC-13)

The `activity_detail_retention_cycles` field declaration must have a `///` triple-slash
doc comment containing both the string `"PhaseFreqTable lookback"` and the string
`"GNN training window"`.

This is verified by the Gate 3c reviewer reading the field declaration, not by a
test function. The IMPLEMENTATION-BRIEF explicitly requires this doc comment.

---

## Structural Assertions

- `RetentionConfig` derives `serde::Deserialize` and `Clone`.
- `RetentionConfig` has `#[serde(default)]` at struct level.
- `UnimatrixConfig` has a field `pub retention: RetentionConfig` (or equivalent) that
  is accessible from `run_maintenance()`.
- `validate()` is called at the same startup call site as `InferenceConfig::validate()`.
  Verify via code inspection that a validation failure aborts startup.

---

## Edge Cases

- All three fields at their documented defaults simultaneously pass `validate()`.
- Partial override (only one of the three fields present in TOML) applies the default
  for the absent fields.
- `max_cycles_per_tick = 1000` (upper bound) passes `validate()`.
- `activity_detail_retention_cycles = 10000` (upper bound) passes `validate()`.
- `audit_log_retention_days = 3650` (upper bound) passes `validate()`.
