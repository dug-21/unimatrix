# Agent Report: nan-009-agent-2-testplan

Phase: Test Plan Design (Stage 3a)
Feature: nan-009 Phase-Stratified Eval Scenarios (GH #400)

---

## Output Files

All seven test plan files produced:

- `/workspaces/unimatrix/product/features/nan-009/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/nan-009/test-plan/scenario-extraction.md`
- `/workspaces/unimatrix/product/features/nan-009/test-plan/result-passthrough.md`
- `/workspaces/unimatrix/product/features/nan-009/test-plan/report-aggregation.md`
- `/workspaces/unimatrix/product/features/nan-009/test-plan/report-rendering.md`
- `/workspaces/unimatrix/product/features/nan-009/test-plan/report-entrypoint.md`
- `/workspaces/unimatrix/product/features/nan-009/test-plan/documentation.md`

---

## Risk Coverage Mapping

| Risk ID | Priority | Test Scenario(s) | Component File |
|---------|----------|-----------------|----------------|
| R-01 (Critical) | `test_compute_phase_stats_null_bucket_label`, `test_report_round_trip_phase_section_null_label` | report-aggregation, report-rendering |
| R-02 (Critical) | `test_report_round_trip_phase_section_7_distribution` (assertions 2+4+5), updated `test_report_contains_all_five_sections` | report-rendering |
| R-03 (High) | `test_report_round_trip_phase_section_7_distribution` (assertion 3), `test_scenario_result_phase_round_trip_serde` | report-rendering, result-passthrough |
| R-04 (Critical) | `test_scenarios_extract_phase_non_null`, `test_scenarios_extract_phase_null` + helper update | scenario-extraction |
| R-05 (High) | `test_scenario_result_phase_null_serialized_as_null`, `test_scenario_context_phase_null_absent_from_jsonl` | result-passthrough, scenario-extraction |
| R-06 (High) | `test_replay_scenario_phase_not_in_search_params` + code review | result-passthrough |
| R-07 (Med) | `test_compute_phase_stats_all_null_returns_empty`, `test_render_phase_section_absent_when_stats_empty` | report-aggregation, report-rendering |
| R-08 (Med) | `test_compute_phase_stats_null_bucket_sorts_last` (3 named phases + null) | report-aggregation |
| R-09 (Med) | `test_render_phase_section_empty_input_returns_empty_string`, `test_report_round_trip_null_phase_only_no_section_6` | report-rendering |
| R-10 (Low) | Documentation review (5 AC-07 items) | documentation |
| R-11 (Low) | `wc -l aggregate.rs` file size check | documentation |
| R-12 (Med) | `test_section_2_phase_label_non_null_present`, `test_section_2_phase_label_null_absent` | report-rendering |
| IR-01 | `test_scenarios_extract_phase_non_null` (full SQL path) | scenario-extraction |
| IR-02 | `test_replay_scenario_phase_not_in_search_params` | result-passthrough |
| IR-03 | `test_report_round_trip_phase_section_7_distribution` | report-entrypoint |
| EC-01 | `test_compute_phase_stats_empty_results_returns_empty` | report-aggregation |
| EC-05 | `test_report_deserializes_legacy_result_missing_phase_key` | report-entrypoint |
| EC-06 | `test_report_deserializes_explicit_null_phase_key` | report-entrypoint |

Total new tests specified: 18
Tests requiring updates: 2 (`test_report_contains_all_five_sections`, `test_report_round_trip_cc_at_k_icd_fields_and_section_6`)
Helper updates: 1 (`insert_query_log_row` — add `phase: Option<&str>` parameter)
Code review checkpoints: 1 (R-06 replay purity)

---

## Integration Harness Plan

The infra-001 MCP harness has no applicable suites for nan-009. The eval pipeline is
CLI-only and is not exercised via MCP JSON-RPC.

Required: `pytest -m smoke` mandatory gate to confirm no MCP server regression.

No new infra-001 tests planned — all integration testing is at the Rust level via
`cargo test` with real SQLite databases.

---

## Key Decisions Made in Test Plans

1. **`test_scenario_result_phase_round_trip_serde` placement**: This test bridges the
   runner-side and report-side `ScenarioResult` types. It may need to live in
   `eval/report/tests.rs` rather than a runner test file to access the report module's
   local type. The delivery agent must confirm visibility.

2. **`test_report_contains_all_five_sections` rename**: The delivery agent should rename
   to `test_report_contains_all_seven_sections`. If renaming causes diff noise, the name
   is secondary to the updated assertions.

3. **`"(unset)"` canonical literal**: The null-bucket label test `test_compute_phase_stats_null_bucket_label` is the ground truth for R-01. The implementation must match exactly.

4. **Non-null phase value in round-trip test**: `"delivery"` is specified as the canonical
   non-trivial value throughout all round-trip tests. Using `None` or `Some("")` would
   allow a partial dual-type update to pass silently.

---

## Open Questions

None. All design decisions were resolved in the IMPLEMENTATION-BRIEF (V-1, V-2, V-3
variances resolved; ADR-001, ADR-002, ADR-003 finalized).

---

## Knowledge Stewardship

- Queried: /uni-knowledge-search "nan-009 architectural decisions" (category: decision, topic: nan-009) — no results (ADRs not yet stored in Unimatrix for this feature).
- Queried: /uni-knowledge-search "eval harness round-trip dual-type serde testing patterns" — found #885 (serde gate failure), #3526 (round-trip dual-type pattern), #3522 (nan-008 ADR-003 precedent), #2928 (string-refactor test plan patterns). All applied.
- Stored: entry #3557 "Eval Harness New Field: Dual-Direction Serde Test + Non-Trivial Round-Trip Value" via /uni-store-pattern — novel synthesis: the asymmetric serde annotation pattern (producer always emits, consumer suppresses) combined with the mandatory non-trivial round-trip value requirement was not previously captured as a single actionable pattern.
