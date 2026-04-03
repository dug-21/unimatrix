# Agent Report: crt-042-agent-4-config

## Task
Implement InferenceConfig additions and eval profile for crt-042 (PPR expander).

## Files Modified / Created

- `crates/unimatrix-server/src/infra/config.rs` — modified
- `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` — created

## Implementation Summary

### config.rs — Five coordinated sites updated

**Site 1 (struct body):** Added three fields after `max_s8_pairs_per_batch` with full doc-comments and `#[serde(default = "fn_name")]` annotations:
- `ppr_expander_enabled: bool`
- `expansion_depth: usize`
- `max_expansion_candidates: usize`

**Site 2 (impl Default):** Added three fields to the `InferenceConfig { ... }` literal in `impl Default for InferenceConfig`, delegating to the serde default functions for atomicity.

**Site 3 (serde default functions):** Added `default_ppr_expander_enabled() -> bool { false }`, `default_expansion_depth() -> usize { 2 }`, `default_max_expansion_candidates() -> usize { 200 }` after `default_ppr_max_expand()`.

**Site 4 (validate()):** Added UNCONDITIONAL range checks per ADR-004:
- `expansion_depth` in [1, 10] — validated regardless of `ppr_expander_enabled`
- `max_expansion_candidates` in [1, 1000] — validated regardless of `ppr_expander_enabled`

**Site 5 (merge_configs — hidden):** Extended the `InferenceConfig { ... }` literal in `merge_configs()` with all three new fields using the standard project-wins-if-non-default pattern.

### Unit Tests Added (16 tests)

All tests per test-plan/inference_config.md:

| Test | AC |
|------|-----|
| test_inference_config_expander_fields_defaults | AC-17 |
| test_inference_config_expander_fields_serde_defaults | AC-17 |
| test_unimatrix_config_expander_toml_omitted_produces_defaults | AC-17 |
| test_inference_config_expander_serde_fn_matches_default | R-08 dual-site |
| test_validate_expansion_depth_zero_fails | AC-18 |
| test_validate_expansion_depth_eleven_fails | AC-19 |
| test_validate_expansion_depth_ten_passes | AC-19 boundary |
| test_validate_expansion_depth_one_passes | AC-18 boundary |
| test_validate_max_expansion_candidates_zero_fails | AC-20 |
| test_validate_max_expansion_candidates_1001_fails | AC-21 |
| test_validate_max_expansion_candidates_one_passes | AC-20 boundary |
| test_validate_max_expansion_candidates_1000_passes | AC-21 boundary |
| test_validate_expansion_depth_error_names_field | R-08 error msg |
| test_validate_max_expansion_candidates_error_names_field | R-08 error msg |
| test_inference_config_merged_propagates_expander_fields | R-08 merge site |
| test_inference_config_merged_expander_enabled_project_wins | R-08 merge site |
| test_inference_config_expander_toml_explicit_override | R-08 TOML round-trip |

### eval profile

Created `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` with `[profile]` and `[inference]` sections matching the pseudocode spec exactly.

## Test Results

`cargo test --package unimatrix-server -- config`: **319 passed, 0 failed**

## R-08 Hidden Sites Check

Ran `grep -n "InferenceConfig {" crates/unimatrix-server/src/infra/config.rs`.

Result: All existing `InferenceConfig {` literals in the test suite use `..InferenceConfig::default()` spread — they compile-check correctly without requiring manual updates. The merge function literal (line 2384) was the only non-spread site and was updated as Site 5 above.

No fixes required beyond the merge function, which was already addressed during implementation.

## Issues / Blockers

None. Implementation complete.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (pattern: InferenceConfig coordinated sites) — found entries #3817, #4044, #2730 confirming the known pattern. Entry #4044 described the merge function as a hidden site but did not identify it as the only non-spread literal.
- Stored: entry #4070 "InferenceConfig extension: five sites (four named + hidden merge_configs literal) + unconditional validation" via context_store. Key new insight: the merge function is the ONLY InferenceConfig { literal not using spread, making it the only site the grep check cannot catch by spread-vs-explicit inspection alone — it must always be extended manually.
