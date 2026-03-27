# Test Plan: InferenceConfig Additions (crt-029)

Source file: `crates/unimatrix-server/src/infra/config.rs`
Pseudocode: `pseudocode/inference-config.md`

Risks addressed: R-03 (threshold boundary), R-07 (struct literal trap)

---

## Unit Test Expectations

All tests in `#[cfg(test)]` module inside `config.rs`. Use `InferenceConfig::default()` as
the base; mutate individual fields for boundary tests.

### AC-01 / AC-17 — Default values and TOML deserialization

#### `test_inference_config_defaults`
- Construct via `InferenceConfig::default()`
- Assert `supports_candidate_threshold == 0.5_f32`
- Assert `supports_edge_threshold == 0.7_f32`
- Assert `max_graph_inference_per_tick == 100_usize`
- Assert `graph_inference_k == 10_usize`

#### `test_inference_config_toml_defaults`
- Parse a minimal TOML string that contains an `[inference]` section with `nli_enabled = true`
  and no crt-029 fields
- Assert deserialized values match: `supports_candidate_threshold = 0.5`, `supports_edge_threshold = 0.7`,
  `max_graph_inference_per_tick = 100`, `graph_inference_k = 10`
- This exercises the `#[serde(default = "...")]` attribute path (AC-17)

#### `test_inference_config_toml_explicit_values`
- Parse a TOML string explicitly setting all four fields to non-default values
  (e.g., `supports_candidate_threshold = 0.4`, `supports_edge_threshold = 0.8`,
  `max_graph_inference_per_tick = 50`, `graph_inference_k = 20`)
- Assert deserialized values match the explicit values (not defaults)

---

### AC-02 — Cross-field threshold invariant

The reject predicate is strict `>=`: `supports_candidate_threshold >= supports_edge_threshold`.

#### `test_validate_rejects_equal_thresholds`
- Set `supports_candidate_threshold = 0.7`, `supports_edge_threshold = 0.7`
- Call `validate()` — assert `Err`
- Error message must reference both field names (format consistent with existing
  `nli_contradiction_threshold` guard pattern)

#### `test_validate_rejects_candidate_above_edge`
- Set `supports_candidate_threshold = 0.8`, `supports_edge_threshold = 0.7`
- Call `validate()` — assert `Err`

#### `test_validate_accepts_candidate_below_edge`
- Set `supports_candidate_threshold = 0.69`, `supports_edge_threshold = 0.7`
- Call `validate()` — assert `Ok`
- This is the valid boundary: strict `<` is the passing condition

---

### AC-03 — Individual threshold range (0.0, 1.0) exclusive

#### `test_validate_rejects_candidate_threshold_zero`
- Set `supports_candidate_threshold = 0.0` (with `supports_edge_threshold = 0.7`)
- Call `validate()` — assert `Err`

#### `test_validate_rejects_candidate_threshold_one`
- Set `supports_candidate_threshold = 1.0` (with `supports_edge_threshold` already invalid —
  adjust to avoid triggering the cross-field check; e.g., set candidate > 1.0 or use a
  separately valid config with only this field changed — keep test focused)
- Call `validate()` — assert `Err`

#### `test_validate_rejects_edge_threshold_zero`
- Set `supports_edge_threshold = 0.0` (with a valid `supports_candidate_threshold`)
- Call `validate()` — assert `Err`

#### `test_validate_rejects_edge_threshold_one`
- Set `supports_edge_threshold = 1.0`
- Call `validate()` — assert `Err`

#### `test_validate_accepts_threshold_boundaries` (positive)
- Set `supports_candidate_threshold = 0.01`, `supports_edge_threshold = 0.99`
- Call `validate()` — assert `Ok` (values inside the exclusive range)

---

### AC-04 — `max_graph_inference_per_tick` range [1, 1000]

#### `test_validate_rejects_max_inference_zero`
- Set `max_graph_inference_per_tick = 0`
- Call `validate()` — assert `Err`

#### `test_validate_rejects_max_inference_over_limit`
- Set `max_graph_inference_per_tick = 1001`
- Call `validate()` — assert `Err`

#### `test_validate_accepts_max_inference_at_bounds`
- Set `max_graph_inference_per_tick = 1` — assert `Ok`
- Set `max_graph_inference_per_tick = 1000` — assert `Ok`

---

### AC-04b — `graph_inference_k` range [1, 100]

#### `test_validate_rejects_graph_inference_k_zero`
- Set `graph_inference_k = 0`
- Call `validate()` — assert `Err`

#### `test_validate_rejects_graph_inference_k_over_limit`
- Set `graph_inference_k = 101`
- Call `validate()` — assert `Err`

#### `test_validate_accepts_graph_inference_k_at_bounds`
- Set `graph_inference_k = 1` — assert `Ok`
- Set `graph_inference_k = 100` — assert `Ok`

---

## Pre-Merge Grep Gate (R-07 / AC-18†)

This is a shell check, not a unit test, but belongs to this component's coverage:

```bash
# Verify all existing struct literals include new fields or ..default() tail
grep -rn 'InferenceConfig {' crates/unimatrix-server/src/
```

Expected: every occurrence either:
- Contains all four new fields explicitly, OR
- Ends with `..InferenceConfig::default()`

Current count at spec time: 52 occurrences across `nli_detection.rs` and `config.rs`.

The compiler is the backstop: bare struct literals that omit any field produce a compile error
`missing field 'X' in initializer`. Running `cargo check -p unimatrix-server` is required.

---

## Integration Harness

No new integration tests are planned for this component. Config validation happens at server
startup and affects server behaviour globally; the MCP interface does not expose individual
field values for direct assertion. The existing `tools` suite exercises server startup with
default config; the smoke test provides a regression baseline.

The three new integration tests planned in OVERVIEW.md (lifecycle suite) are the observable
effect of the config fields, not tests of the fields themselves.

---

## Assertions Summary

| AC-ID | Test Name | Expected Result |
|-------|-----------|-----------------|
| AC-01 | `test_inference_config_defaults` | All four defaults match spec |
| AC-17 | `test_inference_config_toml_defaults` | TOML without fields uses defaults |
| AC-02 | `test_validate_rejects_equal_thresholds` | `Err` when candidate == edge |
| AC-02 | `test_validate_rejects_candidate_above_edge` | `Err` when candidate > edge |
| AC-02 | `test_validate_accepts_candidate_below_edge` | `Ok` when candidate < edge |
| AC-03 | `test_validate_rejects_candidate_threshold_zero` | `Err` |
| AC-03 | `test_validate_rejects_candidate_threshold_one` | `Err` |
| AC-03 | `test_validate_rejects_edge_threshold_zero` | `Err` |
| AC-03 | `test_validate_rejects_edge_threshold_one` | `Err` |
| AC-03 | `test_validate_accepts_threshold_boundaries` | `Ok` |
| AC-04 | `test_validate_rejects_max_inference_zero` | `Err` |
| AC-04 | `test_validate_rejects_max_inference_over_limit` | `Err` |
| AC-04 | `test_validate_accepts_max_inference_at_bounds` | `Ok` (both 1 and 1000) |
| AC-04b | `test_validate_rejects_graph_inference_k_zero` | `Err` |
| AC-04b | `test_validate_rejects_graph_inference_k_over_limit` | `Err` |
| AC-04b | `test_validate_accepts_graph_inference_k_at_bounds` | `Ok` (both 1 and 100) |
| R-07/AC-18† | Grep gate (shell) | 52+ occurrences updated, `cargo check` passes |
