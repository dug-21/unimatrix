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
            "**No regressions detected.** All candidate profiles maintain or improve MRR, P@K, trust, and cost across all scenarios.\n\n",
        );
    } else {
        out.push_str(&format!(
            "**{} regression(s) detected:**\n\n",
            profile_regressions.len()
        ));
        // nan-018: Reason now OR-folds MRR/P@K/trust/cost; new columns surface the
        // structured reason codes, the trust verdict, and the advisory cost delta
        // next to the relevance metrics.
        out.push_str("| Scenario | Query | Profile | Reason | Reason Codes | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K | \u{0394} Cost (tokens) | Trust Violations |\n");
        out.push_str("|----------|-------|---------|--------|--------------|-------------|--------------|-------------|---------------|------------------|------------------|\n");
        for reg in &profile_regressions {
            // Cost delta is advisory; show it whenever a regression is recorded
            // (positive = cost grew). "\u{2014}" (em-dash) when the delta is zero.
            let cost_delta = if reg.cost_delta == 0.0 {
                "\u{2014}".to_string()
            } else {
                format!("{:+.1}", reg.cost_delta)
            };
            // Machine-readable reason codes in fixed order (mrr/p@k/trust/cost).
            let reason_codes = if reg.reasons.is_empty() {
                "\u{2014}".to_string()
            } else {
                reg.reasons.join(",")
            };
            let trust_cell = if reg.trust_violations.is_empty() {
                "\u{2014}".to_string()
            } else {
                // Join violations; escape pipes so the Markdown table stays intact.
                reg.trust_violations
                    .iter()
                    .map(|v| v.replace('|', "\\|"))
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {:.4} | {:.4} | {:.4} | {:.4} | {} | {} |\n",
                reg.scenario_id,
                reg.query,
                reg.profile_name,
                reg.reason,
                reason_codes,
                reg.baseline_mrr,
                reg.candidate_mrr,
                reg.baseline_p_at_k,
                reg.candidate_p_at_k,
                cost_delta,
                trust_cell,
            ));
        }
        out.push('\n');
        out.push_str(
            "_This list is a human-reviewed artifact. No automated gate logic is applied. \
             Cost growth is advisory (\u{03B5}=0.0) — it is reported but blocks nothing._\n\n",
        );
    }
    out
}
