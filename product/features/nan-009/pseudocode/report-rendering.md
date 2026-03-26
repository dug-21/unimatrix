# Component: Report Rendering

File: `eval/report/render.rs`

## Purpose

1. Add `render_phase_section` — renders section 6 "Phase-Stratified Metrics" as a
   Markdown table. Returns empty string when `phase_stats` is empty; caller skips.
2. Add `phase_stats: &[PhaseAggregateStats]` parameter to `render_report`.
3. Rename `## 6. Distribution Analysis` to `## 7. Distribution Analysis` (one code site).
4. Update module docstring to list seven sections.
5. Render phase label in section 2 header lines for non-null-phase scenarios (FR-10, RD-04).

---

## Module Docstring Update

Current docstring (lines 1-7):
```
//! Renders the six required sections from aggregated data structures:
//! 1. Summary, 2. Notable Ranking Changes, 3. Latency Distribution,
//! 4. Entry-Level Analysis, 5. Zero-Regression Check,
//! 6. Distribution Analysis.
```

New docstring:
```
//! Renders the seven required sections from aggregated data structures:
//! 1. Summary, 2. Notable Ranking Changes, 3. Latency Distribution,
//! 4. Entry-Level Analysis, 5. Zero-Regression Check,
//! 6. Phase-Stratified Metrics, 7. Distribution Analysis.
//!
//! Section 6 is omitted when all scenario results have phase = None.
```

---

## Import Addition

Add `PhaseAggregateStats` to the existing `use super::` import block:
```
use super::{
    AggregateStats, CcAtKScenarioRow, EntryRankSummary, LatencyBucket,
    PhaseAggregateStats,        // NEW
    RegressionRecord, ScenarioResult,
};
```

---

## Modified Function: `render_report`

### Signature Change

Current:
```
pub(super) fn render_report(
    stats: &[AggregateStats],
    results: &[ScenarioResult],
    regressions: &[RegressionRecord],
    latency_buckets: &[LatencyBucket],
    entry_rank_changes: &EntryRankSummary,
    query_map: &HashMap<String, String>,
    cc_at_k_rows: &[CcAtKScenarioRow],
) -> String
```

New — add `phase_stats` as the second parameter (after `stats`, before `results`):
```
pub(super) fn render_report(
    stats: &[AggregateStats],
    phase_stats: &[PhaseAggregateStats],    // NEW second parameter
    results: &[ScenarioResult],
    regressions: &[RegressionRecord],
    latency_buckets: &[LatencyBucket],
    entry_rank_changes: &EntryRankSummary,
    query_map: &HashMap<String, String>,
    cc_at_k_rows: &[CcAtKScenarioRow],
) -> String
```

Parameter positioning: `phase_stats` follows `stats` because phase-stratified metrics
are logically a sub-view of the aggregate stats. This matches pattern #3529 (parameter
ordering by data dependency: parent aggregate before child phase aggregate).

### Body Changes — Section 2 (Notable Ranking Changes)

Current section 2 render loop (lines 112-137):
```
for (scenario_id, query, tau, baseline_entries, candidate_entries) in &notable {
    md.push_str(&format!("### {scenario_id}\n\n"));
    md.push_str(&format!("**Query**: {query}  \n"));
    md.push_str(&format!("**Kendall τ**: {tau:.4}\n\n"));
    // ... rank table ...
}
```

Modified — add phase label line when non-null (FR-10, RD-04):
```
for result in &notable_results {
    // notable_results: Vec<(scenario_id, query, tau, baseline_entries, candidate_entries)>
    // Phase is read directly from the ScenarioResult, not from NotableEntry tuple (RD-04).
    // Find the phase for this scenario by matching scenario_id.

    md.push_str(&format!("### {}\n\n", result.scenario_id));
    md.push_str(&format!("**Query**: {}  \n", result.query));
    md.push_str(&format!("**Kendall τ**: {:.4}\n", result.tau));

    // Add phase line only when non-null (FR-10, R-12 guard)
    IF result.phase is Some(phase_label) THEN
        md.push_str(&format!("**Phase**: {phase_label}  \n"));
    END IF

    md.push('\n');
    // ... rank table (unchanged) ...
}
```

Implementation note: `find_notable_ranking_changes` returns `Vec<NotableEntry<'a>>` where
`NotableEntry = (String, String, f64, &[ScoredEntry], &[ScoredEntry])`. The architecture
states "read phase directly from ScenarioResult in renderer — do not extend NotableEntry
tuple" (RD-04). To retrieve the phase, the renderer looks up the `ScenarioResult` by
`scenario_id` from the `results` slice. One approach:

```
FOR EACH (scenario_id, query, tau, baseline_entries, candidate_entries) IN notable DO
    // Look up phase from results
    phase_label_opt = results.iter()
        .find(|r| r.scenario_id == scenario_id)
        .and_then(|r| r.phase.as_deref())

    md.push_str(&format!("### {scenario_id}\n\n"));
    md.push_str(&format!("**Query**: {query}  \n"));
    md.push_str(&format!("**Kendall τ**: {tau:.4}\n"));

    IF let Some(phase_label) = phase_label_opt THEN
        md.push_str(&format!("**Phase**: {phase_label}  \n"));
    END IF

    md.push('\n');
    // ... rank table unchanged ...
END FOR
```

The `results.iter().find()` scan is O(n) per notable entry. With at most 10 notable
entries and at most a few thousand results, this is acceptable. No caching needed.

### Body Changes — Section 6 (new) and Section 7 (renumbered)

Current section 6 code (lines 196-199 in render_report):
```
// ----------------------------------------------------------------
// SECTION 6: Distribution Analysis (nan-008, FR-09, AC-05)
// ----------------------------------------------------------------
md.push_str("## 6. Distribution Analysis\n\n");
md.push_str(&render_distribution_analysis(stats, results, cc_at_k_rows));
```

New — insert section 6 before section 7, renumber Distribution Analysis to 7:
```
// ----------------------------------------------------------------
// SECTION 6: Phase-Stratified Metrics (nan-009)
// Only rendered when at least one scenario has a non-null phase.
// ----------------------------------------------------------------
let phase_section = render_phase_section(phase_stats);
IF NOT phase_section.is_empty() THEN
    md.push_str(&phase_section);
END IF
// No else branch — section is omitted entirely when empty (AC-04, R-09)

// ----------------------------------------------------------------
// SECTION 7: Distribution Analysis (formerly section 6, nan-008)
// ----------------------------------------------------------------
md.push_str("## 7. Distribution Analysis\n\n");
md.push_str(&render_distribution_analysis(stats, results, cc_at_k_rows));
```

MUST NOT emit `## 6. Distribution Analysis` anywhere. The old string must be deleted.

---

## New Function: `render_phase_section`

### Signature

```
pub(super) fn render_phase_section(phase_stats: &[PhaseAggregateStats]) -> String
```

Returns empty string when `phase_stats` is empty. Caller checks and skips if empty (R-09).

### Algorithm

```
FUNCTION render_phase_section(phase_stats: &[PhaseAggregateStats]) -> String

    IF phase_stats is empty THEN
        RETURN ""    // R-09 guard: caller must not render an empty section
    END IF

    out = String::new()

    // Section heading
    out += "## 6. Phase-Stratified Metrics\n\n"

    // Interpretation note (brief, matches style of render_distribution_analysis)
    out += "_Metrics are computed from the baseline profile only. "
    out += "Phase is populated for MCP-sourced sessions that called `context_cycle`._\n\n"

    // Table header
    out += "| Phase | Count | P@K | MRR | CC@k | ICD |\n"
    out += "|-------|-------|-----|-----|------|-----|\n"

    // One row per phase stat; already sorted (alphabetical, "(unset)" last) by aggregate.rs
    FOR EACH stat IN phase_stats DO
        out += format!(
            "| {} | {} | {:.4} | {:.4} | {:.4} | {:.4} |\n",
            stat.phase_label,
            stat.scenario_count,
            stat.mean_p_at_k,
            stat.mean_mrr,
            stat.mean_cc_at_k,
            stat.mean_icd,
        )
    END FOR

    out += "\n"    // trailing newline before next section

    RETURN out
```

Column names: `Phase`, `Count`, `P@K`, `MRR`, `CC@k`, `ICD` — matching the per-profile
summary table style from section 1.

Format: `{:.4}` for all float metrics — four decimal places, consistent with section 1
and section 7.

---

## Error Handling

All rendering functions are infallible. String formatting with `{:.4}` on `f64` does not
panic. The `phase_stats` input is already sorted by `compute_phase_stats`.

---

## Key Test Scenarios

Tests live in `eval/report/tests.rs`.

**T1: `test_render_phase_section_empty_input_returns_empty_string`** (R-09 AC-04)
- Call `render_phase_section(&[])`
- Assert returned string is empty

**T2: `test_render_phase_section_table_structure`** (AC-05)
- Construct a small `Vec<PhaseAggregateStats>` with known values
- Call `render_phase_section`
- Assert output contains `## 6. Phase-Stratified Metrics`
- Assert output contains `| Phase | Count |` header row
- Assert output contains the expected phase label and numeric values

**T3: `test_render_phase_section_absent_when_stats_empty`** (AC-04, R-07+R-09 combined)
- Call full `render_report` with `phase_stats = &[]`
- Assert output does NOT contain `## 6. Phase-Stratified Metrics`

**T4: `test_report_round_trip_phase_section_7_distribution`** (ADR-002, SR-02+SR-03 guard)
Full round-trip test. Mandatory. Must:
1. Construct `ScenarioResult` with `phase: Some("delivery")` and non-trivial metric values
2. Write as JSON to TempDir
3. Call `run_report`
4. Assert `content.contains("## 6. Phase-Stratified Metrics")` — new section present
5. Assert `content.contains("## 7. Distribution Analysis")` — renumbered section present
6. Assert `content.contains("delivery")` — phase label in section 6
7. Assert `pos("## 6.") < pos("## 7.")` — section order correct
8. Assert `!content.contains("## 6. Distribution Analysis")` — old heading absent (R-02)

**T5: `test_report_round_trip_null_phase_only_no_section_6`** (AC-04, R-07)
- All `ScenarioResult`s have `phase: None`
- Call `run_report`
- Assert output does NOT contain `## 6. Phase-Stratified Metrics`
- Assert output DOES contain `## 7. Distribution Analysis` (distribution section still there)

**T6: `test_report_contains_all_sections`** (updated from `test_report_contains_all_five_sections`)
- The existing test that asserts five section headings must be updated to assert seven:
  `## 1.` through `## 7.` for a run with at least one non-null phase result
- Assert `## 6. Phase-Stratified Metrics` present
- Assert `## 7. Distribution Analysis` present
- Assert `## 6. Distribution Analysis` absent

**T7: `test_section_2_phase_label_present_for_non_null`** (AC-08, R-12)
- Render with a result that has `phase = Some("design")` and is in notable changes
- Assert section 2 output contains `**Phase**: design`

**T8: `test_section_2_phase_label_absent_for_null`** (AC-08, R-12)
- Render with a result that has `phase = None` and is in notable changes
- Assert section 2 output does NOT contain `**Phase**:` for that scenario header
