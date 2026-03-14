//! Unit tests for the test_scenarios shared infrastructure.

use unimatrix_core::Status;
use unimatrix_engine::confidence;
use unimatrix_engine::test_scenarios::*;

// T-KT-01: Identical rankings -> tau = 1.0
#[test]
fn kendall_tau_identical() {
    let a = vec![1, 2, 3, 4, 5];
    let b = vec![1, 2, 3, 4, 5];
    assert!((kendall_tau(&a, &b) - 1.0).abs() < f64::EPSILON);
}

// T-KT-02: Reversed rankings -> tau = -1.0
#[test]
fn kendall_tau_reversed() {
    let a = vec![1, 2, 3, 4, 5];
    let b = vec![5, 4, 3, 2, 1];
    assert!((kendall_tau(&a, &b) - (-1.0)).abs() < f64::EPSILON);
}

// T-KT-03: Known partial correlation (C=8, D=2, tau = 0.6)
#[test]
fn kendall_tau_partial_correlation() {
    let a = vec![1, 2, 3, 4, 5];
    let b = vec![2, 1, 4, 3, 5];
    let tau = kendall_tau(&a, &b);
    assert!((tau - 0.6).abs() < f64::EPSILON, "expected 0.6, got {tau}");
}

// T-KT-04: Single element -> tau = 1.0
#[test]
fn kendall_tau_single_element() {
    let a = vec![42];
    let b = vec![42];
    assert!((kendall_tau(&a, &b) - 1.0).abs() < f64::EPSILON);
}

// T-KT-05: Two elements, both orderings
#[test]
fn kendall_tau_two_elements() {
    let a = vec![1, 2];
    let b_same = vec![1, 2];
    let b_rev = vec![2, 1];
    assert!((kendall_tau(&a, &b_same) - 1.0).abs() < f64::EPSILON);
    assert!((kendall_tau(&a, &b_rev) - (-1.0)).abs() < f64::EPSILON);
}

// T-PROF-01: Round-trip profile -> EntryRecord -> compute_confidence
#[test]
fn profile_round_trip_expert() {
    let profile = expert_human_fresh();
    let entry = profile_to_entry_record(&profile, 1, CANONICAL_NOW);

    assert_eq!(entry.status, Status::Active);
    assert_eq!(entry.access_count, 30);
    assert_eq!(entry.last_accessed_at, CANONICAL_NOW - 3600);
    assert_eq!(entry.created_at, CANONICAL_NOW - 7 * 24 * 3600);
    assert_eq!(entry.helpful_count, 10);
    assert_eq!(entry.unhelpful_count, 1);
    assert_eq!(entry.correction_count, 1);
    assert_eq!(entry.trust_source, "human");

    let conf = confidence::compute_confidence(&entry, CANONICAL_NOW, 3.0, 3.0);
    assert!(conf > 0.5, "expert should have high confidence, got {conf}");

    let b = confidence::base_score(entry.status);
    assert_eq!(b, 0.5);
    let t = confidence::trust_score(&entry.trust_source);
    assert_eq!(t, 1.0);
}

// T-PROF-02: All 5 standard profiles produce distinct confidence values
#[test]
fn all_profiles_distinct_confidence() {
    let profiles = vec![
        expert_human_fresh(),
        good_agent_entry(),
        auto_extracted_new(),
        stale_deprecated(),
        quarantined_bad(),
    ];

    let confidences: Vec<f64> = profiles
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let entry = profile_to_entry_record(p, i as u64 + 1, CANONICAL_NOW);
            confidence::compute_confidence(&entry, CANONICAL_NOW, 3.0, 3.0)
        })
        .collect();

    for i in 0..confidences.len() {
        for j in (i + 1)..confidences.len() {
            assert!(
                (confidences[i] - confidences[j]).abs() > 1e-6,
                "profiles {} and {} have same confidence: {:.6} vs {:.6}",
                profiles[i].label,
                profiles[j].label,
                confidences[i],
                confidences[j],
            );
        }
    }
}
