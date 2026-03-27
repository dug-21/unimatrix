# Test Plan: Profile Validation (`eval/profile/validation.rs`)

Component 2 of 7.

---

## Scope

`parse_profile_toml` is extended to extract `distribution_change` and `distribution_targets`
from the raw TOML value before the `[profile]` section is stripped for `UnimatrixConfig`
deserialization. This component owns all parse-time validation for the new fields.

All tests are in `eval/profile/tests.rs`.

---

## Unit Test Expectations

### `test_parse_distribution_change_profile_valid` (AC-01, R-02)

Critical test. Validates the extraction-before-strip ordering and successful round-trip from
TOML to `EvalProfile`.

- Arrange:
  ```toml
  [profile]
  name = "ppr-candidate"
  description = "Distribution change profile"
  distribution_change = true

  [profile.distribution_targets]
  cc_at_k_min = 0.60
  icd_min = 1.20
  mrr_floor = 0.35

  [unimatrix]
  # ... minimal valid UnimatrixConfig fields ...
  ```
- Act: `parse_profile_toml(&toml_string, is_baseline: false)`
- Assert:
  - `Ok(profile)` returned
  - `profile.distribution_change == true`
  - `profile.distribution_targets == Some(DistributionTargets { cc_at_k_min: 0.60, icd_min: 1.20, mrr_floor: 0.35 })`
  - No extraction-order regression: if this test fails with `distribution_change = false`, the
    extraction is happening after `[profile]` strip.

---

### `test_parse_distribution_change_missing_targets` (AC-02, R-02)

- Arrange: TOML with `distribution_change = true` but no `[profile.distribution_targets]`
  sub-table.
- Act: `parse_profile_toml`
- Assert:
  - `Err(EvalError::ConfigInvariant(msg))`
  - `msg.contains("distribution_targets")` is `true`
  - `msg` names the missing section (not a generic error)

---

### `test_parse_distribution_change_missing_cc_at_k` (AC-03)

- Arrange: TOML with `distribution_change = true` and `[profile.distribution_targets]`
  containing only `icd_min` and `mrr_floor` (no `cc_at_k_min`).
- Act: `parse_profile_toml`
- Assert:
  - `Err(EvalError::ConfigInvariant(msg))`
  - `msg.contains("cc_at_k_min")` is `true`

---

### `test_parse_distribution_change_missing_icd` (AC-03)

- Arrange: TOML with `distribution_change = true` and `[profile.distribution_targets]`
  containing only `cc_at_k_min` and `mrr_floor` (no `icd_min`).
- Act: `parse_profile_toml`
- Assert:
  - `Err(EvalError::ConfigInvariant(msg))`
  - `msg.contains("icd_min")` is `true`

---

### `test_parse_distribution_change_missing_mrr_floor` (AC-03)

- Arrange: TOML with `distribution_change = true` and `[profile.distribution_targets]`
  containing only `cc_at_k_min` and `icd_min` (no `mrr_floor`).
- Act: `parse_profile_toml`
- Assert:
  - `Err(EvalError::ConfigInvariant(msg))`
  - `msg.contains("mrr_floor")` is `true`

---

### `test_parse_no_distribution_change_flag` (AC-04)

- Arrange: TOML with a valid `[profile]` section that has no `distribution_change` key.
- Act: `parse_profile_toml`
- Assert:
  - `Ok(profile)` returned
  - `profile.distribution_change == false`
  - `profile.distribution_targets.is_none()`
  - Existing tests for other fields continue to pass (no regression)

---

## Additional Test (not in non-negotiable list, but needed for completeness)

### `test_parse_distribution_change_baseline_rejected` (R-03)

Note: this test covers the baseline-rejection invariant. It may live in `profile/tests.rs`
or `report/tests_distribution_gate.rs`. The non-negotiable name for this scenario is
`test_distribution_gate_baseline_rejected` (in `tests_distribution_gate.rs`). However, since
validation occurs in `parse_profile_toml`, a profile-level test is also appropriate.

- Arrange: TOML with `distribution_change = true` and valid targets; call with
  `is_baseline = true` (or however the baseline flag is conveyed to `parse_profile_toml`).
- Act: `parse_profile_toml`
- Assert:
  - `Err(EvalError::ConfigInvariant(msg))`
  - `msg.contains("baseline profile must not declare")` is `true`
  - `msg.contains("distribution_change = true")` is `true`

---

## Integration Test Expectations

No infra-001 integration tests required. Profile validation is entirely within the eval
binary and is fully exercised by unit tests.

---

## Extraction-Order Invariant

The implementation MUST call `raw.get("profile")` to extract `distribution_change` and
`distribution_targets` before any call to `remove("profile")` or equivalent stripping. The
test `test_parse_distribution_change_profile_valid` is the primary regression guard: if
extraction happens after stripping, `distribution_change` would parse as `false` (absent =
default), causing `test_parse_distribution_change_missing_targets` to return `Ok` when it
should return `Err`. Tracing this failure immediately identifies the extraction-order bug.

---

## Risks Covered

| Risk | Test |
|------|------|
| R-02 (extraction after strip) | `test_parse_distribution_change_profile_valid` |
| R-02 (silent drop) | `test_parse_distribution_change_missing_targets` |
| R-03 (baseline silent accept) | `test_distribution_gate_baseline_rejected` |
| AC-03 (per-field error messages) | Three separate missing-field tests |
| NFR-06 (human-readable errors) | All ConfigInvariant tests assert `msg` names the item |
