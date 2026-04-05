# Pseudocode: nan-guards (Item 3)

## Purpose

Prefix all 19 float field guards in `InferenceConfig::validate()` with `!v.is_finite() || `
(Group A) or `!value.is_finite() || ` (Groups B/C). This closes the IEEE 754 NaN trap:
`NaN <= 0.0` and `NaN >= 1.0` both evaluate to false, so comparison-only guards silently
pass NaN. The prefix converts each guard to a finite-then-compare form that catches NaN and
both signed infinities before they reach any scoring pipeline.

## File

`crates/unimatrix-server/src/infra/config.rs`

## Scope

19 guard modifications in `InferenceConfig::validate()`. No new error variants. No changes
to cross-field invariant checks, other config structs, or integer field guards.

---

## Background: IEEE 754 NaN Trap

```
NaN <= 0.0   → false   (NaN comparison is always false)
NaN >= 1.0   → false   (NaN comparison is always false)
NaN < 0.0    → false
NaN > 1.0    → false

Therefore: if v <= 0.0 || v >= 1.0 { ... }  does NOT fire when v = NaN.

Fix: if !v.is_finite() || v <= 0.0 || v >= 1.0 { ... }
     ^^^^^^^^^^^^^^^^^^^
     !v.is_finite() is true for NaN, +Inf, -Inf — all three non-finite cases.
```

The three crt-046 fields (`goal_cluster_similarity_threshold`, `w_goal_cluster_conf`,
`w_goal_boost`) at lines 1381–1411 already have `!v.is_finite()` guards (PR #516,
lesson #4132). These must NOT be modified again.

---

## Modified Function: `InferenceConfig::validate`

### Group A — 11 Individual Field Guards (inline `let v` pattern)

For each Group A field, the transformation is:

```
BEFORE:
    if self.<field> <= 0.0 || self.<field> >= 1.0 {
        return Err(ConfigError::NliFieldOutOfRange { ... });
    }

AFTER:
    let v = self.<field>;
    if !v.is_finite() || v <= 0.0 || v >= 1.0 {
        return Err(ConfigError::NliFieldOutOfRange {
            path: path.to_path_buf(),
            field: "<field>",
            value: v.to_string(),
            reason: "must be in range (0.0, 1.0) exclusive",
        });
    }
```

For fields with `< 0.0 || > 1.0` (inclusive bounds), the comparison operators are preserved:

```
AFTER (inclusive bound variant):
    let v = self.<field>;
    if !v.is_finite() || v < 0.0 || v > 1.0 {
        return Err(ConfigError::NliFieldOutOfRange { ... });
    }
```

Complete Group A — all 11 fields with their current guard forms and source line ranges:

| # | Field | Type | Current guard (comparison only) | Line range (approx) |
|---|-------|------|----------------------------------|---------------------|
| 1 | `nli_entailment_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | ~1028–1035 |
| 2 | `nli_contradiction_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | ~1037–1044 |
| 3 | `nli_auto_quarantine_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | ~1046–1053 |
| 4 | `supports_candidate_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | ~1089–1096 |
| 5 | `supports_edge_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | ~1098–1106 |
| 6 | `ppr_alpha` | f64 | `<= 0.0 \|\| >= 1.0` | ~1221–1228 |
| 7 | `ppr_inclusion_threshold` | f64 | `<= 0.0 \|\| >= 1.0` | ~1241–1248 |
| 8 | `ppr_blend_weight` | f64 | `< 0.0 \|\| > 1.0` | ~1250–1258 |
| 9 | `nli_informs_cosine_floor` | f32 | `<= 0.0 \|\| >= 1.0` | ~1282–1289 |
| 10 | `nli_informs_ppr_weight` | f64 | `< 0.0 \|\| > 1.0` | ~1292–1299 |
| 11 | `supports_cosine_threshold` | f32 | `<= 0.0 \|\| >= 1.0` | ~1301–1309 |

Each field introduces one `let v = self.<field>;` binding before its existing `if` statement.
The `value: self.<field>.to_string()` in the existing error struct is replaced with
`value: v.to_string()`. No other struct fields change.

### Group B — 6 Fusion Weight Fields (loop-body guard)

The `fusion_weight_checks` array and loop are at lines 1151–1169. The array is a
`&[(&'static str, f64)]` slice constructed with tuple literals:

```rust
let fusion_weight_checks: &[(&'static str, f64)] = &[
    ("w_sim",  self.w_sim),
    ("w_nli",  self.w_nli),
    ("w_conf", self.w_conf),
    ("w_coac", self.w_coac),
    ("w_util", self.w_util),
    ("w_prov", self.w_prov),
];
```

The loop iterates over this slice:

```rust
for (field, value) in fusion_weight_checks {
    // value here has type &f64 (reference to tuple element)
    // *value dereferences to f64
    // value.is_finite() auto-derefs through &f64 to call f64::is_finite()
```

BEFORE:
```rust
for (field, value) in fusion_weight_checks {
    if *value < 0.0 || *value > 1.0 {
        return Err(ConfigError::NliFieldOutOfRange {
            path: path.to_path_buf(),
            field,
            value: value.to_string(),
            reason: "fusion weight must be in range [0.0, 1.0]",
        });
    }
}
```

AFTER:
```rust
for (field, value) in fusion_weight_checks {
    if !value.is_finite() || *value < 0.0 || *value > 1.0 {
        return Err(ConfigError::NliFieldOutOfRange {
            path: path.to_path_buf(),
            field,
            value: value.to_string(),
            reason: "fusion weight must be in range [0.0, 1.0]",
        });
    }
}
```

Only one token added: `!value.is_finite() || ` before the existing comparison.
`value.to_string()` already produces `"NaN"` when `*value` is NaN — no format change needed.

Implementation note: `value` in this loop is `&f64`. `value.is_finite()` calls
`<f64 as Float>::is_finite` via auto-deref. `*value < 0.0` continues to use explicit deref
for the comparison, consistent with the existing guard. Both forms are correct; the
implementor should verify that the compiled form produces no Clippy warnings.

Fields covered by Group B: `w_sim` (#12), `w_nli` (#13), `w_conf` (#14), `w_coac` (#15),
`w_util` (#16), `w_prov` (#17).

### Group C — 2 Phase Weight Fields (loop-body guard)

The `phase_weight_checks` array and loop are at lines 1173–1187. Same slice type as Group B.

```rust
let phase_weight_checks: &[(&'static str, f64)] = &[
    ("w_phase_histogram", self.w_phase_histogram),
    ("w_phase_explicit",  self.w_phase_explicit),
];
```

BEFORE:
```rust
for (field, value) in phase_weight_checks {
    if *value < 0.0 || *value > 1.0 {
        return Err(ConfigError::NliFieldOutOfRange {
            path: path.to_path_buf(),
            field,
            value: value.to_string(),
            reason: "fusion weight must be in range [0.0, 1.0]",
        });
    }
}
```

AFTER:
```rust
for (field, value) in phase_weight_checks {
    if !value.is_finite() || *value < 0.0 || *value > 1.0 {
        return Err(ConfigError::NliFieldOutOfRange {
            path: path.to_path_buf(),
            field,
            value: value.to_string(),
            reason: "fusion weight must be in range [0.0, 1.0]",
        });
    }
}
```

Identical transformation to Group B. Fields covered: `w_phase_histogram` (#18),
`w_phase_explicit` (#19).

### What Must Not Change

- Integer field guards (usize, u32): `nli_top_k`, `max_contradicts_per_tick`,
  `max_graph_inference_per_tick`, etc. — not subject to IEEE 754 NaN (C-09).
- crt-046 fields already guarded: `goal_cluster_similarity_threshold`, `w_goal_cluster_conf`,
  `w_goal_boost` at lines 1381–1411 — do NOT add `!v.is_finite()` a second time.
- Cross-field invariant checks (lines 1080–1086, 1110–1116) — not modified.
- `FusionWeightSumExceeded` sum check (lines 1190–1204) — not modified; remains as second
  line of defence after per-field NaN is already caught upstream.
- `ConfigError::NliFieldOutOfRange` variant definition — not modified (no new error variants).

---

## Ordering Constraint

For Group A fields, the `let v = self.<field>;` binding must immediately precede its `if`
statement. Do not introduce a binding before an unrelated block. The pattern established
by crt-046 (lines 1381, 1392, 1403) should be followed exactly.

---

## Error Handling

`InferenceConfig::validate()` returns `Result<(), ConfigError>`. All 19 new guards return
`Err(ConfigError::NliFieldOutOfRange { ... })` using the established variant. No new error
type, no new variant, no panic path.

When `v = NaN`: `v.to_string()` produces `"NaN"` — a valid field string for the error
message.
When `v = f32::INFINITY`: `v.to_string()` produces `"inf"`.
When `v = f32::NEG_INFINITY`: `v.to_string()` produces `"-inf"`.
All three string representations are valid as the `value` field in `NliFieldOutOfRange`.

---

## Key Test Scenarios

All 21 test functions are required (R-03, R-06). Test pattern follows lines 8004–8094 in
`config.rs` (crt-046 NaN tests).

### Test Pattern (Group A — f32 field)

```
GIVEN: InferenceConfig::default() with one field set to f32::NAN
WHEN:  validate() is called
THEN:  returns Err(ConfigError::NliFieldOutOfRange { field: "<field_name>", ... })
AND:   err.to_string().contains("<field_name>")

fn test_nan_guard_nli_entailment_threshold() {
    let mut c = InferenceConfig::default();
    c.nli_entailment_threshold = f32::NAN;
    assert_validate_fails_with_field(&c, "nli_entailment_threshold");
}
```

### Test Pattern (Group B/C — f64 loop field)

```
GIVEN: InferenceConfig::default() with one loop field set to f64::NAN
WHEN:  validate() is called
THEN:  returns Err(ConfigError::NliFieldOutOfRange { field: "<field_name>", ... })

fn test_nan_guard_w_sim() {
    let mut c = InferenceConfig::default();
    c.w_sim = f64::NAN;
    assert_validate_fails_with_field(&c, "w_sim");
}
```

The field name string in `assert_validate_fails_with_field(c, "<field_name>")` must
exactly match the `&'static str` in the `fusion_weight_checks` / `phase_weight_checks`
array entries — because the error's `field` value comes from that array string.

### Required Test Function Names (Gate 3a verified by name — all 21)

Group A NaN tests (11):
- `test_nan_guard_nli_entailment_threshold` (AC-06)
- `test_nan_guard_nli_contradiction_threshold` (AC-07)
- `test_nan_guard_nli_auto_quarantine_threshold` (AC-08)
- `test_nan_guard_supports_candidate_threshold` (AC-09)
- `test_nan_guard_supports_edge_threshold` (AC-10)
- `test_nan_guard_ppr_alpha` (AC-11)
- `test_nan_guard_ppr_inclusion_threshold` (AC-12)
- `test_nan_guard_ppr_blend_weight` (AC-13)
- `test_nan_guard_nli_informs_cosine_floor` (AC-14)
- `test_nan_guard_nli_informs_ppr_weight` (AC-15)
- `test_nan_guard_supports_cosine_threshold` (AC-16)

Group B NaN tests (6):
- `test_nan_guard_w_sim` (AC-17)
- `test_nan_guard_w_nli` (AC-18)
- `test_nan_guard_w_conf` (AC-19)
- `test_nan_guard_w_coac` (AC-20)
- `test_nan_guard_w_util` (AC-21)
- `test_nan_guard_w_prov` (AC-22)

Group C NaN tests (2):
- `test_nan_guard_w_phase_histogram` (AC-23)
- `test_nan_guard_w_phase_explicit` (AC-24)

Representative Inf tests (2):
- `test_inf_guard_nli_entailment_threshold_f32` (AC-25): `c.nli_entailment_threshold = f32::INFINITY`
- `test_inf_guard_ppr_alpha_f64` (AC-26): `c.ppr_alpha = f64::INFINITY`

Regression guard (AC-27): all pre-existing `InferenceConfig::validate()` tests must pass.
Specifically confirm `w_sim` boundary values: 0.0 valid, 0.5 valid, -0.1 invalid, 1.1 invalid.

---

## Risks Addressed

- R-03: All 19 fields are individually enumerated. No sampling. Each field tested by name.
- R-07: Test field name strings must match the array entry strings in the source. For loop
  fields, the array string is the canonical name. Mismatch causes `assert_validate_fails_with_field`
  to fail (error string does not contain the wrong field name).
- R-10: Adding `!v.is_finite()` to the loop guard does not change dereference behavior for
  valid (finite) values. AC-27 boundary tests (w_sim 0.0, 0.5, -0.1, 1.1) are the regression
  check.
- R-12: Per-field `!v.is_finite()` guard fires before the cross-field invariant check runs,
  so cross-field NaN pass-through is prevented at the individual-field stage.

## Knowledge Stewardship

- Lesson #4132 (entry #4132): NaN trap pattern for InferenceConfig — established the
  `!v.is_finite()` prefix form and `ConfigError::NliFieldOutOfRange` as the correct variant.
  This pseudocode applies that pattern to the 16 remaining unguarded fields.
- crt-046 guards (lines 1381–1411) are the implementation reference — the exact same
  `let v = self.<field>; if !v.is_finite() || ...` form.
- Deviations from established patterns: none.
