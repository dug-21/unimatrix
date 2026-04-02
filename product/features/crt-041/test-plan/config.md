# Test Plan: `config` Component

**Source file:** `crates/unimatrix-server/src/infra/config.rs`
**Risk coverage:** R-03, R-17
**AC coverage:** AC-23, AC-24

---

## Component Scope

This component adds five new fields to `InferenceConfig`:
- `s2_vocabulary: Vec<String>` — default `vec![]`
- `max_s1_edges_per_tick: usize` — default 200, range [1, 10000]
- `max_s2_edges_per_tick: usize` — default 200, range [1, 10000]
- `s8_batch_interval_ticks: u32` — default 10, range [1, 1000]
- `max_s8_pairs_per_batch: usize` — default 500, range [1, 10000]

Each field has TWO encoding sites that must be identical:
1. A private `default_*()` function tagged with `#[serde(default = "default_*")]`
2. A field entry in `impl Default for InferenceConfig`

The primary risk (R-03) is that these two sites diverge. Historical precedent: crt-032
had this bug (entry #3817). `test_inference_config_s1_s2_s8_defaults_match_serde` is the
mandatory automated guard.

---

## Unit Test Expectations

All tests in the existing `config.rs::tests` module using standard `#[test]` (sync).

### Mandatory Pre-PR Test (R-03)

**`test_inference_config_s1_s2_s8_defaults_match_serde`** — R-03, AC-23 (BLOCKS DELIVERY)
- Assert: for each of the five new fields, the value from `InferenceConfig::default()`
  equals the value from `toml::from_str::<InferenceConfig>("").unwrap()`.
- Specific assertions:
  ```rust
  let prog = InferenceConfig::default();
  let serde: InferenceConfig = toml::from_str("").unwrap();

  assert_eq!(prog.s2_vocabulary, serde.s2_vocabulary);
  assert_eq!(prog.max_s1_edges_per_tick, serde.max_s1_edges_per_tick);
  assert_eq!(prog.max_s2_edges_per_tick, serde.max_s2_edges_per_tick);
  assert_eq!(prog.s8_batch_interval_ticks, serde.s8_batch_interval_ticks);
  assert_eq!(prog.max_s8_pairs_per_batch, serde.max_s8_pairs_per_batch);
  ```
- This test MUST be in the `config.rs::tests` module (not a sibling file) so it is
  visible during standard `cargo test` runs.

### Default Value Tests (R-03)

**`test_inference_config_s2_vocabulary_default_is_empty`** — R-03, AC-23
- Assert: `InferenceConfig::default().s2_vocabulary == Vec::<String>::new()`.
- This explicitly guards against accidentally setting the 9-term ASS-038 list as the
  default. The SCOPE.md §Design Decision 3 resolution requires the default to be empty
  (operator opt-in, W0-3 domain-agnostic requirement).

**`test_inference_config_numeric_defaults`** — R-03, AC-23
- Assert:
  - `InferenceConfig::default().max_s1_edges_per_tick == 200`
  - `InferenceConfig::default().max_s2_edges_per_tick == 200`
  - `InferenceConfig::default().s8_batch_interval_ticks == 10`
  - `InferenceConfig::default().max_s8_pairs_per_batch == 500`

### Range Validation Tests (R-17)

**`test_inference_config_validate_rejects_zero_s1_cap`** — R-17, AC-24
- Arrange: `InferenceConfig { max_s1_edges_per_tick: 0, ..Default::default() }`.
- Assert: `config.validate()` returns `Err(ConfigError::NliFieldOutOfRange { .. })`.
- The error message must name the field (`max_s1_edges_per_tick`).

**`test_inference_config_validate_rejects_zero_s2_cap`** — R-17, AC-24
- Same as above for `max_s2_edges_per_tick: 0`.

**`test_inference_config_validate_rejects_zero_s8_interval`** — R-17, AC-24 (panic guard)
- Arrange: `InferenceConfig { s8_batch_interval_ticks: 0, ..Default::default() }`.
- Assert: `config.validate()` returns `Err(...)` naming `s8_batch_interval_ticks`.
- Critical: `s8_batch_interval_ticks = 0` causes `current_tick % 0` — integer division by
  zero, a runtime panic. `validate()` MUST catch this before the value reaches the tick loop.

**`test_inference_config_validate_rejects_zero_s8_pair_cap`** — R-17, AC-24
- Arrange: `max_s8_pairs_per_batch: 0`.
- Assert: `validate()` returns error. A zero pair cap produces `LIMIT 0` (silent no-op).

**`test_inference_config_validate_accepts_minimum_values`** — R-17
- Arrange: all four numeric fields set to their minimum valid value (1).
- Assert: `config.validate()` returns `Ok(())`. Lower bound is 1, not 0.

**`test_inference_config_validate_accepts_maximum_values`** — R-17
- Arrange:
  - `max_s1_edges_per_tick = 10000` (range max)
  - `max_s2_edges_per_tick = 10000`
  - `s8_batch_interval_ticks = 1000`
  - `max_s8_pairs_per_batch = 10000`
- Assert: `validate()` returns `Ok(())`.

**`test_inference_config_validate_rejects_above_max_s1`** — R-17
- Arrange: `max_s1_edges_per_tick = 10001`.
- Assert: `validate()` returns `Err(...)`.

**`test_inference_config_validate_rejects_above_max_s8_interval`** — R-17
- Arrange: `s8_batch_interval_ticks = 1001`.
- Assert: `validate()` returns `Err(...)`.

### TOML Deserialization Tests

**`test_inference_config_s2_vocabulary_parses_from_toml`**
- Arrange: TOML string with `s2_vocabulary = ["schema", "migration", "cache"]`.
- Assert: `InferenceConfig` parsed with `s2_vocabulary = vec!["schema", "migration", "cache"]`.

**`test_inference_config_s2_vocabulary_explicit_empty_toml`**
- Arrange: TOML string with `s2_vocabulary = []`.
- Assert: `s2_vocabulary == vec![]`. Confirm explicit empty is valid.

**`test_inference_config_partial_toml_uses_defaults`**
- Arrange: TOML with only `max_s1_edges_per_tick = 50` (all others absent).
- Assert: `max_s1_edges_per_tick = 50`, all others at their defaults.

### `merge_configs` Tests

**`test_merge_configs_includes_new_fields`**
- Confirm that `merge_configs` (or whatever merge function exists) handles all five new
  fields. If `merge_configs` uses `Option<>` or applies file-over-default logic, each new
  field must participate. Test that a file config with `max_s1_edges_per_tick = 50` overrides
  the default of 200 after merge.

---

## Integration Test Expectations

The config component has no direct MCP-visible interface. Integration-level coverage is
indirect: if `InferenceConfig` fails to load (e.g., due to a validate() panic), the server
will not start, which the smoke test gate will catch.

**Smoke gate (indirect config test):** `pytest -m smoke` confirms the server starts with
default config. Any zero-value default that bypasses validate() and causes a runtime panic
on the first tick will surface here (since the availability suite runs ticks).

---

## Assertions Checklist

- [ ] `test_inference_config_s1_s2_s8_defaults_match_serde` passes — BLOCKS DELIVERY (R-03)
- [ ] `s2_vocabulary` default is `vec![]` not the 9-term list (R-03, SCOPE §Design Decision 3)
- [ ] All four numeric defaults match their specified values (200, 200, 10, 500) (R-03)
- [ ] `validate()` rejects 0 for all four range-bounded fields (R-17)
- [ ] `validate()` accepts 1 as the minimum for all four fields (R-17)
- [ ] `validate()` accepts the documented maximum for all four fields (R-17)
- [ ] TOML round-trip works for `s2_vocabulary` as both empty and populated (R-03)
