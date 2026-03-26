//! Metric function tests for eval runner (nan-007).
//!
//! Covers: P@K, MRR, Kendall tau, rank changes, ground truth resolution.
//! These tests call internal metric functions directly and do not require
//! database or filesystem access.

// ADR-003, C-10: kendall_tau from test_scenarios requires test-support feature
use unimatrix_engine::test_scenarios::kendall_tau;

use crate::eval::scenarios::{ScenarioBaseline, ScenarioContext, ScenarioRecord};

use std::collections::HashMap;

use super::metrics::{
    compute_cc_at_k, compute_comparison, compute_icd, compute_mrr, compute_p_at_k,
    compute_rank_changes, compute_tau_safe, determine_ground_truth,
};
use super::output::{ProfileResult, ScoredEntry};

// -----------------------------------------------------------------------
// Helper builders (metrics tests)
// -----------------------------------------------------------------------

fn make_scenario(
    id: &str,
    baseline_ids: Option<Vec<u64>>,
    expected: Option<Vec<u64>>,
) -> ScenarioRecord {
    ScenarioRecord {
        id: id.to_string(),
        query: "test query".to_string(),
        context: ScenarioContext {
            agent_id: "test-agent".to_string(),
            feature_cycle: "nan-007".to_string(),
            session_id: "sess-1".to_string(),
            retrieval_mode: "flexible".to_string(),
        },
        baseline: baseline_ids.map(|ids| {
            let scores = vec![0.9f32; ids.len()];
            ScenarioBaseline {
                entry_ids: ids,
                scores,
            }
        }),
        source: "mcp".to_string(),
        expected,
    }
}

fn make_entries(ids: &[u64]) -> Vec<ScoredEntry> {
    ids.iter()
        .map(|&id| ScoredEntry {
            id,
            title: format!("Entry {id}"),
            category: String::new(),
            final_score: 0.9,
            similarity: 0.85,
            confidence: 0.7,
            status: "Active".to_string(),
            nli_rerank_delta: None,
        })
        .collect()
}

// -----------------------------------------------------------------------
// R-03: kendall_tau reachable from eval module (compile guard)
// -----------------------------------------------------------------------

#[test]
fn test_kendall_tau_reachable_from_eval_runner() {
    // Direct call to unimatrix_engine::test_scenarios::kendall_tau.
    // Removing the test-support feature from Cargo.toml causes a compile error here.
    let tau = kendall_tau(&[1, 2, 3], &[1, 3, 2]);
    // [1,2,3] vs [1,3,2]: pairs (1,2)→OK, (1,3)→OK, (2,3)→inverted
    // concordant=2, discordant=1, total_pairs=3 → tau=(2-1)/3 = 0.333...
    assert!((-1.0..=1.0).contains(&tau), "tau must be in [-1.0, 1.0]");
    let expected = 1.0 / 3.0;
    assert!(
        (tau - expected).abs() < 1e-9,
        "expected tau ≈ 0.333, got {tau}"
    );
}

#[test]
fn test_kendall_tau_single_element_no_panic() {
    let tau = kendall_tau(&[5], &[5]);
    assert!(
        !tau.is_nan(),
        "tau must not be NaN for single-element lists"
    );
    assert_eq!(tau, 1.0, "single-element list: tau must be 1.0");
}

// -----------------------------------------------------------------------
// AC-07, R-08: P@K dual-mode
// -----------------------------------------------------------------------

#[test]
fn test_pak_soft_ground_truth_query_log_scenario() {
    let ground_truth = vec![10u64, 20, 30];
    let entries = make_entries(&[10, 20, 50]);
    let p = compute_p_at_k(&entries, &ground_truth, 3);
    let expected = 2.0 / 3.0;
    assert!(
        (p - expected).abs() < 1e-9,
        "P@3 = {p}, expected ≈ {expected}"
    );
}

#[test]
fn test_pak_hard_labels_hand_authored_scenario() {
    let ground_truth = vec![10u64, 20];
    let entries = make_entries(&[10, 30, 20]);
    let p = compute_p_at_k(&entries, &ground_truth, 3);
    let expected = 2.0 / 3.0;
    assert!(
        (p - expected).abs() < 1e-9,
        "P@3 = {p}, expected ≈ {expected}"
    );
}

#[test]
fn test_pak_hard_labels_not_confused_with_baseline() {
    let record = make_scenario("s1", Some(vec![20, 30]), Some(vec![10]));
    let gt = determine_ground_truth(&record);
    assert_eq!(gt, vec![10u64], "must use expected, not baseline");

    let entries = make_entries(&[10, 20, 30]);
    let p = compute_p_at_k(&entries, &gt, 3);
    let expected = 1.0 / 3.0;
    assert!(
        (p - expected).abs() < 1e-9,
        "P@3 = {p}, expected ≈ {expected} (using hard labels, not baseline)"
    );
}

#[test]
fn test_pak_at_k1_known_result() {
    let ground_truth = vec![99u64];
    let entries = make_entries(&[99, 1, 2]);
    let p = compute_p_at_k(&entries, &ground_truth, 1);
    assert_eq!(p, 1.0, "P@1 must be 1.0 when first result is in GT");
}

#[test]
fn test_pak_empty_ground_truth_returns_zero() {
    let entries = make_entries(&[1, 2, 3]);
    let p = compute_p_at_k(&entries, &[], 3);
    assert_eq!(p, 0.0, "P@K with no GT must be 0.0");
}

// -----------------------------------------------------------------------
// MRR
// -----------------------------------------------------------------------

#[test]
fn test_mrr_known_result() {
    let ground_truth = vec![10u64, 20];
    let entries = make_entries(&[5, 10, 20]);
    let mrr = compute_mrr(&entries, &ground_truth);
    assert!((mrr - 0.5).abs() < 1e-9, "MRR = {mrr}, expected 0.5");
}

#[test]
fn test_mrr_empty_ground_truth_returns_zero() {
    let entries = make_entries(&[1, 2, 3]);
    let mrr = compute_mrr(&entries, &[]);
    assert_eq!(mrr, 0.0, "MRR with no GT must be 0.0");
}

// -----------------------------------------------------------------------
// determine_ground_truth
// -----------------------------------------------------------------------

#[test]
fn test_determine_ground_truth_prefers_expected() {
    let record = make_scenario("s1", Some(vec![20, 30]), Some(vec![10, 11]));
    let gt = determine_ground_truth(&record);
    assert_eq!(gt, vec![10u64, 11]);
}

#[test]
fn test_determine_ground_truth_falls_back_to_baseline() {
    let record = make_scenario("s1", Some(vec![20, 30]), None);
    let gt = determine_ground_truth(&record);
    assert_eq!(gt, vec![20u64, 30]);
}

#[test]
fn test_determine_ground_truth_empty_when_neither() {
    let record = make_scenario("s1", None, None);
    let gt = determine_ground_truth(&record);
    assert!(gt.is_empty(), "no GT available → empty vec");
}

// -----------------------------------------------------------------------
// compute_rank_changes
// -----------------------------------------------------------------------

#[test]
fn test_rank_changes_one_moved_entry() {
    let baseline = vec![1u64, 2, 3];
    let candidate = vec![1u64, 3, 2];
    let changes = compute_rank_changes(&baseline, &candidate);

    assert_eq!(changes.len(), 2);
    let ids: std::collections::HashSet<u64> = changes.iter().map(|c| c.entry_id).collect();
    assert!(ids.contains(&2), "entry 2 must be in changes");
    assert!(ids.contains(&3), "entry 3 must be in changes");

    let b_change = changes.iter().find(|c| c.entry_id == 2).unwrap();
    assert_eq!(b_change.from_rank, 2);
    assert_eq!(b_change.to_rank, 3);

    let c_change = changes.iter().find(|c| c.entry_id == 3).unwrap();
    assert_eq!(c_change.from_rank, 3);
    assert_eq!(c_change.to_rank, 2);
}

#[test]
fn test_rank_changes_entry_dropped_from_candidate() {
    let baseline = vec![1u64, 2];
    let candidate = vec![1u64];
    let changes = compute_rank_changes(&baseline, &candidate);
    assert_eq!(changes.len(), 1);
    let change = &changes[0];
    assert_eq!(change.entry_id, 2);
    assert_eq!(change.from_rank, 2);
    assert_eq!(change.to_rank, 2, "to_rank = candidate_len + 1 = 1 + 1 = 2");
}

#[test]
fn test_rank_changes_entry_new_in_candidate() {
    let baseline = vec![1u64];
    let candidate = vec![1u64, 2];
    let changes = compute_rank_changes(&baseline, &candidate);
    assert_eq!(changes.len(), 1);
    let change = &changes[0];
    assert_eq!(change.entry_id, 2);
    assert_eq!(
        change.from_rank, 2,
        "from_rank = baseline_len + 1 = 1 + 1 = 2"
    );
    assert_eq!(change.to_rank, 2);
}

#[test]
fn test_rank_changes_no_changes_identical_lists() {
    let ids = vec![1u64, 2, 3];
    let changes = compute_rank_changes(&ids, &ids);
    assert!(changes.is_empty(), "identical lists → no rank changes");
}

// -----------------------------------------------------------------------
// compute_tau_safe
// -----------------------------------------------------------------------

#[test]
fn test_tau_safe_empty_lists_returns_zero() {
    let tau = compute_tau_safe(&[], &[]);
    assert_eq!(tau, 0.0);
}

#[test]
fn test_tau_safe_identical_lists() {
    let tau = compute_tau_safe(&[1, 2, 3], &[1, 2, 3]);
    assert!(
        (tau - 1.0).abs() < 1e-9,
        "identical lists → tau = 1.0, got {tau}"
    );
}

#[test]
fn test_tau_safe_disjoint_lists_returns_zero() {
    let tau = compute_tau_safe(&[1, 2, 3], &[4, 5, 6]);
    assert_eq!(tau, 0.0, "disjoint lists → tau = 0.0");
}

// -----------------------------------------------------------------------
// Metric reproducibility (NFR-05)
// -----------------------------------------------------------------------

#[test]
fn test_eval_run_metric_reproducibility() {
    let ground_truth = vec![10u64, 20, 30];
    let entries = make_entries(&[10, 20, 50]);

    let p1 = compute_p_at_k(&entries, &ground_truth, 3);
    let p2 = compute_p_at_k(&entries, &ground_truth, 3);
    assert_eq!(p1, p2, "P@K must be deterministic");

    let m1 = compute_mrr(&entries, &ground_truth);
    let m2 = compute_mrr(&entries, &ground_truth);
    assert_eq!(m1, m2, "MRR must be deterministic");
}

// -----------------------------------------------------------------------
// Helper: build ScoredEntry slices with explicit categories (nan-008)
// -----------------------------------------------------------------------

fn make_entries_with_categories(pairs: &[(u64, &str)]) -> Vec<ScoredEntry> {
    pairs
        .iter()
        .map(|&(id, cat)| ScoredEntry {
            id,
            title: format!("Entry {id}"),
            category: cat.to_string(),
            final_score: 0.9,
            similarity: 0.85,
            confidence: 0.7,
            status: "Active".to_string(),
            nli_rerank_delta: None,
        })
        .collect()
}

fn make_profile_result_for_comparison(cc_at_k: f64, icd: f64) -> ProfileResult {
    ProfileResult {
        entries: make_entries_with_categories(&[(1, "decision")]),
        latency_ms: 10,
        p_at_k: 0.5,
        mrr: 0.5,
        cc_at_k,
        icd,
    }
}

// -----------------------------------------------------------------------
// AC-10: CC@k boundary tests
// -----------------------------------------------------------------------

#[test]
fn test_cc_at_k_all_categories_present() {
    let entries = make_entries_with_categories(&[(1, "a"), (2, "b"), (3, "c")]);
    let configured = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let result = compute_cc_at_k(&entries, &configured);
    assert_eq!(result, 1.0, "all three categories covered → CC@k = 1.0");
}

#[test]
fn test_cc_at_k_one_category_present() {
    let entries = make_entries_with_categories(&[(1, "a"), (2, "a"), (3, "a")]);
    let configured = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let result = compute_cc_at_k(&entries, &configured);
    let expected = 1.0 / 3.0;
    assert!(
        (result - expected).abs() < 1e-9,
        "only 1 of 3 categories covered → CC@k ≈ 0.333, got {result}"
    );
}

#[test]
fn test_icd_maximum_entropy() {
    // Four entries, each a different category → uniform distribution → entropy = ln(4)
    let entries = make_entries_with_categories(&[(1, "a"), (2, "b"), (3, "c"), (4, "d")]);
    let result = compute_icd(&entries);
    let expected = f64::ln(4.0);
    assert!(
        (result - expected).abs() < 1e-9,
        "uniform 4-category distribution → ICD = ln(4) ≈ {expected}, got {result}"
    );
}

#[test]
fn test_icd_single_category() {
    let entries = make_entries_with_categories(&[(1, "a"), (2, "a"), (3, "a")]);
    let result = compute_icd(&entries);
    assert_eq!(result, 0.0, "single category → ICD = 0.0");
}

// -----------------------------------------------------------------------
// AC-10: guard tests
// -----------------------------------------------------------------------

#[test]
fn test_cc_at_k_empty_configured_categories_returns_zero() {
    let entries = make_entries_with_categories(&[(1, "a"), (2, "b")]);
    let result = compute_cc_at_k(&entries, &[]);
    assert_eq!(result, 0.0, "empty configured_categories → 0.0, no panic");
}

#[test]
fn test_icd_nan_guard() {
    // Mixed distribution: p(a)=2/3, p(b)=1/3 — exercises non-uniform path.
    let entries = make_entries_with_categories(&[(1, "a"), (2, "a"), (3, "b")]);
    let result = compute_icd(&entries);
    assert!(!result.is_nan(), "ICD must not be NaN");
    assert!(!result.is_infinite(), "ICD must not be infinite");
    assert!(result > 0.0, "mixed categories → ICD > 0.0");
}

// -----------------------------------------------------------------------
// AC-10: intersection semantics guard (WARN-2 resolution)
// -----------------------------------------------------------------------

#[test]
fn test_cc_at_k_intersection_semantics_category_outside_configured_not_counted() {
    // "legacy-cat" is in entries but NOT in configured_categories.
    // Intersection semantics: only "decision" counts → 1/2.
    let entries = make_entries_with_categories(&[(1, "legacy-cat"), (2, "decision")]);
    let configured = vec!["decision".to_string(), "convention".to_string()];
    let result = compute_cc_at_k(&entries, &configured);
    let expected = 1.0 / 2.0;
    assert!(
        (result - expected).abs() < 1e-9,
        "intersection semantics: only configured categories count; expected {expected}, got {result}"
    );
    assert!(result <= 1.0, "CC@k must never exceed 1.0, got {result}");
}

// -----------------------------------------------------------------------
// Additional edge cases from test plan
// -----------------------------------------------------------------------

#[test]
fn test_cc_at_k_entries_empty_configured_non_empty() {
    let result = compute_cc_at_k(&[], &["a".to_string(), "b".to_string(), "c".to_string()]);
    assert_eq!(result, 0.0, "no entries → zero coverage");
}

#[test]
fn test_icd_empty_entries_returns_zero() {
    let result = compute_icd(&[]);
    assert_eq!(result, 0.0, "empty entries → ICD = 0.0");
}

#[test]
fn test_icd_two_entries_one_category_each() {
    let entries = make_entries_with_categories(&[(1, "a"), (2, "b")]);
    let result = compute_icd(&entries);
    let expected = f64::ln(2.0);
    assert!(
        (result - expected).abs() < 1e-9,
        "two equal-probability categories → ICD = ln(2) ≈ {expected}, got {result}"
    );
}

// -----------------------------------------------------------------------
// Delta sign tests for compute_comparison (R-10)
// -----------------------------------------------------------------------

#[test]
fn test_compute_comparison_delta_positive() {
    let baseline = make_profile_result_for_comparison(0.4, 0.6);
    let candidate = make_profile_result_for_comparison(0.7, 1.1);

    let mut profiles = HashMap::new();
    profiles.insert("baseline".to_string(), baseline);
    profiles.insert("candidate".to_string(), candidate);

    let result = compute_comparison(&profiles, "baseline").expect("comparison failed");

    assert!(
        result.cc_at_k_delta > 0.0,
        "cc_at_k_delta must be positive, got {}",
        result.cc_at_k_delta
    );
    assert!(
        result.icd_delta > 0.0,
        "icd_delta must be positive, got {}",
        result.icd_delta
    );
    assert!(
        (result.cc_at_k_delta - 0.3).abs() < 1e-9,
        "cc_at_k_delta = 0.7 - 0.4 = 0.3, got {}",
        result.cc_at_k_delta
    );
    assert!(
        (result.icd_delta - 0.5).abs() < 1e-9,
        "icd_delta = 1.1 - 0.6 = 0.5, got {}",
        result.icd_delta
    );
}

#[test]
fn test_compute_comparison_delta_negative() {
    let baseline = make_profile_result_for_comparison(0.8, 1.2);
    let candidate = make_profile_result_for_comparison(0.5, 0.9);

    let mut profiles = HashMap::new();
    profiles.insert("baseline".to_string(), baseline);
    profiles.insert("candidate".to_string(), candidate);

    let result = compute_comparison(&profiles, "baseline").expect("comparison failed");

    assert!(
        result.cc_at_k_delta < 0.0,
        "cc_at_k_delta must be negative, got {}",
        result.cc_at_k_delta
    );
    assert!(
        result.icd_delta < 0.0,
        "icd_delta must be negative, got {}",
        result.icd_delta
    );
}

// -----------------------------------------------------------------------
// Determinism (NFR reproducibility extension for nan-008 metrics)
// -----------------------------------------------------------------------

#[test]
fn test_cc_at_k_deterministic() {
    let entries = make_entries_with_categories(&[(1, "a"), (2, "b"), (3, "a")]);
    let configured = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let r1 = compute_cc_at_k(&entries, &configured);
    let r2 = compute_cc_at_k(&entries, &configured);
    assert_eq!(r1, r2, "CC@k must be deterministic");
}

#[test]
fn test_icd_deterministic() {
    let entries = make_entries_with_categories(&[(1, "a"), (2, "b"), (3, "c")]);
    let r1 = compute_icd(&entries);
    let r2 = compute_icd(&entries);
    assert_eq!(r1, r2, "ICD must be deterministic");
}
