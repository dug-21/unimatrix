# Test Plan: `InferenceConfig::validate()` NaN Guards (Item 3) — AC-06 through AC-27

## Component

`InferenceConfig::validate()` in `crates/unimatrix-server/src/infra/config.rs`

## Risks Covered

| Risk | Priority | ACs |
|------|----------|-----|
| R-03: Missing per-field NaN coverage | Critical | AC-06..AC-24 (all 19 NaN tests, all mandatory) |
| R-06: Test module absent | High | Gate 3a presence-count = 19 |
| R-07: Wrong field name string, vacuous pass | High | Spot-check AC-17..AC-24 |
| R-10: Existing boundary-value tests regress | Med | AC-27 |
| R-12: Cross-field invariant NaN pass-through | Low | Covered by AC-07 + AC-08 upstream |

---

## Test Helper

All 21 new NaN/Inf tests use the existing helper at line 4615 in `config.rs`:

```rust
fn assert_validate_fails_with_field(config: InferenceConfig, field_name: &str) {
    let err = config.validate(Path::new("/fake")).unwrap_err();
    assert!(
        err.to_string().contains(field_name),
        "Error message must name the offending field '{field_name}'; got: '{err}'"
    );
}
```

Note: the helper signature takes `InferenceConfig` by value (or reference — verify at the
actual call site). Use it exactly as established for crt-046 tests (lines 8004–8094).

**Pattern** (identical for all 19 NaN tests):
```rust
#[test]
fn test_nan_guard_<field>() {
    let mut c = InferenceConfig::default();
    c.<field> = f32::NAN;   // or f64::NAN for f64 fields
    assert_validate_fails_with_field(c, "<field>");
}
```

---

## Group A — Individual Threshold Fields (11 tests, inline guard pattern)

These fields use the `let v = self.<field>;` inline pattern. Each guard becomes
`!v.is_finite() || <existing comparison>`.

### AC-06: `test_nan_guard_nli_entailment_threshold`
- Set `c.nli_entailment_threshold = f32::NAN`
- Assert `assert_validate_fails_with_field(c, "nli_entailment_threshold")`
- Field type: `f32`. Current guard: `<= 0.0 || >= 1.0`

### AC-07: `test_nan_guard_nli_contradiction_threshold`
- Set `c.nli_contradiction_threshold = f32::NAN`
- Assert `assert_validate_fails_with_field(c, "nli_contradiction_threshold")`
- Field type: `f32`. Current guard: `<= 0.0 || >= 1.0`

### AC-08: `test_nan_guard_nli_auto_quarantine_threshold`
- Set `c.nli_auto_quarantine_threshold = f32::NAN`
- Assert `assert_validate_fails_with_field(c, "nli_auto_quarantine_threshold")`
- Field type: `f32`. Current guard: `<= 0.0 || >= 1.0`
- R-12 note: AC-08 plus AC-07 together ensure cross-field NaN invariant check is not the
  only defence for these fields.

### AC-09: `test_nan_guard_supports_candidate_threshold`
- Set `c.supports_candidate_threshold = f32::NAN`
- Assert `assert_validate_fails_with_field(c, "supports_candidate_threshold")`
- Field type: `f32`. Current guard: `<= 0.0 || >= 1.0`

### AC-10: `test_nan_guard_supports_edge_threshold`
- Set `c.supports_edge_threshold = f32::NAN`
- Assert `assert_validate_fails_with_field(c, "supports_edge_threshold")`
- Field type: `f32`. Current guard: `<= 0.0 || >= 1.0`

### AC-11: `test_nan_guard_ppr_alpha`
- Set `c.ppr_alpha = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "ppr_alpha")`
- Field type: `f64`. Current guard: `<= 0.0 || >= 1.0`

### AC-12: `test_nan_guard_ppr_inclusion_threshold`
- Set `c.ppr_inclusion_threshold = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "ppr_inclusion_threshold")`
- Field type: `f64`. Current guard: `<= 0.0 || >= 1.0`

### AC-13: `test_nan_guard_ppr_blend_weight`
- Set `c.ppr_blend_weight = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "ppr_blend_weight")`
- Field type: `f64`. Current guard: `< 0.0 || > 1.0` (inclusive range — note difference)

### AC-14: `test_nan_guard_nli_informs_cosine_floor`
- Set `c.nli_informs_cosine_floor = f32::NAN`
- Assert `assert_validate_fails_with_field(c, "nli_informs_cosine_floor")`
- Field type: `f32`. Current guard: `<= 0.0 || >= 1.0`

### AC-15: `test_nan_guard_nli_informs_ppr_weight`
- Set `c.nli_informs_ppr_weight = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "nli_informs_ppr_weight")`
- Field type: `f64`. Current guard: `< 0.0 || > 1.0` (inclusive range)

### AC-16: `test_nan_guard_supports_cosine_threshold`
- Set `c.supports_cosine_threshold = f32::NAN`
- Assert `assert_validate_fails_with_field(c, "supports_cosine_threshold")`
- Field type: `f32`. Current guard: `<= 0.0 || >= 1.0`

---

## Group B — Fusion Weight Fields (6 tests, loop-body guard pattern)

These fields use the `fusion_weight_checks` loop. The guard inside the loop becomes
`!value.is_finite() || *value < 0.0 || *value > 1.0`.

**Important**: The field name in the error (`NliFieldOutOfRange.field`) comes from the
`&'static str` entry in the `fusion_weight_checks` array. The test's second argument to
`assert_validate_fails_with_field` must match that exact string literal. Spot-check required
for these tests — see R-07 mitigation below.

### AC-17: `test_nan_guard_w_sim`
- Set `c.w_sim = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "w_sim")`
- Field type: `f64`. Loop guard: `< 0.0 || > 1.0`

### AC-18: `test_nan_guard_w_nli`
- Set `c.w_nli = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "w_nli")`

### AC-19: `test_nan_guard_w_conf`
- Set `c.w_conf = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "w_conf")`

### AC-20: `test_nan_guard_w_coac`
- Set `c.w_coac = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "w_coac")`

### AC-21: `test_nan_guard_w_util`
- Set `c.w_util = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "w_util")`

### AC-22: `test_nan_guard_w_prov`
- Set `c.w_prov = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "w_prov")`

---

## Group C — Phase Weight Fields (2 tests, loop-body guard pattern)

These fields use the `phase_weight_checks` loop. Same guard form as Group B.

### AC-23: `test_nan_guard_w_phase_histogram`
- Set `c.w_phase_histogram = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "w_phase_histogram")`
- Field name must match the `&'static str` in the `phase_weight_checks` array exactly.

### AC-24: `test_nan_guard_w_phase_explicit`
- Set `c.w_phase_explicit = f64::NAN`
- Assert `assert_validate_fails_with_field(c, "w_phase_explicit")`
- Field name must match the `&'static str` in the `phase_weight_checks` array exactly.

---

## Representative Inf Tests (2 tests)

### AC-25: `test_inf_guard_nli_entailment_threshold_f32`
- Set `c.nli_entailment_threshold = f32::INFINITY`
- Assert `assert_validate_fails_with_field(c, "nli_entailment_threshold")`
- Rationale: `!v.is_finite()` catches both NAN and INFINITY. This is a representative f32
  Inf test confirming the `is_finite()` guard handles non-NaN non-finite values.
- Note: `f32::NEG_INFINITY` is also caught by `!v.is_finite()`; no separate negative-Inf
  test is required. Document in gate report.

### AC-26: `test_inf_guard_ppr_alpha_f64`
- Set `c.ppr_alpha = f64::INFINITY`
- Assert `assert_validate_fails_with_field(c, "ppr_alpha")`
- Rationale: representative f64 Inf test, paired with AC-25 f32 Inf test.

---

## Pre-Existing Regression Guard (AC-27)

### AC-27: Pre-existing boundary tests pass after Item 3 changes

**Verification**: Shell command, not a named test function.

```bash
cargo test -p unimatrix-server -- infra::config 2>&1 | tail -20
```

All pre-existing `InferenceConfig::validate()` tests must pass with zero new failures.

**Specific boundary check** — `w_sim` (most likely to break if loop-body dereference is
changed incorrectly):
- Valid inputs (must pass validate): `w_sim = 0.0`, `w_sim = 0.5`
- Invalid inputs (must fail validate): `w_sim = -0.1`, `w_sim = 1.1`

If these four boundary assertions fail after Item 3 changes, the loop-body guard dereference
is incorrect. This is R-10.

---

## R-07 Mitigation: Field Name Spot-Check (required at Gate 3a)

For Group B and Group C loop fields (AC-17 through AC-24), the field name in the error
message comes from the `&'static str` in the check arrays, not from the struct field name
directly. A mismatch between the test string and the array entry produces a vacuous pass.

**Spot-check procedure** (required before Gate 3a sign-off):

1. Read the `fusion_weight_checks` array in `InferenceConfig::validate()` and confirm the
   `&'static str` entries for each field. Verify that `"w_sim"`, `"w_nli"`, `"w_conf"`,
   `"w_coac"`, `"w_util"`, `"w_prov"`, `"w_phase_histogram"`, `"w_phase_explicit"` are the
   exact values used.
2. Confirm that the test strings in AC-17 through AC-24 match those exact strings.
3. Document in gate report: "Field name strings for AC-17..AC-24 verified against
   `fusion_weight_checks` / `phase_weight_checks` array entries."

---

## Gate 3a Presence Verification

**Required before marking delivery complete**: Verify all 21 test function names are present
in `config.rs` by searching the `#[cfg(test)]` module:

Group A (11): `test_nan_guard_nli_entailment_threshold`, `test_nan_guard_nli_contradiction_threshold`,
`test_nan_guard_nli_auto_quarantine_threshold`, `test_nan_guard_supports_candidate_threshold`,
`test_nan_guard_supports_edge_threshold`, `test_nan_guard_ppr_alpha`,
`test_nan_guard_ppr_inclusion_threshold`, `test_nan_guard_ppr_blend_weight`,
`test_nan_guard_nli_informs_cosine_floor`, `test_nan_guard_nli_informs_ppr_weight`,
`test_nan_guard_supports_cosine_threshold`

Group B (6): `test_nan_guard_w_sim`, `test_nan_guard_w_nli`, `test_nan_guard_w_conf`,
`test_nan_guard_w_coac`, `test_nan_guard_w_util`, `test_nan_guard_w_prov`

Group C (2): `test_nan_guard_w_phase_histogram`, `test_nan_guard_w_phase_explicit`

Inf (2): `test_inf_guard_nli_entailment_threshold_f32`, `test_inf_guard_ppr_alpha_f64`

Count: 11 + 6 + 2 + 2 = **21 tests**. Count must equal 21 before Gate 3a passes.

A test that fails to compile does not satisfy the presence requirement.

---

## Out-of-Scope Fields (must NOT be modified)

The following are explicitly excluded from Item 3 (C-09, C-10):

- `usize` and `u32` fields: `nli_top_k`, `max_contradicts_per_tick`,
  `max_graph_inference_per_tick` — not subject to IEEE 754 NaN.
- crt-046 fields already guarded in PR #516: `goal_cluster_similarity_threshold`,
  `w_goal_cluster_conf`, `w_goal_boost` — must NOT receive a second `!v.is_finite()` prefix.
- Fields in `RetentionConfig` or `CoherenceConfig` — different structs, not in scope.

Stage 3c tester must verify the diff touches only the 19 listed fields in
`InferenceConfig::validate()` and does not touch any excluded field.
