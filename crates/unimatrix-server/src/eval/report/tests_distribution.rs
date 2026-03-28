//! Distribution Analysis (CC@k, ICD) unit tests for eval/report (nan-008).
//!
//! Unit tests for `compute_aggregate_stats` and `compute_cc_at_k_scenario_rows`.
//! Pipeline (run_report) tests are in tests_distribution_pipeline.rs.
//!
//! Covers: R-11 (CC@k and ICD aggregation correctness), R-12 (scenario row sort order).

use std::collections::HashMap;

use super::aggregate::{compute_aggregate_stats, compute_cc_at_k_scenario_rows};
use super::{ComparisonMetrics, ProfileResult, ScenarioResult, default_comparison};

/// Build a two-profile ScenarioResult with explicit CC@k and ICD values on each
/// profile and the corresponding deltas on the comparison object.
#[allow(clippy::too_many_arguments)]
fn make_scenario_result_with_metrics(
    id: &str,
    query: &str,
    baseline_p: f64,
    baseline_mrr: f64,
    baseline_cc: f64,
    baseline_icd: f64,
    candidate_p: f64,
    candidate_mrr: f64,
    candidate_cc: f64,
    candidate_icd: f64,
) -> ScenarioResult {
    let mut profiles = HashMap::new();
    profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 50,
            p_at_k: baseline_p,
            mrr: baseline_mrr,
            cc_at_k: baseline_cc,
            icd: baseline_icd,
        },
    );
    profiles.insert(
        "candidate".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 60,
            p_at_k: candidate_p,
            mrr: candidate_mrr,
            cc_at_k: candidate_cc,
            icd: candidate_icd,
        },
    );
    ScenarioResult {
        scenario_id: id.to_string(),
        query: query.to_string(),
        profiles,
        phase: None,
        comparison: ComparisonMetrics {
            kendall_tau: 0.8,
            rank_changes: Vec::new(),
            mrr_delta: candidate_mrr - baseline_mrr,
            p_at_k_delta: candidate_p - baseline_p,
            latency_overhead_ms: 10,
            cc_at_k_delta: candidate_cc - baseline_cc,
            icd_delta: candidate_icd - baseline_icd,
        },
    }
}

// ---------------------------------------------------------------------------
// nan-008: test_aggregate_stats_cc_at_k_mean (R-11 guard)
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_stats_cc_at_k_mean() {
    let r1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.6, 0.8);
    let r2 = make_scenario_result_with_metrics("s2", "q2", 0.6, 0.5, 0.6, 0.7, 0.8, 0.7, 0.8, 1.0);
    let r3 = make_scenario_result_with_metrics("s3", "q3", 0.6, 0.5, 0.2, 0.3, 0.8, 0.7, 0.4, 0.6);

    let stats = compute_aggregate_stats(&[r1, r2, r3]);

    let baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap();
    // baseline mean_cc_at_k = (0.4 + 0.6 + 0.2) / 3 = 0.4
    assert!(
        (baseline.mean_cc_at_k - 0.4).abs() < 1e-9,
        "baseline mean_cc_at_k expected 0.4, got {}",
        baseline.mean_cc_at_k
    );

    let candidate = stats
        .iter()
        .find(|s| s.profile_name == "candidate")
        .unwrap();
    // candidate mean_cc_at_k = (0.6 + 0.8 + 0.4) / 3 ≈ 0.6
    assert!(
        (candidate.mean_cc_at_k - 0.6).abs() < 1e-9,
        "candidate mean_cc_at_k expected 0.6, got {}",
        candidate.mean_cc_at_k
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_aggregate_stats_icd_mean (R-11 symmetric)
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_stats_icd_mean() {
    let r1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.6, 0.8);
    let r2 = make_scenario_result_with_metrics("s2", "q2", 0.6, 0.5, 0.6, 0.7, 0.8, 0.7, 0.8, 1.0);
    let r3 = make_scenario_result_with_metrics("s3", "q3", 0.6, 0.5, 0.2, 0.3, 0.8, 0.7, 0.4, 0.6);

    let stats = compute_aggregate_stats(&[r1, r2, r3]);

    let baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap();
    // baseline mean_icd = (0.5 + 0.7 + 0.3) / 3 ≈ 0.5
    assert!(
        (baseline.mean_icd - 0.5).abs() < 1e-9,
        "baseline mean_icd expected 0.5, got {}",
        baseline.mean_icd
    );

    let candidate = stats
        .iter()
        .find(|s| s.profile_name == "candidate")
        .unwrap();
    // candidate mean_icd = (0.8 + 1.0 + 0.6) / 3 ≈ 0.8
    assert!(
        (candidate.mean_icd - 0.8).abs() < 1e-9,
        "candidate mean_icd expected 0.8, got {}",
        candidate.mean_icd
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_aggregate_stats_cc_at_k_delta_mean (R-11 for delta)
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_stats_cc_at_k_delta_mean() {
    // cc_at_k_delta values: 0.2, 0.4, 0.0 (candidate_cc - baseline_cc).
    let r1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.6, 0.7, 0.6, 0.8);
    // cc_at_k_delta = 0.6 - 0.4 = 0.2
    let r2 = make_scenario_result_with_metrics("s2", "q2", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.8, 0.8);
    // cc_at_k_delta = 0.8 - 0.4 = 0.4
    let r3 = make_scenario_result_with_metrics("s3", "q3", 0.6, 0.5, 0.5, 0.5, 0.8, 0.7, 0.5, 0.8);
    // cc_at_k_delta = 0.5 - 0.5 = 0.0

    let stats = compute_aggregate_stats(&[r1, r2, r3]);

    let candidate = stats
        .iter()
        .find(|s| s.profile_name == "candidate")
        .unwrap();
    let expected = (0.2 + 0.4 + 0.0) / 3.0;
    assert!(
        (candidate.cc_at_k_delta - expected).abs() < 1e-9,
        "candidate cc_at_k_delta expected {expected}, got {}",
        candidate.cc_at_k_delta
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_aggregate_stats_baseline_has_zero_cc_at_k_delta
// ---------------------------------------------------------------------------

#[test]
fn test_aggregate_stats_baseline_has_zero_cc_at_k_delta() {
    let r1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.7, 0.6, 0.7, 0.8);
    let r2 = make_scenario_result_with_metrics("s2", "q2", 0.5, 0.4, 0.3, 0.4, 0.6, 0.5, 0.6, 0.7);

    let stats = compute_aggregate_stats(&[r1, r2]);

    let baseline = stats.iter().find(|s| s.profile_name == "baseline").unwrap();
    assert_eq!(
        baseline.cc_at_k_delta, 0.0,
        "baseline cc_at_k_delta must be 0.0"
    );
    assert_eq!(baseline.icd_delta, 0.0, "baseline icd_delta must be 0.0");
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_sort_order (R-12 guard)
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_sort_order() {
    // Deltas: s1=0.1, s2=-0.3, s3=0.5, s4=-0.1, s5=0.2
    // Expected descending order: s3(0.5), s5(0.2), s1(0.1), s4(-0.1), s2(-0.3)
    let s1 = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.7, 0.6, 0.5, 0.7);
    // cc_at_k_delta = 0.1
    let s2 = make_scenario_result_with_metrics("s2", "q2", 0.6, 0.5, 0.7, 0.5, 0.7, 0.6, 0.4, 0.7);
    // cc_at_k_delta = -0.3
    let s3 = make_scenario_result_with_metrics("s3", "q3", 0.6, 0.5, 0.2, 0.5, 0.7, 0.6, 0.7, 0.7);
    // cc_at_k_delta = 0.5
    let s4 = make_scenario_result_with_metrics("s4", "q4", 0.6, 0.5, 0.6, 0.5, 0.7, 0.6, 0.5, 0.7);
    // cc_at_k_delta = -0.1
    let s5 = make_scenario_result_with_metrics("s5", "q5", 0.6, 0.5, 0.3, 0.5, 0.7, 0.6, 0.5, 0.7);
    // cc_at_k_delta = 0.2

    let rows = compute_cc_at_k_scenario_rows(&[s1, s2, s3, s4, s5]);

    assert_eq!(rows.len(), 5, "expected 5 rows");
    assert_eq!(rows[0].scenario_id, "s3", "s3 (delta=0.5) must be first");
    assert_eq!(rows[1].scenario_id, "s5", "s5 (delta=0.2) must be second");
    assert_eq!(rows[2].scenario_id, "s1", "s1 (delta=0.1) must be third");
    assert_eq!(rows[3].scenario_id, "s4", "s4 (delta=-0.1) must be fourth");
    assert_eq!(rows[4].scenario_id, "s2", "s2 (delta=-0.3) must be last");
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_single_profile_returns_empty
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_single_profile_returns_empty() {
    // Single-profile result: no candidate, so no comparison rows possible.
    let mut r = ScenarioResult {
        scenario_id: "s1".to_string(),
        query: "q".to_string(),
        profiles: HashMap::new(),
        phase: None,
        comparison: default_comparison(),
    };
    r.profiles.insert(
        "baseline".to_string(),
        ProfileResult {
            entries: Vec::new(),
            latency_ms: 50,
            p_at_k: 0.6,
            mrr: 0.5,
            cc_at_k: 0.4,
            icd: 0.5,
        },
    );

    let rows = compute_cc_at_k_scenario_rows(&[r]);
    assert!(
        rows.is_empty(),
        "single-profile result must produce no rows"
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_uses_comparison_delta
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_uses_comparison_delta() {
    // comparison.cc_at_k_delta = 0.3 (stored), but candidate(0.7) - baseline(0.4) = 0.3.
    // The row must use the stored comparison delta, not a recomputed one.
    // We verify this by setting the stored delta to a slightly different value
    // from the naive subtraction to confirm the stored value wins.
    let mut r =
        make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.7, 0.8);
    // Override comparison.cc_at_k_delta to a distinct stored value (0.3 instead of 0.3).
    // Actually use 0.30000001 to make the stored vs. recomputed distinction explicit.
    r.comparison.cc_at_k_delta = 0.30000001_f64;

    let rows = compute_cc_at_k_scenario_rows(&[r]);
    assert_eq!(rows.len(), 1);
    assert!(
        (rows[0].cc_at_k_delta - 0.30000001_f64).abs() < 1e-12,
        "row must use stored comparison delta, got {}",
        rows[0].cc_at_k_delta
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_single_scenario
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_single_scenario() {
    let r = make_scenario_result_with_metrics("s1", "q1", 0.6, 0.5, 0.4, 0.5, 0.8, 0.7, 0.7, 0.8);
    // cc_at_k_delta = 0.7 - 0.4 = 0.3

    let rows = compute_cc_at_k_scenario_rows(&[r]);
    assert_eq!(
        rows.len(),
        1,
        "single two-profile scenario must produce 1 row"
    );
    assert!(
        (rows[0].cc_at_k_delta - 0.3).abs() < 1e-9,
        "cc_at_k_delta expected 0.3, got {}",
        rows[0].cc_at_k_delta
    );
}

// ---------------------------------------------------------------------------
// nan-008: test_cc_at_k_scenario_rows_empty
// ---------------------------------------------------------------------------

#[test]
fn test_cc_at_k_scenario_rows_empty() {
    let rows = compute_cc_at_k_scenario_rows(&[]);
    assert!(rows.is_empty(), "empty input must produce empty rows");
}

// ---------------------------------------------------------------------------
// GH-407: test_cc_at_k_scenario_rows_unicode_query_no_panic
// ---------------------------------------------------------------------------

/// Verify that compute_cc_at_k_scenario_rows does not panic when the query
/// contains multi-byte UTF-8 characters (CJK, 3 bytes each) whose total byte
/// length exceeds the 60-char truncation limit, and that the resulting query
/// field is truncated at a char boundary with the ellipsis appended.
#[test]
fn test_cc_at_k_scenario_rows_unicode_query_no_panic() {
    // "あ" is U+3042, encoded as 3 bytes in UTF-8.
    // 25 repetitions = 25 chars, 75 bytes — exceeds the 60-byte limit that
    // previously caused a panic.
    let long_unicode_query: String = "あ".repeat(25);
    assert_eq!(long_unicode_query.len(), 75, "precondition: 75 bytes");
    assert_eq!(
        long_unicode_query.chars().count(),
        25,
        "precondition: 25 chars"
    );

    let r = make_scenario_result_with_metrics(
        "unicode-1",
        &long_unicode_query,
        0.5,
        0.5,
        0.5,
        0.5,
        0.7,
        0.7,
        0.7,
        0.7,
    );

    // Must not panic.
    let rows = compute_cc_at_k_scenario_rows(&[r]);

    assert_eq!(rows.len(), 1, "expected 1 row");

    // 25 chars is less than 60, so no truncation or ellipsis should occur.
    assert_eq!(
        rows[0].query, long_unicode_query,
        "query shorter than 60 chars must be returned unchanged"
    );

    // Now test with a query that truly exceeds 60 chars (70 × "あ" = 70 chars, 210 bytes).
    let very_long_query: String = "あ".repeat(70);
    assert_eq!(
        very_long_query.chars().count(),
        70,
        "precondition: 70 chars"
    );

    let r2 = make_scenario_result_with_metrics(
        "unicode-2",
        &very_long_query,
        0.5,
        0.5,
        0.5,
        0.5,
        0.7,
        0.7,
        0.7,
        0.7,
    );

    // Must not panic.
    let rows2 = compute_cc_at_k_scenario_rows(&[r2]);

    assert_eq!(rows2.len(), 1, "expected 1 row for long unicode query");

    // The query must end with the ellipsis character.
    assert!(
        rows2[0].query.ends_with('…'),
        "truncated query must end with ellipsis, got: {:?}",
        rows2[0].query
    );

    // The prefix before the ellipsis must be exactly 60 "あ" characters.
    let expected_prefix: String = "あ".repeat(60);
    let expected = format!("{}…", expected_prefix);
    assert_eq!(
        rows2[0].query, expected,
        "truncated query must be 60 chars + ellipsis"
    );
}
