//! Phase-Stratified Metrics section renderer (nan-009).
//!
//! Extracted from render.rs to keep that file within the 500-line limit.
//! Renders Section 6 of the eval report: a Markdown table of per-phase
//! aggregate P@K, MRR, CC@k, and ICD means.
//!
//! Section 6 is omitted entirely when all scenario results have `phase = None`;
//! this is signalled by calling `render_phase_section` with an empty slice,
//! which returns an empty string (AC-04, R-09).

use super::PhaseAggregateStats;

// ---------------------------------------------------------------------------
// render_phase_section (Section 6 — nan-009)
// ---------------------------------------------------------------------------

/// Renders the Phase-Stratified Metrics section as a Markdown table.
///
/// Returns an empty string when `phase_stats` is empty; the caller in
/// `render_report` checks for this and skips the section entirely (AC-04, R-09).
/// Input is assumed already sorted: alphabetical ascending for named phases,
/// `"(unset)"` unconditionally last (guaranteed by `compute_phase_stats`).
pub(super) fn render_phase_section(phase_stats: &[PhaseAggregateStats]) -> String {
    if phase_stats.is_empty() {
        return String::new();
    }

    let mut out = String::new();

    out.push_str("## 6. Phase-Stratified Metrics\n\n");

    // Interpretation note matching style of render_distribution_analysis.
    out.push_str(
        "_Metrics are computed from the baseline profile only. \
Phase is populated for MCP-sourced sessions that called `context_cycle`._\n\n",
    );

    // Table header — columns: Phase | Count | P@K | MRR | CC@k | ICD
    out.push_str("| Phase | Count | P@K | MRR | CC@k | ICD |\n");
    out.push_str("|-------|-------|-----|-----|------|-----|\n");

    // One row per phase stat; already sorted by aggregate.rs.
    for stat in phase_stats {
        out.push_str(&format!(
            "| {} | {} | {:.4} | {:.4} | {:.4} | {:.4} |\n",
            stat.phase_label,
            stat.scenario_count,
            stat.mean_p_at_k,
            stat.mean_mrr,
            stat.mean_cc_at_k,
            stat.mean_icd,
        ));
    }

    // Trailing newline before next section.
    out.push('\n');

    out
}
