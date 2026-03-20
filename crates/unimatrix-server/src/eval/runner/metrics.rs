//! Metric computation for eval runner (nan-007).
//!
//! Pure functions: P@K, MRR, Kendall tau (via `unimatrix_engine::test_scenarios`),
//! rank change list, ground truth resolution, and comparison metric assembly.
//!
//! All functions are `pub(super)` — consumed only by `replay.rs` and `mod.rs`.

use std::collections::{HashMap, HashSet};

// ADR-003, C-10: kendall_tau from test_scenarios requires test-support feature
use unimatrix_engine::test_scenarios::kendall_tau;

use crate::eval::scenarios::ScenarioRecord;

use super::output::{ComparisonMetrics, ProfileResult, RankChange, ScoredEntry};

// ---------------------------------------------------------------------------
// Ground truth resolution (AC-07, R-08)
// ---------------------------------------------------------------------------

/// Resolve ground truth with dual-mode semantics.
///
/// Priority: `expected` (hard labels from hand-authored scenarios) takes
/// precedence over `baseline.entry_ids` (soft ground truth from query_log).
/// Returns empty Vec when neither is present (P@K = 0.0, MRR = 0.0).
pub(super) fn determine_ground_truth(record: &ScenarioRecord) -> Vec<u64> {
    if let Some(expected) = &record.expected {
        expected.clone()
    } else if let Some(baseline) = &record.baseline {
        baseline.entry_ids.clone()
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Metric computation
// ---------------------------------------------------------------------------

/// Precision at K.
///
/// Returns the fraction of top-K results that appear in `ground_truth`.
/// Returns 0.0 if `ground_truth` is empty or `entries` is empty.
pub(super) fn compute_p_at_k(entries: &[ScoredEntry], ground_truth: &[u64], k: usize) -> f64 {
    if ground_truth.is_empty() || entries.is_empty() {
        return 0.0;
    }
    let gt_set: HashSet<u64> = ground_truth.iter().copied().collect();
    let top_k_len = k.min(entries.len());
    let hits = entries
        .iter()
        .take(k)
        .filter(|e| gt_set.contains(&e.id))
        .count();
    hits as f64 / top_k_len as f64
}

/// Mean Reciprocal Rank.
///
/// Returns the reciprocal of the rank of the first relevant result.
/// Returns 0.0 if `ground_truth` is empty or no relevant result is found.
pub(super) fn compute_mrr(entries: &[ScoredEntry], ground_truth: &[u64]) -> f64 {
    if ground_truth.is_empty() || entries.is_empty() {
        return 0.0;
    }
    let gt_set: HashSet<u64> = ground_truth.iter().copied().collect();
    for (i, entry) in entries.iter().enumerate() {
        if gt_set.contains(&entry.id) {
            return 1.0 / (i + 1) as f64;
        }
    }
    0.0
}

/// Compute ComparisonMetrics for baseline vs. first candidate profile.
///
/// For single-profile runs (no candidate), self-comparison produces
/// `kendall_tau = 1.0` and all deltas = 0.
pub(super) fn compute_comparison(
    profile_results: &HashMap<String, ProfileResult>,
    baseline_name: &str,
) -> Result<ComparisonMetrics, Box<dyn std::error::Error>> {
    let baseline = profile_results
        .get(baseline_name)
        .ok_or_else(|| format!("baseline profile '{}' not found in results", baseline_name))?;

    // Candidate = first non-baseline profile, or self (single-profile run)
    let candidate = profile_results
        .keys()
        .find(|k| k.as_str() != baseline_name)
        .and_then(|name| profile_results.get(name))
        .unwrap_or(baseline);

    let baseline_ids: Vec<u64> = baseline.entries.iter().map(|e| e.id).collect();
    let candidate_ids: Vec<u64> = candidate.entries.iter().map(|e| e.id).collect();

    // Kendall tau: only valid when both lists have the same elements.
    // When profiles produce different result sets, compute tau over the intersection.
    let tau = compute_tau_safe(&baseline_ids, &candidate_ids);

    let rank_changes = compute_rank_changes(&baseline_ids, &candidate_ids);

    Ok(ComparisonMetrics {
        kendall_tau: tau,
        rank_changes,
        mrr_delta: candidate.mrr - baseline.mrr,
        p_at_k_delta: candidate.p_at_k - baseline.p_at_k,
        latency_overhead_ms: candidate.latency_ms as i64 - baseline.latency_ms as i64,
    })
}

/// Compute Kendall tau safely when baseline and candidate may have different entry sets.
///
/// `kendall_tau()` from `unimatrix_engine::test_scenarios` requires both slices
/// to contain exactly the same elements. When profiles differ, their result sets
/// may diverge. We compute tau over the shared intersection in baseline order.
///
/// Special cases:
/// - Empty lists → 0.0 (undefined)
/// - No overlap → 0.0 (no ranking signal)
/// - Single element → 1.0 (per `kendall_tau` convention for n <= 1)
pub(super) fn compute_tau_safe(baseline_ids: &[u64], candidate_ids: &[u64]) -> f64 {
    if baseline_ids.is_empty() || candidate_ids.is_empty() {
        return 0.0;
    }

    let candidate_set: HashSet<u64> = candidate_ids.iter().copied().collect();
    let baseline_set: HashSet<u64> = baseline_ids.iter().copied().collect();

    // Intersection in baseline order
    let common_baseline: Vec<u64> = baseline_ids
        .iter()
        .copied()
        .filter(|id| candidate_set.contains(id))
        .collect();

    // Intersection in candidate order
    let common_candidate: Vec<u64> = candidate_ids
        .iter()
        .copied()
        .filter(|id| baseline_set.contains(id))
        .collect();

    if common_baseline.is_empty() {
        return 0.0;
    }

    kendall_tau(&common_baseline, &common_candidate)
}

/// Compute rank changes between baseline and candidate result lists.
///
/// Entries that moved, appeared in only one list, or dropped out are recorded.
/// Sorted by magnitude of rank change (largest first).
pub(super) fn compute_rank_changes(baseline_ids: &[u64], candidate_ids: &[u64]) -> Vec<RankChange> {
    let baseline_pos: HashMap<u64, usize> = baseline_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i + 1)) // 1-indexed
        .collect();

    let candidate_pos: HashMap<u64, usize> = candidate_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i + 1))
        .collect();

    let all_ids: HashSet<u64> = baseline_pos
        .keys()
        .chain(candidate_pos.keys())
        .copied()
        .collect();

    let mut changes: Vec<RankChange> = Vec::new();

    for id in all_ids {
        let from = baseline_pos.get(&id).copied();
        let to = candidate_pos.get(&id).copied();
        match (from, to) {
            (Some(f), Some(t)) if f != t => {
                changes.push(RankChange {
                    entry_id: id,
                    from_rank: f,
                    to_rank: t,
                });
            }
            (Some(f), None) => {
                // Dropped from candidate results
                changes.push(RankChange {
                    entry_id: id,
                    from_rank: f,
                    to_rank: candidate_ids.len() + 1,
                });
            }
            (None, Some(t)) => {
                // New in candidate results
                changes.push(RankChange {
                    entry_id: id,
                    from_rank: baseline_ids.len() + 1,
                    to_rank: t,
                });
            }
            _ => {} // unchanged or not in either list
        }
    }

    // Sort by magnitude of rank change, largest first
    changes.sort_by(|a, b| {
        let delta_a = (a.to_rank as i64 - a.from_rank as i64).unsigned_abs();
        let delta_b = (b.to_rank as i64 - b.from_rank as i64).unsigned_abs();
        delta_b.cmp(&delta_a)
    });

    changes
}
