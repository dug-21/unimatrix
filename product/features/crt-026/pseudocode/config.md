# crt-026: Component — `infra/config.rs`

File: `crates/unimatrix-server/src/infra/config.rs`
Wave: 1

---

## Purpose

Add `w_phase_explicit` and `w_phase_histogram` to `InferenceConfig` following the
established `default_w_*` serde pattern. Add per-field `[0.0, 1.0]` range checks
in `validate()`. The existing six-weight sum check is NOT modified (ADR-004, OQ-A).

---

## Current State (Relevant Context)

The six existing fusion weight fields in `InferenceConfig` follow this exact pattern:

```
#[serde(default = "default_w_sim")]
pub w_sim: f64,
```

Each has a corresponding private `fn default_w_sim() -> f64 { 0.25 }` at the bottom
of the file in the "Fusion weight default value functions" section (lines 424-449).

`InferenceConfig::validate()` uses two mechanisms for the existing six fields:
1. Per-field range check via `fusion_weight_checks: &[(&'static str, f64)]` slice (lines 576-593)
2. Six-field sum constraint check (lines 596-611)

`InferenceConfig::Default::default()` provides an explicit struct literal (lines 356-388)
that lists all six weight fields. New fields must be added here.

There is also a merged-config construction in the file (around lines 1541-1574) that
merges project + global config using epsilon comparison; new fields need the same treatment.

---

## Modifications to `InferenceConfig` struct

### New fields (add after `w_prov` field, before the closing `}` of the struct)

```
// crt-026: Session context histogram weight (WA-2, ADR-004).
// Additive term outside the six-weight sum constraint; sum goes 0.95 → 0.97 with defaults.
// W3-1 cold-start seed value: 0.02 (ASS-028 calibrated value, full session signal budget).
#[serde(default = "default_w_phase_histogram")]
pub w_phase_histogram: f64,

// crt-026: Explicit phase term weight (WA-2, ADR-003).
// Reserved at 0.0; W3-1 placeholder. Will carry explicit phase signal once W3-1 is trained.
// Not part of the six-weight sum constraint.
#[serde(default = "default_w_phase_explicit")]
pub w_phase_explicit: f64,
```

---

## Modifications to `InferenceConfig::Default::default()`

Add the two new fields to the struct literal after `w_prov: 0.05`:

```
w_phase_histogram: 0.02,   // crt-026: full session signal budget (ADR-004)
w_phase_explicit: 0.0,     // crt-026: W3-1 placeholder (ADR-003)
```

---

## New default value functions

Add to the "Fusion weight default value functions" section after `default_w_prov`:

```
// crt-026: default_w_phase_histogram — 0.02 (ASS-028 calibrated, full session signal budget)
fn default_w_phase_histogram() -> f64 {
    0.02
}

// crt-026: default_w_phase_explicit — 0.0 (W3-1 placeholder, ADR-003)
fn default_w_phase_explicit() -> f64 {
    0.0
}
```

---

## Modifications to `InferenceConfig::validate()`

### Where to insert

After the existing `fusion_weight_checks` slice and its loop (after line 593, before the
six-field sum check at line 596), add a separate range check slice for the two new fields:

```
// crt-026: Per-field range checks for phase weight fields [0.0, 1.0] (ADR-004, R-11).
// These fields are NOT included in the six-weight sum constraint check below.
let phase_weight_checks: &[(&'static str, f64)] = &[
    ("w_phase_histogram", self.w_phase_histogram),
    ("w_phase_explicit",  self.w_phase_explicit),
];

for (field, value) in phase_weight_checks {
    if *value < 0.0 || *value > 1.0 {
        return Err(ConfigError::FusionWeightOutOfRange {
            path: path.to_path_buf(),
            field: field,       // name matches the pattern of existing error (see existing for exact type)
            value: *value,
        });
    }
}
```

NOTE: The existing `FusionWeightOutOfRange` error variant and the existing range-check loop
use the same `ConfigError::FusionWeightOutOfRange` error type. Inspect the current error
variant definition to confirm the exact field names; the pattern is identical to existing
per-field checks for `w_sim`, `w_nli`, etc.

### What is NOT changed

The six-weight sum check remains exactly as-is:
```
let fusion_weight_sum =
    self.w_sim + self.w_nli + self.w_conf + self.w_coac + self.w_util + self.w_prov;
if fusion_weight_sum > 1.0 { ... }
```

`w_phase_histogram` and `w_phase_explicit` are NOT added to this sum. The existing
sum of 0.95 continues to pass. Total including phase fields = 0.97, within `<= 1.0`.

---

## Modifications to merged-config construction

The file contains a project+global config merge block (around lines 1541-1574) that uses
epsilon comparison to select the overridden field per weight. Add the two new fields:

```
w_phase_histogram: if (project.inference.w_phase_histogram
                       - default.inference.w_phase_histogram).abs() > f64::EPSILON {
    project.inference.w_phase_histogram
} else {
    global.inference.w_phase_histogram
},
w_phase_explicit: if (project.inference.w_phase_explicit
                      - default.inference.w_phase_explicit).abs() > f64::EPSILON {
    project.inference.w_phase_explicit
} else {
    global.inference.w_phase_explicit
},
```

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `w_phase_histogram = 1.5` | `validate()` returns `Err(ConfigError::FusionWeightOutOfRange)` |
| `w_phase_explicit = -0.1` | `validate()` returns `Err(ConfigError::FusionWeightOutOfRange)` |
| `w_phase_histogram = 0.02` (default) | `validate()` passes |
| `w_phase_explicit = 0.0` (default) | `validate()` passes |
| TOML without new fields | `#[serde(default)]` applies; fields get 0.02 and 0.0 respectively |

---

## Key Test Scenarios

See `test-plan/config.md` for the full test plan. Key scenarios:

1. **AC-09 / R-11 (gate blocker)**: `InferenceConfig::default()` returns
   `w_phase_histogram = 0.02` and `w_phase_explicit = 0.0`.

2. **R-11**: `w_phase_histogram = 1.5` → `validate()` returns error naming `"w_phase_histogram"`.

3. **R-11**: `w_phase_explicit = -0.1` → `validate()` returns error naming `"w_phase_explicit"`.

4. **Backward compat (FM-04)**: TOML without the new fields deserializes correctly;
   new fields receive their serde defaults.

5. **Sum invariant**: `w_phase_histogram = 0.02` with default six-field weights (sum = 0.95)
   does NOT cause the six-field sum check to fail (0.95 < 1.0; phase fields excluded).

6. **Existing tests must still pass**: `test_inference_config_default_weights_sum_within_headroom`
   sums only the six original fields; the sum assertion `<= 0.95 + 1e-9` remains valid because
   phase fields are not in that sum expression.
