# Component 6: Section 5 Dispatch

**File**: `eval/report/render.rs`
**Action**: Modify
**Prerequisite**: Pre-split A complete (boundary module exists and `render.rs` builds).

---

## Purpose

Extend `render_report` to accept `profile_meta` (one new parameter), then dispatch Section 5
rendering per non-baseline profile — either to `render_distribution_gate_section` (when
`distribution_change=true`) or to the existing zero-regression block (when false or absent).

`check_distribution_targets` is called inline inside `render_report` when a distribution-change
profile is encountered. The caller (`run_report`) does not pre-compute gate results.

---

## `render_report` Signature Extension

Add ONE new parameter after the existing `cc_at_k_rows` parameter:

```
pub(super) fn render_report(
    stats: &[AggregateStats],
    phase_stats: &[PhaseAggregateStats],
    results: &[ScenarioResult],
    regressions: &[RegressionRecord],
    latency_buckets: &[LatencyBucket],
    entry_rank_changes: &EntryRankSummary,
    query_map: &HashMap<String, String>,
    cc_at_k_rows: &[CcAtKScenarioRow],
    profile_meta: &HashMap<String, ProfileMetaEntry>,   // new (nan-010)
) -> String
```

`ProfileMetaEntry` is imported from `crate::eval::runner::profile_meta` (or via
`eval/runner/mod.rs` re-export).

---

## Modified Section 5 Block

Replace the current Section 5 block in `render_report` entirely:

```
// ----------------------------------------------------------------
// SECTION 5: Per-profile gate (distribution gate or zero-regression check)
// nan-010: dispatch per non-baseline profile independently (ADR-005)
// ----------------------------------------------------------------

// Identify non-baseline profiles: all stats entries except the first (baseline).
// Baseline is always first in stats (enforced by compute_aggregate_stats sort).
let non_baseline_stats: &[AggregateStats] = if stats.len() > 1 {
    &stats[1..]
} else {
    &[]   // only baseline present — no Section 5 content
}

// Determine heading level: single vs. multi (ADR-005)
let multi_profile = non_baseline_stats.len() > 1

// For multi-profile runs, emit the parent heading once (ADR-005, OQ-02)
if multi_profile:
    md.push_str("## 5. Distribution Gate / Zero-Regression Check\n\n")

// Iterate non-baseline profiles
for (idx_zero_based, stat) in non_baseline_stats.iter().enumerate():
    profile_name = &stat.profile_name
    index = idx_zero_based + 1   // 1-based for heading labels (5.1, 5.2, ...)

    // Look up distribution_change flag from profile_meta
    meta_entry = profile_meta.get(profile_name)
    distribution_change = meta_entry
        .map(|e| e.distribution_change)
        .unwrap_or(false)

    if distribution_change:
        // Distribution gate path — compute gate result inline
        baseline_stats = stats.first()   // always present if we're in Section 5

        // Get targets from meta entry (must be Some when distribution_change=true,
        // validated at parse time). Treat None as unexpected — emit a WARN block.
        targets_opt = meta_entry
            .and_then(|e| e.distribution_targets.as_ref())

        if targets_opt is None or baseline_stats is None:
            // Unexpected state — emit a visible inline warning; do not panic
            // eval report still exits 0 (C-07, FR-29)
            md.push_str(&format!(
                "<!-- WARN: distribution gate targets missing for profile '{}' -->\n\n",
                profile_name
            ))
        else:
            // Convert DistributionTargetsJson → DistributionTargets and compute gate
            targets = DistributionTargets {
                cc_at_k_min: targets_opt.cc_at_k_min,
                icd_min:     targets_opt.icd_min,
                mrr_floor:   targets_opt.mrr_floor,
            }
            gate = check_distribution_targets(stat, &targets)

            heading = if multi_profile {
                HeadingLevel::Multi { index }
            } else {
                HeadingLevel::Single
            }
            let section = render_distribution_gate_section(
                profile_name,
                &gate,
                baseline_stats,
                heading,
            )
            md.push_str(&section)

    else:
        // Zero-regression path (existing behavior)
        // For multi-profile: sub-heading per profile (ADR-005)
        if multi_profile:
            md.push_str(&format!(
                "### 5.{index} Zero-Regression Check — {profile_name}\n\n"
            ))
        else:
            md.push_str("## 5. Zero-Regression Check\n\n")

        // Filter regressions to this profile only
        profile_regressions: Vec<&RegressionRecord> = regressions
            .iter()
            .filter(|r| r.profile_name == profile_name)
            .collect()

        if profile_regressions.is_empty():
            md.push_str("**No regressions detected.** All candidate profiles maintain or improve MRR and P@K across all scenarios.\n\n")
        else:
            md.push_str(&format!(
                "**{} regression(s) detected:**\n\n",
                profile_regressions.len()
            ))
            md.push_str("| Scenario | Query | Profile | Reason | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K |\n")
            md.push_str("|----------|-------|---------|--------|-------------|--------------|-------------|---------------|\n")
            for reg in &profile_regressions:
                md.push_str(&format!(
                    "| {} | {} | {} | {} | {:.4} | {:.4} | {:.4} | {:.4} |\n",
                    reg.scenario_id, reg.query, reg.profile_name, reg.reason,
                    reg.baseline_mrr, reg.candidate_mrr,
                    reg.baseline_p_at_k, reg.candidate_p_at_k,
                ))
            md.push('\n')
            md.push_str(
                "_This list is a human-reviewed artifact. No automated gate logic is applied._\n\n"
            )
```

---

## Backward Compatibility

When `profile_meta` is an empty `HashMap` (absent sidecar — pre-nan-010 results):
- `meta_entry = profile_meta.get(profile_name)` returns `None`
- `distribution_change = false` (default)
- All profiles take the zero-regression path
- Section 5 renders identically to pre-nan-010 behavior (AC-11, AC-14)

---

## Data Flow

Inputs (new):
- `profile_meta: &HashMap<String, ProfileMetaEntry>` — from `load_profile_meta`

Outputs: `String` (the complete Markdown report, unchanged type)

---

## Line Budget for `render.rs`

Current: 499 lines. After Pre-split A adds two lines (`mod` + `use`), the file is at 501 lines.

The implementation agent must verify the final line count after all changes. If the file
would exceed 500 lines, extract the zero-regression rendering into a new `render_zero_regression.rs`
sibling module following the same pattern as `render_phase.rs` and `render_distribution_gate.rs`.

---

## Error Handling

`render_report` returns `String`, not `Result`. No error propagation from gate computation.
The report still exits 0 (C-07, FR-29). The missing-targets WARN block is the only defensive
path and should not occur in correct operation (parse-time validation prevents it).

---

## Key Test Scenarios

Tests in `eval/report/tests_distribution_gate.rs`:

```
test_report_without_profile_meta_json:
    Call render_report with profile_meta=HashMap::new()
    Assert: Section 5 renders "## 5. Zero-Regression Check"
    Assert: No "Distribution Gate" text in output

Backward-compat round-trip (AC-11):
    Run full run_report against a results dir with no profile-meta.json
    Assert: Section 5 renders as Zero-Regression Check

Multi-profile heading dispatch (R-09):
    2 non-baseline candidates: one distribution_change=true, one false
    Assert: parent heading "## 5. Distribution Gate / Zero-Regression Check"
    Assert: sub-headings "### 5.1 Distribution Gate — {name1}" and
            "### 5.2 Zero-Regression Check — {name2}"

Single-profile distribution gate:
    1 non-baseline candidate with distribution_change=true
    Assert: heading "## 5. Distribution Gate" (no "### 5.1" prefix)

test_distribution_gate_exit_code_zero (R-12):
    Full run_report with a failing distribution gate
    Assert: run_report returns Ok(())
    Assert: report file exists and contains "FAILED"
```

---

## Notes

- The iteration starts from `stats[1..]` not from all `stats`. The assumption that
  `stats[0]` is always the baseline is enforced by `compute_aggregate_stats` which always
  sorts baseline first. The implementation agent must verify this assumption holds.
- `regressions` is still computed for all profiles before calling `render_report` — the
  zero-regression path filters by `profile_name` within the loop. No change to how
  `find_regressions` is called.
- `check_distribution_targets` is called inside `render_report` when `distribution_change=true`.
  This is intentional — the caller (`run_report`) does not pre-compute gate results.
- The `HeadingLevel` enum import: `use render_distribution_gate::HeadingLevel;` must be
  added alongside `render_distribution_gate_section` in the existing `use` statement.
