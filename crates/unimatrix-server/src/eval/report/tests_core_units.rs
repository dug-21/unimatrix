//! Core aggregate unit tests for eval/report (nan-007).
//!
//! Unit tests for `compute_aggregate_stats`, `find_regressions`,
//! and `compute_latency_buckets`. Split from tests.rs for 500-line compliance.

use std::collections::HashMap;

use super::aggregate::{compute_aggregate_stats, compute_latency_buckets, find_regressions};
use super::{ProfileResult, ScenarioResult, default_comparison};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_profile_result(p_at_k: f64, mrr: f64, latency_ms: u64) -> ProfileResult {
    ProfileResult {
        entries: Vec::new(),
        latency_ms,
        p_at_k,
        mrr,
        cc_at_k: 0.0,
        icd: 0.0,
    }
}

fn make_scenario_result(
    id: &str,
    _query: &str,
    baseline_p: f64,
    baseline_mrr: f64,
    candidate_p: f64,
    candidate_mrr: f64,
) -> ScenarioResult {
    use super::ComparisonMetrics;
    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        make_profile_result(baseline_p, baseline_mrr, 50),
    );
    profiles.insert(
        "candidate".to_string(),
        make_profile_result(candidate_p, candidate_mrr, 60),
    );
    ScenarioResult {
        scenario_id: id.to_string(),
        query: _query.to_string(),
        profiles,
        phase: None,
        comparison: ComparisonMetrics {
            kendall_tau: 0.8,
            rank_changes: Vec::new(),
            mrr_delta: candidate_mrr - baseline_mrr,
            p_at_k_delta: candidate_p - baseline_p,
            latency_overhead_ms: 10,
            cc_at_k_delta: 0.0,
            icd_delta: 0.0,
        },
    }
}

// ---------------------------------------------------------------------------
// Unit: compute_aggregate_stats — baseline has zero deltas
// ---------------------------------------------------------------------------

#[test]
fn test_compute_aggregate_stats_baseline_has_zero_deltas() {
    let r = make_scenario_result("s1", "q1", 0.6, 0.5, 0.7, 0.6);
    let stats = compute_aggregate_stats(&[r]);

    let baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap();
    assert_eq!(baseline.p_at_k_delta, 0.0);
    assert_eq!(baseline.mrr_delta, 0.0);
    assert_eq!(baseline.latency_delta_ms, 0.0);
}

// ---------------------------------------------------------------------------
// Unit: find_regressions — multiple regressions sorted worst-first
// ---------------------------------------------------------------------------

#[test]
fn test_find_regressions_sorted_worst_mrr_first() {
    let r1 = make_scenario_result("s1", "q1", 0.6, 0.8, 0.6, 0.3); // MRR delta = 0.5
    let r2 = make_scenario_result("s2", "q2", 0.6, 0.8, 0.6, 0.6); // MRR delta = 0.2
    let query_map = HashMap::new();
    let regressions = find_regressions(&[r1, r2], &query_map);

    assert_eq!(regressions.len(), 2);
    assert_eq!(regressions[0].scenario_id, "s1");
    assert_eq!(regressions[1].scenario_id, "s2");
}

// ---------------------------------------------------------------------------
// Unit: compute_latency_buckets — correct bucket placement
// ---------------------------------------------------------------------------

#[test]
fn test_compute_latency_buckets_correct_placement() {
    let mut r = make_scenario_result("s1", "q1", 0.6, 0.5, 0.7, 0.6);
    r.profiles.get_mut("baseline").unwrap().latency_ms = 50;
    r.profiles.get_mut("candidate").unwrap().latency_ms = 150;

    let buckets = compute_latency_buckets(&[r]);

    let b50 = buckets.iter().find(|b| b.le_ms == 50).unwrap();
    assert_eq!(b50.count, 1, "latency=50 must land in ≤50 bucket");

    let b200 = buckets.iter().find(|b| b.le_ms == 200).unwrap();
    assert_eq!(b200.count, 1, "latency=150 must land in ≤200 bucket");
}

// ---------------------------------------------------------------------------
// Unit: find_regressions — stable when only one profile (no candidate)
// ---------------------------------------------------------------------------

#[test]
fn test_find_regressions_single_profile_no_regressions() {
    let mut r = ScenarioResult {
        scenario_id: "s1".to_string(),
        query: "q".to_string(),
        profiles: HashMap::new(),
        phase: None,
        comparison: default_comparison(),
    };
    r.profiles
        .insert("baseline".to_string(), make_profile_result(0.6, 0.5, 50));

    let query_map = HashMap::new();
    let regressions = find_regressions(&[r], &query_map);
    assert!(
        regressions.is_empty(),
        "single profile must produce no regressions"
    );
}
