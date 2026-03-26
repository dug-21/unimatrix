//! Aggregation functions for eval report (nan-007 D4).
//!
//! Computes aggregate statistics, regressions, latency buckets, and
//! entry-level rank change summaries from per-scenario results.

use std::collections::HashMap;

use super::{AggregateStats, EntryRankSummary, LatencyBucket, RegressionRecord, ScenarioResult};

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
        let mut count = 0_usize;

        for result in results {
            if let Some(prof_result) = result.profiles.get(profile_name) {
                p_at_k_sum += prof_result.p_at_k;
                mrr_sum += prof_result.mrr;
                latency_sum += prof_result.latency_ms as f64;
                if profile_name != &baseline_name {
                    p_at_k_delta_sum += result.comparison.p_at_k_delta;
                    mrr_delta_sum += result.comparison.mrr_delta;
                    latency_delta_sum += result.comparison.latency_overhead_ms as f64;
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
                // nan-008 Wave 2 (aggregate.rs): populate mean_cc_at_k, mean_icd,
                // cc_at_k_delta, icd_delta from cc_at_k/icd sums.
                ..Default::default()
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
