# Test Plan: Profile Types (`eval/profile/types.rs`)

Component 1 of 7.

---

## Scope

This component adds `DistributionTargets` (new struct) and extends `EvalProfile` with two new
fields. It has no serde derives and no file I/O. All behavior is structural — tests verify
field types, defaults, and presence on instances produced by Component 2 (validation).

There are no standalone unit tests for `types.rs` itself because the structs have no
associated logic. Coverage comes through Component 2's parse tests, which produce instances
of the new types.

---

## Unit Test Expectations

### Via `eval/profile/tests.rs` (Component 2 tests exercise Component 1 types)

**`test_parse_distribution_change_profile_valid`** (AC-01, R-02)
- Arrange: TOML string with `[profile]` containing `distribution_change = true` and a valid
  `[profile.distribution_targets]` table (`cc_at_k_min = 0.60`, `icd_min = 1.20`,
  `mrr_floor = 0.35`).
- Act: call `parse_profile_toml` with the TOML string.
- Assert:
  - `result.is_ok()` is `true`
  - `profile.distribution_change == true`
  - `profile.distribution_targets` is `Some(t)` where:
    - `t.cc_at_k_min == 0.60_f64`
    - `t.icd_min == 1.20_f64`
    - `t.mrr_floor == 0.35_f64`

**`test_parse_no_distribution_change_flag`** (AC-04)
- Arrange: TOML string with `[profile]` that has no `distribution_change` key at all.
- Act: call `parse_profile_toml`.
- Assert:
  - `result.is_ok()` is `true`
  - `profile.distribution_change == false`
  - `profile.distribution_targets.is_none()` is `true`

**`test_parse_distribution_change_false_explicit`** (AC-04 variant — existing behavior guard)
- Arrange: TOML with `distribution_change = false` explicitly set.
- Act: call `parse_profile_toml`.
- Assert: same as no-flag case — `distribution_change = false`, `distribution_targets = None`.

---

## Structural Assertions (gate-3b static checks)

1. `DistributionTargets` has exactly three fields: `cc_at_k_min: f64`, `icd_min: f64`,
   `mrr_floor: f64`. No serde derives on this type.
2. `EvalProfile` has new fields `distribution_change: bool` (default `false`) and
   `distribution_targets: Option<DistributionTargets>`. No other fields on `EvalProfile`
   are changed or removed.
3. Field count of `ScenarioResult` in both `runner/output.rs` and `report/mod.rs` is
   unchanged from pre-nan-010 (dual-type constraint, R-15).

---

## Edge Cases

| Scenario | Expected Behavior |
|----------|------------------|
| `distribution_change = true` with no targets | Validation error from Component 2 — never reaches types as a valid instance |
| `cc_at_k_min = 0.0` | Valid `DistributionTargets` instance; zero floor is a user choice |
| `mrr_floor = 1.5` (> 1.0) | Valid `DistributionTargets` instance; out-of-range values not rejected at parse time |
| `distribution_change` key absent | `distribution_change` defaults to `false`; `distribution_targets` is `None` |

---

## Risks Covered

- R-02 (extraction order): `test_parse_distribution_change_profile_valid` fails if
  `distribution_change` is read after the `[profile]` strip (returns `false` instead of `true`).
- R-15 (dual-type): structural field-count check at gate-3b catches any `ScenarioResult`
  modification.
