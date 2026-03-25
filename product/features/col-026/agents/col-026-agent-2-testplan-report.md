# Agent Report: col-026-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Status**: COMPLETE

---

## Output Files

- `/workspaces/unimatrix/product/features/col-026/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/col-026/test-plan/retrospective-report-extensions.md`
- `/workspaces/unimatrix/product/features/col-026/test-plan/phase-stats.md`
- `/workspaces/unimatrix/product/features/col-026/test-plan/knowledge-reuse-extension.md`
- `/workspaces/unimatrix/product/features/col-026/test-plan/formatter-overhaul.md`
- `/workspaces/unimatrix/product/features/col-026/test-plan/recommendation-fix.md`

---

## Risk Coverage Summary

| Risk ID | Priority | Covered By | Test File |
|---------|----------|-----------|-----------|
| R-01 | Critical | `test_phase_stats_obs_in_correct_window_millis_boundary`, `test_phase_stats_static_no_inline_multiply`, `test_cycle_ts_to_obs_millis_overflow_guard` | phase-stats.md |
| R-02 | Critical | `test_phase_stats_no_phase_end_events`, `test_phase_stats_zero_duration_no_panic`, `test_phase_stats_empty_phase_name_on_phase_end`, `test_phase_stats_no_observations_in_window` | phase-stats.md |
| R-03 | Critical | 8 `test_gate_result_*` scenarios including compass edge case | phase-stats.md |
| R-04 | Critical | `test_knowledge_reuse_partial_meta_lookup`, `test_knowledge_reuse_all_meta_missing`, `test_knowledge_reuse_empty_entry_set_skips_lookup`, `test_entry_meta_lookup_called_once` | knowledge-reuse-extension.md |
| R-05 | Critical | `test_is_in_progress_none_when_no_events`, `test_is_in_progress_some_true_renders_in_progress`, `test_is_in_progress_some_false_omits_status_line`, `test_is_in_progress_serde_roundtrip_none` | retrospective-report-extensions.md + phase-stats.md |
| R-06 | High | `test_what_went_well_direction_table_all_16_metrics` (data-driven over full spec table) | formatter-overhaul.md |
| R-07 | High | `test_section_order` (golden test — all 11 headers in sequence) | formatter-overhaul.md |
| R-08 | High | `test_format_claim_with_baseline_*` (3 paths), `test_no_threshold_language` (9 claim formats) | formatter-overhaul.md |
| R-09 | High | `test_attribution_path_labels` (3 paths + None case) | phase-stats.md + formatter-overhaul.md |
| R-10 | High | `test_finding_phase_multi_evidence`, `test_finding_phase_out_of_bounds_timestamp` | formatter-overhaul.md |
| R-11 | Med | `test_threshold_language_count_snapshot` (source scan) | formatter-overhaul.md |
| R-12 | Med | `test_phase_stats_some_empty_present_in_json`, `test_phase_stats_empty_events_produces_none` | retrospective-report-extensions.md + phase-stats.md |
| R-13 | Low | `cargo build` CI gate (compile-time enforcement) | all components |

---

## Integration Suite Plan

**Suites to run in Stage 3c**: `smoke` (mandatory), `tools`, `protocol`, `lifecycle`.

**New integration tests needed** (add to infra-001 in Stage 3c):
1. `test_cycle_review_phase_timeline_present` — in `suites/test_tools.py`
2. `test_cycle_review_is_in_progress_json` — in `suites/test_tools.py`
3. `test_cycle_review_knowledge_reuse_cross_feature_split` — in `suites/test_lifecycle.py`

All three use the `server` fixture (fresh DB, function scope). Phase Timeline and is_in_progress
tests seed `cycle_events` via `context_cycle` tool calls only.

---

## AC Coverage Summary

All 19 ACs covered:

- AC-01 → `test_header_rebrand`
- AC-02 → `test_header_goal_present` / `test_header_goal_absent`
- AC-03 → `test_cycle_type_classification` (data-driven, 5 keyword groups)
- AC-04 → `test_attribution_path_labels`
- AC-05 → `test_is_in_progress_three_states` (3 states)
- AC-06 → `test_phase_timeline_table`
- AC-07 → `test_phase_timeline_rework_annotation`
- AC-08 → `test_finding_phase_annotation`
- AC-09 → `test_burst_notation_rendering`
- AC-10 → `test_what_went_well_present` / `test_what_went_well_absent`
- AC-11 → `test_section_order` (golden order test)
- AC-12 → `test_knowledge_reuse_section`
- AC-13 → `test_no_threshold_language` + `test_no_allowlist_in_compile_cycles`
- AC-14 → `test_session_table_enhancement`
- AC-15 → `test_top_file_zones`
- AC-16 → `test_json_format_new_fields` + `test_new_report_fields_absent_when_none`
- AC-17 → `cargo test -p unimatrix-server -- retrospective` (existing tests pass)
- AC-18 → `test_knowledge_reuse_serde_backward_compat`
- AC-19 → `test_recommendation_compile_cycles_above_threshold` (updated) + `test_permission_friction_recommendation_independence`

---

## Open Questions

1. **R-03 scenario 7 tie-break rule**: When `outcome = "pass after rework"` contains both
   "pass" and "rework", which `GateResult` wins? The spec suggests `Rework` takes precedence
   when `pass_count > 1` (i.e., the pass/rework check is pass_count-based, not keyword-order-based).
   The implementation agent must pin the exact logic and `test_gate_result_multi_keyword_pass_rework`
   documents the result.

2. **R-09 all-paths-empty case**: When all three attribution paths return empty, what is
   `attribution_path`? The risk strategy says `None` or a "no data" sentinel. The spec says
   `None`. The implementation agent must confirm and the test `test_attribution_path_all_empty`
   documents the outcome.

3. **`total_served` vs `delivery_count`**: Clarify whether `total_served` is a new name for
   the same count as `delivery_count`, or whether it is distinct (e.g., counts include
   injection-only entries that don't get a category resolved). The test
   `test_total_served_equals_delivery_count` will detect any divergence.

4. **compile_cycles rationale**: The rationale string at `report.rs` line 88 still contains
   `(threshold: 10)`. This is in the JSON `rationale` field, not rendered in markdown. No
   AC requires changing it, but it's inconsistent with the spirit of AC-19. The implementation
   agent should decide and document.

5. **`entry_meta_lookup` feature_cycle=None handling**: If an entry's `feature_cycle` is NULL
   in the DB, `EntryMeta.feature_cycle` is `Option<String>`. The split logic for
   cross-feature vs intra-cycle must define behavior for `None`. The implementation agent
   must pin this (assumed: treat as cross-feature / unknown origin).

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for col-026 ADRs (category: decision, topic: col-026) — found all 5 ADRs (#3421–#3425) and 1 pre-existing formatter pattern (#3426)
- Queried: `/uni-knowledge-search` for testing procedures — found procedures #553, #750, #487, #296, #2326 (general patterns, not col-026 specific)
- Queried: `/uni-knowledge-search` for retrospective formatter testing patterns — found #952 (ADR-003 module structure), #2928 (string-refactor test patterns), #3426 (golden-output risk warning)
- Stored: entry #3427 "col-026: pattern" via `/uni-store-pattern` — golden section-order test pattern and Option-bool serde test pattern for formatter overhaul testing
