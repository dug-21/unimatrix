# Test Plan: Runner Profile Meta Sidecar (`eval/runner/profile_meta.rs`)

Component 3 of 7.

---

## Scope

New module `eval/runner/profile_meta.rs`. Responsible for:
1. Defining `ProfileMetaFile`, `ProfileMetaEntry`, `DistributionTargetsJson` (serde types).
2. Implementing `write_profile_meta(profiles: &[EvalProfile], out: &Path) -> Result<(), EvalError>`.
3. Atomic write protocol: `{out}/profile-meta.json.tmp` → rename → `{out}/profile-meta.json`.

Tests are in `eval/report/tests_distribution_gate.rs`.

---

## Unit Test Expectations

### `test_write_profile_meta_schema` (AC-05, R-04, R-10)

Primary test for this component. Exercises the full write path and validates JSON schema.
Also the primary defense for R-10 (schema mismatch between writer and reader).

- Arrange:
  - Create a temp directory.
  - Construct `EvalProfile` instances:
    - Profile `"ppr-candidate"` with `distribution_change = true`, targets
      `{ cc_at_k_min: 0.60, icd_min: 1.20, mrr_floor: 0.35 }`.
    - Profile `"baseline"` with `distribution_change = false`, targets `None`.
- Act: `write_profile_meta(&profiles, &tmp_dir)`
- Assert:
  - `Ok(())` returned
  - File `profile-meta.json` exists in `tmp_dir`
  - File `profile-meta.json.tmp` does NOT exist in `tmp_dir` (rename completed)
  - Deserialize `profile-meta.json` as `ProfileMetaFile`; assert no error
  - `file.version == 1`
  - `file.profiles["ppr-candidate"].distribution_change == true`
  - `file.profiles["ppr-candidate"].distribution_targets` is `Some(t)` where:
    - `t.cc_at_k_min == 0.60_f64`
    - `t.icd_min == 1.20_f64`
    - `t.mrr_floor == 0.35_f64`
  - `file.profiles["baseline"].distribution_change == false`
  - `file.profiles["baseline"].distribution_targets.is_none()`

---

## Serde Direction Tests (knowledge package #3557 — two independent directions required)

The knowledge package explicitly requires two separate serde direction tests for new
passthrough fields (pattern from nan-009). This applies to `DistributionTargetsJson` and
`ProfileMetaEntry`.

### Serialize direction (within `test_write_profile_meta_schema`)
The write call produces JSON. The assertions above verify deserialization of that JSON,
implicitly validating serialize → JSON → deserialize round-trip.

### Deserialize direction (explicit JSON string test)
Within `test_write_profile_meta_schema` or a separate helper, also assert the deserialize
direction from a hand-crafted JSON string:

```rust
let json = r#"{
  "version": 1,
  "profiles": {
    "ppr-candidate": {
      "distribution_change": true,
      "distribution_targets": {
        "cc_at_k_min": 0.60,
        "icd_min": 1.20,
        "mrr_floor": 0.35
      }
    }
  }
}"#;
let file: ProfileMetaFile = serde_json::from_str(json).unwrap();
assert!(file.profiles["ppr-candidate"].distribution_change);
assert_eq!(file.profiles["ppr-candidate"].distribution_targets.as_ref().unwrap().cc_at_k_min, 0.60);
```

This catches the case where the writer uses field name `"cc_at_k"` but the reader expects
`"cc_at_k_min"` (R-10 schema mismatch).

---

## Atomic Write Verification

The following scenarios must be covered (may be inline within `test_write_profile_meta_schema`
or as a separate test):

1. **No orphan `.tmp`**: After a successful write, assert that `profile-meta.json.tmp` does
   not exist in the output directory.

2. **`.tmp` not read by eval report**: Create a `profile-meta.json.tmp` with invalid content
   in a results directory that lacks `profile-meta.json`. Invoke `load_profile_meta` (Component 7).
   Assert it returns `Ok(HashMap::new())` — the `.tmp` file is ignored because `load_profile_meta`
   reads only `profile-meta.json`.

---

## Integration Test Expectations

No infra-001 integration tests required. The sidecar write is internal to the eval binary.

---

## Edge Cases

| Scenario | Expected Behavior |
|----------|------------------|
| Single profile with `distribution_change = true` | `profiles` map has one entry; `version = 1` |
| All profiles have `distribution_change = false` | All entries have `distribution_targets: null`; file is still written |
| Zero profiles (empty slice) | `write_profile_meta(&[], &out)` writes `{ "version": 1, "profiles": {} }` — no error |
| Output directory does not exist | Returns `Err(EvalError::...)` — not a panic |

---

## Risks Covered

| Risk | Test |
|------|------|
| R-04 (non-atomic write) | `test_write_profile_meta_schema` — no orphan `.tmp` assertion |
| R-10 (schema mismatch) | Deserialize direction from hand-crafted JSON string |
| R-10 (round-trip) | Serialize → write → read → deserialize in one test |
| Knowledge package #3557 (dual-direction serde) | Both serialize and deserialize directions explicitly tested |
| Knowledge package #3585 (atomic sidecar pattern) | `.tmp` → rename path verified; no `.tmp` post-success |
