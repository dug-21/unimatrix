//! Aggregation functions for eval report (nan-007 D4, extended nan-008).
//!
//! Computes aggregate statistics, regressions, latency buckets, and
//! entry-level rank change summaries from per-scenario results.

use std::collections::HashMap;

use super::{
    AggregateStats, CcAtKScenarioRow, EntryRankSummary, LatencyBucket, PhaseAggregateStats,
    RegressionRecord, ScenarioResult,
};

// ---------------------------------------------------------------------------
// compute_aggregate_stats
// ---------------------------------------------------------------------------

pub(super) fn compute_aggregate_stats(results: &[ScenarioResult]) -> Vec<AggregateStats> {
    if results.is_empty() {
        return Vec::new();
    }

    // Collect all profile names across all results for completeness.
    let mut profile_name_set: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for result in results {
        for name in result.profiles.keys() {
            profile_name_set.insert(name.clone());
        }
    }

    // Sort profile names: "baseline" forced first, then alphabetical.
    let mut profile_names: Vec<String> = profile_name_set.into_iter().collect();
    profile_names.sort();
    if let Some(pos) = profile_names
        .iter()
        .position(|n| n.to_lowercase() == "baseline")
    {
        let baseline = profile_names.remove(pos);
        profile_names.insert(0, baseline);
    }

    let baseline_name = profile_names.first().cloned().unwrap_or_default();

    let mut stats: Vec<AggregateStats> = Vec::new();

    for profile_name in &profile_names {
        let mut p_at_k_sum = 0.0_f64;
        let mut mrr_sum = 0.0_f64;
        let mut latency_sum = 0.0_f64;
        let mut p_at_k_delta_sum = 0.0_f64;
        let mut mrr_delta_sum = 0.0_f64;
        let mut latency_delta_sum = 0.0_f64;
        let mut cc_at_k_sum = 0.0_f64;
        let mut icd_sum = 0.0_f64;
        let mut cc_at_k_delta_sum = 0.0_f64;
        let mut icd_delta_sum = 0.0_f64;
        let mut count = 0_usize;

        for result in results {
            if let Some(prof_result) = result.profiles.get(profile_name) {
                p_at_k_sum += prof_result.p_at_k;
                mrr_sum += prof_result.mrr;
                latency_sum += prof_result.latency_ms as f64;
                cc_at_k_sum += prof_result.cc_at_k;
                icd_sum += prof_result.icd;
                if profile_name != &baseline_name {
                    p_at_k_delta_sum += result.comparison.p_at_k_delta;
                    mrr_delta_sum += result.comparison.mrr_delta;
                    latency_delta_sum += result.comparison.latency_overhead_ms as f64;
                    cc_at_k_delta_sum += result.comparison.cc_at_k_delta;
                    icd_delta_sum += result.comparison.icd_delta;
                }
                count += 1;
            }
        }

        if count > 0 {
            let is_baseline = profile_name == &baseline_name;
            stats.push(AggregateStats {
                profile_name: profile_name.clone(),
                scenario_count: count,
                mean_p_at_k: p_at_k_sum / count as f64,
                mean_mrr: mrr_sum / count as f64,
                mean_latency_ms: latency_sum / count as f64,
                p_at_k_delta: if is_baseline {
                    0.0
                } else {
                    p_at_k_delta_sum / count as f64
                },
                mrr_delta: if is_baseline {
                    0.0
                } else {
                    mrr_delta_sum / count as f64
                },
                latency_delta_ms: if is_baseline {
                    0.0
                } else {
                    latency_delta_sum / count as f64
                },
                mean_cc_at_k: cc_at_k_sum / count as f64,
                mean_icd: icd_sum / count as f64,
                cc_at_k_delta: if is_baseline {
                    0.0
                } else {
                    cc_at_k_delta_sum / count as f64
                },
                icd_delta: if is_baseline {
                    0.0
                } else {
                    icd_delta_sum / count as f64
                },
            });
        }
    }

    stats
}

// ---------------------------------------------------------------------------
// find_regressions
// ---------------------------------------------------------------------------

/// Detect regressions using OR semantics (AC-09, R-12).
///
/// A scenario-profile pair is a regression if the candidate MRR < baseline MRR
/// OR the candidate P@K < baseline P@K. Strict less-than: equal is NOT a regression.
///
/// WARN-C mitigation: uses sorted profile names to ensure stable baseline selection.
pub(super) fn find_regressions(
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

            // OR semantics: regression if MRR OR P@K is strictly lower.
            let mrr_regressed = prof_result.mrr < baseline_result.mrr;
            let p_at_k_regressed = prof_result.p_at_k < baseline_result.p_at_k;

            if mrr_regressed || p_at_k_regressed {
                let reason = match (mrr_regressed, p_at_k_regressed) {
                    (true, true) => "both MRR and P@K dropped".to_string(),
                    (true, false) => "MRR dropped".to_string(),
                    (false, true) => "P@K dropped".to_string(),
                    _ => unreachable!(),
                };

                let query_text = query_map
                    .get(&result.scenario_id)
                    .cloned()
                    .unwrap_or_else(|| result.query.clone());

                regressions.push(RegressionRecord {
                    scenario_id: result.scenario_id.clone(),
                    query: query_text,
                    profile_name: profile_name.to_string(),
                    baseline_mrr: baseline_result.mrr,
                    candidate_mrr: prof_result.mrr,
                    baseline_p_at_k: baseline_result.p_at_k,
                    candidate_p_at_k: prof_result.p_at_k,
                    reason,
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

// ---------------------------------------------------------------------------
// compute_latency_buckets
// ---------------------------------------------------------------------------

pub(super) fn compute_latency_buckets(results: &[ScenarioResult]) -> Vec<LatencyBucket> {
    const BOUNDARIES: &[u64] = &[50, 100, 200, 500, 1000, 2000, u64::MAX];
    let mut counts = vec![0_usize; BOUNDARIES.len()];

    for result in results {
        for prof_result in result.profiles.values() {
            let lat = prof_result.latency_ms;
            for (i, &bound) in BOUNDARIES.iter().enumerate() {
                if lat <= bound {
                    counts[i] += 1;
                    break;
                }
            }
        }
    }

    BOUNDARIES
        .iter()
        .zip(counts.iter())
        .map(|(&le_ms, &count)| LatencyBucket { le_ms, count })
        .collect()
}

// ---------------------------------------------------------------------------
// compute_entry_rank_changes
// ---------------------------------------------------------------------------

pub(super) fn compute_entry_rank_changes(results: &[ScenarioResult]) -> EntryRankSummary {
    // Accumulate per-entry rank deltas across all scenarios.
    // rank_delta = from_rank - to_rank (positive = promoted, negative = demoted).
    let mut entry_deltas: HashMap<u64, (String, Vec<i64>)> = HashMap::new();

    for result in results {
        for change in &result.comparison.rank_changes {
            let delta = change.from_rank as i64 - change.to_rank as i64;
            entry_deltas
                .entry(change.entry_id)
                .or_insert_with(|| ("unknown".to_string(), Vec::new()))
                .1
                .push(delta);
        }
    }

    // Enrich titles from profile results.
    for result in results {
        for prof_result in result.profiles.values() {
            for scored in &prof_result.entries {
                if let Some(record) = entry_deltas.get_mut(&scored.id)
                    && record.0 == "unknown"
                {
                    record.0 = scored.title.clone();
                }
            }
        }
    }

    // Compute mean delta per entry.
    let mut mean_deltas: Vec<(u64, String, f64)> = entry_deltas
        .into_iter()
        .map(|(id, (title, deltas))| {
            let mean = deltas.iter().sum::<i64>() as f64 / deltas.len() as f64;
            (id, title, mean)
        })
        .collect();

    // Sort for deterministic output.
    mean_deltas.sort_by(|a, b| a.0.cmp(&b.0));

    // Most promoted: highest positive delta (ascending from_rank to_rank means promoted).
    let mut promoted = mean_deltas.clone();
    promoted.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let most_promoted: Vec<(u64, String, i64)> = promoted
        .into_iter()
        .filter(|(_, _, d)| *d > 0.0)
        .take(10)
        .map(|(id, title, d)| (id, title, d.round() as i64))
        .collect();

    // Most demoted: most negative delta.
    let mut demoted = mean_deltas;
    demoted.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
    let most_demoted: Vec<(u64, String, i64)> = demoted
        .into_iter()
        .filter(|(_, _, d)| *d < 0.0)
        .take(10)
        .map(|(id, title, d)| (id, title, d.round() as i64))
        .collect();

    EntryRankSummary {
        most_promoted,
        most_demoted,
    }
}

// ---------------------------------------------------------------------------
// compute_cc_at_k_scenario_rows
// ---------------------------------------------------------------------------

/// Collect per-scenario CC@k rows for the Distribution Analysis section (nan-008).
///
/// Only produces rows when two or more profiles are present in a result (baseline +
/// candidate comparison is meaningful). Single-profile results are skipped. The
/// returned Vec is sorted by `cc_at_k_delta` descending (most improved first,
/// most degraded last) so that `render.rs` can take the first N and last N rows
/// without re-sorting.
///
/// `cc_at_k_delta` is taken directly from `result.comparison.cc_at_k_delta` rather
/// than recomputed, keeping it consistent with the Summary table values.
pub(super) fn compute_cc_at_k_scenario_rows(results: &[ScenarioResult]) -> Vec<CcAtKScenarioRow> {
    let mut rows: Vec<CcAtKScenarioRow> = Vec::new();

    for result in results {
        // Need at least two profiles for a meaningful comparison.
        if result.profiles.len() < 2 {
            continue;
        }

        // Determine baseline using the same sort+force-first logic as
        // compute_aggregate_stats for deterministic, consistent selection.
        let mut profile_names: Vec<&str> = result.profiles.keys().map(|s| s.as_str()).collect();
        profile_names.sort();
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

        // Take the first non-baseline profile as the primary candidate.
        let candidate_name = match profile_names.iter().find(|&&n| n != baseline_name) {
            Some(n) => *n,
            None => continue,
        };

        let candidate_result = match result.profiles.get(candidate_name) {
            Some(r) => r,
            None => continue,
        };

        // Truncate query to ~60 chars for readability in the report table.
        let query = if result.query.len() > 60 {
            format!("{}…", &result.query[..60])
        } else {
            result.query.clone()
        };

        rows.push(CcAtKScenarioRow {
            scenario_id: result.scenario_id.clone(),
            query,
            baseline_cc_at_k: baseline_result.cc_at_k,
            candidate_cc_at_k: candidate_result.cc_at_k,
            // Use the stored delta — not recomputed — for consistency with Summary table.
            cc_at_k_delta: result.comparison.cc_at_k_delta,
        });
    }

    // Sort descending by cc_at_k_delta: largest positive delta first,
    // most negative last (R-12 guard). render.rs can then take rows[0..N]
    // for top improvements and rows[len-N..] reversed for top degradations.
    rows.sort_by(|a, b| {
        b.cc_at_k_delta
            .partial_cmp(&a.cc_at_k_delta)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    rows
}

// ---------------------------------------------------------------------------
// compute_phase_stats
// ---------------------------------------------------------------------------

/// Group results by phase and compute per-phase mean metrics (nan-009).
///
/// Returns empty `Vec` when all phases are `None` (R-07, AC-04: section 6 omitted).
/// Sort: named phases alphabetically; `"(unset)"` last — plain lex is wrong because
/// `(` (ASCII 40) < `a` (ASCII 97); explicit override required (ADR-003).
pub(super) fn compute_phase_stats(results: &[ScenarioResult]) -> Vec<PhaseAggregateStats> {
    if results.is_empty() {
        return Vec::new();
    }

    // All-null guard: return empty so caller skips section 6 (R-07, AC-04).
    if !results.iter().any(|r| r.phase.is_some()) {
        return Vec::new();
    }

    struct Acc {
        count: usize,
        sum_p: f64,
        sum_mrr: f64,
        sum_cc: f64,
        sum_icd: f64,
    }

    let mut groups: HashMap<Option<String>, Acc> = HashMap::new();

    for result in results {
        let Some((p, mrr, cc, icd)) = baseline_metrics(result) else {
            continue;
        };
        let acc = groups.entry(result.phase.clone()).or_insert(Acc {
            count: 0,
            sum_p: 0.0,
            sum_mrr: 0.0,
            sum_cc: 0.0,
            sum_icd: 0.0,
        });
        acc.count += 1;
        acc.sum_p += p;
        acc.sum_mrr += mrr;
        acc.sum_cc += cc;
        acc.sum_icd += icd;
    }

    let mut stats: Vec<PhaseAggregateStats> = groups
        .into_iter()
        .filter(|(_, a)| a.count > 0)
        .map(|(key, a)| PhaseAggregateStats {
            phase_label: key.unwrap_or_else(|| "(unset)".to_string()), // ADR-003; NOT "(none)"
            scenario_count: a.count,
            mean_p_at_k: a.sum_p / a.count as f64,
            mean_mrr: a.sum_mrr / a.count as f64,
            mean_cc_at_k: a.sum_cc / a.count as f64,
            mean_icd: a.sum_icd / a.count as f64,
        })
        .collect();

    // Explicit sort: named phases ascending; "(unset)" last.
    // '(' (ASCII 40) < 'a' — naive lex would put "(unset)" first. ADR-003 requires last.
    stats.sort_by(
        |a, b| match (a.phase_label.as_str(), b.phase_label.as_str()) {
            ("(unset)", "(unset)") => std::cmp::Ordering::Equal,
            ("(unset)", _) => std::cmp::Ordering::Greater,
            (_, "(unset)") => std::cmp::Ordering::Less,
            (x, y) => x.cmp(y),
        },
    );

    stats
}

/// Select the baseline profile and return (p_at_k, mrr, cc_at_k, icd).
///
/// Baseline selection: "baseline" (case-insensitive) forced first; otherwise the
/// alphabetically first profile name. Consistent with `compute_aggregate_stats`.
/// Returns `None` if the result has no profiles.
fn baseline_metrics(result: &ScenarioResult) -> Option<(f64, f64, f64, f64)> {
    if result.profiles.is_empty() {
        return None;
    }
    let mut names: Vec<&str> = result.profiles.keys().map(|s| s.as_str()).collect();
    names.sort();
    if let Some(pos) = names.iter().position(|n| n.to_lowercase() == "baseline") {
        let b = names.remove(pos);
        names.insert(0, b);
    }
    let prof = result.profiles.get(*names.first()?)?;
    Some((prof.p_at_k, prof.mrr, prof.cc_at_k, prof.icd))
}

pub(super) mod distribution;
pub(super) use distribution::{DistributionGateResult, MetricGateRow, check_distribution_targets};
