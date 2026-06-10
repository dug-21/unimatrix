//! Correlated Trust / Relevance / Cost section (nan-018, AC-04/AC-14).
//!
//! Extracted from render.rs to respect the 500-line file limit (ADR-001). Renders
//! the "## 5C." section: for the SAME scenarios, in one table, the trust verdict +
//! P@5/MRR + token-weighted cost (with a cost delta vs the sorted-key baseline) so
//! a steepness sweep reads as "steepness X -> trust held AND relevance did not
//! regress AND cost moved by delta" (the AC-14 condition-1 surface).

use super::ScenarioResult;

/// Baseline profile name for a result: "baseline" (case-insensitive) forced first,
/// else the alphabetically first profile name. Sorted-key selection (R-03 #2610) —
/// never HashMap iteration order. Returns `None` for an empty profile set.
fn correlated_baseline_name(result: &ScenarioResult) -> Option<String> {
    if result.profiles.is_empty() {
        return None;
    }
    let mut names: Vec<&str> = result.profiles.keys().map(|s| s.as_str()).collect();
    names.sort();
    if let Some(pos) = names.iter().position(|n| n.to_lowercase() == "baseline") {
        let b = names.remove(pos);
        names.insert(0, b);
    }
    names.first().map(|s| s.to_string())
}

/// Render the correlated trust + P@5/MRR + cost section: for EVERY profile of
/// EVERY scenario, one row carrying the trust verdict, P@K, MRR, and cost (with a
/// cost delta vs the scenario's sorted-key baseline).
pub(super) fn render_correlated_trust_cost(results: &[ScenarioResult]) -> String {
    let mut out = String::new();
    // Heading is "## 5C." (no period after 5) so it does NOT collide with the
    // existing `## 5.` section-count assertions in the report pipeline tests.
    out.push_str("## 5C. Correlated Trust / Relevance / Cost\n\n");

    if results.is_empty() {
        out.push_str("_No results to correlate._\n\n");
        return out;
    }

    out.push_str(
        "_Trust = property-assertion verdict (absence + rank). Cost = token-weighted \u{0394} vs baseline (advisory)._\n\n",
    );
    out.push_str("| Scenario | Profile | Trust | P@K | MRR | Cost (tokens) | \u{0394} Cost |\n");
    out.push_str("|----------|---------|-------|-----|-----|---------------|--------|\n");

    for result in results {
        let baseline_name = match correlated_baseline_name(result) {
            Some(n) => n,
            None => continue,
        };
        let baseline_cost = result
            .profiles
            .get(&baseline_name)
            .map(|p| p.cost_tokens)
            .unwrap_or(0.0);

        // Deterministic per-scenario ordering: sorted keys, baseline first.
        let mut names: Vec<&str> = result.profiles.keys().map(|s| s.as_str()).collect();
        names.sort();
        if let Some(pos) = names.iter().position(|n| *n == baseline_name.as_str()) {
            let b = names.remove(pos);
            names.insert(0, b);
        }

        for name in names {
            let prof = match result.profiles.get(name) {
                Some(p) => p,
                None => continue,
            };
            // Trust verdict: PASS only when BOTH verdicts hold; otherwise FAIL.
            let trust = if prof.trust.absence_pass && prof.trust.rank_pass {
                "pass"
            } else {
                "FAIL"
            };
            let delta = prof.cost_tokens - baseline_cost;
            let delta_str = if name == baseline_name.as_str() {
                "\u{2014}".to_string()
            } else {
                format!("{delta:+.1}")
            };
            out.push_str(&format!(
                "| {} | {} | {} | {:.4} | {:.4} | {:.1} | {} |\n",
                result.scenario_id, name, trust, prof.p_at_k, prof.mrr, prof.cost_tokens, delta_str,
            ));
        }
    }
    out.push('\n');
    out
}
