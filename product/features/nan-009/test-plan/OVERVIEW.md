# Test Plan Overview: nan-009 Phase-Stratified Eval Scenarios

GH Issue: #400

---

## Overall Test Strategy

nan-009 is a pure pipeline instrumentation feature: no retrieval logic changes, no new
MCP tools, no schema changes. All test activity is unit and integration at the Rust level,
within `crates/unimatrix-server/src/eval/`. The infra-001 MCP harness is not applicable
(eval pipeline is CLI-only, not exercised via MCP JSON-RPC).

### Test Layers

| Layer | Target | Framework |
|-------|--------|-----------|
| Unit | `compute_phase_stats`, `render_phase_section`, serde annotations | `cargo test` (sync `#[test]`) |
| Integration (Rust DB) | `run_scenarios` extraction via real SQLite, `run_report` pipeline | `cargo test` (`#[tokio::test]` / sync `#[test]`) |
| Code review | `replay.rs` phase-not-in-search-params | Manual + test assertion |
| Documentation | `docs/testing/eval-harness.md` | Manual review |

---

## Risk-to-Test Mapping

| Risk ID | Priority | Test Scenario(s) | Component File |
|---------|----------|-----------------|----------------|
| R-01 | Critical | `test_compute_phase_stats_null_bucket_label`, `test_report_round_trip_phase_section_null_label` | report-aggregation, report-rendering |
| R-02 | Critical | `test_report_round_trip_phase_section_7_distribution` (negative assertion + order), `test_report_contains_all_five_sections` updated to seven | report-rendering, report-entrypoint |
| R-03 | High | `test_report_round_trip_phase_section_7_distribution` (asserts "delivery" in section 6), `test_scenario_result_phase_round_trip_serde` | report-entrypoint, result-passthrough |
| R-04 | Critical | `test_scenarios_extract_phase_non_null`, `test_scenarios_extract_phase_null` (require updated helper) | scenario-extraction |
| R-05 | High | `test_scenario_result_phase_null_serialized_as_null`, `test_scenario_context_phase_null_absent_from_jsonl` | result-passthrough, scenario-extraction |
| R-06 | High | `test_replay_scenario_phase_not_in_search_params` + code review | result-passthrough |
| R-07 | Med | `test_compute_phase_stats_all_null_returns_empty`, `test_render_phase_section_absent_when_stats_empty` | report-aggregation, report-rendering |
| R-08 | Med | `test_compute_phase_stats_null_bucket_sorts_last` | report-aggregation |
| R-09 | Med | `test_render_phase_section_empty_input_returns_empty_string`, `test_report_round_trip_null_phase_only_no_section_6` | report-rendering |
| R-10 | Low | Documentation review | documentation |
| R-11 | Low | File size check (`wc -l aggregate.rs`) | documentation |
| R-12 | Med | `test_section_2_phase_label_non_null_present`, `test_section_2_phase_label_null_absent` | report-rendering |
| IR-01 | High | `test_scenarios_extract_phase_non_null` (full SQL path) | scenario-extraction |
| IR-02 | Med | `test_replay_scenario_phase_not_in_search_params` | result-passthrough |
| IR-03 | High | `test_report_round_trip_phase_section_7_distribution` | report-entrypoint |
| EC-01 | Med | `test_compute_phase_stats_empty_results_returns_empty` | report-aggregation |
| EC-05 | Med | `test_report_deserializes_legacy_result_missing_phase_key` | report-entrypoint |
| EC-06 | Med | `test_report_deserializes_explicit_null_phase_key` | report-entrypoint |

---

## Cross-Component Test Dependencies

```
scenario-extraction tests
    └─ insert_query_log_row helper (MUST be updated to accept phase: Option<&str>)
           └─ test_scenarios_extract_phase_non_null
           └─ test_scenarios_extract_phase_null

result-passthrough tests
    └─ runner ScenarioResult serde (no skip_serializing_if)
           └─ test_scenario_result_phase_null_serialized_as_null
    └─ replay_scenario ServiceSearchParams inspection
           └─ test_replay_scenario_phase_not_in_search_params

report-aggregation tests
    └─ PhaseAggregateStats struct + compute_phase_stats (new)
           └─ test_compute_phase_stats_* (unit, sync)

report-rendering tests
    └─ render_phase_section (new) + render_report (signature change)
           └─ test_render_phase_section_*
           └─ test_section_2_phase_*

report-entrypoint tests (run_report pipeline — integration)
    └─ test_report_round_trip_phase_section_7_distribution  ← PRIMARY GUARD
           depends on: runner ScenarioResult serializes phase correctly
           depends on: report-side ScenarioResult deserializes phase correctly
           depends on: compute_phase_stats called and wired
           depends on: render_phase_section produces "## 6." before "## 7."
    └─ test_report_contains_all_five_sections  ← UPDATE EXISTING
           update to assert seven sections, not five
    └─ test_report_round_trip_cc_at_k_icd_fields_and_section_6  ← UPDATE EXISTING
           update to assert "## 7. Distribution Analysis" (was "## 6.")
```

---

## Integration Harness Plan (infra-001)

### Applicability Assessment

nan-009 modifies the `eval/` CLI pipeline, not the MCP server. The infra-001 harness
exclusively exercises the MCP JSON-RPC interface. No infra-001 suite tests the
`eval scenarios`, `eval run`, or `eval report` CLI commands.

**Conclusion: no infra-001 suites apply to nan-009.**

The mandatory smoke gate (`pytest -m smoke`) must still be executed to confirm no
regression in the MCP server itself — the eval code path shares the same crate.

### Required Runs in Stage 3c

| Suite | Rationale |
|-------|-----------|
| `smoke` (mandatory gate) | Confirm eval-module changes did not break MCP server compilation or runtime |

### New Integration Tests (infra-001)

None. nan-009 has no MCP-visible behavior. All integration testing is at the Rust level
(`cargo test`) exercising real SQLite databases and file I/O, which is the appropriate
boundary for this feature's integration risk.

---

## Test File Targets

| Component | Test file | New tests | Updated tests |
|-----------|-----------|-----------|---------------|
| Scenario Extraction | `eval/scenarios/tests.rs` | `test_scenarios_extract_phase_non_null`, `test_scenarios_extract_phase_null`, `test_scenario_context_phase_null_absent_from_jsonl`, `test_scenario_context_phase_non_null_present` | `insert_query_log_row` helper signature |
| Result Passthrough | `eval/runner/` (tests.rs or inline) | `test_scenario_result_phase_null_serialized_as_null`, `test_replay_scenario_phase_not_in_search_params` | — |
| Report Aggregation | `eval/report/tests.rs` | `test_compute_phase_stats_*` (5 tests), `test_render_phase_section_*` (2 tests) | — |
| Report Rendering | `eval/report/tests.rs` | `test_section_2_phase_label_*` (2 tests), `test_report_round_trip_null_phase_only_no_section_6`, `test_report_round_trip_phase_section_7_distribution` | `test_report_contains_all_five_sections`, `test_report_round_trip_cc_at_k_icd_fields_and_section_6` |
| Report Entry Point | `eval/report/tests.rs` | `test_report_deserializes_legacy_result_missing_phase_key`, `test_report_deserializes_explicit_null_phase_key` | — |
| Documentation | `docs/testing/eval-harness.md` | — | — |

---

## Minimum Required Test Count

- New tests: 18 scenario-level tests (per RISK-TEST-STRATEGY coverage summary)
- Updated tests: 2 existing tests
- Updated helper: 1 (`insert_query_log_row`)
- Code review checkpoint: 1 (R-06 replay purity)
- Documentation review: 1 (AC-07)
