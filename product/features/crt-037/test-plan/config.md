# crt-037 Test Plan: config.rs (InferenceConfig Extension)

**Component**: `crates/unimatrix-server/src/infra/config.rs`
**Nature of change**: Three new fields with serde defaults and `validate()` range checks.
No other changes.
**Risks addressed**: R-08 (domain string placement — strings live here, not in detection
logic), R-18 (validation boundary errors).

---

## Unit Tests

### Serde Defaults (AC-07, AC-08, AC-09)

**Test**: `test_inference_config_default_informs_category_pairs`
- Arrange: `let raw = ""`; empty TOML string
- Act: `toml::from_str::<InferenceConfig>(raw)` (or use `InferenceConfig::default()`)
- Assert: `config.informs_category_pairs` equals exactly:
  ```
  [["lesson-learned", "decision"],
   ["lesson-learned", "convention"],
   ["pattern", "decision"],
   ["pattern", "convention"]]
  ```
  Length = 4. Order matters for deterministic test assertions.
  — covers AC-07

**Test**: `test_inference_config_default_nli_informs_cosine_floor`
- Arrange: empty TOML
- Act: deserialize
- Assert: `config.nli_informs_cosine_floor == 0.45_f32` — covers AC-08

**Test**: `test_inference_config_default_nli_informs_ppr_weight`
- Arrange: empty TOML
- Act: deserialize
- Assert: `config.nli_informs_ppr_weight == 0.6_f32` — covers AC-09

**Test**: `test_inference_config_default_passes_validate`
- Arrange: `InferenceConfig::default()`
- Act: `config.validate()`
- Assert: `Ok(())` — covers AC-12
- Rationale: validates that default values are within their own range constraints

**Test**: `test_inference_config_toml_override_informs_fields`
- Arrange: TOML with `nli_informs_cosine_floor = 0.55` and `nli_informs_ppr_weight = 0.4`
- Act: deserialize
- Assert: `config.nli_informs_cosine_floor == 0.55_f32`
         `config.nli_informs_ppr_weight == 0.4_f32`
         (existing non-informs fields are unaffected)

### Validation: `nli_informs_cosine_floor` Exclusive Bounds (AC-10, R-18)

**Test**: `test_validate_nli_informs_cosine_floor_zero_is_error`
- Arrange: `config.nli_informs_cosine_floor = 0.0`
- Act: `config.validate()`
- Assert: `Err(_)` — covers AC-10 lower bound

**Test**: `test_validate_nli_informs_cosine_floor_one_is_error`
- Arrange: `config.nli_informs_cosine_floor = 1.0`
- Act: `config.validate()`
- Assert: `Err(_)` — covers AC-10 upper bound

**Test**: `test_validate_nli_informs_cosine_floor_valid_value_is_ok`
- Arrange: `config.nli_informs_cosine_floor = 0.45`
- Act: `config.validate()`
- Assert: `Ok(())` — covers AC-10 nominal

**Test**: `test_validate_nli_informs_cosine_floor_near_boundaries`
- Arrange: test values just inside bounds: `0.001` (→ Ok), `0.999` (→ Ok)
- Arrange: test values at boundaries: `0.0` (→ Err), `1.0` (→ Err)
- Assert: as indicated — exclusive bounds on both sides

### Validation: `nli_informs_ppr_weight` Inclusive Bounds (AC-11, R-18)

**Test**: `test_validate_nli_informs_ppr_weight_zero_is_ok`
- Arrange: `config.nli_informs_ppr_weight = 0.0`
- Act: `config.validate()`
- Assert: `Ok(())` — covers AC-11 lower inclusive bound

**Test**: `test_validate_nli_informs_ppr_weight_one_is_ok`
- Arrange: `config.nli_informs_ppr_weight = 1.0`
- Act: `config.validate()`
- Assert: `Ok(())` — covers AC-11 upper inclusive bound

**Test**: `test_validate_nli_informs_ppr_weight_negative_is_error`
- Arrange: `config.nli_informs_ppr_weight = -0.01`
- Act: `config.validate()`
- Assert: `Err(_)` — covers AC-11 below lower bound

**Test**: `test_validate_nli_informs_ppr_weight_above_one_is_error`
- Arrange: `config.nli_informs_ppr_weight = 1.01`
- Act: `config.validate()`
- Assert: `Err(_)` — covers AC-11 above upper bound

### Domain String Placement (R-08 / C-12)

Domain vocabulary strings (`"lesson-learned"`, `"decision"`, `"pattern"`, `"convention"`)
are the *only* place outside `config.rs` where this vocabulary must appear. The CI grep
gate in AC-22 enforces the absence from `nli_detection_tick.rs`. No unit test needed for
this in `config.rs` — the presence of the strings in `default_informs_category_pairs` is
the correct behavior.

CI grep gate (enforced at Stage 3c):
```bash
grep -n '"lesson-learned"\|"decision"\|"pattern"\|"convention"' \
  crates/unimatrix-server/src/services/nli_detection_tick.rs
# Expected: empty
```

---

## Acceptance Criteria Covered

| AC-ID | Test Name |
|-------|-----------|
| AC-07 | `test_inference_config_default_informs_category_pairs` |
| AC-08 | `test_inference_config_default_nli_informs_cosine_floor` |
| AC-09 | `test_inference_config_default_nli_informs_ppr_weight` |
| AC-10 | `test_validate_nli_informs_cosine_floor_*` (3 tests) |
| AC-11 | `test_validate_nli_informs_ppr_weight_*` (4 tests) |
| AC-12 | `test_inference_config_default_passes_validate` |
