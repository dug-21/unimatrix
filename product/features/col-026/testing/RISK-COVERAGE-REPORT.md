# Risk Coverage Report: col-026

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Inline `* 1000` timestamp conversion in PhaseStats bypasses `cycle_ts_to_obs_millis()` | `test_phase_stats_no_inline_multiply`, `test_phase_stats_obs_in_correct_window_millis_boundary`, `test_cycle_ts_to_obs_millis_overflow_guard` | PASS | Full |
| R-02 | Phase window extraction produces wrong boundaries when `cycle_phase_end` events are absent, malformed, or share timestamps | `test_phase_stats_no_phase_end_events`, `test_phase_stats_zero_duration_no_panic`, `test_phase_timeline_empty_phase_name` | PASS | Full |
| R-03 | GateResult inference broken by free-form outcome text — multi-keyword collisions | `test_gate_result_inference` (8 scenarios: PASS/pass, failed/error, rework/REWORK, approved, empty, None, multi-keyword, compass) | PASS | Full |
| R-04 | IN-clause batch query returns fewer rows than requested — cross-feature split arithmetic silently wrong | `test_knowledge_reuse_partial_meta_lookup`, `test_knowledge_reuse_all_meta_missing`, `test_entry_meta_lookup_skipped_on_empty` | PASS | Full |
| R-05 | `is_in_progress` derivation omits the `None` branch — pre-col-024 historical retros misreported | `test_derive_is_in_progress_three_states`, `test_header_status_in_progress`, `test_header_status_omitted_when_none`, `test_header_status_omitted_when_some_false`, `test_cycle_review_is_in_progress_json` (integration) | PASS | Full |
| R-06 | `What Went Well` metric direction table mis-classification | `test_what_went_well_direction_table_all_16_metrics`, `test_what_went_well_present`, `test_what_went_well_absent_no_favorable`, `test_what_went_well_excludes_outlier_metrics`, `test_metric_not_in_direction_table_excluded` | PASS | Full |
| R-07 | Formatter section reorder causes regression in untargeted sections | `test_section_order`, `test_markdown_output_starts_with_header`, `test_all_none_optional_fields_valid_markdown` | PASS | Full |
| R-08 | Threshold language regex fails on composite claim strings | `test_no_threshold_language`, `test_format_claim_with_baseline_no_threshold_pattern`, `test_format_claim_threshold_zero_value`, `test_no_allowlist_in_compile_cycles` | PASS | Full |
| R-09 | `attribution_path` assignment wrong for Mixed/multi-session case | `test_attribution_path_labels` (all 3 path labels), `test_attribution_path_absent_when_none`, `test_attribution_partial`, `test_attribution_full_not_rendered` | PASS | Full |
| R-10 | `phase_stats` formatter annotates wrong phase when finding evidence spans multiple phases | `test_finding_phase_annotation`, `test_finding_phase_multi_evidence`, `test_finding_phase_out_of_bounds_timestamp`, `test_finding_no_phase_annotation_when_phase_stats_none`, `test_finding_phase_no_phase_stats` | PASS | Full |
| R-11 | Tenth threshold language site added by future detection rule goes undetected | `test_no_threshold_language` (scans all 9 enumerated sites) | PASS | Partial — count snapshot not implemented; general regex coverage via `format_claim_with_baseline` |
| R-12 | `phase_stats = Some(vec![])` vs `None` semantic difference | `test_phase_timeline_absent_when_phase_stats_none`, `test_phase_timeline_absent_when_phase_stats_empty`, `test_phase_stats_empty_events_produces_empty_vec` | PASS | Full |
| R-13 | `FeatureKnowledgeReuse` construction sites not updated — compile-time break | Compilation gate: `cargo build` workspace passes clean (3,554 unit tests pass) | PASS | Full |

---

## Test Results

### Unit Tests

**cargo test --workspace summary (all crates)**

| Crate / Module | Passed | Failed |
|---------------|--------|--------|
| unimatrix-observe (all) | 50 | 0 |
| unimatrix-server :: retrospective | 148 | 0 |
| unimatrix-server :: knowledge_reuse | 51 | 0 |
| unimatrix-server :: phase_stats | 19 | 0 |
| unimatrix-server :: report (includes col-026 rec tests) | 33 | 0 |
| **Full workspace total** | **3,554** | **0** |

All 3,554 unit tests across the workspace pass with zero failures.

**Col-026 specific unit test highlights:**

- `test_recommendation_compile_cycles_above_threshold` — asserts action contains "batch" or "iterative", not "allowlist" (AC-19)
- `test_permission_friction_recommendation_independence` — asserts two templates are independent (AC-19)
- `test_compile_cycles_action_no_allowlist` — asserts no "allowlist" or "settings.json" in compile_cycles action (AC-19)
- `test_what_went_well_direction_table_all_16_metrics` — validates all 16 metric direction entries (R-06)
- `test_section_order` — golden section-order test, all 12 section headers in correct sequence (R-07)
- `test_derive_is_in_progress_three_states` — None/Some(true)/Some(false) (R-05)
- `test_gate_result_inference` — all 8 keyword scenarios (R-03)
- `test_phase_stats_no_inline_multiply` — static scan confirms no `* 1000` in PhaseStats code (R-01)

### Integration Tests

**Smoke suite** (`-m smoke`): **20/20 PASS** — mandatory gate confirmed.

**Protocol suite** (`suites/test_protocol.py`): **13/13 PASS**

**Lifecycle suite** (`suites/test_lifecycle.py`): **37 passed, 2 xfailed, 0 failed**
- xfailed (pre-existing, unrelated to col-026):
  - GH#305 — `test_retrospective_baseline_present` (synthetic baseline null)
  - 1 additional pre-existing xfail (lifecycle suite)
- New col-026 test: `test_cycle_review_knowledge_reuse_cross_feature_split` — PASS

**Tools suite** (`suites/test_tools.py`): **94 passed, 1 xfailed, 0 failed** (after fixing bad assertion)
- `test_retrospective_markdown_default` — bad test assertion (asserted old `# Retrospective:` header, col-026 rebrands to `# Unimatrix Cycle Review —`). Fixed per triage rule: bad assertion → fix the test. Documented here.
- xfailed (pre-existing): GH#305 — `test_retrospective_baseline_present`
- New col-026 tests:
  - `test_cycle_review_phase_timeline_present` — PASS (AC-06)
  - `test_cycle_review_is_in_progress_json` — PASS (AC-05, R-05)

**Integration tests by suite:**

| Suite | Tests Run | Passed | Failed | xfailed |
|-------|-----------|--------|--------|---------|
| Smoke | 20 | 20 | 0 | 0 |
| Protocol | 13 | 13 | 0 | 0 |
| Tools | 95 | 94 | 0 | 1 |
| Lifecycle | 39 | 37 | 0 | 2 |
| **Total** | **167** | **164** | **0** | **3** |

All xfailed tests have corresponding GH Issues and are pre-existing — none are caused by col-026.

---

## Test Assertion Fix

**Test**: `test_retrospective_markdown_default` (`suites/test_tools.py`)

**Issue**: The test asserted `result.text.strip().startswith("# Retrospective:")`. The col-026 implementation rebrands the header to `# Unimatrix Cycle Review —` (AC-01). This was an expected change documented in test-plan/OVERVIEW.md line 119.

**Triage**: Bad test assertion — col-026 intentionally changed the header. The assertion was updated to `startswith("# Unimatrix Cycle Review —")`.

**Not filed as GH Issue**: This is a test assertion update required by the col-026 scope (not a pre-existing bug).

---

## Gaps

**R-11 partial**: The count-based snapshot test (count of `threshold` occurrences in detection code as a regression guard for a future 10th site) was not implemented as a separate `#[test]`. Instead, `test_no_threshold_language` validates that the formatter's post-processing renders no threshold language in output. The general regex in `format_claim_with_baseline` provides the catch-all for future claim strings. The count snapshot is documented as a future enhancement.

No other gaps. All 13 risks have at least partial test coverage. All 5 critical risks (R-01 through R-05) have full coverage.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_header_rebrand` (retrospective.rs), `test_retrospective_markdown_default` (integration, assertion updated) |
| AC-02 | PASS | `test_header_goal_present`, `test_header_goal_absent` |
| AC-03 | PASS | `test_cycle_type_classification` (5 keyword groups + absent-goal path) |
| AC-04 | PASS | `test_attribution_path_labels` — all 3 path labels verified in handler |
| AC-05 | PASS | `test_is_in_progress_three_states` (unit), `test_cycle_review_is_in_progress_json` (integration) |
| AC-06 | PASS | `test_phase_timeline_table`, `test_cycle_review_phase_timeline_present` (integration) |
| AC-07 | PASS | `test_phase_timeline_rework_annotation` |
| AC-08 | PASS | `test_finding_phase_annotation` |
| AC-09 | PASS | `test_burst_notation_rendering`, `test_burst_notation_single_evidence`, `test_burst_notation_truncation_at_ten` |
| AC-10 | PASS | `test_what_went_well_present`, `test_what_went_well_absent_no_baseline`, `test_what_went_well_absent_no_favorable` |
| AC-11 | PASS | `test_section_order` — all 12 section headers verified in correct sequence |
| AC-12 | PASS | `test_knowledge_reuse_section`, `test_knowledge_reuse_full`, `test_cycle_review_knowledge_reuse_cross_feature_split` (integration) |
| AC-13 | PASS | `test_no_threshold_language`, `test_no_allowlist_in_compile_cycles` |
| AC-14 | PASS | `test_session_table_enhancement` |
| AC-15 | PASS | `test_top_file_zones` |
| AC-16 | PASS | `test_json_format_new_fields` (via retrospective.rs), `test_knowledge_reuse_serde_backward_compat` |
| AC-17 | PASS | `cargo test -p unimatrix-server -- context_cycle_review`: 4 passed, 0 failed |
| AC-18 | PASS | `test_knowledge_reuse_serde_backward_compat` — JSON lacking new fields deserializes correctly |
| AC-19 | PASS | `test_recommendation_compile_cycles_above_threshold` (updated), `test_permission_friction_recommendation_independence`, `test_compile_cycles_action_no_allowlist`, `test_compile_cycles_rationale_no_threshold_language` |

All 19 acceptance criteria verified. All PASS.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for testing procedures — server unavailable at time of execution; proceeded without blocking.
- Stored: nothing novel to store — col-026 testing followed established patterns (`_seed_observation_sql`, `_seed_cycle_events_sql`, `_compute_db_path` helpers); SQL schema column verification (`ts` not `ts_millis` on `query_log`) is a minor fix, not a reusable pattern.
