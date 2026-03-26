# Agent Report: nan-008-agent-6-report-aggregate

## Task

Extend `compute_aggregate_stats` with CC@k and ICD accumulation, and add `compute_cc_at_k_scenario_rows` to `report/aggregate.rs`.

## Files Modified

- `/workspaces/unimatrix-nan-008/crates/unimatrix-server/src/eval/report/aggregate.rs`
- `/workspaces/unimatrix-nan-008/crates/unimatrix-server/src/eval/report/tests.rs`

## Changes Summary

### aggregate.rs

1. Extended `use super::{...}` import to include `CcAtKScenarioRow`.
2. In `compute_aggregate_stats`: added `cc_at_k_sum`, `icd_sum`, `cc_at_k_delta_sum`, `icd_delta_sum` accumulators per profile loop; removed `..Default::default()` from `AggregateStats` construction and replaced with explicit `mean_cc_at_k`, `mean_icd`, `cc_at_k_delta`, `icd_delta` fields.
3. Added `compute_cc_at_k_scenario_rows`: collects per-scenario baseline+candidate CC@k values, uses stored `comparison.cc_at_k_delta` (not recomputed), truncates query to 60 chars, sorts descending by `cc_at_k_delta`, skips single-profile results.

### tests.rs

1. Added `compute_cc_at_k_scenario_rows` to import.
2. Added `make_scenario_result_with_metrics` helper (10-arg, sets cc_at_k/icd on both profiles and deltas on comparison).
3. Added 8 new tests:
   - `test_aggregate_stats_cc_at_k_mean` (R-11 guard)
   - `test_aggregate_stats_icd_mean` (R-11 symmetric)
   - `test_aggregate_stats_cc_at_k_delta_mean` (R-11 for delta)
   - `test_aggregate_stats_baseline_has_zero_cc_at_k_delta`
   - `test_cc_at_k_scenario_rows_sort_order` (R-12 guard)
   - `test_cc_at_k_scenario_rows_single_profile_returns_empty`
   - `test_cc_at_k_scenario_rows_uses_comparison_delta`
   - `test_cc_at_k_scenario_rows_single_scenario`
   - `test_cc_at_k_scenario_rows_empty`

## Test Results

```
running 25 tests
... all pass ...
test result: ok. 25 passed; 0 failed; 0 ignored
```

(17 pre-existing + 8 new = 25 total in `eval::report`)

## Issues

None. All constraints met:
- No async, no tokio in aggregate.rs
- `..Default::default()` removed; all fields explicit
- Division by scenario `count`, not entry count (R-11)
- Sort descending by `cc_at_k_delta` (R-12)
- Stored delta used, not recomputed

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `aggregate stats accumulation eval report` -- no directly matching entries; closest were dual-type constraint pattern (#3512) and SQL aggregation pattern (#726), neither covering the scenario-count denominator invariant.
- Stored: nothing novel to store -- `/uni-store-pattern` returned `MCP error -32003: Agent 'anonymous' lacks Write capability`. The key finding (divide by scenario count, not entry count) should be stored by a coordinator with Write capability: "In `compute_aggregate_stats`, `count` increments once per scenario. All means divide by `count`. Dividing by `entries.len()` inflates means by factor k."
