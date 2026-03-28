# Agent Report: col-031-agent-4-inference-config

## Task

Implement `infra/config.rs` changes for col-031:
- Raise `default_w_phase_explicit()` from 0.0 to 0.05
- Add `query_log_lookback_days: u32` field with default 30
- Add `[1, 3650]` range check in `validate()`
- Update `FusionWeights` doc-comment in `search.rs`
- Update `Default` impl and `merge_configs`
- Write unit tests per test plan (AC-09, AC-10, R-08)

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs`
- `crates/unimatrix-server/src/services/search.rs`

## Changes Made

1. `default_w_phase_explicit()` — returns `0.05` (was `0.0`); comment updated to cite col-031, ADR-004.
2. `default_query_log_lookback_days()` — new private fn returning `30u32`.
3. `InferenceConfig` struct — new `query_log_lookback_days: u32` field with full doc-comment and `#[serde(default = "default_query_log_lookback_days")]`.
4. `InferenceConfig::Default` impl — `w_phase_explicit` now calls `default_w_phase_explicit()` (0.05); `query_log_lookback_days` added calling `default_query_log_lookback_days()`.
5. `InferenceConfig::validate()` — range check added for `query_log_lookback_days < 1 || > 3650` using existing `ConfigError::NliFieldOutOfRange` (no new variant needed).
6. `merge_configs()` — `query_log_lookback_days` added with `!= default` pattern matching all other `u32` fields.
7. `FusionWeights` doc-comment (search.rs) — updated from `0.97` total to `1.02` total with ADR-004 attribution.
8. `FusionWeights.w_phase_explicit` inline comment — updated to cite col-031 and 0.05 default.
9. `w_phase_explicit` field doc-comment in `InferenceConfig` — updated to reflect activation.

### Tests Added (new)
- `test_w_phase_explicit_default_from_empty_toml` — AC-09 serde path
- `test_inference_config_query_log_lookback_days_default` — AC-10 Default path
- `test_query_log_lookback_days_default_from_empty_toml` — AC-10 serde path
- `test_query_log_lookback_days_deserializes_from_toml` — AC-10 explicit value
- `test_validate_lookback_days_zero_is_error` — R-08 boundary: 0 fails
- `test_validate_lookback_days_3651_is_error` — R-08 boundary: 3651 fails
- `test_validate_lookback_days_boundary_values_pass` — R-08 boundaries: 1, 30, 3650 pass

### Tests Updated (3 pre-existing tests asserting 0.0)
- `test_inference_config_default_phase_weights` — AC-09: assert 0.05 (was 0.0)
- `test_inference_config_missing_phase_fields_use_defaults` — assert 0.05 (was 0.0)
- `test_phase_explicit_norm_placeholder_fields_present` — assert 0.05 (was 0.0)
- `test_inference_config_six_weight_sum_unchanged_by_phase_fields` — total updated to 1.02 (was 0.97)

## Test Results

- 2235 passed, 0 failed (up from 2232 — 3 new tests net; 7 added, 0 removed)
- Build: PASS (zero errors, 13 pre-existing warnings unchanged)

## Issues / Blockers

None. `ConfigError::NliFieldOutOfRange` was a direct fit for the `query_log_lookback_days`
range check — its `field: &'static str`, `value: String`, `reason: &'static str` shape
exactly matches the need. No new ConfigError variant was required.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3206, #3182, #3181
  confirming the additive weight pattern outside the six-term sum constraint and the
  NliFieldOutOfRange reuse convention. Consistent with implementation. No surprises.
- Stored: nothing novel to store — the reuse of `NliFieldOutOfRange` for non-NLI fields
  (it is a general-purpose inference field range error despite its name) is already
  implied by entries #3182 and #3181. Entry #3206 covers the additive weight extension
  pattern. No new gotchas discovered.
