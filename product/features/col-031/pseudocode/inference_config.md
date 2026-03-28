# col-031: infra/config.rs InferenceConfig Additions — Pseudocode

File: `crates/unimatrix-server/src/infra/config.rs`
Status: MODIFIED

---

## Purpose

Two changes to `InferenceConfig`:
1. Raise `default_w_phase_explicit()` from `0.0` to `0.05` (weight activation, ADR-004).
2. Add `query_log_lookback_days: u32` field with default `30` and validation `[1, 3650]`.

Also update the FusionWeights sum-check doc-comment in search.rs to reflect the new sum.

---

## Change 1: `default_w_phase_explicit` Function

Locate the existing function (currently returns 0.0):

```rust
// crt-026: default_w_phase_explicit — 0.0 (W3-1 placeholder, ADR-003)
fn default_w_phase_explicit() -> f64 {
    0.0
}
```

Change to:

```
// col-031: raised from 0.0 to 0.05 — PhaseFreqTable activates this term (ADR-004).
// Previously reserved as W3-1 placeholder at 0.0 (crt-026, ADR-003).
// Additive term outside the six-weight sum constraint (ADR-004, crt-026).
// Total weight sum with defaults: 0.95 + 0.02 + 0.05 = 1.02.
fn default_w_phase_explicit() -> f64 {
    0.05
}
```

---

## Change 2: New `query_log_lookback_days` Default Function

Add immediately after `default_w_phase_explicit`:

```
// col-031: default lookback window for PhaseFreqTable rebuild (ADR-002).
// 30 days covers approximately 2 delivery cycles at typical session frequency.
// Range [1, 3650] enforced by validate(). #409 owns cycle-aligned GC as the
// long-term successor to this time-based approximation.
fn default_query_log_lookback_days() -> u32 {
    30
}
```

---

## Change 3: New `query_log_lookback_days` Field in `InferenceConfig` Struct

Add at the end of the `InferenceConfig` struct, after `graph_inference_k`:

```
/// col-031: Lookback window in days for PhaseFreqTable rebuild SQL.
///
/// Controls the `WHERE ts > strftime('%s','now') - lookback_days * 86400`
/// filter in `query_phase_freq_table`. Larger values include more history
/// at the cost of including older access patterns. Smaller values are more
/// reactive but may miss low-frequency entries.
///
/// This governs the rebuild SQL window only — not data deletion. Data GC
/// belongs to #409 (cycle-aligned GC).
///
/// Range: [1, 3650] (1 day to 10 years). Enforced by validate().
/// Default: 30 (two typical delivery cycles at session frequency).
#[serde(default = "default_query_log_lookback_days")]
pub query_log_lookback_days: u32,
```

---

## Change 4: `InferenceConfig::Default` Implementation

In the `impl Default for InferenceConfig` block, add to the `InferenceConfig { ... }`
struct literal (after the `graph_inference_k` field):

```
query_log_lookback_days: default_query_log_lookback_days(),   // col-031
```

Also confirm that `w_phase_explicit` in the Default impl is being set via the
`default_w_phase_explicit()` function call (which now returns 0.05), OR update the
hardcoded value in the Default block. In the current code the Default impl has:

```rust
w_phase_explicit: 0.0,   // crt-026: W3-1 placeholder (ADR-003)
```

This hardcoded `0.0` must be changed to use the function or updated to `0.05`:

```
w_phase_explicit: default_w_phase_explicit(),  // col-031: 0.05 (ADR-004)
```

OR simply:

```
w_phase_explicit: 0.05,  // col-031: raised from 0.0 (ADR-004, crt-026 ADR-003)
```

Either form is correct. The `serde(default = "default_w_phase_explicit")` on the
field will pick up the function return value for deserialization. The `Default` impl
must also reflect the new value so that `InferenceConfig::default()` in tests is
consistent with TOML deserialization.

---

## Change 5: `InferenceConfig::validate()` — Range Check

In the `validate()` method, add a range check for `query_log_lookback_days`.
The existing method checks `rayon_pool_size`, NLI fields, weight ranges, etc.
Add the new check after the per-field weight range checks (or at the end of
the method before `Ok(())`):

```
// col-031: query_log_lookback_days range check (R-08, ADR-002).
// 0 would make the WHERE clause include no rows (empty window -> use_fallback=true).
// >3650 is effectively unbounded and likely an operator misconfiguration.
if self.query_log_lookback_days < 1 || self.query_log_lookback_days > 3650 {
    return Err(ConfigError::InferenceFieldOutOfRange {
        field: "query_log_lookback_days".to_string(),
        value: self.query_log_lookback_days as f64,
        min: 1.0,
        max: 3650.0,
    });
}
```

If `ConfigError::InferenceFieldOutOfRange` does not exist, check the actual error
enum variants in the file and use the appropriate variant (e.g.,
`ConfigError::InvalidInferenceField` or similar). The error must identify the field
name and the invalid value. If no suitable variant exists, the implementation agent
must add one — the error must be machine-readable for diagnostic purposes.

---

## Change 6: `merge_configs` — Merge Logic for New Field

In `merge_configs`, add merge logic for `query_log_lookback_days` following the
same diff-from-default pattern used for all other `InferenceConfig` fields.
The existing pattern is:

```rust
field_name: if (project.inference.field_name - default.inference.field_name).abs()
                > f64::EPSILON
            {
                project.inference.field_name
            } else {
                global.inference.field_name
            },
```

For a `u32` field:

```
query_log_lookback_days: if project.inference.query_log_lookback_days
    != default.inference.query_log_lookback_days
{
    project.inference.query_log_lookback_days
} else {
    global.inference.query_log_lookback_days
},
```

---

## Change 7: FusionWeights Sum-Check Doc-Comment (in search.rs)

In `search.rs`, find the `FusionWeights` struct doc-comment. The current comment
says:

```
/// w_phase_histogram and w_phase_explicit are additive terms excluded from this
/// constraint. Their sum does not enter the six-term sum check. With defaults,
/// total sum = 0.95 + 0.02 + 0.0 = 0.97, within <= 1.0.
```

Update to:

```
/// w_phase_histogram and w_phase_explicit are additive terms excluded from this
/// constraint (ADR-004, crt-026). With col-031 defaults:
/// // 0.95 + 0.02 + 0.05 = 1.02 — w_phase_explicit is additive outside the six-weight
/// // constraint (ADR-004, crt-026). The six-weight sum check is unchanged.
```

The exact comment text from ADR-004 / IMPLEMENTATION-BRIEF is:
```
// 0.95 + 0.02 + 0.05 = 1.02 — w_phase_explicit is additive outside the six-weight constraint (ADR-004, crt-026)
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `query_log_lookback_days = 0` | `validate()` returns `Err`; server fails to start |
| `query_log_lookback_days = 3651` | `validate()` returns `Err`; server fails to start |
| `query_log_lookback_days = 1` | Valid; passes |
| `query_log_lookback_days = 3650` | Valid; passes |
| TOML with no `query_log_lookback_days` key | Deserialized as `30` via serde default |
| TOML with no `w_phase_explicit` key | Deserialized as `0.05` via serde default (after this change) |

---

## Key Test Scenarios

### AC-09: `w_phase_explicit` default is 0.05

```
// InferenceConfig::default() must return w_phase_explicit = 0.05.
// Existing test `test_inference_config_default_phase_weights` must be updated
// to assert 0.05, not 0.0. The test was passing 0.0 before col-031.
assert_eq!(config.w_phase_explicit, 0.05);
```

### AC-10: `query_log_lookback_days` default is 30

```
// Deserialize a minimal TOML (no [inference] section) into UnimatrixConfig.
// Assert: config.inference.query_log_lookback_days == 30.
let config: UnimatrixConfig = toml::from_str("").unwrap();
assert_eq!(config.inference.query_log_lookback_days, 30);
```

### R-08: Validation boundary tests

```
// validate() with query_log_lookback_days = 0 -> Err
// validate() with query_log_lookback_days = 3651 -> Err
// validate() with query_log_lookback_days = 1 -> Ok
// validate() with query_log_lookback_days = 3650 -> Ok
```
