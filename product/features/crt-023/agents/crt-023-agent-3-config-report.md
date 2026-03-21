# Agent Report: crt-023-agent-3-config

**Task**: Config Extension — add 10 NLI fields to InferenceConfig
**Feature**: crt-023 (NLI + Cross-Encoder Re-ranking)
**Date**: 2026-03-20

## Work Completed

### Files Modified

- `crates/unimatrix-server/src/infra/config.rs` — added 10 NLI fields to `InferenceConfig`, 7 serde default functions, `is_recognized_nli_model_name()` helper, extended `validate()` with all range checks and cross-field invariant, added `NliFieldOutOfRange` and `NliThresholdInvariantViolated` to `ConfigError` with Display arms, updated `merge_configs()` InferenceConfig section for all 10 new fields, fixed 7 existing test struct literals to use `..InferenceConfig::default()`, added 34 NLI unit tests

- `crates/unimatrix-server/src/error.rs` — added `ServerError::NliNotReady` and `ServerError::NliFailed(String)` variants with Display arms, `From<ServerError> for ErrorData` arms (both map to `ERROR_EMBED_NOT_READY` -32004), 6 new unit tests

- `crates/unimatrix-server/src/infra/mod.rs` — added `pub mod nli_handle;`

### Files Created

- `crates/unimatrix-server/src/infra/nli_handle.rs` — empty stub (Wave 2 placeholder, compiles clean)

## Tests

**Pass: 1633 lib tests (0 failures)**
- NLI-specific tests: 34 pass
- Error variant tests: 6 pass (part of 68 error module tests)
- Pre-existing doctest failure at config.rs line 21 (`~/.unimatrix` path in code block) — confirmed pre-existing before my changes, not introduced here

## Validation Decisions

- `nli_model_name` recognition implemented inline via `is_recognized_nli_model_name()` rather than importing `NliModel::from_config_name` from `unimatrix-embed`. This decouples config compilation from the parallel Wave 1 embed crate extension.
- `merge_configs()` uses `Option::or()` for `nli_model_name`, `nli_model_path`, `nli_model_sha256` (per-project Some wins). Uses `f32::EPSILON` comparison for f32 threshold fields (not `!=`) to correctly detect non-default values.
- Pool floor logic (`rayon_pool_size.max(6).min(8)` when `nli_enabled=true`) is intentionally NOT in `validate()` — it belongs in startup wiring per the pseudocode design. Tests verify the floor logic pattern directly.

## Issues

None. All components of the spawn prompt implemented as specified.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found pattern #1265 (Dual-Path Validation) and existing crt-023 NLI patterns (#2728). No InferenceConfig extension patterns existed before this work.
- Stored: entry #2730 "InferenceConfig struct extension: use ..Default::default() in all existing literal constructions" via `/uni-store-pattern` — covers the struct literal trap, merge_configs update requirements, serde default function rules, and NliModel decoupling technique.
