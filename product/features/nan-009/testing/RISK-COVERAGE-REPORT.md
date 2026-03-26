# Risk Coverage Report: nan-009

GH Issue: #400

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `"(none)"` vs `"(unset)"` null-label conflict | `test_compute_phase_stats_null_bucket_label`, `test_report_round_trip_phase_section_null_label` | PASS | Full |
| R-02 | Section renumbering: Distribution Analysis shifts §6→§7 | `test_report_round_trip_phase_section_7_distribution`, `test_report_contains_all_seven_sections` | PASS | Full |
| R-03 | Dual-type partial update: `ScenarioResult.phase` added to runner but not report | `test_report_round_trip_phase_section_7_distribution`, `test_scenario_result_phase_absent_key_deserializes_as_none` | PASS | Full |
| R-04 | `insert_query_log_row` helper not updated to bind `phase` | `test_scenarios_extract_phase_non_null`, `test_scenarios_extract_phase_null` | PASS | Full |
| R-05 | `skip_serializing_if` on wrong copy suppresses `phase` in result JSON | `test_scenario_result_phase_null_serialized_as_null`, `test_scenario_context_phase_null_absent_from_jsonl` | PASS | Full |
| R-06 | Phase injected into `ServiceSearchParams` or `AuditContext` during replay | `test_scenario_result_phase_null_serialized_as_null` + code review of `replay.rs` | PASS | Full |
| R-07 | `compute_phase_stats` returns non-empty vec when all phases are `None` | `test_compute_phase_stats_all_null_returns_empty`, `test_render_phase_section_absent_when_stats_empty` | PASS | Full |
| R-08 | `"(unset)"` bucket sort order: ASCII `(` precedes `a-z` | `test_compute_phase_stats_null_bucket_sorts_last` | PASS | Full |
| R-09 | `render_phase_section` called with empty stats produces orphaned heading | `test_render_phase_section_empty_input_returns_empty_string`, `test_report_round_trip_null_phase_only_no_section_6` | PASS | Full |
| R-10 | UDS-only corpus: absence of section 6 misread as bug | Documentation review (AC-07) | PASS | Full (doc) |
| R-11 | `aggregate.rs` grows past 500 lines | File size check: 487 lines | PASS | Full |
| R-12 | Section 2 phase label rendered for null-phase scenarios | `test_report_section_2_includes_phase_label_when_non_null`, `test_report_section_2_phase_label_null_absent` | PASS | Full |
| IR-01 | SQL column name mismatch in `try_get("phase")` | `test_scenarios_extract_phase_non_null` (full SQL path) | PASS | Full |
| IR-02 | Phase passthrough location in `replay.rs` | `test_scenario_result_phase_null_serialized_as_null` + code review | PASS | Full |
| IR-03 | `run_report` pipeline wiring: `compute_phase_stats` not called before `render_report` | `test_report_round_trip_phase_section_7_distribution` | PASS | Full |

---

## Test Results

### Unit Tests (cargo test -p unimatrix-server --lib)

- Total: 2159
- Passed: 2159
- Failed: 0
- Finished in: 5.99s

### Targeted Eval/Phase Tests (subset of unit tests)

28 nan-009-specific tests executed, all passing:

| Test | Module | Risk(s) |
|------|--------|---------|
| `test_compute_phase_stats_null_bucket_label` | `eval::report::tests_phase` | R-01 |
| `test_compute_phase_stats_empty_input_returns_empty` | `eval::report::tests_phase` | EC-01, FM-03 |
| `test_compute_phase_stats_all_null_returns_empty` | `eval::report::tests_phase` | R-07, AC-09(4) |
| `test_compute_phase_stats_null_bucket_sorts_last` | `eval::report::tests_phase` | R-08, AC-05 |
| `test_compute_phase_stats_single_phase` | `eval::report::tests_phase` | AC-05 |
| `test_compute_phase_stats_mean_values_correct` | `eval::report::tests_phase` | AC-05, AC-09(3) |
| `test_compute_phase_stats_multiple_phases` | `eval::report::tests_phase` | AC-09(3) |
| `test_render_phase_section_empty_input_returns_empty_string` | `eval::report::tests_phase` | R-09 |
| `test_render_phase_section_renders_table_header` | `eval::report::tests_phase` | AC-04 |
| `test_render_phase_section_renders_unset_bucket` | `eval::report::tests_phase` | R-01 |
| `test_scenario_result_phase_absent_key_deserializes_as_none` | `eval::report::tests_phase` | AC-06, EC-05 |
| `test_report_deserializes_explicit_null_phase_key` | `eval::report::tests_phase` | AC-06, EC-06 |
| `test_render_phase_section_absent_when_stats_empty` | `eval::report::tests_phase_pipeline` | R-07, AC-04, AC-09(5) |
| `test_report_round_trip_null_phase_only_no_section_6` | `eval::report::tests_phase_pipeline` | R-09, AC-04 |
| `test_report_round_trip_phase_section_7_distribution` | `eval::report::tests_phase_pipeline` | R-02, R-03, AC-11, AC-12 |
| `test_report_round_trip_phase_section_null_label` | `eval::report::tests_phase_pipeline` | R-01 |
| `test_report_section_2_includes_phase_label_when_non_null` | `eval::report::tests_phase_pipeline` | R-12, AC-08 |
| `test_report_section_2_phase_label_null_absent` | `eval::report::tests_phase_pipeline` | R-12, AC-08 |
| `test_report_section_6_omitted_when_all_phases_null` | `eval::report::tests_phase_pipeline` | R-07, R-09 |
| `test_report_section_6_present_when_phase_non_null` | `eval::report::tests_phase_pipeline` | AC-04 |
| `test_report_contains_all_seven_sections` | `eval::report::tests` | R-02, AC-12 |
| `test_report_round_trip_cc_at_k_icd_fields_and_section_6` | `eval::report::tests_distribution_pipeline` | SR-02, R-02 (negative assertion: `## 7. Distribution Analysis` present, old `## 6. Distribution Analysis` absent) |
| `test_scenario_result_phase_null_serialized_as_null` | `eval::runner::output::tests` | R-05, AC-03 |
| `test_scenario_result_phase_non_null_serialized` | `eval::runner::output::tests` | R-05, AC-03 |
| `test_scenario_context_phase_null_absent_from_jsonl` | `eval::scenarios::tests::tests` | R-05, AC-02, AC-09(2) |
| `test_scenario_context_phase_non_null_present_in_jsonl` | `eval::scenarios::tests::tests` | AC-01, AC-09(1) |
| `test_scenarios_extract_phase_non_null` | `eval::scenarios::tests::tests` | R-04, IR-01, AC-01, AC-10 |
| `test_scenarios_extract_phase_null` | `eval::scenarios::tests::tests` | R-04, AC-02, AC-10 |

### Integration Tests (infra-001 smoke gate)

- Suite: `smoke` (`-m smoke`)
- Total: 20
- Passed: 20
- Failed: 0
- Finished in: 175.02s (2:55)

No infra-001 suites beyond smoke apply to nan-009 (eval pipeline is CLI-only, not
exercised via MCP JSON-RPC — confirmed in test-plan/OVERVIEW.md).

---

## Code Review Results

### R-06: Phase Not Forwarded to ServiceSearchParams (VERIFIED)

Manual code review of `crates/unimatrix-server/src/eval/runner/replay.rs`:

- Line 80: `phase: record.context.phase.clone()` — assigned to `ScenarioResult` only.
- Lines 96–108: `ServiceSearchParams` construction contains no `phase` field.
- Lines 110–118: `AuditContext` construction contains no `phase` field.

**Result: PASS.** Phase is metadata-only during replay. Measurement purity constraint
(SCOPE.md Constraint 3, FR-06) is satisfied.

### R-11: aggregate.rs File Size (VERIFIED)

`wc -l crates/unimatrix-server/src/eval/report/aggregate.rs` → **487 lines**.

Below the 500-line limit (Constraint 7). No extraction to `aggregate_phase.rs` required.

---

## Implementation Details Verified

| Item | Expected | Actual | Status |
|------|----------|--------|--------|
| `ScenarioContext.phase` serde | `#[serde(default, skip_serializing_if = "Option::is_none")]` | Confirmed in `types.rs` line 73 | PASS |
| Runner `ScenarioResult.phase` serde | `#[serde(default)]` only, no `skip_serializing_if` | Confirmed in `runner/output.rs` | PASS |
| Report `ScenarioResult.phase` serde | `#[serde(default)]` only | Confirmed in `report/mod.rs` line 134 | PASS |
| `PhaseAggregateStats` location | `report/mod.rs` with `#[derive(Debug, Default)]` | Confirmed lines 153/174 | PASS |
| `compute_phase_stats` wired in `run_report` | Called at Step 4, result passed to `render_report` | Confirmed `mod.rs` lines 277, 283 | PASS |
| `phase` in SELECT clause | `output.rs` SELECT includes `phase` | Confirmed `output.rs` lines 108–109 | PASS |
| `phase` in `build_scenario_record` | `row.try_get::<Option<String>, _>("phase")?` | Confirmed `extract.rs` line 35 | PASS |
| `insert_query_log_row` helper | Accepts `phase: Option<&str>` at position 9 | Confirmed `tests.rs` lines 40, 56 | PASS |
| Documentation (AC-07) | 5 items in `docs/testing/eval-harness.md` | All 5 confirmed: `context.phase` field, vocabulary snapshot, section 6 reference, `context_cycle` requirement, migration governance | PASS |

---

## Gaps

None. All 12 risks (R-01 through R-12) have test coverage. All integration risks
(IR-01 through IR-03) have test coverage. All edge cases with testable behavior
(EC-01, EC-05, EC-06) have test coverage.

Low-priority risks confirmed:
- **R-10** (UDS corpus, no section 6): documentation-only mitigation confirmed in
  `docs/testing/eval-harness.md` (SR-06 note present).
- **R-11** (file size): `aggregate.rs` is 487 lines — 13 lines under the 500-line
  constraint. No extraction required.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_scenarios_extract_phase_non_null`: insert `phase="delivery"`, assert `context.phase == "delivery"` in JSONL |
| AC-02 | PASS | `test_scenarios_extract_phase_null` + `test_scenario_context_phase_null_absent_from_jsonl`: null phase omits key from JSONL |
| AC-03 | PASS | `test_scenario_result_phase_null_serialized_as_null`: runner always emits `"phase":null`; `test_scenario_result_phase_non_null_serialized`: non-null value emitted correctly |
| AC-04 | PASS | `test_report_section_6_present_when_phase_non_null`: section 6 present when non-null phase exists; `test_report_section_6_omitted_when_all_phases_null` + `test_report_round_trip_null_phase_only_no_section_6`: section 6 absent when all null |
| AC-05 | PASS | `test_compute_phase_stats_null_bucket_sorts_last`: one row per distinct phase, `"(unset)"` last; `test_compute_phase_stats_mean_values_correct`: correct means |
| AC-06 | PASS | `test_scenario_result_phase_absent_key_deserializes_as_none`: missing key defaults to `None`; `test_report_deserializes_explicit_null_phase_key`: explicit null defaults to `None` |
| AC-07 | PASS | Manual review of `docs/testing/eval-harness.md`: (1) `context.phase` field documented at line 170; (2) known vocabulary snapshot at line 180; (3) section 6 reference at line 432; (4) `context_cycle` population requirement at line 171; (5) migration-based governance at line 184 |
| AC-08 | PASS | `test_report_section_2_includes_phase_label_when_non_null`: phase label in section 2 for non-null; `test_report_section_2_phase_label_null_absent`: no label for null-phase scenarios |
| AC-09 | PASS | All 5 items: (1) `test_scenario_context_phase_non_null_present_in_jsonl`; (2) `test_scenario_context_phase_null_absent_from_jsonl`; (3) `test_compute_phase_stats_multiple_phases`; (4) `test_compute_phase_stats_all_null_returns_empty`; (5) `test_report_section_6_omitted_when_all_phases_null` |
| AC-10 | PASS | `test_scenarios_extract_phase_non_null` (`phase=Some("delivery")`) + `test_scenarios_extract_phase_null` (`phase=None`) using updated `insert_query_log_row` helper |
| AC-11 | PASS | `test_report_round_trip_phase_section_7_distribution`: runner JSON with `phase="delivery"` → `run_report` → asserts section 6 present, "delivery" in section 6, section 7 present, `pos("## 6.") < pos("## 7.")`, old heading absent |
| AC-12 | PASS | `test_report_round_trip_phase_section_7_distribution` includes `!content.contains("## 6. Distribution Analysis")` + position order assertion; `test_report_contains_all_seven_sections` enumerates all 7 headings in order |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for testing procedures (category: procedure) — found
  entries #553, #750, #296, #487 (testing procedures), #3479 (coupled-test pattern).
  Most relevant: #487 (workspace tests without hanging) and #3479 (coupled-test pattern).
  Entries confirmed prior knowledge; no new testing procedure patterns emerged from this
  feature's execution.
- Stored: nothing novel to store — all patterns already documented (#3426 section-order
  regression, #3526 round-trip dual-type, #3543 nullable column helper, #3550 dual-type
  constraint). The nan-009 execution followed these patterns exactly as documented;
  no new patterns warranted storage.
