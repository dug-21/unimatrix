# infra/config.rs — InferenceConfig Pseudocode
# crt-039: Raise nli_informs_cosine_floor default from 0.45 to 0.50

## Purpose

`infra/config.rs` owns `InferenceConfig` and its default value functions. crt-039 changes
the default value of `nli_informs_cosine_floor` from 0.45 to 0.50 (ADR-003). The change
is isolated to two locations in the file. No validation change is needed — 0.5 is within
the existing exclusive range check `(0.0, 1.0)`.

## Modified Function: `default_nli_informs_cosine_floor`

```
// BEFORE:
fn default_nli_informs_cosine_floor() -> f32 {
    0.45
}

// AFTER:
fn default_nli_informs_cosine_floor() -> f32 {
    0.5  // raised from 0.45 (crt-039 ADR-003): NLI neutral guard removed; floor compensates
}
```

## Modified Field in `InferenceConfig::default()`

The `InferenceConfig::default()` implementation sets `nli_informs_cosine_floor` via the
backing function. No change to the calling pattern — only the function's return value changes.

```
impl Default for InferenceConfig {
    fn default() -> Self {
        InferenceConfig {
            // ... [all other fields unchanged] ...

            // crt-039: raised from 0.45 (ADR-003). Floor compensates for NLI neutral guard removal.
            nli_informs_cosine_floor: default_nli_informs_cosine_floor(),

            // ... [all other fields unchanged] ...
        }
    }
}
```

Note: `nli_informs_ppr_weight` (0.6) and `informs_category_pairs` defaults are unchanged.
No other `InferenceConfig` fields are touched.

## Validation Unchanged (C-09)

`InferenceConfig::validate()` contains an exclusive range check on `nli_informs_cosine_floor`:

```
// Existing validation (UNCHANGED):
if self.nli_informs_cosine_floor <= 0.0 || self.nli_informs_cosine_floor >= 1.0 {
    return Err(ConfigError::InvalidField { ... });
}
```

0.5 is strictly within `(0.0, 1.0)`. No change to the validation condition is needed or
permitted. The operator note in ADR-003 applies: TOML overrides take precedence; deployers
who set `nli_informs_cosine_floor = 0.45` in config.toml retain that value.

## Error Handling

No error handling changes. `default_nli_informs_cosine_floor()` is an infallible value
function. The `validate()` function's range check is structurally identical for 0.45 and
0.50 — both pass the `(0.0, 1.0)` exclusive check.

## Key Test Scenarios

### Tests to Update (TC-U)

**`test_inference_config_default_nli_informs_cosine_floor`** (in config.rs test module)

```
// BEFORE:
assert_eq!(InferenceConfig::default().nli_informs_cosine_floor, 0.45_f32);

// AFTER:
assert_eq!(InferenceConfig::default().nli_informs_cosine_floor, 0.5_f32);
```

**`test_validate_nli_informs_cosine_floor_valid_value_is_ok`**

If this test uses `0.45` as the nominal valid value to pass to `validate()`, update it
to use `0.5` as the nominal valid value. The assertion `is_ok()` is unchanged.

**`test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold`** (in nli_detection_tick.rs)

This test verifies that Phase 4b uses `nli_informs_cosine_floor` and not
`supports_candidate_threshold`. The floor band changes from `[0.45, 0.50)` to
`[0.50, supports_threshold)`. The test must:
- Use cosine = 0.50 (inclusive at new floor) to verify Phase 4b accepts it.
- Use cosine just below 0.50 (e.g., 0.499) to verify Phase 4b rejects it.
- Use cosine just above `supports_candidate_threshold` to verify Phase 4 accepts it
  but Phase 4b rejects it (explicit subtraction covers this — TC-07).

### New Test for Floor Boundary (AC-04, AC-05)

Tests TC-05 and TC-06 in nli_detection_tick.md cover the floor boundary semantics at
the new 0.50 value. Config.rs itself needs the updated assertion in
`test_inference_config_default_nli_informs_cosine_floor` only.
