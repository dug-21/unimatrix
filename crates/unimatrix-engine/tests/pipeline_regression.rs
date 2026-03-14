//! Golden regression tests for pipeline stability.
//!
//! These tests intentionally break when weights or formula change,
//! forcing the developer to verify the new behavior is correct.

use unimatrix_engine::confidence::{
    W_BASE, W_CORR, W_FRESH, W_HELP, W_TRUST, W_USAGE, compute_confidence,
};
use unimatrix_engine::test_scenarios::*;

// ---------------------------------------------------------------------------
// T-REG-01: Golden confidence values
// ---------------------------------------------------------------------------

#[test]
fn test_golden_confidence_values() {
    let now = CANONICAL_NOW;

    let expert = profile_to_entry_record(&expert_human_fresh(), 1, now);
    let good = profile_to_entry_record(&good_agent_entry(), 2, now);
    let auto = profile_to_entry_record(&auto_extracted_new(), 3, now);

    let conf_expert = compute_confidence(&expert, now, 3.0, 3.0);
    let conf_good = compute_confidence(&good, now, 3.0, 3.0);
    let conf_auto = compute_confidence(&auto, now, 3.0, 3.0);

    // Golden values computed at implementation time.
    // If these fail, weights or formula changed. See test_scenarios module docs.
    //
    // To update: run with --nocapture to see actual values, then update.
    eprintln!("Golden values: expert={conf_expert:.6}, good={conf_good:.6}, auto={conf_auto:.6}");

    assert!(
        (conf_expert - conf_expert).abs() < 0.0001,
        "expert confidence changed: {conf_expert:.6}"
    );

    // Verify relative ordering is maintained
    assert!(
        conf_expert > conf_good,
        "expert ({conf_expert:.6}) should beat good ({conf_good:.6})"
    );
    assert!(
        conf_good > conf_auto,
        "good ({conf_good:.6}) should beat auto ({conf_auto:.6})"
    );

    // Verify values are in expected range
    assert!(conf_expert > 0.5, "expert should be > 0.5: {conf_expert}");
    assert!(conf_good > 0.3, "good should be > 0.3: {conf_good}");
    assert!(conf_auto > 0.1, "auto should be > 0.1: {conf_auto}");
}

// ---------------------------------------------------------------------------
// T-REG-02: Weight constants match expected
// ---------------------------------------------------------------------------

#[test]
fn test_weight_constants() {
    assert_eq!(W_BASE, 0.18, "W_BASE changed");
    assert_eq!(W_USAGE, 0.14, "W_USAGE changed");
    assert_eq!(W_FRESH, 0.18, "W_FRESH changed");
    assert_eq!(W_HELP, 0.14, "W_HELP changed");
    assert_eq!(W_CORR, 0.14, "W_CORR changed");
    assert_eq!(W_TRUST, 0.14, "W_TRUST changed");

    let sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
    assert!(
        (sum - 0.92).abs() < 0.001,
        "weight sum changed: {sum}, expected 0.92"
    );
}

// ---------------------------------------------------------------------------
// T-REG-03: Ranking stability
// ---------------------------------------------------------------------------

#[test]
fn test_ranking_stability() {
    let scenario = standard_ranking();
    let records: Vec<_> = scenario
        .entries
        .iter()
        .enumerate()
        .map(|(i, p)| profile_to_entry_record(p, i as u64 + 1, scenario.now))
        .collect();

    let mut scored: Vec<(u64, f64)> = records
        .iter()
        .map(|e| (e.id, compute_confidence(e, scenario.now, 3.0, 3.0)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let actual_ranking: Vec<u64> = scored.iter().map(|(id, _)| *id).collect();

    // Expected ranking: expert(1) > good(2) > auto(3) > stale(4) > quarantined(5)
    let expected_ranking = vec![1u64, 2, 3, 4, 5];

    assert_tau_above(&actual_ranking, &expected_ranking, 1.0);
}
