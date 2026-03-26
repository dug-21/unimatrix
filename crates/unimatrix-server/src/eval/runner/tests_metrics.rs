//! Metric function tests for eval runner (nan-007).
//!
//! Covers: P@K, MRR, Kendall tau, rank changes, ground truth resolution.
//! These tests call internal metric functions directly and do not require
//! database or filesystem access.

// ADR-003, C-10: kendall_tau from test_scenarios requires test-support feature
use unimatrix_engine::test_scenarios::kendall_tau;

use crate::eval::scenarios::{ScenarioBaseline, ScenarioContext, ScenarioRecord};

use super::metrics::{
    compute_mrr, compute_p_at_k, compute_rank_changes, compute_tau_safe, determine_ground_truth,
};
use super::output::ScoredEntry;

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
