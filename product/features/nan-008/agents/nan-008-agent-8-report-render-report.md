# Agent Report: nan-008-agent-8-report-render

**Feature**: nan-008 — Distribution-Aware Metrics (CC@k and ICD)
**Component**: report/render.rs + report/tests.rs
**Agent ID**: nan-008-agent-8-report-render

## Summary

Extended `render_report` with CC@k/ICD columns in the Summary table and a new
Section 6 Distribution Analysis. Added two mandatory round-trip/section-order
tests. All 27 eval::report tests pass.

## Files Modified

- `/workspaces/unimatrix-nan-008/crates/unimatrix-server/src/eval/report/render.rs`
- `/workspaces/unimatrix-nan-008/crates/unimatrix-server/src/eval/report/mod.rs`
- `/workspaces/unimatrix-nan-008/crates/unimatrix-server/src/eval/report/tests.rs`

## Changes Made

### render.rs

- `render_report` gains `cc_at_k_rows: &[CcAtKScenarioRow]` as 7th parameter
- Summary table header extended: `CC@k | ICD (max=ln(n)) | … | ΔCC@k | ΔICD`
- Summary table rows extended with `mean_cc_at_k`, `mean_icd`, `cc_at_k_delta`,
  `icd_delta` values (delta shows em-dash when zero, `{:+.4}` otherwise)
- Section 6 `## 6. Distribution Analysis` appended after section 5 via new
  private helper `render_distribution_analysis(stats, results, cc_at_k_rows)`
- Section 6 contains:
  - ICD interpretation note referencing ln(n_categories) (ADR-002)
  - Per-profile CC@k range table (min/max from raw results, mean from AggregateStats)
  - Per-profile ICD range table with `max=ln(n)` heading (AC-14)
  - For two-profile runs with cc_at_k_rows: Top-5 improvement and degradation
    sub-tables; omitted for single-profile runs (consistent with Section 2)
- `CcAtKScenarioRow` import added to render.rs

### mod.rs

- Added `compute_cc_at_k_scenario_rows` to aggregate import
- `run_report` now calls `compute_cc_at_k_scenario_rows(&scenario_results)` and
  passes result as `&cc_at_k_rows` to `render_report`

### tests.rs

- `test_report_round_trip_cc_at_k_icd_fields_and_section_6` (ADR-003 AC-12):
  Builds synthetic ScenarioResult with cc_at_k=0.857, icd=1.234, delta=0.143;
  serializes to JSON, deserializes, calls run_report, asserts values in output
  and pos("## 5.") < pos("## 6.")
- `test_report_contains_all_six_sections` (AC-13): two-profile fixture, asserts
  strict section position ordering 1-6, CC@k+ICD in Summary, no section duplicated

## Test Results

```
running 27 tests
...
test result: ok. 27 passed; 0 failed; 0 ignored
```

## Issues / Deviations

None. Implementation follows pseudocode exactly. Option A selected for min/max
computation (pass `results` to helper) as specified in the pseudocode NOTE on
data availability.

## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-server eval report rendering — found
  ADR-003 through ADR-005 for nan-008, pattern #3512 (dual-type constraint),
  pattern #3426 (section-order regression risk). Applied: no async in render.rs,
  section 6 strictly appended after section 5, round-trip test guards both values
  and position order.
- Stored: entry #3529 "eval/report render_report: pass new aggregate slices as
  parameters, not struct fields" via /uni-store-pattern
