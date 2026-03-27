// Zero-regression block renderer (nan-010).
// Extracted from render.rs Section 5 to respect the 500-line file limit (ADR-001).
// Called per non-baseline profile when distribution_change = false (or absent sidecar).

use super::RegressionRecord;

/// Render the zero-regression check block for one non-baseline profile.
///
/// - `multi_profile`: when true, uses `### 5.{index}` sub-heading; otherwise `## 5.`
/// - Filters `regressions` to those belonging to `profile_name`.
/// - Returns a complete Markdown block ready to be appended to the report.
pub(super) fn render_zero_regression_block(
    regressions: &[RegressionRecord],
    profile_name: &str,
    index: usize,
    multi_profile: bool,
) -> String {
    let mut out = String::new();
    if multi_profile {
        out.push_str(&format!(
            "### 5.{index} Zero-Regression Check — {profile_name}\n\n"
        ));
    } else {
        out.push_str("## 5. Zero-Regression Check\n\n");
    }
    let profile_regressions: Vec<&RegressionRecord> = regressions
        .iter()
        .filter(|r| r.profile_name == profile_name)
        .collect();
    if profile_regressions.is_empty() {
        out.push_str(
            "**No regressions detected.** All candidate profiles maintain or improve MRR and P@K across all scenarios.\n\n",
        );
    } else {
        out.push_str(&format!(
            "**{} regression(s) detected:**\n\n",
            profile_regressions.len()
        ));
        out.push_str("| Scenario | Query | Profile | Reason | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K |\n");
        out.push_str("|----------|-------|---------|--------|-------------|--------------|-------------|---------------|\n");
        for reg in &profile_regressions {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {:.4} | {:.4} | {:.4} | {:.4} |\n",
                reg.scenario_id,
                reg.query,
                reg.profile_name,
                reg.reason,
                reg.baseline_mrr,
                reg.candidate_mrr,
                reg.baseline_p_at_k,
                reg.candidate_p_at_k,
            ));
        }
        out.push('\n');
        out.push_str(
            "_This list is a human-reviewed artifact. No automated gate logic is applied._\n\n",
        );
    }
    out
}
