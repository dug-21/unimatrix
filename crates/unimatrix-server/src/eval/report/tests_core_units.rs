//! Core aggregate unit tests for eval/report (nan-007).
//!
//! Unit tests for `compute_aggregate_stats`, `find_regressions`,
//! and `compute_latency_buckets`. Split from tests.rs for 500-line compliance.

use std::collections::HashMap;

use super::aggregate::{compute_aggregate_stats, compute_latency_buckets, find_regressions};
use super::{ProfileResult, ScenarioResult, TrustOutcome, default_comparison};

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
        cost_tokens: 0.0,
        trust: TrustOutcome::default(),
    }
}

/// Build a `ProfileResult` carrying explicit cost + trust verdicts, for the
/// nan-018 trust-flip / cost-growth / OR-composition regression tests.
fn make_profile_result_full(
    p_at_k: f64,
    mrr: f64,
    cost_tokens: f64,
    absence_pass: bool,
    rank_pass: bool,
    violations: Vec<String>,
) -> ProfileResult {
    ProfileResult {
        entries: Vec::new(),
        latency_ms: 50,
        p_at_k,
        mrr,
        cc_at_k: 0.0,
        icd: 0.0,
        cost_tokens,
        trust: TrustOutcome {
            absence_pass,
            rank_pass,
            violations,
        },
    }
}

/// Assemble a two-profile (`baseline` + `candidate`) `ScenarioResult` directly
/// from full profile results, for the nan-018 regression-fold tests.
fn scenario_from_profiles(
    id: &str,
    query: &str,
    baseline: ProfileResult,
    candidate: ProfileResult,
) -> ScenarioResult {
    let mut profiles = HashMap::new();
    profiles.insert("baseline".to_string(), baseline);
    profiles.insert("candidate".to_string(), candidate);
    ScenarioResult {
        scenario_id: id.to_string(),
        query: query.to_string(),
        profiles,
        comparison: default_comparison(),
        phase: None,
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

// ===========================================================================
// nan-018 — report extensions: trust-flip + cost-growth OR-fold (R-12, R-17)
// ===========================================================================

/// Trust-flip registers a regression (AC-02/03, R-12): baseline satisfies an
/// assertion (absence), candidate violates it ⇒ appears in the Section 5 list.
#[test]
fn test_trust_flip_registers_regression() {
    // Relevance held equal (no MRR/P@K drop), cost equal — only trust flips.
    let baseline = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let candidate = make_profile_result_full(
        0.6,
        0.5,
        10.0,
        false, // absence verdict flipped
        true,
        vec!["absence: forbidden 'F' present at rank 0".to_string()],
    );
    let r = scenario_from_profiles("s-trust-flip", "q", baseline, candidate);

    let regressions = find_regressions(&[r], &HashMap::new());
    assert_eq!(
        regressions.len(),
        1,
        "trust flip must register a regression"
    );
    let reg = &regressions[0];
    assert!(reg.reasons.contains(&"trust".to_string()));
    assert_eq!(
        reg.trust_violations.len(),
        1,
        "candidate trust violations must be surfaced for triage"
    );
    assert!(reg.reason.contains("trust"));
}

/// No-flip: both baseline and candidate satisfy ⇒ no regression (R-12.2).
#[test]
fn test_trust_no_flip_no_regression() {
    let baseline = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let candidate = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let r = scenario_from_profiles("s-no-flip", "q", baseline, candidate);

    let regressions = find_regressions(&[r], &HashMap::new());
    assert!(
        regressions.is_empty(),
        "both satisfy + equal relevance/cost ⇒ no regression"
    );
}

/// A trust pass that REPAIRS a baseline failure is NOT a regression: baseline
/// failed, candidate passes ⇒ no trust flip (the asymmetry of the flip check).
#[test]
fn test_trust_repair_is_not_a_regression() {
    let baseline = make_profile_result_full(0.6, 0.5, 10.0, false, true, vec!["v".to_string()]);
    let candidate = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let r = scenario_from_profiles("s-repair", "q", baseline, candidate);

    let regressions = find_regressions(&[r], &HashMap::new());
    assert!(
        regressions.is_empty(),
        "candidate repairing a baseline trust failure is not a regression"
    );
}

/// OR-composition (R-12.3): a candidate that HOLDS trust but REGRESSES MRR is
/// still flagged (trust pass does not mask a relevance regression).
#[test]
fn test_or_composition_trust_holds_mrr_regresses_flagged() {
    let baseline = make_profile_result_full(0.6, 0.8, 10.0, true, true, vec![]);
    let candidate = make_profile_result_full(0.6, 0.3, 10.0, true, true, vec![]); // MRR drops
    let r = scenario_from_profiles("s-or-mrr", "q", baseline, candidate);

    let regressions = find_regressions(&[r], &HashMap::new());
    assert_eq!(regressions.len(), 1, "MRR regression must still be flagged");
    assert!(regressions[0].reasons.contains(&"mrr".to_string()));
    assert!(
        !regressions[0].reasons.contains(&"trust".to_string()),
        "trust held — must not be listed as a trust flip"
    );
}

/// OR-composition inverse (R-12.3): a candidate that HOLDS relevance but FLIPS a
/// trust assertion IS flagged (relevance pass does not mask a trust flip).
#[test]
fn test_or_composition_relevance_holds_trust_flips_flagged() {
    let baseline = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let candidate = make_profile_result_full(
        0.6,
        0.5,
        10.0,
        true,
        false, // rank verdict flipped
        vec!["rank_below: 'A'(rank 0) not below 'B'(rank 1)".to_string()],
    );
    let r = scenario_from_profiles("s-or-trust", "q", baseline, candidate);

    let regressions = find_regressions(&[r], &HashMap::new());
    assert_eq!(regressions.len(), 1, "trust flip must be flagged");
    assert!(regressions[0].reasons.contains(&"trust".to_string()));
}

/// Cost growth (any positive delta, ε=0.0) is reported in the regression block
/// (AC-09) — even when relevance and trust both hold.
#[test]
fn test_cost_growth_reported_advisory() {
    let baseline = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let candidate = make_profile_result_full(0.6, 0.5, 10.5, true, true, vec![]); // +0.5 cost
    let r = scenario_from_profiles("s-cost", "q", baseline, candidate);

    let regressions = find_regressions(&[r], &HashMap::new());
    assert_eq!(
        regressions.len(),
        1,
        "any cost growth (>0.0) must be reported"
    );
    let reg = &regressions[0];
    assert!(reg.reasons.contains(&"cost".to_string()));
    assert!(
        (reg.cost_delta - 0.5).abs() < 1e-9,
        "cost_delta must be +0.5"
    );
    assert!(reg.reason.contains("advisory"), "cost reason is advisory");
}

/// Cost growth blocks NOTHING (FR-12a): the report still always returns Ok and
/// exits 0 — proven via the public `run_report` path in the exit-code test below.
/// Here we assert the structured signal: a cost-only regression has NO relevance
/// or trust reason, confirming it is purely advisory body content.
#[test]
fn test_cost_growth_blocks_nothing_advisory_only() {
    let baseline = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let candidate = make_profile_result_full(0.6, 0.5, 99.0, true, true, vec![]);
    let r = scenario_from_profiles("s-cost-only", "q", baseline, candidate);

    let regressions = find_regressions(&[r], &HashMap::new());
    assert_eq!(regressions.len(), 1);
    let reasons = &regressions[0].reasons;
    assert_eq!(
        reasons,
        &vec!["cost".to_string()],
        "a cost-only regression must carry ONLY the advisory cost reason"
    );
}

/// No cost growth (delta <= 0.0) with everything else equal ⇒ NOT a regression.
/// Confirms the strict `> 0.0` (ε=0.0) boundary: equal cost is not growth.
#[test]
fn test_cost_equal_is_not_growth() {
    let baseline = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let candidate = make_profile_result_full(0.6, 0.5, 10.0, true, true, vec![]);
    let r = scenario_from_profiles("s-cost-eq", "q", baseline, candidate);

    assert!(
        find_regressions(&[r], &HashMap::new()).is_empty(),
        "equal cost is not growth (strict > 0.0 boundary)"
    );
}

/// Baseline determinism (#2610 lineage): baseline = first profile by SORTED key,
/// never HashMap iteration order. Two profiles whose names sort such that the
/// LOWER metric profile is the baseline ⇒ the candidate is a regression; the
/// verdict must not depend on insertion order.
#[test]
fn test_baseline_selection_sorts_profile_keys() {
    use super::ComparisonMetrics;
    // Names "aaa" (sorts first ⇒ baseline) and "zzz" (candidate). "aaa" has the
    // higher MRR so "zzz" regresses. No "baseline"-named profile is present, so
    // selection falls back to the sorted-first key.
    let mut profiles = HashMap::new();
    profiles.insert(
        "zzz".to_string(),
        make_profile_result_full(0.6, 0.3, 10.0, true, true, vec![]),
    );
    profiles.insert(
        "aaa".to_string(),
        make_profile_result_full(0.6, 0.8, 10.0, true, true, vec![]),
    );
    let r = ScenarioResult {
        scenario_id: "s-sorted".to_string(),
        query: "q".to_string(),
        profiles,
        phase: None,
        comparison: ComparisonMetrics {
            kendall_tau: 1.0,
            rank_changes: Vec::new(),
            mrr_delta: 0.0,
            p_at_k_delta: 0.0,
            latency_overhead_ms: 0,
            cc_at_k_delta: 0.0,
            icd_delta: 0.0,
        },
    };

    let regressions = find_regressions(&[r], &HashMap::new());
    assert_eq!(
        regressions.len(),
        1,
        "candidate 'zzz' must regress vs sorted-first baseline 'aaa'"
    );
    assert_eq!(
        regressions[0].profile_name, "zzz",
        "the non-baseline (sorted-later) profile is the regressing candidate"
    );
}
