# crt-036: RetentionConfig — Pseudocode

**File:** `crates/unimatrix-server/src/infra/config.rs`
**Action:** Add struct, default fns, validate(), ConfigError variant, wire into UnimatrixConfig

---

## Purpose

Expose operator-configurable retention parameters under the `[retention]` TOML section.
The struct follows every existing config section's `#[serde(default)]` pattern.
`validate()` follows `InferenceConfig::validate()` in both structure and error type.
`activity_detail_retention_cycles` must document its role as the governing ceiling for
PhaseFreqTable lookback and future GNN training window (AC-13).

---

## New ConfigError Variant

Add to the existing `ConfigError` enum in `config.rs`.
Pattern mirrors `NliFieldOutOfRange` — carries path, field name, actual value, reason string.

```
// In pub enum ConfigError { ... }

/// A [retention] config field is outside its valid range (crt-036).
RetentionFieldOutOfRange {
    path:   PathBuf,        // config file path for operator diagnosis
    field:  &'static str,   // field name, e.g. "activity_detail_retention_cycles"
    value:  String,         // actual value that failed (displayed to operator)
    reason: &'static str,   // human-readable valid range, e.g. "must be in range [1, 10000]"
}
```

The `Display` impl for `ConfigError` must include a match arm for this variant
that formats: `"config error at {path}: {field} = {value}: {reason}"`.
Follow the identical formatting pattern used for `NliFieldOutOfRange`.

---

## RetentionConfig Struct

```
// Place after ObservationConfig, before the Preset enum.

/// `[retention]` section — activity data and audit log retention policy.
///
/// All fields have compiled defaults via `#[serde(default = "...")]` so an absent
/// `[retention]` block in config.toml applies defaults without error.
#[derive(serde::Deserialize, Debug, Clone)]
#[serde(default)]
pub struct RetentionConfig {
    /// Number of completed (reviewed) feature cycles whose activity data
    /// (observations, query_log, sessions, injection_log) is retained.
    ///
    /// This value is the governing ceiling for PhaseFreqTable lookback and the
    /// future GNN training window. Reducing this value will truncate the data
    /// available to PhaseFreqTable::rebuild. Cycles outside this window are
    /// eligible for GC after their cycle_review_index row is confirmed present.
    ///
    /// Cross-reference: inference_config.query_log_lookback_days. When
    /// query_log_lookback_days implies a window older than the oldest retained
    /// cycle's computed_at, a tracing::warn! fires each tick (ADR-003 alignment guard).
    ///
    /// Range: [1, 10000]. Default: 50.
    #[serde(default = "default_activity_detail_retention_cycles")]
    pub activity_detail_retention_cycles: u32,

    /// Retention window in days for audit_log rows.
    ///
    /// audit_log is an accountability record, not a learning signal. Time-based
    /// retention is appropriate. Rows older than this value are deleted during
    /// the maintenance tick's step 4f.
    ///
    /// Range: [1, 3650]. Default: 180.
    #[serde(default = "default_audit_log_retention_days")]
    pub audit_log_retention_days: u32,

    /// Maximum purgeable cycles to process in a single maintenance tick.
    ///
    /// Limits the write-pool time consumed by GC. On first deployment with a large
    /// backlog, older cycles drain incrementally at this rate per tick. Oldest cycles
    /// (lowest computed_at) are processed first.
    ///
    /// Range: [1, 1000]. Default: 10.
    #[serde(default = "default_max_cycles_per_tick")]
    pub max_cycles_per_tick: u32,
}
```

---

## Default Functions

```
fn default_activity_detail_retention_cycles() -> u32 { 50 }
fn default_audit_log_retention_days() -> u32          { 180 }
fn default_max_cycles_per_tick() -> u32               { 10 }
```

---

## Default Impl

Required for `#[serde(default)]` at the struct level (applied when the entire
`[retention]` section is absent from config.toml):

```
impl Default for RetentionConfig {
    fn default() -> Self {
        RetentionConfig {
            activity_detail_retention_cycles: default_activity_detail_retention_cycles(),
            audit_log_retention_days:         default_audit_log_retention_days(),
            max_cycles_per_tick:              default_max_cycles_per_tick(),
        }
    }
}
```

---

## validate() Method

```
impl RetentionConfig {
    /// Validate all RetentionConfig fields against their documented ranges.
    ///
    /// Called during server startup alongside InferenceConfig::validate().
    /// An out-of-range value aborts startup with a structured error naming the field.
    ///
    /// Checks:
    ///   - activity_detail_retention_cycles in [1, 10000]
    ///   - audit_log_retention_days in [1, 3650]
    ///   - max_cycles_per_tick in [1, 1000]
    pub fn validate(&self, path: &Path) -> Result<(), ConfigError> {

        if self.activity_detail_retention_cycles < 1
            || self.activity_detail_retention_cycles > 10_000
        {
            return Err(ConfigError::RetentionFieldOutOfRange {
                path:   path.to_path_buf(),
                field:  "activity_detail_retention_cycles",
                value:  self.activity_detail_retention_cycles.to_string(),
                reason: "must be in range [1, 10000]",
            })
        }

        if self.audit_log_retention_days < 1 || self.audit_log_retention_days > 3_650 {
            return Err(ConfigError::RetentionFieldOutOfRange {
                path:   path.to_path_buf(),
                field:  "audit_log_retention_days",
                value:  self.audit_log_retention_days.to_string(),
                reason: "must be in range [1, 3650]",
            })
        }

        if self.max_cycles_per_tick < 1 || self.max_cycles_per_tick > 1_000 {
            return Err(ConfigError::RetentionFieldOutOfRange {
                path:   path.to_path_buf(),
                field:  "max_cycles_per_tick",
                value:  self.max_cycles_per_tick.to_string(),
                reason: "must be in range [1, 1000]",
            })
        }

        Ok(())
    }
}
```

---

## Wire into UnimatrixConfig

```
// In pub struct UnimatrixConfig { ... }
// Add after the existing `observation` field:

    #[serde(default)]
    pub retention: RetentionConfig,
```

Also add `retention: RetentionConfig { ... }` to the merge logic in `load_config()`
(wherever the merged config is assembled from global + project). Follow the identical
field-by-field merge pattern used for `inference: InferenceConfig { ... }`.
Use struct update syntax: `retention: project.retention` if project overrides global,
following the same "project replaces global" semantics (ADR-003, replace semantics).

Call site: `validate_config()` must call `config.retention.validate(path)?` alongside
the existing `config.inference.validate(path)?` call.

---

## config.toml [retention] Block

Add to `config.toml`:

```toml
[retention]
# Number of completed (reviewed) feature cycles to retain activity data for.
# Observations, query_log, sessions, and injection_log for cycles beyond this
# window are deleted after their cycle_review_index row exists.
# This value is the governing ceiling for PhaseFreqTable lookback and
# future GNN training window. Reducing it truncates data for PhaseFreqTable::rebuild.
# Range: [1, 10000]. Default: 50.
activity_detail_retention_cycles = 50

# Maximum number of purgeable cycles to process in a single maintenance tick.
# Limits tick budget consumed by GC. Older cycles (lowest computed_at) are processed
# first. Deferred cycles are picked up on the next tick.
# Range: [1, 1000]. Default: 10.
max_cycles_per_tick = 10

# Retention window in days for audit_log rows.
# Audit data is an accountability record, not a learning signal.
# Range: [1, 3650]. Default: 180.
audit_log_retention_days = 180
```

---

## Error Handling

- `validate()` returns `Err(ConfigError::RetentionFieldOutOfRange { ... })` for any
  out-of-range value. One error returned per call (first failure, not accumulated).
- `validate_config()` propagates the error via `?` — startup aborts with the error message.
- Display impl on the new variant must include the field name so operator can identify
  the offending TOML key from the error alone (AC-11, AC-12, AC-12b).

---

## Key Test Scenarios

- `RetentionConfig::default()` produces `activity_detail_retention_cycles = 50`,
  `audit_log_retention_days = 180`, `max_cycles_per_tick = 10` (AC-10 unit test).
- `activity_detail_retention_cycles = 0` → `validate()` returns `Err(_)` with message
  containing `"activity_detail_retention_cycles"` (AC-11).
- `audit_log_retention_days = 0` → `validate()` returns `Err(_)` with message containing
  `"audit_log_retention_days"` (AC-12).
- `max_cycles_per_tick = 0` → `validate()` returns `Err(_)` with message containing
  `"max_cycles_per_tick"` (AC-12b).
- Upper bound: `max_cycles_per_tick = 1001` → `Err(_)` (R-10).
- Boundary values `activity_detail_retention_cycles = 1`, `audit_log_retention_days = 1`,
  `max_cycles_per_tick = 1` → all pass `validate()`.
- Parse a config.toml with absent `[retention]` section → fields equal defaults (AC-10
  integration test — must be a real TOML parse, not just `Default::default()`).
- Parse a config.toml with explicit `[retention]` values → explicit values applied (AC-10).
