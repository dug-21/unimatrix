# crt-026 Agent 5 (config) — Implementation Report

Agent ID: crt-026-agent-5-config
Feature: crt-026 WA-2 Session Context Enrichment
Component: InferenceConfig — new weight fields
Issue: #341

---

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs` — primary scope
- `crates/unimatrix-server/src/services/search.rs` — compilation blocker fix (FusionWeights effective() pass-through + test literal sites)
- `crates/unimatrix-server/src/services/briefing.rs` — compilation blocker fix (ServiceSearchParams call site)
- `crates/unimatrix-server/src/mcp/tools.rs` — compilation blocker fix (ServiceSearchParams call site + crt-026 histogram pre-resolution)

---

## Changes in config.rs (Primary Scope)

1. Added `w_phase_histogram: f64` field to `InferenceConfig` with `#[serde(default = "default_w_phase_histogram")]`; doc-comment cites WA-2, ADR-004, ASS-028.
2. Added `w_phase_explicit: f64` field to `InferenceConfig` with `#[serde(default = "default_w_phase_explicit")]`; doc-comment cites ADR-003, W3-1 placeholder.
3. Added `default_w_phase_histogram() -> f64 { 0.02 }` and `default_w_phase_explicit() -> f64 { 0.0 }` default functions in the "Fusion weight default value functions" section.
4. Extended `InferenceConfig::default()` struct literal with `w_phase_histogram: 0.02` and `w_phase_explicit: 0.0`.
5. Added `phase_weight_checks` slice in `validate()` after the existing six-field range-check loop; both new fields get `[0.0, 1.0]` per-field range checks using `ConfigError::NliFieldOutOfRange`. The six-weight sum check is NOT modified (ADR-004, OQ-A).
6. Extended merged-config construction block with epsilon-comparison merge for both new fields.

---

## Compilation Blockers Fixed (Out-of-Scope but Required to Compile)

The wave-1 search.rs agent had added `w_phase_histogram` and `w_phase_explicit` to `FusionWeights` and `FusedScoreInputs` struct definitions, but three things were incomplete:

1. **`FusionWeights::effective()` return paths** (search.rs lines 147-191): All three return sites (NLI-active, zero-denominator guard, NLI-absent re-normalized) were missing the new fields. Fixed: pass-through unchanged on all paths, with comment explaining they are excluded from the re-normalization denominator (ADR-004).

2. **`FusedScoreInputs` test literals in search.rs**: 27 test construction sites were missing `phase_histogram_norm` and `phase_explicit_norm`. Fixed via Python script adding both fields (0.0 values) after each `prov_norm:` line in the test section.

3. **`FusionWeights` test literals in search.rs**: 8 test construction sites were missing `w_phase_histogram` and `w_phase_explicit`. Fixed via Python script adding both fields (0.0 values) after each `w_prov:` line in the test section.

4. **`ServiceSearchParams` call sites**: `briefing.rs` line 321 and `tools.rs` line 303 missing `session_id` and `category_histogram`. Fixed with appropriate `None` (briefing, no session context) and crt-026 histogram pre-resolution pattern (tools.rs).

---

## Tests

All 6 tests from test-plan/config.md implemented and passing:

| Test | Status |
|------|--------|
| `test_inference_config_default_phase_weights` (T-CFG-01) | PASS |
| `test_config_validation_rejects_out_of_range_phase_weights` (T-CFG-02) | PASS |
| `test_inference_config_six_weight_sum_unchanged_by_phase_fields` (T-CFG-03) | PASS |
| `test_inference_config_serde_round_trip_phase_fields` (T-CFG-04) | PASS |
| `test_inference_config_missing_phase_fields_use_defaults` (T-CFG-05) | PASS |
| `test_phase_explicit_norm_placeholder_fields_present` (T-CFG-06, AC-09) | PASS |

Full lib test run: **1847 passed, 0 failed**.

---

## Issues / Blockers

None. The workspace compiles clean with zero errors. The intermittent `col018_topic_signal_from_feature_id` failure observed in a workspace-wide run is a pre-existing test isolation issue (passes when run in isolation); confirmed by the fact it passes in the `--lib` run.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — found entry #2730 (InferenceConfig struct extension: use `..Default::default()` in existing literal constructions), entry #646 (backward-compatible config extension via serde(default)). Both applied.
- Stored: entry #3181 "FusionWeights effective() — three return paths all require new fields; test literal sites require mass-update" via `/uni-store-pattern`. This is invisible in source code because the tests compile separately from the lib, making the failures non-obvious during a `cargo build` check.
