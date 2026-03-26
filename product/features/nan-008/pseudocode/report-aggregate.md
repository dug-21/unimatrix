# nan-008 Pseudocode: report/aggregate.rs

## Purpose

Aggregates per-scenario results into per-profile summary statistics and
per-scenario CC@k rows for the Distribution Analysis section. Entirely
synchronous; no async, no database, no I/O.

## Import Changes

The existing `use super::{AggregateStats, EntryRankSummary, LatencyBucket, RegressionRecord, ScenarioResult}`
line must be extended:

```
use super::{
    AggregateStats, CcAtKScenarioRow, EntryRankSummary,
    LatencyBucket, RegressionRecord, ScenarioResult,
};
```

## Modified Functions

### compute_aggregate_stats — extend accumulation for CC@k and ICD

```
pub(super) fn compute_aggregate_stats(results: &[ScenarioResult]) -> Vec<AggregateStats>

Algorithm changes (additions only — all existing logic preserved):

In the per-profile accumulation loop, alongside the existing `p_at_k_sum`,
`mrr_sum`, `latency_sum`, `p_at_k_delta_sum`, `mrr_delta_sum`, `latency_delta_sum`
locals, add:

    let mut cc_at_k_sum = 0.0_f64;         // NEW
    let mut icd_sum = 0.0_f64;             // NEW
    let mut cc_at_k_delta_sum = 0.0_f64;   // NEW
    let mut icd_delta_sum = 0.0_f64;       // NEW

Inside the inner loop `for result in results { if let Some(prof_result) = ... }`:
    cc_at_k_sum += prof_result.cc_at_k;    // NEW
    icd_sum += prof_result.icd;            // NEW
    if profile_name != &baseline_name {
        // existing delta accumulation:
        p_at_k_delta_sum += result.comparison.p_at_k_delta;
        mrr_delta_sum += result.comparison.mrr_delta;
        latency_delta_sum += result.comparison.latency_overhead_ms as f64;
        // NEW delta accumulation:
        cc_at_k_delta_sum += result.comparison.cc_at_k_delta;
        icd_delta_sum += result.comparison.icd_delta;
    }
    count += 1;

In the AggregateStats construction (after `if count > 0`):
    AggregateStats {
        // existing fields unchanged:
        profile_name: profile_name.clone(),
        scenario_count: count,
        mean_p_at_k: p_at_k_sum / count as f64,
        mean_mrr: mrr_sum / count as f64,
        mean_latency_ms: latency_sum / count as f64,
        p_at_k_delta: if is_baseline { 0.0 } else { p_at_k_delta_sum / count as f64 },
        mrr_delta: if is_baseline { 0.0 } else { mrr_delta_sum / count as f64 },
        latency_delta_ms: if is_baseline { 0.0 } else { latency_delta_sum / count as f64 },
        // NEW fields:
        mean_cc_at_k: cc_at_k_sum / count as f64,
        mean_icd: icd_sum / count as f64,
        cc_at_k_delta: if is_baseline { 0.0 } else { cc_at_k_delta_sum / count as f64 },
        icd_delta: if is_baseline { 0.0 } else { icd_delta_sum / count as f64 },
    }

Division denominator: always `count` (number of scenarios for this profile),
NOT `entries.len()` or any total-entry count. This is the existing pattern for
`mean_p_at_k` and `mean_mrr`. Using entries_count would be wrong by a factor of k.
(R-11 guard: mean is scenario-level, not entry-level.)
```

## New Functions

### compute_cc_at_k_scenario_rows

```
pub(super) fn compute_cc_at_k_scenario_rows(
    results: &[ScenarioResult],
) -> Vec<CcAtKScenarioRow>

Purpose: Collect per-scenario CC@k values for the Distribution Analysis section 6.
Only produces rows when two profiles are present (baseline and candidate comparison
is meaningful). Single-profile runs return an empty Vec.

Algorithm:
  1. rows = Vec::new()

  2. For each result in results:
       a. Determine baseline_name using the same sort+force-first logic as
          compute_aggregate_stats:
            profile_names = result.profiles.keys() sorted alphabetically
            "baseline" forced to front if present (case-insensitive)
            baseline_name = profile_names[0]

       b. If result.profiles.len() < 2: continue (no comparison possible)

       c. baseline_result = result.profiles.get(baseline_name)  (skip if absent)

       d. Collect non-baseline profile names:
            candidate_names = profile_names filtered where name != baseline_name

       e. For the first candidate_name (primary comparison pair):
            candidate_result = result.profiles.get(candidate_name)  (skip if absent)

            rows.push(CcAtKScenarioRow {
                scenario_id: result.scenario_id.clone(),
                query: result.query.clone(),
                baseline_cc_at_k: baseline_result.cc_at_k,
                candidate_cc_at_k: candidate_result.cc_at_k,
                cc_at_k_delta: result.comparison.cc_at_k_delta,
            })

          Note: `result.comparison.cc_at_k_delta` is used directly (not recomputed)
          because it was computed by `compute_comparison` in the runner with the
          same sign convention (candidate - baseline). This avoids recomputing and
          keeps the delta consistent with the Summary table.

  3. Sort rows by cc_at_k_delta descending (largest positive delta first).
     This allows render.rs to take the first N rows as "top improved" and the
     last N rows (reversed) as "top degraded" without re-sorting.

  4. return rows

Sort direction: descending by cc_at_k_delta (R-12 guard: improvement at front,
degradation at end). The render.rs section 6 takes:
  - top-5 improvement: rows[0..5.min(rows.len())]
  - top-5 degradation: rows[(rows.len()-5.min(rows.len()))..] reversed
    (or: collect all rows where cc_at_k_delta < 0.0, take last 5, reverse)

Alternative sort strategy (simpler for render.rs):
  Pass the full sorted-descending Vec to render.rs. Render takes first N for
  improvement and last N for degradation. This avoids render.rs needing to
  re-sort or filter.
```

## Data Flow

```
ScenarioResult.comparison.cc_at_k_delta  (f64, already computed by runner)
ScenarioResult.profiles[baseline].cc_at_k
ScenarioResult.profiles[candidate].cc_at_k
ScenarioResult.query
ScenarioResult.scenario_id
    |
    v
compute_cc_at_k_scenario_rows
    |
    v
Vec<CcAtKScenarioRow> sorted descending by cc_at_k_delta
    |
    v
render_report (cc_at_k_rows parameter)
    |
    v
section 6 top-5 improvement / top-5 degradation tables
```

## Error Handling

Both functions are pure (no I/O, no `Result`). `compute_aggregate_stats` returns
`Vec<AggregateStats>` — unchanged error contract. `compute_cc_at_k_scenario_rows`
returns `Vec<CcAtKScenarioRow>` — empty Vec when no multi-profile results exist.

Floating-point: no NaN risk. `cc_at_k_sum` and `icd_sum` accumulate values that
are guaranteed finite (produced by `compute_cc_at_k` and `compute_icd` which never
produce NaN or infinity). Division by `count` is safe because the `if count > 0`
guard precedes it.

## Key Test Scenarios

1. `test_aggregate_stats_cc_at_k_mean` (R-11 guard)
   Input: 3 ScenarioResult values, single profile, with
          cc_at_k values = [0.2, 0.4, 0.6]
   Expected: stats[0].mean_cc_at_k ≈ 0.4 (tolerance 1e-9)

2. `test_aggregate_stats_icd_mean`
   Input: 3 ScenarioResult values, single profile, with
          icd values = [0.5, 1.0, 1.5]
   Expected: stats[0].mean_icd ≈ 1.0 (tolerance 1e-9)

3. `test_aggregate_stats_cc_at_k_delta_mean`
   Input: 2 profiles (baseline + candidate), 3 scenarios with
          comparison.cc_at_k_delta = [0.1, 0.2, 0.3]
   Expected: candidate stats.cc_at_k_delta ≈ 0.2; baseline stats.cc_at_k_delta = 0.0

4. `test_cc_at_k_scenario_rows_sort_order` (R-12 guard)
   Input: 3 ScenarioResult values with 2 profiles each,
          cc_at_k_delta values = [-0.2, 0.5, 0.1]
   Expected:
     rows[0].cc_at_k_delta == 0.5  (largest improvement first)
     rows[1].cc_at_k_delta == 0.1
     rows[2].cc_at_k_delta == -0.2 (degradation last)

5. `test_cc_at_k_scenario_rows_single_profile_returns_empty`
   Input: results with only one profile each
   Expected: Vec is empty (no cross-profile comparison possible)

6. `test_cc_at_k_scenario_rows_uses_comparison_delta`
   Input: ScenarioResult with comparison.cc_at_k_delta = 0.3,
          baseline.cc_at_k = 0.4, candidate.cc_at_k = 0.7
   Expected: row.cc_at_k_delta == 0.3 (taken from comparison, not recomputed)
   This verifies the row uses the stored delta (consistency with Summary table).
