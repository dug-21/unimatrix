# Agent Report: crt-024-agent-3-inference-config

## Task

Implement `InferenceConfig` additions for crt-024 (Ranking Signal Fusion WA-0):
- Six `f64` fusion weight fields on `InferenceConfig`
- Six default functions
- Extended `Default` impl
- Extended `validate()` with per-field range checks and sum invariant
- New `FusionWeightSumExceeded` `ConfigError` variant with `Display`
- Extended `merge_configs` for the six new fields
- Unit tests per test-plan/inference-config.md

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/config.rs`

## Changes Made

### `InferenceConfig` struct
Added six `f64` weight fields after the NLI block, each with `#[serde(default = "default_w_xxx")]`:
`w_sim=0.25, w_nli=0.35, w_conf=0.15, w_coac=0.10, w_util=0.05, w_prov=0.05`

### Default functions
Six named functions (`default_w_sim` through `default_w_prov`) added in a new section after the NLI defaults.

### `Default` impl
Extended `InferenceConfig::default()` with all six new fields at their ADR-003 values (sum = 0.95).

### `ConfigError` enum
Added `FusionWeightSumExceeded { path, sum, w_sim, w_nli, w_conf, w_coac, w_util, w_prov }` variant with full `Display` arm naming all six fields and the computed sum.

### `InferenceConfig::validate()`
Two new blocks inserted after the `NliThresholdInvariantViolated` cross-field check and before `Ok(())`:
1. Per-field loop over six `(field_name, value)` pairs — reuses `NliFieldOutOfRange` for values outside `[0.0, 1.0]`
2. Six-term sum check — returns `FusionWeightSumExceeded` when `sum > 1.0`

### `merge_configs`
Extended the `InferenceConfig` literal with six new fields using the f64 epsilon comparison pattern consistent with existing NLI float merge fields.

## Test Results

- **35 `test_inference_config_*` tests pass** (21 pre-existing + 14 new)
- **1 `test_display_fusion_weight_sum_exceeded` test passes**
- **Total workspace: 0 failures across all test suites**

Test coverage per test plan:
- `test_inference_config_weight_defaults_when_absent` — AC-01
- `test_inference_config_default_weights_sum_within_headroom` — AC-01b / R-06
- `test_inference_config_validate_rejects_sum_exceeding_one` — AC-02
- `test_inference_config_validate_accepts_sum_exactly_one` — AC-02b / EC-02
- `test_inference_config_validate_accepts_all_zeros` — EC-01
- `test_inference_config_validate_uses_result_not_panic` — AC-12
- `test_inference_config_partial_toml_gets_defaults_not_error` — R-13
- 6x `test_inference_config_validate_rejects_w_{field}_below_zero` — AC-03 (negative boundary)
- 6x `test_inference_config_validate_rejects_w_{field}_above_one` — AC-03 (>1.0 boundary)
- `test_inference_config_validate_accepts_default_weights` — T-IC-06
- `test_inference_config_existing_nli_invariant_still_works` — T-IC-08
- `test_display_fusion_weight_sum_exceeded` — operator diagnostic coverage

## Issues / Blockers

None. Implementation follows pseudocode exactly. No deviations from architecture.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` config validation — entry #2730 confirmed `..Default::default()` pattern for `InferenceConfig` struct literals; entry #2969-#2972 confirmed ADR decisions
- Stored: nothing novel to store — the `NliFieldOutOfRange` reuse pattern for per-field validation and `..Default::default()` in struct literals were already documented. No new runtime-invisible traps were encountered.
