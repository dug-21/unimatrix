# Test Plan: inference-config (Wave 1b)

**File modified:** `crates/unimatrix-server/src/infra/config.rs`

**Changes:**
1. Add `supports_cosine_threshold: f32` field (dual-site: serde backing fn + impl Default)
2. Add `default_supports_cosine_threshold() -> f32 { 0.65 }` backing function
3. Add range validation in `InferenceConfig::validate()`: `(0.0, 1.0)` exclusive
4. Update config merge function: add `supports_cosine_threshold` following `nli_informs_cosine_floor` f32 epsilon pattern
5. Remove `nli_post_store_k` field and all 6 associated sites

**Risk coverage:** R-03 (High), R-04 (Medium), R-13 (Medium)

---

## Unit Test Expectations

Tests live in `#[cfg(test)] mod tests` inside `config.rs`. Pattern: mirror the existing
`test_inference_config_default_nli_informs_cosine_floor` (TC-06a/TC-06b pattern) for the
three independent assertions required by AC-16.

### TC-01: Backing function returns 0.65 (AC-16, R-03 — first independent assertion)

```
fn test_default_supports_cosine_threshold_fn()
```

- Assert: `assert_eq!(default_supports_cosine_threshold(), 0.65_f32, "TC-01a")`
- This test calls the backing function directly. It must NOT use `InferenceConfig::default()`
  or `toml::from_str`. It is independent of the serde path.
- Covers: AC-16 (assertion 1 of 3), R-03

### TC-02: impl Default path returns 0.65 (AC-10, AC-16, R-03 — second independent assertion)

```
fn test_inference_config_default_supports_cosine_threshold()
```

- Assert: `assert_eq!(InferenceConfig::default().supports_cosine_threshold, 0.65_f32, "TC-02")`
- This test uses `InferenceConfig::default()` directly. Must NOT call `toml::from_str`.
- Covers: AC-10, AC-16 (assertion 2 of 3), R-03
- Note: if this test fails but TC-01 passes, the impl Default struct literal is missing the
  `supports_cosine_threshold` entry — the exact impl Default trap (ADR-002, pattern #3817).

### TC-03: Serde deserialization from empty TOML returns 0.65 (AC-16, R-03 — third independent assertion)

```
fn test_inference_config_toml_empty_supports_cosine_threshold()
```

- Arrange: `let config: InferenceConfig = toml::from_str("").unwrap()`
- Assert: `assert_eq!(config.supports_cosine_threshold, 0.65_f32, "TC-03")`
- This test uses `toml::from_str` exclusively. Must NOT use `InferenceConfig::default()`.
- Covers: AC-16 (assertion 3 of 3), R-03

### TC-04: TOML override propagates correctly

```
fn test_inference_config_toml_override_supports_cosine_threshold()
```

- Arrange:
  ```
  let toml = "supports_cosine_threshold = 0.80\n";
  let config: InferenceConfig = toml::from_str(toml).unwrap();
  ```
- Assert: `assert!((config.supports_cosine_threshold - 0.80_f32).abs() < 1e-6)`
- Covers: FR-08 config deserialization with non-default value

### TC-05: validate() rejects 0.0 (AC-09, exclusive lower bound)

```
fn test_validate_supports_cosine_threshold_zero_fails()
```

- Arrange: `InferenceConfig { supports_cosine_threshold: 0.0, ..InferenceConfig::default() }`
- Assert: `c.validate(Path::new("/fake")).is_err()`
- Assert: error message names the field `"supports_cosine_threshold"` (pattern: `assert_validate_fails_with_field`)
- Covers: AC-09

### TC-06: validate() rejects 1.0 (AC-09, exclusive upper bound)

```
fn test_validate_supports_cosine_threshold_one_fails()
```

- Arrange: `InferenceConfig { supports_cosine_threshold: 1.0, ..InferenceConfig::default() }`
- Assert: `c.validate(Path::new("/fake")).is_err()`, field name in error is `"supports_cosine_threshold"`
- Covers: AC-09

### TC-07: validate() accepts 0.65 (nominal default)

```
fn test_validate_supports_cosine_threshold_default_is_ok()
```

- Arrange: `InferenceConfig { supports_cosine_threshold: 0.65, ..InferenceConfig::default() }`
- Assert: `c.validate(Path::new("/fake")).is_ok()`
- Covers: AC-09

### TC-08: validate() accepts boundary-adjacent values 0.001 and 0.999

```
fn test_validate_supports_cosine_threshold_near_bounds_ok()
```

- Assert both `0.001` and `0.999` pass `validate()` (inclusive-exclusive range means these
  are valid)
- Covers: AC-09

### TC-09: Config merge propagates project-level override (R-13)

```
fn test_config_merge_supports_cosine_threshold_project_overrides()
```

- Arrange:
  - `project` config: `supports_cosine_threshold = 0.70`
  - `base`/global config: `supports_cosine_threshold = 0.65` (default)
- Act: call the config merge function
- Assert: merged config `supports_cosine_threshold == 0.70_f32` (within `f32::EPSILON`)
- Covers: R-13, FR-08 merge function update
- Note: grep the merge function body for `nli_informs_cosine_floor` to find the correct
  insertion site. The merge uses f32 epsilon comparison, not `==`:
  ```rust
  if (project.inference.supports_cosine_threshold
      - default.inference.supports_cosine_threshold).abs() > f32::EPSILON
  { project.inference.supports_cosine_threshold }
  else { global.inference.supports_cosine_threshold }
  ```

### TC-10: Config merge keeps global when project equals default (R-13)

```
fn test_config_merge_supports_cosine_threshold_global_when_not_overridden()
```

- Arrange: project config has `supports_cosine_threshold = 0.65` (same as default);
  global config has `supports_cosine_threshold = 0.75`
- Assert: merged result is `0.75` (global wins when project == default)
- Covers: R-13, correct merge semantics

### TC-11: nli_post_store_k absent — forward-compat serde test (AC-18, R-04)

```
fn test_inference_config_toml_with_nli_post_store_k_succeeds()
```

- Arrange: `let toml = "nli_post_store_k = 5\n";`
- Act: `toml::from_str::<InferenceConfig>(toml)`
- Assert: returns `Ok(_)` — serde silently discards the unknown field
- Assert: does NOT panic
- Covers: AC-18, NFR-08 — confirms `deny_unknown_fields` is NOT active

### TC-12: nli_post_store_k grep gate (AC-17, R-04)

This is a **static verification step** specified in the test plan for Stage 3c, not a Rust unit test:

```bash
grep -n "nli_post_store_k" crates/unimatrix-server/src/infra/config.rs
```

Expected: zero results.

Stage 3c must verify this before marking AC-17 complete. Document the grep result in
RISK-COVERAGE-REPORT.md under AC-17.

### TC-13: Existing nli_post_store_k tests removed

After the `nli_post_store_k` field is removed, these existing tests must be deleted:
- `test_validate_nli_post_store_k_zero_fails`
- `test_validate_nli_post_store_k_101_fails`
- The assertion `assert_eq!(config.nli_post_store_k, 10)` in multiple tests

The test count for the config module will decrease. Stage 3c must verify the overall test
suite still compiles without these references.

---

## Integration Test Expectations

No new infra-001 integration tests are required for `inference-config`. The `supports_cosine_threshold`
field is only exercised by Path C in the background tick; its MCP-visible effect is captured
by the `test_context_status_supports_edge_count_increases_after_tick` test planned in OVERVIEW.md.

The `tools` suite exercises `context_status` and server startup, which will fail if config
deserialization is broken — serving as a regression guard.

---

## Edge Cases

| Edge Case | Expected Behavior |
|-----------|------------------|
| `supports_cosine_threshold = 0.0` | `validate()` rejects with field name in error (TC-05) |
| `supports_cosine_threshold = 1.0` | `validate()` rejects (TC-06) |
| `supports_cosine_threshold` absent from TOML | Serde applies backing function default (0.65) (TC-03) |
| `nli_post_store_k = 5` in TOML after removal | Serde silently discards (TC-11) |
| `supports_cosine_threshold` set in both project and global configs | Merge function: f32 epsilon comparison selects project value when different from default (TC-09) |

---

## Assertions Summary

| AC-ID | Test | Assertion |
|-------|------|-----------|
| AC-09 | TC-05, TC-06, TC-07, TC-08 | validate() boundary behavior for `supports_cosine_threshold` |
| AC-10 | TC-02 | `InferenceConfig::default().supports_cosine_threshold == 0.65` |
| AC-16 | TC-01, TC-02, TC-03 | Three independent paths (backing fn, impl Default, serde) all return 0.65 |
| AC-17 | TC-12 (grep) | Zero occurrences of `nli_post_store_k` in config.rs after removal |
| AC-18 | TC-11 | Deserializing TOML with `nli_post_store_k = 5` returns `Ok(_)` |
| R-13 | TC-09, TC-10 | Config merge function propagates project-level `supports_cosine_threshold` override |
