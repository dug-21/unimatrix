# Risk Coverage Report: vnc-011

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Finding collapse produces incorrect severity when grouping mixed severities | `test_collapse_mixed_severity_picks_highest`, `test_collapse_same_severity`, `test_findings_ordering` | PASS | Full |
| R-02 | evidence_limit default change inflates JSON response size | `test_json_evidence_limit_default_3`, `test_json_evidence_limit_explicit_5`, `test_json_evidence_limit_explicit_0_no_truncation`, `test_markdown_ignores_evidence_limit` | PASS | Full |
| R-03 | Formatter panics or produces malformed markdown when all Optional fields are None | `test_all_none_optional_fields_valid_markdown`, `test_single_optional_session_summaries`, `test_single_optional_baseline_comparison`, `test_single_optional_attribution`, `test_single_optional_feature_knowledge_reuse`, `test_single_optional_rework`, `test_single_optional_reload` | PASS | Full |
| R-04 | Narrative-to-finding matching fails when hotspot_type does not match rule_name | `test_findings_with_narrative_match`, `test_findings_narrative_no_match`, `test_findings_narrative_summary_replaces_claim`, `test_findings_sequence_pattern`, `test_collapse_narrative_summary_populated`, `test_collapse_narrative_summary_none_when_no_match` | PASS | Full |
| R-05 | Timestamp-based k=3 example selection edge cases | `test_evidence_empty_pool`, `test_evidence_one_record`, `test_evidence_three_records`, `test_evidence_ten_records_earliest_three`, `test_evidence_same_timestamp` | PASS | Full |
| R-06 | Session table rendering breaks with edge-case data | `test_session_table_two_rows`, `test_session_empty_tool_dist`, `test_session_zero_duration`, `test_session_with_outcome`, `test_session_no_outcome` | PASS | Full |
| R-07 | Baseline outlier filtering omits section header or sample count | `test_baseline_all_normal_omits`, `test_baseline_mixed_statuses`, `test_baseline_empty_vec`, `test_baseline_single_outlier`, `test_baseline_new_signal_included` | PASS | Full |
| R-08 | Recommendation deduplication drops distinct actions | `test_recommendations_dedup`, `test_recommendations_distinct`, `test_recommendations_empty` | PASS | Full |
| R-09 | Zero-activity phase suppression heuristic hides legitimate phases | `test_phase_outlier_zero_activity_suppressed` | PASS | Full |
| R-10 | Duration formatting edge cases | `test_duration_zero`, `test_duration_over_24h`, `test_duration_minutes_only`, `test_duration_hours_and_minutes`, `test_duration_exact_hour` | PASS | Full |
| R-11 | Markdown table alignment breaks with pipe characters | `test_pipe_in_metric_name` | PASS | Full |
| R-12 | CollapsedFinding total_events f64 floating-point artifacts | `test_float_sum_formatting` | PASS | Full |
| R-13 | Format parameter accepts arbitrary strings | `test_retrospective_params_format_markdown`, `test_retrospective_params_format_json`, `test_retrospective_params_format_absent`, `test_retrospective_params_format_unknown`, `test_dispatch_markdown_default`, `test_dispatch_markdown_explicit`, `test_dispatch_json_explicit`, `test_dispatch_invalid_format_returns_error`, `test_dispatch_summary_routes_to_markdown` | PASS | Full |
| R-14 | Phase outlier rendering does not apply zero-activity suppression | `test_phase_outliers_filters`, `test_phase_outlier_zero_activity_suppressed` | PASS | Full |
| IR-01 | Cross-crate type compatibility (observe -> server) | Compilation success | PASS | Full |
| IR-02 | Handler dispatch routes to correct formatter | `test_dispatch_markdown_default`, `test_dispatch_json_explicit`, integration: `test_retrospective_markdown_default`, `test_retrospective_json_explicit` | PASS | Full |
| IR-03 | Clone-and-truncate only applies to JSON path | `test_markdown_ignores_evidence_limit`, `test_json_evidence_limit_default_3` | PASS | Full |
| IR-04 | retrospective.rs gated behind mcp-briefing feature | `mod retrospective` wrapped in `#[cfg(feature = "mcp-briefing")]` in response/mod.rs; verified by inspection | PASS | Full |

## Test Results

### Unit Tests
- Total: 2049
- Passed: 2049
- Failed: 0
- Ignored: 18

#### vnc-011 Specific Unit Tests
- retrospective.rs formatter tests: 80
- tools.rs RetrospectiveParams tests: 9
- Related (validation, UDS listener): 5
- **Total vnc-011 unit tests: 94**

### Integration Tests

#### Smoke Gate (mandatory)
- Total: 19
- Passed: 18
- xfailed: 1 (GH#111 -- pre-existing volume rate limit, not vnc-011 related)

#### Protocol Suite
- Total: 13
- Passed: 13
- Failed: 0

#### Tools Suite
- Total: 71 (68 existing + 3 new vnc-011 tests)
- Passed: 70
- xfailed: 1 (GH#187 -- pre-existing status observation field, not vnc-011 related)

#### New Integration Tests Added (vnc-011)
| Test | File | Result |
|------|------|--------|
| `test_retrospective_markdown_default` | `suites/test_tools.py` | PASS |
| `test_retrospective_json_explicit` | `suites/test_tools.py` | PASS |
| `test_retrospective_format_invalid` | `suites/test_tools.py` | PASS |

## xfail Markers (pre-existing, not vnc-011)

| Test | GH Issue | Description |
|------|----------|-------------|
| `test_store_1000_entries` | GH#111 | Rate limit blocks volume test |
| `test_status_includes_observation_fields` | GH#187 | file_count field missing from observation section |

No new xfail markers were added. No integration tests were deleted or commented out.

## Gaps

None. All 14 risks and 4 integration risks from RISK-TEST-STRATEGY.md have full test coverage through unit tests. The 3 integration risks identified in test-plan/OVERVIEW.md (format dispatch through MCP) are covered by the 3 new integration tests.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_markdown_output_starts_with_header`, `test_dispatch_markdown_default`, integration `test_retrospective_markdown_default` |
| AC-02 | PASS | `test_dispatch_json_explicit`, `test_json_output_matches_direct_call`, `test_json_path_produces_valid_json`, integration `test_retrospective_json_explicit` |
| AC-03 | PASS | `test_baseline_mixed_statuses`, `test_baseline_all_normal_omits` |
| AC-04 | PASS | `test_collapse_groups_by_rule_name` |
| AC-05 | PASS | `test_evidence_ten_records_earliest_three` |
| AC-06 | PASS | `test_phase_outlier_zero_activity_suppressed` |
| AC-07 | PASS | `test_recommendations_dedup` |
| AC-08 | PASS | `test_json_evidence_limit_default_3`, `test_evidence_limit_default` (params) |
| AC-09 | PASS | `test_session_table_two_rows` |
| AC-10 | PASS | `test_large_report_performance` (validates token reduction via size comparison) |
| AC-11 | PASS | `cargo test --workspace` -- 2049 passed, 0 failed |
| AC-12 | PASS | `test_all_none_optional_fields_valid_markdown` (session_summaries=None, no Sessions heading) |
| AC-13 | PASS | `test_attribution_partial` |
| AC-14 | PASS | `test_knowledge_reuse_full` |
| AC-15 | PASS | `test_findings_with_narrative_match`, `test_findings_sequence_pattern` |
| AC-16 | PASS | `test_all_none_optional_fields_valid_markdown` |
| AC-17 | PASS | `test_rework_present` |
| AC-18 | PASS | `test_reload_present` |
| AC-19 | PASS | `test_retrospective_params_format_markdown`, `test_retrospective_params_format_json`, `test_retrospective_params_format_absent` |
| AC-20 | PASS | `test_dispatch_markdown_default`, `test_dispatch_markdown_explicit`, `test_dispatch_json_explicit`, `test_dispatch_invalid_format_returns_error` |
| AC-21 | PASS | File exists at `crates/unimatrix-server/src/mcp/response/retrospective.rs`; gated behind `#[cfg(feature = "mcp-briefing")]` in `response/mod.rs` |
| AC-22 | PASS | `test_findings_ordering` |
