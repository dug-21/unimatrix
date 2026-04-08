# Risk Coverage Report: crt-050
# Phase-Conditioned Category Affinity (Explicit Read Rebuild)

GH Issue: #542

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Spec C-02/AC-SV-01 incorrectly describes double-encoding on hook-listener write path | `test_observation_input_json_extract_returns_id_for_hook_path` (query_log_tests.rs) | PASS | Full |
| R-02 | `outcome_weight()` vocabulary drift from `infer_gate_result()` | `test_outcome_weight_pass_variants_return_1_0`, `test_outcome_weight_rework_variants_return_0_5`, `test_outcome_weight_fail_variants_return_0_5`, `test_outcome_weight_unknown_and_empty_return_1_0`, `test_outcome_weight_rework_checked_before_fail` (phase_freq_table_tests.rs) | PASS | Full |
| R-03 | Mixed-weight bucket ordering breaks rank-normalization invariant | `test_apply_outcome_weights_mixed_cycles_uses_per_phase_mean`, `test_apply_outcome_weights_per_phase_mean_not_per_cycle` (phase_freq_table_tests.rs) | PASS | Full |
| R-04 | `min_phase_session_pairs` threshold — sparse-signal masking vs. spurious fallback | `test_observations_coverage_warn_below_threshold`, `test_observations_coverage_no_warn_above_threshold`, `test_observations_coverage_no_warn_at_threshold_boundary`, `test_observations_coverage_threshold_one_with_one_pair_no_warn`, `test_observations_coverage_zero_count_fires_warn` (status.rs) | PASS | Full |
| R-05 | `MILLIS_PER_DAY` constant value and boundary arithmetic | `test_millis_per_day_constant_value`, `test_query_phase_freq_observations_respects_ts_millis_boundary`, `test_query_phase_freq_observations_lookback_30_days_arithmetic` (query_log_tests.rs) | PASS | Full |
| R-06 | Config field rename — serde alias and struct-literal sites | `test_inference_config_phase_freq_lookback_days_new_name_deserializes`, `test_inference_config_query_log_lookback_days_alias_deserializes` (config.rs) | PASS | Full |
| R-07 | `phase_category_weights()` breadth formula vs. weighted-freq sum | `test_phase_category_weights_breadth_not_freq_sum`, `test_phase_category_weights_single_category_returns_1_0`, `test_phase_category_weights_two_categories_sums_to_1_0`, `test_phase_category_weights_multiple_phases_independent` (phase_freq_table_tests.rs) | PASS | Full |
| R-08 | NULL `feature_cycle` sessions produce unweighted signal without error | `test_query_phase_outcome_map_excludes_null_feature_cycle_sessions` (query_log_tests.rs), `test_phase_freq_rebuild_null_feature_cycle` (test_lifecycle.py) | PASS | Full |
| R-09 | `phase_category_weights()` visibility deferred to W3-1 | Documented open item in ADR-008 / spec C-10; no blocking test required | N/A | Documented gap (intentional deferral) |
| R-10 | `hook` vs. `hook_event` column name | `test_query_phase_freq_observations_filters_pretooluse_only` (query_log_tests.rs) — runtime SQL error would surface if wrong column name used | PASS | Full |
| R-11 | No index on `observations.hook` / `observations.phase`; full-scan latency | No test — operational/monitoring concern only; ts_millis index narrows window first | N/A | Documented gap (non-testable via unit/integration) |
| R-12 | Unknown outcome strings default to 1.0 silently | Covered by `test_outcome_weight_unknown_and_empty_return_1_0` (R-02 exhaustive test) | PASS | Full (via R-02) |

---

## Test Results

### Unit Tests

- **Total across workspace:** 4714
- **Passed:** 4714
- **Failed:** 0

#### crt-050 Feature-Specific Unit Tests

| Module | Tests | Notes |
|--------|-------|-------|
| `unimatrix-store::query_log_tests` | 20 crt-050 tests | Query A/B, MILLIS_PER_DAY, write-path contract, count_phase_session_pairs |
| `unimatrix-server::services::phase_freq_table_tests` | ~30 new tests | outcome_weight, apply_outcome_weights, phase_category_weights, rebuild contracts |
| `unimatrix-server::infra::config` | ~10 new tests | serde alias, min_phase_session_pairs default/range, phase_freq_lookback_days |
| `unimatrix-server::services::status` | ~5 new tests | observations_coverage warn/no-warn at threshold boundary |

All unit tests: **PASS** (0 failures, 0 ignored related to crt-050)

### Integration Tests

#### Smoke Suite (`-m smoke`)

- **Total:** 23
- **Passed:** 23
- **Failed:** 0
- **Duration:** ~3m 19s
- **Result:** PASS — mandatory gate cleared

#### Lifecycle Suite (`test_lifecycle.py`)

- **Total:** 49 (includes 1 new crt-050 test)
- **Passed:** 49
- **xfailed:** 5 (pre-existing, not caused by crt-050)
- **xpassed:** 2 (`test_search_multihop_injects_terminal_active`, `test_inferred_edge_count_unchanged_by_cosine_supports`) — pre-existing xfail markers whose underlying bugs were incidentally fixed in earlier features; not caused by crt-050; markers and issues remain per USAGE-PROTOCOL.md (removal is the responsibility of the bug-fix PR, not this feature)
- **Failed:** 0
- **Duration:** ~9m 27s
- **Result:** PASS

**New test added:**

`test_phase_freq_rebuild_null_feature_cycle` — validates AC-15 / FR-10 / R-08: sessions without `feature_cycle` do not cause errors; cold-start PhaseFreqTable (`use_fallback=true`) produces neutral 1.0 scores; `context_search` returns results unblocked.

---

## AC-09 Grep Verification

```
grep -r 'query_phase_freq_table' crates/ --include='*.rs'
```

Result: 1 match — a doc comment in `query_log.rs` saying "Replaces `query_phase_freq_table`." No call sites remain. **PASS.**

---

## Config Field Rename Grep Verification

```
grep -r 'query_log_lookback_days' crates/ --include='*.rs'
```

Result: Matches only in:
- `#[serde(alias = "query_log_lookback_days")]` annotation (correct — the alias)
- Comments and doc strings referencing the old name for context
- The serde alias test (`test_inference_config_query_log_lookback_days_alias_deserializes`)

No remaining struct literal sites using the old field name as a Rust identifier. **PASS.**

---

## Gaps

**R-09 (visibility deferral):** `phase_category_weights()` visibility to W3-1 (ASS-029) is intentionally deferred. The method is `pub` within `unimatrix-server` but not re-exported from the crate root. A tracked open item exists per ADR-008 / spec C-10. No test required at this stage — unit tests confirm the method is callable within the server crate.

**R-11 (index latency):** No index on `observations.hook` or `observations.phase`. The PreToolUse filter applies post-scan on the `ts_millis`-indexed window. This is an operational/monitoring concern; no unit or integration test applies. Acceptable given the ts_millis index narrows the window first.

**AC-12 (MRR eval harness):** Out of scope for this report — the eval harness gate (`product/research/ass-039/harness/`) is a separate execution gate, not executed in Stage 3c.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_query_phase_freq_observations_returns_rows_when_observations_populated` (a); `test_query_phase_freq_observations_returns_empty_when_observations_empty` (b) |
| AC-02 | PASS | `test_query_phase_freq_observations_includes_all_four_tool_variants`; `test_query_phase_freq_observations_excludes_context_search_tool`; `test_query_phase_freq_observations_filters_pretooluse_only` |
| AC-03 | PASS | `test_query_phase_freq_observations_cast_handles_string_form_id` |
| AC-04 | PASS | `test_apply_outcome_weights_single_cycle_pass_weights_1_0`; `test_apply_outcome_weights_single_cycle_rework_weights_0_5`; no SQL CASE expression confirmed by code review |
| AC-05 | PASS | `test_apply_outcome_weights_no_outcome_rows_defaults_to_1_0`; `test_apply_outcome_weights_missing_phase_defaults_to_1_0` |
| AC-06 | PASS | All 7 pre-existing `PhaseFreqTable` contract tests pass without modification |
| AC-07 | PASS | `test_query_phase_freq_observations_respects_ts_millis_boundary`; `test_query_phase_freq_observations_lookback_30_days_arithmetic` |
| AC-08 | PASS | `test_phase_category_weights_cold_start_returns_empty_map`; `test_phase_category_weights_two_categories_sums_to_1_0`; `test_phase_category_weights_single_category_returns_1_0` |
| AC-09 | PASS | grep confirms zero call sites for `query_phase_freq_table` in `*.rs` (only doc comment reference remains) |
| AC-10 | PASS | `test_inference_config_phase_freq_lookback_days_new_name_deserializes`; `test_inference_config_query_log_lookback_days_alias_deserializes`; `test_inference_config_crt050_defaults` |
| AC-11 | PASS | `test_observations_coverage_warn_below_threshold` (warn emitted); `test_observations_coverage_no_warn_above_threshold` (no warn); `run_phase_freq_table_alignment_check` references `phase_freq_lookback_days`; grep confirms no `query_log_lookback_days` in status.rs diagnostic code |
| AC-12 | DEFERRED | MRR eval harness gate — separate execution environment, not Stage 3c scope |
| AC-13a | PASS | `test_query_phase_freq_observations_returns_empty_when_observations_empty` |
| AC-13b | PASS | `test_apply_outcome_weights_single_cycle_pass_weights_1_0` |
| AC-13c | PASS | `test_apply_outcome_weights_single_cycle_rework_weights_0_5` |
| AC-13d | PASS | `test_outcome_weight_fail_variants_return_0_5` |
| AC-13e | PASS | `test_apply_outcome_weights_no_outcome_rows_defaults_to_1_0`; `test_apply_outcome_weights_missing_phase_defaults_to_1_0` |
| AC-13f | PASS | `test_query_phase_freq_observations_includes_all_four_tool_variants` |
| AC-13g | PASS | `test_query_phase_freq_observations_excludes_null_id_observations` |
| AC-13h | PASS | `test_phase_category_weights_cold_start_returns_empty_map`; `test_phase_category_weights_two_categories_sums_to_1_0` |
| AC-14 | PASS | `test_observations_coverage_warn_below_threshold` (N-1 → use_fallback); `test_observations_coverage_no_warn_at_threshold_boundary` (N → normal); `test_inference_config_min_phase_session_pairs_deserializes`; `test_validate_min_phase_session_pairs_zero_is_error` |
| AC-15 | PASS | `test_query_phase_outcome_map_excludes_null_feature_cycle_sessions`; `test_phase_freq_rebuild_null_feature_cycle` (infra-001 lifecycle) |
| AC-SV-01 | PASS | `test_observation_input_json_extract_returns_id_for_hook_path` — confirms plain JSON, no double-encoding |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3806, #3004, #3736, #4210, #4147 (testing patterns, phase-affinity, lesson-learned on gate failures). Entry #3004 (causal integration test three-step pattern) was directly relevant to structuring the lifecycle integration test.
- Stored: nothing novel to store — the NULL feature_cycle degradation path pattern is feature-specific to crt-050/AC-15 and is not yet a cross-feature testing pattern worth promoting to the knowledge base.
