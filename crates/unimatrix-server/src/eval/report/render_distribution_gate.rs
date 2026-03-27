//! Distribution Gate section renderer (nan-010).
//!
//! Extracted into this sibling module to keep render.rs within the 500-line limit (ADR-001).
//! Renders Section 5 for candidate profiles with distribution_change = true.
//! Follows the render_phase.rs extraction pattern.

use super::AggregateStats;
use super::aggregate::distribution::DistributionGateResult;

// ---------------------------------------------------------------------------
// HeadingLevel
// ---------------------------------------------------------------------------

/// Controls whether this profile's Section 5 uses a top-level or sub-level heading.
///
/// - `Single`: exactly one non-baseline candidate → `## 5. Distribution Gate`
/// - `Multi { index }`: multiple non-baseline candidates → `### 5.N Distribution Gate — {name}`
///   The index is 1-based (first candidate is 5.1).
pub(super) enum HeadingLevel {
    Single,
    Multi { index: usize },
}

// ---------------------------------------------------------------------------
// render_distribution_gate_section
// ---------------------------------------------------------------------------

/// Renders the Distribution Gate block for one candidate profile as a Markdown string.
///
/// Emits:
/// 1. Section heading (level determined by `heading_level`).
/// 2. Declaration notice.
/// 3. Diversity target table (CC@k and ICD rows with Target / Actual / Result columns).
/// 4. Diversity gate verdict.
/// 5. MRR floor table (gate row + informational "Baseline MRR (reference)" row).
/// 6. MRR floor verdict.
/// 7. Overall verdict with distinguishable failure messages (ADR-003, AC-10).
///
/// This function is infallible — pure string formatting.
pub(super) fn render_distribution_gate_section(
    profile_name: &str,
    gate: &DistributionGateResult,
    baseline_stats: &AggregateStats,
    heading_level: HeadingLevel,
) -> String {
    let mut out = String::new();

    // ---- Heading ----
    match heading_level {
        HeadingLevel::Single => {
            out.push_str("## 5. Distribution Gate\n\n");
        }
        HeadingLevel::Multi { index } => {
            // Note: the parent "## 5. Distribution Gate" heading is written by render.rs
            // before the loop. Each profile gets a sub-heading.
            out.push_str(&format!(
                "### 5.{index} Distribution Gate — {profile_name}\n\n"
            ));
        }
    }

    // ---- Declaration notice ----
    out.push_str("Distribution change declared. Evaluating against CC@k and ICD targets.\n\n");

    // ---- Diversity target table (CC@k and ICD rows) ----
    out.push_str("| Metric | Target | Actual | Result |\n");
    out.push_str("|--------|--------|--------|--------|\n");
    out.push_str(&format!(
        "| CC@k | ≥ {:.4} | {:.4} | {} |\n",
        gate.cc_at_k.target,
        gate.cc_at_k.actual,
        pass_fail_label(gate.cc_at_k.passed),
    ));
    out.push_str(&format!(
        "| ICD | ≥ {:.4} | {:.4} | {} |\n",
        gate.icd.target,
        gate.icd.actual,
        pass_fail_label(gate.icd.passed),
    ));
    out.push('\n');

    // ---- Diversity gate verdict ----
    if gate.diversity_passed {
        out.push_str("**Diversity gate: PASSED**\n\n");
    } else {
        out.push_str("**Diversity gate: FAILED** — Diversity targets not met.\n\n");
    }

    // ---- MRR floor table ----
    out.push_str("MRR floor (veto):\n\n");
    out.push_str("| Metric | Floor | Actual | Result |\n");
    out.push_str("|--------|-------|--------|--------|\n");
    out.push_str(&format!(
        "| MRR | ≥ {:.4} | {:.4} | {} |\n",
        gate.mrr_floor.target,
        gate.mrr_floor.actual,
        pass_fail_label(gate.mrr_floor.passed),
    ));
    // Informational row: baseline MRR reference (AC-08, SCOPE.md design decision #5).
    // Not a gate criterion — no pass/fail column (em-dash for Floor and Result columns).
    out.push_str(&format!(
        "| Baseline MRR (reference) | — | {:.4} | — |\n",
        baseline_stats.mean_mrr,
    ));
    out.push('\n');

    // ---- MRR floor verdict ----
    if gate.mrr_floor_passed {
        out.push_str("**MRR floor: PASSED**\n\n");
    } else {
        out.push_str("**MRR floor: FAILED**\n\n");
    }

    // ---- Overall verdict with distinguishable failure modes (ADR-003, AC-10) ----
    if gate.overall_passed {
        out.push_str("**Overall: PASSED**\n\n");
    } else {
        match (gate.diversity_passed, gate.mrr_floor_passed) {
            (false, true) => {
                out.push_str("**Overall: FAILED** — Diversity targets not met.\n\n");
            }
            (true, false) => {
                out.push_str(
                    "**Overall: FAILED** — Diversity targets met, but ranking floor breached.\n\n",
                );
            }
            (false, false) => {
                out.push_str(
                    "**Overall: FAILED** — Diversity targets not met. Ranking floor breached.\n\n",
                );
            }
            (true, true) => {
                // Cannot reach this branch: overall_passed would be true.
                // Defensive fallthrough treats as passed.
                out.push_str("**Overall: PASSED**\n\n");
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn pass_fail_label(passed: bool) -> &'static str {
    if passed { "PASSED" } else { "FAILED" }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::aggregate::distribution::MetricGateRow;
    use super::*;

    fn passing_gate() -> DistributionGateResult {
        DistributionGateResult {
            cc_at_k: MetricGateRow {
                target: 0.60,
                actual: 0.6234,
                passed: true,
            },
            icd: MetricGateRow {
                target: 1.20,
                actual: 1.3101,
                passed: true,
            },
            mrr_floor: MetricGateRow {
                target: 0.35,
                actual: 0.3812,
                passed: true,
            },
            diversity_passed: true,
            mrr_floor_passed: true,
            overall_passed: true,
        }
    }

    fn baseline_stats() -> AggregateStats {
        AggregateStats {
            profile_name: "baseline".to_string(),
            scenario_count: 10,
            mean_p_at_k: 0.80,
            mean_mrr: 0.5103,
            mean_cc_at_k: 0.55,
            mean_icd: 1.10,
            ..Default::default()
        }
    }

    #[test]
    fn test_distribution_gate_section_header() {
        let gate = passing_gate();
        let base = baseline_stats();

        // Single-profile: must use ## 5. heading
        let out =
            render_distribution_gate_section("ppr-candidate", &gate, &base, HeadingLevel::Single);
        assert!(
            out.contains("## 5. Distribution Gate"),
            "missing ## 5. heading"
        );
        assert!(
            out.contains("Distribution change declared"),
            "missing declaration notice"
        );
        assert!(
            out.contains("Evaluating against CC@k and ICD targets"),
            "missing evaluation notice"
        );
        assert!(
            !out.contains("### 5."),
            "single-profile must not use sub-heading"
        );

        // Multi-profile: must use ### 5.N sub-heading
        let out_multi = render_distribution_gate_section(
            "ppr-candidate",
            &gate,
            &base,
            HeadingLevel::Multi { index: 1 },
        );
        assert!(
            out_multi.contains("### 5.1 Distribution Gate"),
            "missing ### 5.1 sub-heading"
        );
        assert!(
            out_multi.contains("ppr-candidate"),
            "multi sub-heading must include profile name"
        );
        assert!(
            !out_multi.contains("## 5. Distribution Gate"),
            "multi must not use top-level ## heading"
        );
    }

    #[test]
    fn test_distribution_gate_table_content() {
        let gate = passing_gate();
        let base = baseline_stats();

        let out =
            render_distribution_gate_section("ppr-candidate", &gate, &base, HeadingLevel::Single);

        // Diversity table structure
        assert!(
            out.contains("| Metric | Target | Actual | Result |"),
            "missing table header"
        );
        assert!(out.contains("CC@k"), "missing CC@k metric");
        assert!(out.contains("ICD"), "missing ICD metric");

        // Numeric values (4dp formatting)
        assert!(out.contains("0.6000"), "missing CC@k target 0.6000");
        assert!(out.contains("0.6234"), "missing CC@k actual 0.6234");
        assert!(out.contains("1.2000"), "missing ICD target 1.2000");
        assert!(out.contains("1.3101"), "missing ICD actual 1.3101");

        // MRR floor table
        assert!(out.contains("0.3500"), "missing MRR floor target 0.3500");
        assert!(out.contains("0.3812"), "missing MRR actual 0.3812");

        // Baseline MRR reference row
        assert!(
            out.contains("Baseline MRR (reference)"),
            "missing Baseline MRR (reference) row"
        );
        assert!(out.contains("0.5103"), "missing baseline MRR value 0.5103");

        // Reference row uses em-dashes for Floor and Result (not a gate criterion)
        assert!(
            out.contains("— | — |"),
            "reference row must use em-dashes for non-gate columns"
        );

        // R-13: no regression-related text must appear
        assert!(
            !out.contains("Regressions"),
            "regression text must not bleed into distribution gate output"
        );
    }

    #[test]
    fn test_distribution_gate_pass_condition() {
        let gate = passing_gate();
        let base = baseline_stats();

        let out =
            render_distribution_gate_section("ppr-candidate", &gate, &base, HeadingLevel::Single);

        assert!(
            out.contains("**Overall: PASSED**"),
            "expected overall PASSED"
        );
        assert!(
            out.contains("**Diversity gate: PASSED**"),
            "expected diversity PASSED"
        );
        assert!(
            out.contains("**MRR floor: PASSED**"),
            "expected MRR floor PASSED"
        );
        assert!(!out.contains("FAILED"), "no FAILED text in all-pass output");
    }

    #[test]
    fn test_distribution_gate_mrr_floor_veto() {
        // CC@k and ICD pass; MRR floor fails — diversity passes but veto triggers.
        let gate = DistributionGateResult {
            cc_at_k: MetricGateRow {
                target: 0.60,
                actual: 0.6234,
                passed: true,
            },
            icd: MetricGateRow {
                target: 1.20,
                actual: 1.3101,
                passed: true,
            },
            mrr_floor: MetricGateRow {
                target: 0.35,
                actual: 0.28,
                passed: false,
            },
            diversity_passed: true,
            mrr_floor_passed: false,
            overall_passed: false,
        };
        let base = baseline_stats();

        let out =
            render_distribution_gate_section("ppr-candidate", &gate, &base, HeadingLevel::Single);

        assert!(
            out.contains("**Diversity gate: PASSED**"),
            "diversity should pass"
        );
        assert!(
            out.contains("**MRR floor: FAILED**"),
            "MRR floor should fail"
        );
        assert!(out.contains("FAILED"), "overall should fail");
        assert!(
            !out.contains("**Overall: PASSED**"),
            "overall must not be PASSED"
        );
        assert!(
            out.contains("ranking floor breached"),
            "must use 'ranking floor breached' message for MRR-only failure"
        );
        assert!(
            !out.contains("Diversity targets not met"),
            "must NOT say 'Diversity targets not met' when diversity passed"
        );
    }

    #[test]
    fn test_distribution_gate_distinct_failure_modes() {
        let base = baseline_stats();

        // Case A: diversity fails, MRR passes
        let gate_a = DistributionGateResult {
            cc_at_k: MetricGateRow {
                target: 0.60,
                actual: 0.45,
                passed: false,
            },
            icd: MetricGateRow {
                target: 1.20,
                actual: 1.00,
                passed: false,
            },
            mrr_floor: MetricGateRow {
                target: 0.35,
                actual: 0.40,
                passed: true,
            },
            diversity_passed: false,
            mrr_floor_passed: true,
            overall_passed: false,
        };
        let out_a =
            render_distribution_gate_section("ppr-candidate", &gate_a, &base, HeadingLevel::Single);
        assert!(
            out_a.contains("Diversity targets not met"),
            "Case A: must contain 'Diversity targets not met'"
        );
        assert!(
            !out_a.contains("ranking floor breached"),
            "Case A: must NOT contain 'ranking floor breached'"
        );

        // Case B: diversity passes, MRR fails
        let gate_b = DistributionGateResult {
            cc_at_k: MetricGateRow {
                target: 0.60,
                actual: 0.65,
                passed: true,
            },
            icd: MetricGateRow {
                target: 1.20,
                actual: 1.35,
                passed: true,
            },
            mrr_floor: MetricGateRow {
                target: 0.35,
                actual: 0.28,
                passed: false,
            },
            diversity_passed: true,
            mrr_floor_passed: false,
            overall_passed: false,
        };
        let out_b =
            render_distribution_gate_section("ppr-candidate", &gate_b, &base, HeadingLevel::Single);
        assert!(
            out_b.contains("ranking floor breached"),
            "Case B: must contain 'ranking floor breached'"
        );
        assert!(
            !out_b.contains("Diversity targets not met"),
            "Case B: must NOT contain 'Diversity targets not met'"
        );

        // Case C: both fail
        let gate_c = DistributionGateResult {
            cc_at_k: MetricGateRow {
                target: 0.60,
                actual: 0.45,
                passed: false,
            },
            icd: MetricGateRow {
                target: 1.20,
                actual: 1.00,
                passed: false,
            },
            mrr_floor: MetricGateRow {
                target: 0.35,
                actual: 0.28,
                passed: false,
            },
            diversity_passed: false,
            mrr_floor_passed: false,
            overall_passed: false,
        };
        let out_c =
            render_distribution_gate_section("ppr-candidate", &gate_c, &base, HeadingLevel::Single);
        assert!(
            out_c.contains("Diversity targets not met"),
            "Case C: must contain 'Diversity targets not met'"
        );
        assert!(
            out_c.contains("Ranking floor breached") || out_c.contains("ranking floor breached"),
            "Case C: must mention ranking floor breach"
        );
        assert!(
            out_c.contains("**Overall: FAILED**"),
            "Case C: overall must be FAILED"
        );
    }
}
