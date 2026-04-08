# Component: config
# File: `crates/unimatrix-server/src/infra/config.rs`

---

## Purpose

Rename `InferenceConfig::query_log_lookback_days` → `phase_freq_lookback_days` with a
backward-compat serde alias, and add `min_phase_session_pairs: u32` (default 5,
range [1, 1000]). Update all five affected sites in the file.

The serde alias handles TOML deserialization backward compatibility only. All Rust
struct-literal construction sites (in tests and in the config merge block) require
manual update — the compiler enforces this (SR-04, R-06).

---

## Field Rename: query_log_lookback_days → phase_freq_lookback_days

### Before (current code, ~lines 464–468)

```rust
/// Range: [1, 3650] (1 day to 10 years). Enforced by validate().
/// Default: 30 (two typical delivery cycles at session frequency).
#[serde(default = "default_query_log_lookback_days")]
pub query_log_lookback_days: u32,
```

### After

```rust
/// Lookback window (days) for observations-sourced PhaseFreqTable rebuild.
///
/// Governs the time window of observations.ts_millis queried by
/// query_phase_freq_observations. This field was formerly named
/// `query_log_lookback_days` (col-031 ADR-002); the serde alias preserves
/// backward compatibility for TOML configs using the old name (ADR-004).
///
/// Range: [1, 3650]. Default: 30.
#[serde(alias = "query_log_lookback_days")]
#[serde(default = "default_phase_freq_lookback_days")]
pub phase_freq_lookback_days: u32,
```

Note: two separate `#[serde(...)]` attributes or combined with comma — match the
project's existing serde attribute style.

---

## New Field: min_phase_session_pairs

### Insertion point

Place immediately after `phase_freq_lookback_days` in the `InferenceConfig` struct.

```rust
/// Minimum distinct (phase, session_id) pair count required for a valid
/// PhaseFreqTable rebuild.
///
/// When the count of distinct (phase, session_id) observation pairs within the
/// lookback window falls below this threshold, PhaseFreqTable::rebuild() sets
/// use_fallback = true and emits tracing::warn! (FR-17, AC-14).
///
/// Default: 5. Range: [1, 1000].
/// Conservative default — low enough to not trigger spuriously in dev/test
/// environments while providing a non-zero signal-quality floor (ADR-003 OQ-3).
#[serde(default = "default_min_phase_session_pairs")]
pub min_phase_session_pairs: u32,
```

---

## New Default Function: default_phase_freq_lookback_days

```
// 30 days covers approximately 2 delivery cycles at typical session frequency.
// Range [1, 3650] enforced by validate().
fn default_phase_freq_lookback_days() -> u32 {
    30
}
```

Replace the body of the old `default_query_log_lookback_days` function (same value),
OR rename the function in-place. Renaming is preferred to avoid leaving an unused
function. The serde `default = "..."` attribute in the struct field must reference the
new function name.

---

## New Default Function: default_min_phase_session_pairs

```
fn default_min_phase_session_pairs() -> u32 {
    5
}
```

Place alongside other `default_*` functions at the bottom of the module (or near the
existing `default_query_log_lookback_days` / `default_phase_freq_lookback_days`).

---

## Updated: InferenceConfig::Default impl (~line 691 region)

### Before

```rust
query_log_lookback_days: default_query_log_lookback_days(),
```

### After

```rust
phase_freq_lookback_days: default_phase_freq_lookback_days(),
min_phase_session_pairs: default_min_phase_session_pairs(),
```

Both must be present in the `InferenceConfig { ... }` struct literal in the `Default` impl.

---

## Updated: InferenceConfig::validate() (~line 1211 region)

### Lookback days validation — field name update

Update the field name in the validation block and in the error struct:

```
// Before:
if self.query_log_lookback_days < 1 || self.query_log_lookback_days > 3650 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "query_log_lookback_days",
        value: self.query_log_lookback_days.to_string(),
        reason: "must be in range [1, 3650]",
    });
}

// After:
if self.phase_freq_lookback_days < 1 || self.phase_freq_lookback_days > 3650 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "phase_freq_lookback_days",
        value: self.phase_freq_lookback_days.to_string(),
        reason: "must be in range [1, 3650]",
    });
}
```

### New validation block: min_phase_session_pairs range [1, 1000]

Insert after the `phase_freq_lookback_days` block:

```
// -- crt-050: min_phase_session_pairs range check [1, 1000]. --
// 0 would allow any observation count (meaningless floor).
// >1000 is implausibly high for any production workload.
if self.min_phase_session_pairs < 1 || self.min_phase_session_pairs > 1000 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "min_phase_session_pairs",
        value: self.min_phase_session_pairs.to_string(),
        reason: "must be in range [1, 1000]",
    });
}
```

---

## Updated: Config merge block (~line 2638 region)

### Before

```rust
// col-031: phase frequency table fields
query_log_lookback_days: if project.inference.query_log_lookback_days
    != default.inference.query_log_lookback_days
{
    project.inference.query_log_lookback_days
} else {
    global.inference.query_log_lookback_days
},
```

### After

```rust
// crt-050: phase frequency table fields
phase_freq_lookback_days: if project.inference.phase_freq_lookback_days
    != default.inference.phase_freq_lookback_days
{
    project.inference.phase_freq_lookback_days
} else {
    global.inference.phase_freq_lookback_days
},
min_phase_session_pairs: if project.inference.min_phase_session_pairs
    != default.inference.min_phase_session_pairs
{
    project.inference.min_phase_session_pairs
} else {
    global.inference.min_phase_session_pairs
},
```

---

## Updated: RetentionConfig doc comment (~line 1444 region)

The cross-reference comment in `RetentionConfig` that mentions `query_log_lookback_days`:

```
// Before:
/// Cross-reference: inference_config.query_log_lookback_days. When
/// query_log_lookback_days implies a window older than the oldest retained
/// cycle's computed_at, a tracing::warn! fires each tick (ADR-003 alignment guard).

// After:
/// Cross-reference: inference_config.phase_freq_lookback_days (formerly
/// query_log_lookback_days, renamed in crt-050 ADR-004). When
/// phase_freq_lookback_days implies a window older than the oldest retained
/// cycle's computed_at, a tracing::warn! fires each tick (crt-036 ADR-003
/// alignment guard, updated in crt-050 status-diagnostics component).
```

---

## Test Struct Literal Audit (SR-04, R-06)

Grep for all `InferenceConfig {` struct literal constructions in test code and
update `query_log_lookback_days:` → `phase_freq_lookback_days:`. The compiler
will reject any missed site.

Known locations from the grep output:
- `test_inference_config_valid_lower_bound` (~line 4419)
- `test_inference_config_valid_upper_bound` (~line 4432)
- `test_inference_config_rejects_zero` (~line 4445)
- `test_inference_config_rejects_sixty_five` (~line 4471)
- `test_inference_config_valid_eight` (~line 4493)
- `test_inference_config_valid_four` (~line 4506)
- `assert_validate_fails_with_field` helper (~line 4626)

All of these use `..InferenceConfig::default()` and only set `rayon_pool_size` — they
do not set `query_log_lookback_days` directly. They will compile unchanged because
`..Default::default()` handles the renamed field. However, confirm by grepping:

```
grep -n 'query_log_lookback_days' crates/unimatrix-server/src/infra/config.rs
```

After implementation, the only remaining occurrence must be the `#[serde(alias = ...)]`
annotation. Any other occurrence is a missed update (R-06 gate).

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| `phase_freq_lookback_days` out of range | `ConfigError::NliFieldOutOfRange` with field name and value |
| `min_phase_session_pairs` out of range | `ConfigError::NliFieldOutOfRange` with field name and value |
| TOML with old field name `query_log_lookback_days` | Serde alias accepts it; `phase_freq_lookback_days` populated correctly |
| TOML with neither field | `default_phase_freq_lookback_days()` returns 30; `default_min_phase_session_pairs()` returns 5 |

---

## Key Test Scenarios

**T-CFG-01: Serde alias — old name deserializes to new field (AC-10, R-06)**
- Deserialize `{"query_log_lookback_days": 30}` as `InferenceConfig`.
- Assert: `config.phase_freq_lookback_days == 30`.

**T-CFG-02: Serde new name deserializes (AC-10)**
- Deserialize `{"phase_freq_lookback_days": 45}` as `InferenceConfig`.
- Assert: `config.phase_freq_lookback_days == 45`.

**T-CFG-03: Default values correct**
- `InferenceConfig::default()`.
- Assert: `phase_freq_lookback_days == 30`.
- Assert: `min_phase_session_pairs == 5`.

**T-CFG-04: phase_freq_lookback_days validation — lower bound**
- `InferenceConfig { phase_freq_lookback_days: 0, ..Default::default() }`.
- Assert: `validate()` returns `Err` with field name "phase_freq_lookback_days".

**T-CFG-05: phase_freq_lookback_days validation — upper bound**
- `InferenceConfig { phase_freq_lookback_days: 3651, ..Default::default() }`.
- Assert: `validate()` returns `Err` with field name "phase_freq_lookback_days".

**T-CFG-06: phase_freq_lookback_days validation — valid boundaries**
- `phase_freq_lookback_days: 1` → `validate()` returns Ok.
- `phase_freq_lookback_days: 3650` → `validate()` returns Ok.

**T-CFG-07: min_phase_session_pairs validation — lower bound**
- `InferenceConfig { min_phase_session_pairs: 0, ..Default::default() }`.
- Assert: `validate()` returns `Err` with field "min_phase_session_pairs".

**T-CFG-08: min_phase_session_pairs validation — upper bound**
- `InferenceConfig { min_phase_session_pairs: 1001, ..Default::default() }`.
- Assert: `validate()` returns `Err` with field "min_phase_session_pairs".

**T-CFG-09: min_phase_session_pairs validation — valid boundaries**
- `min_phase_session_pairs: 1` → Ok.
- `min_phase_session_pairs: 1000` → Ok.

**T-CFG-10: Grep gate — no remaining `query_log_lookback_days` field references**
- After implementation, `grep -n 'query_log_lookback_days' config.rs` must return
  only the `#[serde(alias = "...")]` line. This is verified by CI build + grep.
