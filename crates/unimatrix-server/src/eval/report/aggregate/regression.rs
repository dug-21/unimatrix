//! Regression detection for the eval report (nan-007 D4; OR-fold extended nan-018).
//!
//! Extracted from `aggregate/mod.rs` to respect the 500-line file limit (ADR-001).
//! `find_regressions` OR-folds the relevance regressions (MRR / P@K) with the
//! nan-018 trust-flip and cost-growth signals, producing the body-only Section 5
//! list. Exit-code semantics are UNCHANGED (R-17): the report still always exits 0.

use std::collections::HashMap;

use crate::eval::report::{RegressionRecord, ScenarioResult};

/// Detect regressions using OR semantics (AC-09, R-12; extended nan-018).
///
/// A scenario-profile pair is a regression if ANY of the following hold:
/// - candidate MRR < baseline MRR (strict; equal is NOT a regression)
/// - candidate P@K < baseline P@K (strict)
/// - **trust flip** (nan-018, ADR-004): the baseline satisfied an assertion the
///   candidate now violates — `(base.absence_pass && !cand.absence_pass)` OR
///   `(base.rank_pass && !cand.rank_pass)`. Trust pass does NOT mask a relevance
///   regression and vice-versa (OR-extension, never AND-masking).
/// - **cost growth** (nan-018, ADR-003, ε=0.0 ADVISORY): `candidate.cost_tokens
///   - baseline.cost_tokens > 0.0`. Any growth is REPORTED but blocks nothing.
///
/// Exit-code invariance (R-17, LOAD-BEARING): this extends only the report BODY
/// (Section 5 list). `eval report` exit code is unchanged — trust flips and cost
/// growth are body-only, the SAME fail-in-body semantics as the existing MRR/P@K
/// check. No path here makes the exit code non-zero.
///
/// Baseline selection (R-03 lineage #2610): the first profile by SORTED key
/// ("baseline" forced first), NEVER HashMap iteration order.
pub(in crate::eval::report) fn find_regressions(
    results: &[ScenarioResult],
    query_map: &HashMap<String, String>,
) -> Vec<RegressionRecord> {
    let mut regressions: Vec<RegressionRecord> = Vec::new();

    for result in results {
        // Build a sorted list of profile names for this result to ensure
        // deterministic baseline selection (WARN-C mitigation).
        let mut profile_names: Vec<&str> = result.profiles.keys().map(|s| s.as_str()).collect();
        profile_names.sort();

        // "baseline" forced first; otherwise alphabetical first entry is baseline.
        if let Some(pos) = profile_names
            .iter()
            .position(|n| n.to_lowercase() == "baseline")
        {
            let baseline = profile_names.remove(pos);
            profile_names.insert(0, baseline);
        }

        let baseline_name = match profile_names.first() {
            Some(n) => *n,
            None => continue,
        };

        let baseline_result = match result.profiles.get(baseline_name) {
            Some(r) => r,
            None => continue,
        };

        for profile_name in &profile_names {
            if *profile_name == baseline_name {
                continue;
            }

            let prof_result = match result.profiles.get(*profile_name) {
                Some(r) => r,
                None => continue,
            };

            // OR-fold (R-12): regression if MRR OR P@K is strictly lower, OR a
            // trust assertion the baseline satisfied is now violated, OR cost grew.
            let mrr_regressed = prof_result.mrr < baseline_result.mrr;
            let p_at_k_regressed = prof_result.p_at_k < baseline_result.p_at_k;

            // Trust flip (ADR-004): baseline passed, candidate fails — for either
            // the absence verdict or the rank verdict.
            let trust_flip = (baseline_result.trust.absence_pass
                && !prof_result.trust.absence_pass)
                || (baseline_result.trust.rank_pass && !prof_result.trust.rank_pass);

            // Cost growth (ADR-003, ε=0.0 strict): ANY positive delta is reported.
            let cost_delta = prof_result.cost_tokens - baseline_result.cost_tokens;
            let cost_growth = cost_delta > 0.0;

            if mrr_regressed || p_at_k_regressed || trust_flip || cost_growth {
                // Fixed-order structured reasons (deterministic across runs).
                let mut reasons: Vec<String> = Vec::new();
                if mrr_regressed {
                    reasons.push("mrr".to_string());
                }
                if p_at_k_regressed {
                    reasons.push("p@k".to_string());
                }
                if trust_flip {
                    reasons.push("trust".to_string());
                }
                if cost_growth {
                    reasons.push("cost".to_string());
                }

                let reason =
                    render_reason(mrr_regressed, p_at_k_regressed, trust_flip, cost_growth);

                let query_text = query_map
                    .get(&result.scenario_id)
                    .cloned()
                    .unwrap_or_else(|| result.query.clone());

                let trust_violations = if trust_flip {
                    prof_result.trust.violations.clone()
                } else {
                    Vec::new()
                };

                regressions.push(RegressionRecord {
                    scenario_id: result.scenario_id.clone(),
                    query: query_text,
                    profile_name: profile_name.to_string(),
                    baseline_mrr: baseline_result.mrr,
                    candidate_mrr: prof_result.mrr,
                    baseline_p_at_k: baseline_result.p_at_k,
                    candidate_p_at_k: prof_result.p_at_k,
                    reason,
                    reasons,
                    trust_violations,
                    cost_delta,
                });
            }
        }
    }

    // Sort by MRR delta descending (worst regression first).
    regressions.sort_by(|a, b| {
        let delta_a = a.baseline_mrr - a.candidate_mrr;
        let delta_b = b.baseline_mrr - b.candidate_mrr;
        delta_b
            .partial_cmp(&delta_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    regressions
}

/// Render the human-readable `reason` string from the four OR-folded flags.
///
/// At least one flag is always true (the caller only constructs a record when the
/// OR holds). The MRR/P@K phrasing is PRESERVED from the pre-nan-018 renderer
/// ("both MRR and P@K dropped" / "MRR dropped" / "P@K dropped") for backward-compat
/// with existing report tests; trust + cost clauses are appended. Cost is annotated
/// `(advisory)` to signal it blocks nothing (ε=0.0, §7.1).
fn render_reason(mrr: bool, p_at_k: bool, trust: bool, cost: bool) -> String {
    let mut parts: Vec<&str> = Vec::new();
    // Preserve the original combined relevance phrasing.
    match (mrr, p_at_k) {
        (true, true) => parts.push("both MRR and P@K dropped"),
        (true, false) => parts.push("MRR dropped"),
        (false, true) => parts.push("P@K dropped"),
        (false, false) => {}
    }
    if trust {
        parts.push("trust flipped");
    }
    if cost {
        parts.push("cost grew (advisory)");
    }
    parts.join("; ")
}
