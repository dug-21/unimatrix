//! Markdown rendering functions for eval report (nan-007 D4).
//!
//! Renders the five required sections from aggregated data structures:
//! 1. Summary, 2. Notable Ranking Changes, 3. Latency Distribution,
//! 4. Entry-Level Analysis, 5. Zero-Regression Check.

use std::collections::HashMap;

use super::ScoredEntry;
use super::{AggregateStats, EntryRankSummary, LatencyBucket, RegressionRecord, ScenarioResult};

// ---------------------------------------------------------------------------
// Type alias for the complex return type of find_notable_ranking_changes
// ---------------------------------------------------------------------------

/// `(scenario_id, query, kendall_tau, baseline_entries, candidate_entries)`
pub(super) type NotableEntry<'a> = (String, String, f64, &'a [ScoredEntry], &'a [ScoredEntry]);

// ---------------------------------------------------------------------------
// render_report
// ---------------------------------------------------------------------------

pub(super) fn render_report(
    stats: &[AggregateStats],
    results: &[ScenarioResult],
    regressions: &[RegressionRecord],
    latency_buckets: &[LatencyBucket],
    entry_rank_changes: &EntryRankSummary,
    query_map: &HashMap<String, String>,
) -> String {
    let mut md = String::new();

    // Title and metadata.
    let timestamp = chrono_now();
    let scenario_count = results.len();
    md.push_str("# Unimatrix Eval Report\n\n");
    md.push_str(&format!(
        "Generated: {timestamp} | Scenarios: {scenario_count}\n\n"
    ));

    // ----------------------------------------------------------------
    // SECTION 1: Summary (FR-27 item 1, AC-08)
    // ----------------------------------------------------------------
    md.push_str("## 1. Summary\n\n");
    if stats.is_empty() {
        md.push_str("_No results to summarize._\n\n");
    } else {
        md.push_str(
            "| Profile | Scenarios | P@K | MRR | Avg Latency (ms) | \u{0394}P@K | \u{0394}MRR | \u{0394}Latency (ms) |\n",
        );
        md.push_str(
            "|---------|-----------|-----|-----|-----------------|------|------|---------------|\n",
        );
        for stat in stats {
            let delta_p = if stat.p_at_k_delta == 0.0 {
                "\u{2014}".to_string()
            } else {
                format!("{:+.4}", stat.p_at_k_delta)
            };
            let delta_mrr = if stat.mrr_delta == 0.0 {
                "\u{2014}".to_string()
            } else {
                format!("{:+.4}", stat.mrr_delta)
            };
            let delta_lat = if stat.latency_delta_ms == 0.0 {
                "\u{2014}".to_string()
            } else {
                format!("{:+.1}", stat.latency_delta_ms)
            };
            md.push_str(&format!(
                "| {} | {} | {:.4} | {:.4} | {:.1} | {} | {} | {} |\n",
                stat.profile_name,
                stat.scenario_count,
                stat.mean_p_at_k,
                stat.mean_mrr,
                stat.mean_latency_ms,
                delta_p,
                delta_mrr,
                delta_lat,
            ));
        }
        md.push('\n');
    }

    // ----------------------------------------------------------------
    // SECTION 2: Notable Ranking Changes (FR-27 item 2, AC-08)
    // ----------------------------------------------------------------
    md.push_str("## 2. Notable Ranking Changes\n\n");
    let notable = find_notable_ranking_changes(results, query_map, 10);
    if notable.is_empty() {
        md.push_str("_No ranking changes across all scenarios._\n\n");
    } else {
        for (scenario_id, query, tau, baseline_entries, candidate_entries) in &notable {
            md.push_str(&format!("### {scenario_id}\n\n"));
            md.push_str(&format!("**Query**: {query}  \n"));
            md.push_str(&format!("**Kendall \u{03C4}**: {tau:.4}\n\n"));
            md.push_str("| Rank | Baseline Entry | Candidate Entry |\n");
            md.push_str("|------|---------------|-----------------|\n");
            let max_rows = baseline_entries.len().max(candidate_entries.len()).min(10);
            for i in 0..max_rows {
                let b_entry = baseline_entries
                    .get(i)
                    .map(|e| {
                        let title_len = e.title.len().min(30);
                        format!("{}: {}", e.id, &e.title[..title_len])
                    })
                    .unwrap_or_else(|| "-".to_string());
                let c_entry = candidate_entries
                    .get(i)
                    .map(|e| {
                        let title_len = e.title.len().min(30);
                        format!("{}: {}", e.id, &e.title[..title_len])
                    })
                    .unwrap_or_else(|| "-".to_string());
                md.push_str(&format!("| {} | {} | {} |\n", i + 1, b_entry, c_entry));
            }
            md.push('\n');
        }
    }

    // ----------------------------------------------------------------
    // SECTION 3: Latency Distribution (FR-27 item 3, AC-08)
    // ----------------------------------------------------------------
    md.push_str("## 3. Latency Distribution\n\n");
    md.push_str("| \u{2264} ms | Count |\n");
    md.push_str("|------|-------|\n");
    for bucket in latency_buckets {
        let label = if bucket.le_ms == u64::MAX {
            "> 2000".to_string()
        } else {
            format!("{}", bucket.le_ms)
        };
        md.push_str(&format!("| {} | {} |\n", label, bucket.count));
    }
    md.push('\n');

    // ----------------------------------------------------------------
    // SECTION 4: Entry-Level Analysis (FR-27 item 4, AC-08)
    // ----------------------------------------------------------------
    md.push_str("## 4. Entry-Level Analysis\n\n");
    md.push_str(&render_entry_analysis(entry_rank_changes));

    // ----------------------------------------------------------------
    // SECTION 5: Zero-Regression Check (FR-27 item 5, AC-08, AC-09)
    // ----------------------------------------------------------------
    md.push_str("## 5. Zero-Regression Check\n\n");
    if regressions.is_empty() {
        // Explicit empty-list indicator (AC-09, FR-28).
        md.push_str("**No regressions detected.** All candidate profiles maintain or improve MRR and P@K across all scenarios.\n\n");
    } else {
        md.push_str(&format!(
            "**{} regression(s) detected:**\n\n",
            regressions.len()
        ));
        md.push_str("| Scenario | Query | Profile | Reason | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K |\n");
        md.push_str("|----------|-------|---------|--------|-------------|--------------|-------------|---------------|\n");
        for reg in regressions {
            md.push_str(&format!(
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
        md.push('\n');
        md.push_str(
            "_This list is a human-reviewed artifact. No automated gate logic is applied._\n\n",
        );
    }

    md
}

// ---------------------------------------------------------------------------
// find_notable_ranking_changes (helper for Section 2)
// ---------------------------------------------------------------------------

/// Find the `top_n` scenarios with the lowest Kendall tau (most reordered).
///
/// Returns a vec of `(scenario_id, query, kendall_tau, baseline_entries, candidate_entries)`.
/// baseline_entries and candidate_entries are slices from the first vs second profiles.
fn find_notable_ranking_changes<'a>(
    results: &'a [ScenarioResult],
    query_map: &HashMap<String, String>,
    top_n: usize,
) -> Vec<NotableEntry<'a>> {
    // Collect scenarios that have at least two profiles (needed for comparison).
    let mut notable: Vec<NotableEntry<'a>> = results
        .iter()
        .filter(|r| r.profiles.len() >= 2)
        .filter_map(|result| {
            let tau = result.comparison.kendall_tau;

            // Build sorted profile names for deterministic ordering.
            let mut names: Vec<&str> = result.profiles.keys().map(|s| s.as_str()).collect();
            names.sort();
            if let Some(pos) = names.iter().position(|n| n.to_lowercase() == "baseline") {
                let baseline = names.remove(pos);
                names.insert(0, baseline);
            }

            let baseline_name = names.first()?;
            let candidate_name = names.get(1)?;

            let baseline_entries = result.profiles.get(*baseline_name)?.entries.as_slice();
            let candidate_entries = result.profiles.get(*candidate_name)?.entries.as_slice();

            let query = query_map
                .get(&result.scenario_id)
                .cloned()
                .unwrap_or_else(|| result.query.clone());

            Some((
                result.scenario_id.clone(),
                query,
                tau,
                baseline_entries,
                candidate_entries,
            ))
        })
        .collect();

    // Sort by Kendall tau ascending (lowest tau = most changed ordering).
    notable.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    notable.truncate(top_n);
    notable
}

// ---------------------------------------------------------------------------
// render_entry_analysis (helper for Section 4)
// ---------------------------------------------------------------------------

fn render_entry_analysis(summary: &EntryRankSummary) -> String {
    let mut out = String::new();

    if summary.most_promoted.is_empty() && summary.most_demoted.is_empty() {
        out.push_str("_No entry rank changes recorded._\n\n");
        return out;
    }

    if !summary.most_promoted.is_empty() {
        out.push_str("**Most Promoted Entries** (avg rank gain):\n\n");
        out.push_str("| Entry ID | Title | Avg Rank Gain |\n");
        out.push_str("|----------|-------|---------------|\n");
        for (id, title, gain) in &summary.most_promoted {
            let title_len = title.len().min(40);
            out.push_str(&format!(
                "| {} | {} | +{} |\n",
                id,
                &title[..title_len],
                gain
            ));
        }
        out.push('\n');
    }

    if !summary.most_demoted.is_empty() {
        out.push_str("**Most Demoted Entries** (avg rank loss):\n\n");
        out.push_str("| Entry ID | Title | Avg Rank Loss |\n");
        out.push_str("|----------|-------|---------------|\n");
        for (id, title, loss) in &summary.most_demoted {
            let title_len = title.len().min(40);
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                id,
                &title[..title_len],
                loss
            ));
        }
        out.push('\n');
    }

    out
}

// ---------------------------------------------------------------------------
// chrono_now — RFC 3339-ish timestamp without chrono dependency
// ---------------------------------------------------------------------------

pub(super) fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Simple epoch-seconds fallback since chrono is not a dependency.
    // Full timestamp formatting would require chrono; we use a readable substitute.
    format!("{secs} (unix epoch)")
}
