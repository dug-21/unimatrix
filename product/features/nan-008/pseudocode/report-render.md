# nan-008 Pseudocode: report/render.rs

## Purpose

Renders aggregated data into the final Markdown report. Two changes in nan-008:
(1) extend the Section 1 Summary table with CC@k and ICD columns, and (2) append
section 6 Distribution Analysis after section 5.

## Import Changes

```
use super::{AggregateStats, CcAtKScenarioRow, EntryRankSummary, LatencyBucket,
            RegressionRecord, ScenarioResult};
```

`ScoredEntry` is already imported. Add `CcAtKScenarioRow` to the existing import.

## Modified Functions

### render_report — add cc_at_k_rows parameter, extend section 1, add section 6

```
pub(super) fn render_report(
    stats: &[AggregateStats],
    results: &[ScenarioResult],
    regressions: &[RegressionRecord],
    latency_buckets: &[LatencyBucket],
    entry_rank_changes: &EntryRankSummary,
    query_map: &HashMap<String, String>,
    cc_at_k_rows: &[CcAtKScenarioRow],    // NEW parameter
) -> String
```

### Section 1 — Summary table extension

The existing header is:
```
| Profile | Scenarios | P@K | MRR | Avg Latency (ms) | ΔP@K | ΔMRR | ΔLatency (ms) |
```

New header extends it with CC@k and ICD columns:
```
| Profile | Scenarios | P@K | MRR | CC@k | ICD (max=ln(n)) | Avg Latency (ms) |
  ΔP@K | ΔMRR | ΔCC@k | ΔICD | ΔLatency (ms) |
```

Column insertion position: CC@k and ICD after MRR, before Avg Latency.
Delta columns: ΔCC@k and ΔICD after ΔMRR, before ΔLatency.

Algorithm for table header and separator:
```
  has_comparison = stats.len() > 1 (at least one non-baseline profile exists
                   — checked by whether any stat has non-zero cc_at_k_delta
                   or simply by stats.len() > 1)

  Header line:
      "| Profile | Scenarios | P@K | MRR | CC@k | ICD (max=ln(n)) |
       Avg Latency (ms) |"
      + if has_comparison: " ΔP@K | ΔMRR | ΔCC@k | ΔICD | ΔLatency (ms) |"
      else: " ΔP@K | ΔMRR | ΔCC@k | ΔICD | ΔLatency (ms) |"
      // Delta columns always present; baseline row shows "—" for all deltas.

  Separator line: matching number of |----|--- cells.
```

Algorithm for each row:
```
  For each stat in stats:
      delta_p = if stat.p_at_k_delta == 0.0 { "—" } else { format("{:+.4}", stat.p_at_k_delta) }
      delta_mrr = if stat.mrr_delta == 0.0 { "—" } else { format("{:+.4}", stat.mrr_delta) }
      delta_cc_at_k = if stat.cc_at_k_delta == 0.0 { "—" } else { format("{:+.4}", stat.cc_at_k_delta) }  // NEW
      delta_icd = if stat.icd_delta == 0.0 { "—" } else { format("{:+.4}", stat.icd_delta) }              // NEW
      delta_lat = if stat.latency_delta_ms == 0.0 { "—" } else { format("{:+.1}", stat.latency_delta_ms) }

      Row:
      "| {profile_name} | {scenario_count} | {mean_p_at_k:.4} | {mean_mrr:.4} |
         {mean_cc_at_k:.4} | {mean_icd:.4} | {mean_latency_ms:.1} |
         {delta_p} | {delta_mrr} | {delta_cc_at_k} | {delta_icd} | {delta_lat} |"
```

Note on "ICD (max=ln(n))" column header: The header uses the literal string
`ICD (max=ln(n))` as specified in ADR-002. If the configured category count is
available in the `AggregateStats` context in a future iteration, the `n` can be
replaced with the actual count. For nan-008, `AggregateStats` does not carry the
configured category count, so the literal `ln(n)` annotation is used as a reminder
to the reader. The Distribution Analysis section 6 provides interpretation guidance.

### Section 6 — Distribution Analysis (new, appended after section 5)

```
  // Append AFTER the existing section 5 content (after the closing \n\n of section 5)

  md.push_str("## 6. Distribution Analysis\n\n");
  md.push_str(render_distribution_analysis(stats, cc_at_k_rows).as_str());
```

The section 6 rendering is delegated to a new private helper function to keep
`render_report` within manageable length.

## New Private Helper Functions

### render_distribution_analysis

```
fn render_distribution_analysis(
    stats: &[AggregateStats],
    cc_at_k_rows: &[CcAtKScenarioRow],
) -> String

Algorithm:
  1. out = String::new()

  2. Add interpretation guidance note (ADR-002):
     out += "_ICD is raw Shannon entropy (natural log). Maximum value is ln(n_categories).\n"
     out += "Values are comparable across profiles run with the same configured categories._\n\n"

  3. Render per-profile CC@k range table:
     out += "### CC@k Range by Profile\n\n"
     out += "| Profile | Scenarios | Min | Max | Mean |\n"
     out += "|---------|-----------|-----|-----|------|\n"

     For each stat in stats:
         // CC@k range must be computed from cc_at_k_rows for this profile
         // OR from the raw ScenarioResult slice if passed.
         // Since render_distribution_analysis only receives stats and cc_at_k_rows,
         // compute min/max from cc_at_k_rows filtered by matching profile name.
         //
         // Alternative: pass &[ScenarioResult] too. Given that render_report already
         // receives `results: &[ScenarioResult]`, pass it through to this helper.
         // See NOTE below.
         ...

  NOTE on data availability for min/max computation:
  `AggregateStats` only carries `mean_cc_at_k` — not min/max. Two options:
    Option A: Pass `results: &[ScenarioResult]` to `render_distribution_analysis`
              and compute min/max directly from `result.profiles[profile_name].cc_at_k`
              for each result.
    Option B: Add `min_cc_at_k` and `max_cc_at_k` fields to `AggregateStats` and
              compute them in `compute_aggregate_stats`.

  Option A is preferred (no struct changes, data already available in render_report
  context). render_report already receives `results`, so it passes it to the helper.

  Revised helper signature:
  fn render_distribution_analysis(
      stats: &[AggregateStats],
      results: &[ScenarioResult],
      cc_at_k_rows: &[CcAtKScenarioRow],
  ) -> String

  Updated render_report call site:
  md.push_str(render_distribution_analysis(stats, results, cc_at_k_rows).as_str());

  Per-profile CC@k range algorithm:
      For each stat in stats:
          profile_cc_at_k_values: Vec<f64> = results
              .iter()
              .filter_map(|r| r.profiles.get(&stat.profile_name))
              .map(|pr| pr.cc_at_k)
              .collect()

          if profile_cc_at_k_values.is_empty():
              // No data for this profile — render em-dash for min/max
              min_str = "—", max_str = "—"
          else:
              min_val = profile_cc_at_k_values.iter().cloned()
                            .fold(f64::INFINITY, f64::min)
              max_val = profile_cc_at_k_values.iter().cloned()
                            .fold(f64::NEG_INFINITY, f64::max)
              min_str = format("{:.4}", min_val)
              max_str = format("{:.4}", max_val)

          row: "| {profile_name} | {scenario_count} | {min_str} | {max_str} | {mean_cc_at_k:.4} |"

  4. Render per-profile ICD range table (same pattern as CC@k):
     out += "\n### ICD Range by Profile (max=ln(n))\n\n"
     out += "| Profile | Scenarios | Min | Max | Mean |\n"
     out += "|---------|-----------|-----|-----|------|\n"

     Same algorithm using `prof_result.icd` instead of `prof_result.cc_at_k`.

  5. If two or more profiles exist (has_comparison = stats.len() >= 2)
     AND cc_at_k_rows is non-empty:

     5a. Top-5 CC@k improvement scenarios:
         improvement_rows = cc_at_k_rows.iter()
             .filter(|r| r.cc_at_k_delta > 0.0)
             .take(5)   // already sorted descending by cc_at_k_delta
             .collect()

         if improvement_rows is non-empty:
             out += "\n### Top Scenarios by CC@k Improvement\n\n"
             out += "| Scenario | Query | Baseline CC@k | Candidate CC@k | Δ CC@k |\n"
             out += "|----------|-------|--------------|----------------|--------|\n"
             for row in improvement_rows:
                 query_truncated = row.query[..40.min(row.query.len())]
                 out += "| {scenario_id} | {query_truncated} | {baseline_cc_at_k:.4} |
                          {candidate_cc_at_k:.4} | {cc_at_k_delta:+.4} |"

     5b. Top-5 CC@k degradation scenarios:
         // cc_at_k_rows is sorted descending; degradations are at the end
         degradation_rows = cc_at_k_rows.iter()
             .filter(|r| r.cc_at_k_delta < 0.0)
             .rev()      // most negative first (worst degradation)
             .take(5)
             .collect()
         // or equivalently: collect all negative-delta rows, then sort ascending
         // and take(5). Either approach is valid.

         if degradation_rows is non-empty:
             out += "\n### Top Scenarios by CC@k Degradation\n\n"
             out += "| Scenario | Query | Baseline CC@k | Candidate CC@k | Δ CC@k |\n"
             out += "|----------|-------|--------------|----------------|--------|\n"
             for row in degradation_rows:
                 (same format as improvement, delta will be negative)

  6. Single-profile or empty cc_at_k_rows: omit improvement/degradation sub-tables
     entirely. Only the CC@k range table and ICD range table are rendered.
     (Consistent with Section 2 behavior for single-profile runs.)

  7. return out
```

## Algorithm Notes

### "has_comparison" determination

The existing code in `render_report` uses `stats.len() >= 2` and profile name
sorting (e.g., in `find_notable_ranking_changes`). The Distribution Analysis section
should use the same criterion: if `stats.len() < 2` or `cc_at_k_rows.is_empty()`,
omit the comparison sub-tables.

### Section order (R-02, ADR-003)

Section 6 is appended AFTER section 5. The pseudocode ensures this by the ordering
of `push_str` calls in `render_report`:
  1. Section 1 (Summary)
  2. Section 2 (Notable Ranking Changes)
  3. Section 3 (Latency Distribution)
  4. Section 4 (Entry-Level Analysis)
  5. Section 5 (Zero-Regression Check)
  6. Section 6 (Distribution Analysis)  <-- must be last

The round-trip integration test (ADR-003) asserts position ordering using
`content.find("## 1.")`, `content.find("## 2.")`, etc. with `<` comparisons.

## Error Handling

`render_report` and `render_distribution_analysis` are pure string-building
functions; they cannot fail. The `f64::min`/`f64::max` fold returns `f64::INFINITY`
and `f64::NEG_INFINITY` when the input is empty, but the `is_empty()` guard handles
that path before formatting. No NaN risk: values come from `ProfileResult.cc_at_k`
and `.icd`, which are guaranteed finite by `compute_cc_at_k` and `compute_icd`.

## Key Test Scenarios

Tests live in `report/tests.rs`.

1. `test_report_contains_all_six_sections` (extend existing five-section test, R-02)
   - Render a full report from a minimal fixture with two profiles.
   - Assert position ordering:
     `pos("## 1.") < pos("## 2.") < pos("## 3.") < pos("## 4.") < pos("## 5.") < pos("## 6.")`
   - `pos` = `rendered.find(substring).unwrap()`.

2. `test_report_summary_table_cc_at_k_icd_columns` (AC-04)
   - Render a report with known mean_cc_at_k = 0.75 and mean_icd = 1.1.
   - Assert `rendered.contains("CC@k")`.
   - Assert `rendered.contains("ICD")`.
   - Assert `rendered.contains("0.7500")` (CC@k value formatted as .4).

3. `test_report_icd_column_annotated_with_ln_n` (AC-14)
   - Render any report (single or multi profile).
   - Assert `rendered.contains("ln(")` somewhere in the Distribution Analysis section
     or ICD column header.

4. `test_distribution_analysis_single_profile_no_comparison_tables`
   - Render a report with exactly one profile.
   - Assert `rendered.contains("## 6. Distribution Analysis")`.
   - Assert `!rendered.contains("Top Scenarios by CC@k Improvement")`.
   - Assert `!rendered.contains("Top Scenarios by CC@k Degradation")`.

5. `test_distribution_analysis_two_profiles_shows_comparison_tables`
   - Render a report with two profiles and at least one positive and one negative
     cc_at_k_delta.
   - Assert `rendered.contains("Top Scenarios by CC@k Improvement")`.
   - Assert `rendered.contains("Top Scenarios by CC@k Degradation")`.

6. `test_report_round_trip_cc_at_k_icd_fields_and_section_6` (ADR-003 primary test)
   - See report-mod.md for full specification of this test. The render-level
     assertion is that the non-zero values `0.857` and `1.234` appear in the
     rendered output, and section 6 follows section 5.
