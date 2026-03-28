# Test Plan: inference_config
# `crates/unimatrix-server/src/infra/config.rs`

## Component Responsibilities

Two changes to `InferenceConfig`:

1. `default_w_phase_explicit()` returns `0.05` (raised from `0.0`).
2. New field `query_log_lookback_days: u32` with default `30`.
3. `validate()` gains a range check for `query_log_lookback_days`: `[1, 3650]`.
4. `FusionWeights` doc-comment updated to state `0.95 + 0.02 + 0.05 = 1.02`.

The existing test `test_inference_config_default_phase_weights` must be updated to
assert `w_phase_explicit == 0.05` (it currently asserts `0.0`).

---

## Unit Test Expectations

Tests in `#[cfg(test)] mod tests` inside `config.rs`.

### AC-09 / w_phase_explicit Default Raised

**`test_inference_config_default_phase_weights`** (existing test, must be updated)
- Assert: default `InferenceConfig` has `w_phase_explicit == 0.05f64`.
- This test currently asserts `0.0` — update it to assert `0.05`.
- If the test is not updated, Stage 3b delivery fails to compile clean tests.

**`test_w_phase_explicit_default_from_empty_toml`**
- Arrange: deserialize `InferenceConfig` from an empty TOML string `""`.
- Assert: `w_phase_explicit == 0.05f64`.

### AC-10 / query_log_lookback_days Field

**`test_inference_config_query_log_lookback_days_default`**
- Arrange: construct default `InferenceConfig`.
- Assert: `query_log_lookback_days == 30u32`.

**`test_query_log_lookback_days_default_from_empty_toml`**
- Arrange: deserialize from empty TOML.
- Assert: `query_log_lookback_days == 30u32`.

**`test_query_log_lookback_days_deserializes_from_toml`**
- Arrange: TOML with `query_log_lookback_days = 7`.
- Assert: `config.query_log_lookback_days == 7u32`.

### R-08 / validate() Range Check for lookback_days

**`test_validate_lookback_days_zero_is_error`**  ← R-08 boundary
- Arrange: `InferenceConfig { query_log_lookback_days: 0, ..Default::default() }`.
- Act: call `config.validate()`.
- Assert: returns `Err(_)` (any error variant).

**`test_validate_lookback_days_3651_is_error`**  ← R-08 boundary
- Arrange: `query_log_lookback_days = 3651`.
- Assert: `validate()` returns `Err(_)`.

**`test_validate_lookback_days_boundary_values_pass`**  ← R-08 valid boundaries
- `query_log_lookback_days = 1` → `validate()` returns `Ok(())`.
- `query_log_lookback_days = 3650` → `validate()` returns `Ok(())`.
- `query_log_lookback_days = 30` (default) → `validate()` returns `Ok(())`.

### Doc-Comment Update (code review check, not a Rust test)

At code review: confirm `FusionWeights` (or `InferenceConfig`) carries a comment
stating `// 0.95 + 0.02 + 0.05 = 1.02 — w_phase_explicit is additive outside
the six-weight constraint (ADR-004, crt-026)`.

---

## Edge Cases

- `query_log_lookback_days = 1`: minimal window. The SQL arithmetic
  `strftime('%s','now') - 1 * 86400` is correct for INTEGER `ts`.
- `query_log_lookback_days = 3650`: 10-year window. Should not cause issues —
  effectively covers all rows (similar to unbounded but within defined range).
- Both boundaries must be confirmed to pass `validate()`.

---

## Covered Risks

| Risk | Test |
|------|------|
| R-08 (`lookback_days` not validated in `validate()`) | `test_validate_lookback_days_zero_is_error`, `test_validate_lookback_days_3651_is_error`, `test_validate_lookback_days_boundary_values_pass` |
