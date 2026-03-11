# Test Plan: retrospective-formatter

Component: `crates/unimatrix-server/src/mcp/response/retrospective.rs` (new file)

All tests are unit tests in `#[cfg(test)] mod tests` within `retrospective.rs`. Each test constructs a `RetrospectiveReport` with controlled field values, calls the relevant function, and asserts on the output string.

## Test Helpers

A `make_report()` helper should build a minimal valid `RetrospectiveReport` with all Optional fields as `None`, empty `hotspots`, empty `recommendations`, and sensible defaults for required fields (`feature_cycle: "test-001"`, `session_count: 1`, `total_records: 10`). Individual tests override fields as needed.

A `make_finding(rule_name, severity, measured, evidence_count)` helper should produce a `HotspotFinding` with the given values and `evidence_count` evidence records with sequential timestamps starting at 1000.

## Unit Test Expectations

### format_retrospective_markdown (top-level)

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_markdown_output_starts_with_header` | Default report | Output starts with `# Retrospective: test-001` |
| `test_markdown_output_is_call_tool_result` | Default report | Returns `CallToolResult` with text content |
| `test_all_none_optional_fields_valid_markdown` | All Optional = None, empty vecs | Valid markdown, contains header line, no section headings for optional parts (R-03) |
| `test_single_optional_some_others_none` | Each Optional field set individually | Corresponding section appears, all others absent. Repeat for: `session_summaries`, `baseline_comparison`, `narratives`, `feature_knowledge_reuse`, `rework_session_count`, `context_reload_pct`, `attribution` (R-03, 7 sub-tests or parameterized) |
| `test_full_report_all_sections` | All fields populated | All sections present: Sessions, Outliers, Findings, Phase Outliers, Knowledge Reuse, Recommendations |

### render_header

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_header_contains_feature_cycle` | `feature_cycle = "nxs-010"` | Contains `# Retrospective: nxs-010` |
| `test_header_contains_session_count` | `session_count = 5` | Contains `5 sessions` |
| `test_header_contains_total_records` | `total_records = 312` | Contains `312 tool calls` |
| `test_header_contains_duration` | `total_duration_secs = 6840` | Contains `1h 54m` |

### format_duration

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_duration_zero` | 0 | `"0m"` (R-10) |
| `test_duration_minutes_only` | 57*60 = 3420 | `"57m"` |
| `test_duration_hours_and_minutes` | 3600+54*60 = 6840 | `"1h 54m"` |
| `test_duration_over_24h` | 90000 | `"25h"` (R-10) -- 90000 / 3600 = 25h, 0 remaining minutes |
| `test_duration_exact_hour` | 3600 | `"1h"` -- pseudocode's `hours > 0 && minutes > 0` branch is false when minutes=0, falls to `hours > 0` returning `"1h"` |

### render_sessions

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_session_table_two_rows` | 2 SessionSummary entries | Table has header + separator + 2 data rows (R-06) |
| `test_session_empty_tool_dist` | Empty HashMap | Calls column shows `0` (R-06) |
| `test_session_zero_duration` | `duration_secs = 0` | Window shows `(0m)` or similar (R-06) |
| `test_session_with_outcome` | `outcome = Some("success")` | Outcome column shows `success` |
| `test_session_no_outcome` | `outcome = None` | Outcome column shows `-` (R-06) |

### render_attribution_note

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_attribution_partial` | `attributed: 3, total: 5` | Contains `> Note: 3/5 sessions attributed` (AC-13) |
| `test_attribution_full` | `attributed: 5, total: 5` | Returns empty string (no note needed) |

### render_baseline_outliers

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_baseline_all_normal_omits` | All `Normal` status | Returns empty string (R-07) |
| `test_baseline_mixed_statuses` | Normal + Outlier + NewSignal + NoVariance | Output contains only Outlier and NewSignal metric names (AC-03, R-07) |
| `test_baseline_empty_vec` | Empty vec | Returns empty string (R-07) |
| `test_baseline_single_outlier` | One `Outlier` entry | Contains `## Outliers` heading and one data row (R-07) |
| `test_baseline_new_signal_included` | One `NewSignal` entry | Contains the metric name |

### collapse_findings

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_collapse_groups_by_rule_name` | 4 findings, 2 rule_names (2 each) | Returns 2 `CollapsedFinding`s (AC-04) |
| `test_collapse_mixed_severity_picks_highest` | Same rule: Info, Warning, Critical | `severity == Critical` (R-01) |
| `test_collapse_same_severity` | Same rule: all Warning | `severity == Warning` (R-01) |
| `test_collapse_total_events_summed` | measured values: 5.0, 3.0, 2.0 | `total_events == 10.0` |
| `test_collapse_tool_breakdown` | Evidence with tool fields: Bash(3), Read(2) | `tool_breakdown` contains `("Bash", 3)`, `("Read", 2)` |
| `test_collapse_evidence_pool_combined` | 3 findings with 2 evidence each | `examples.len() == 3` (k=3 from pool of 6) |
| `test_collapse_narrative_summary_populated` | Findings with matching narrative (`summary: "High tool churn"`) | `collapsed.narrative_summary == Some("High tool churn".to_string())` (FR-09) |
| `test_collapse_narrative_summary_none_when_no_match` | Findings with no matching narrative | `collapsed.narrative_summary.is_none()` (FR-09) |

### Evidence selection (k=3, earliest by timestamp)

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_evidence_empty_pool` | 0 evidence records | No examples section or empty (R-05) |
| `test_evidence_one_record` | 1 record at ts=100 | 1 example bullet (R-05) |
| `test_evidence_three_records` | 3 records at ts=100,200,300 | All 3 rendered (R-05) |
| `test_evidence_ten_records_earliest_three` | 10 records, ts=100..1000 | Exactly 3 rendered, ts=100,200,300 (R-05, AC-05) |
| `test_evidence_same_timestamp` | 5 records all ts=100 | 3 rendered, no panic (R-05) |

### render_findings

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_findings_empty` | Empty hotspots | Section omitted or `## Findings (0)` |
| `test_findings_ordering` | Critical group, Warning group, Info group | F-01 is Critical, F-02 is Warning, F-03 is Info (R-01, AC-22) |
| `test_findings_with_narrative_match` | Narrative with matching `hotspot_type` | Output contains cluster count, summary rendered as finding description line via `narrative_summary` (R-04, AC-15, FR-09) |
| `test_findings_narrative_summary_replaces_claim` | Narrative with matching `hotspot_type` and `summary: "Tool overuse detected"` | Finding description line is `"Tool overuse detected"` (from `narrative_summary`), NOT `claims[0]` (FR-09) |
| `test_findings_narrative_no_match` | Narrative with non-matching `hotspot_type` | Finding renders `claims[0]` as description (no `narrative_summary`) (R-04) |
| `test_findings_sequence_pattern` | Narrative with `sequence_pattern: Some("30s->60s")` | Output contains `Escalation pattern: 30s->60s` (R-04, AC-15) |
| `test_findings_single_finding_no_collapse` | 1 finding | Renders as F-01 without grouping issues |

### render_phase_outliers

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_phase_outliers_filters` | Mix of phase-level statuses | Only Outlier/NewSignal rows (R-14) |
| `test_phase_outlier_zero_activity_suppressed` | Phase with `tool_call_count=0, duration=0` as Outlier | Phase row absent (R-09, R-14, AC-06) |

### render_knowledge_reuse

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_knowledge_reuse_full` | `delivery_count=15, cross_session=8, gaps=["procedure"]` | Contains `15 entries delivered`, `8 cross-session`, `Gaps: procedure` (AC-14) |
| `test_knowledge_reuse_no_gaps` | `category_gaps` empty | No "Gaps:" segment |

### render_rework_reload

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_rework_present` | `rework_session_count: Some(2)` | Contains `2 rework sessions` (AC-17) |
| `test_rework_zero` | `rework_session_count: Some(0)` | Omitted or `0 rework sessions` |
| `test_reload_present` | `context_reload_pct: Some(0.345)` | Contains `35% context reload` (AC-18) -- value is a fraction (0.0-1.0), multiplied by 100 in renderer |
| `test_both_present` | Both Some | Both strings appear |
| `test_both_none` | Both None | Returns empty string |

### render_recommendations

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_recommendations_dedup` | 2 recs same `hotspot_type`, different actions | Only first action rendered (R-08, AC-07) |
| `test_recommendations_distinct` | 3 recs, distinct types | All 3 actions rendered (R-08) |
| `test_recommendations_empty` | Empty vec | Section omitted (R-08) |

## Edge Cases

| Test | Setup | Assertion |
|------|-------|-----------|
| `test_unicode_in_claim` | Finding with claim containing unicode | Renders without panic (R-11) |
| `test_float_sum_formatting` | measured values: 0.1, 0.2, 0.3 | Total renders cleanly, not `0.6000000000000001` (R-12) |
| `test_nan_measured` | measured = f64::NAN | No panic; renders some representation |
| `test_pipe_in_metric_name` | Metric name containing `|` | Table row does not break (R-11) |
| `test_large_report_performance` | 50 findings, 100 baselines | Completes in <5ms (NFR-03) |
