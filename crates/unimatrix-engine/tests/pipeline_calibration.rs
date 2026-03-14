//! Confidence calibration and signal ablation tests.
//!
//! Validates that the confidence formula produces expected orderings
//! for standard scenarios, and that each signal contributes meaningfully.

use unimatrix_engine::confidence::{
    self, W_BASE, W_CORR, W_FRESH, W_HELP, W_TRUST, W_USAGE, base_score, compute_confidence,
    correction_score, freshness_score, helpfulness_score, rerank_score, trust_score, usage_score,
};
use unimatrix_engine::test_scenarios::*;

// ---------------------------------------------------------------------------
// T-CAL-01: Standard ranking ordering
// ---------------------------------------------------------------------------

#[test]
fn test_standard_ranking_ordering() {
    let scenario = standard_ranking();
    let records: Vec<_> = scenario
        .entries
        .iter()
        .enumerate()
        .map(|(i, p)| profile_to_entry_record(p, i as u64 + 1, scenario.now))
        .collect();

    let expected_ids: Vec<u64> = scenario
        .expected_ordering
        .iter()
        .map(|&idx| idx as u64 + 1)
        .collect();

    assert_confidence_ordering(&records, &expected_ids, scenario.now);
}

// ---------------------------------------------------------------------------
// T-CAL-02: Trust source ordering
// ---------------------------------------------------------------------------

#[test]
fn test_trust_source_ordering() {
    let scenario = trust_source_ordering();
    let records: Vec<_> = scenario
        .entries
        .iter()
        .enumerate()
        .map(|(i, p)| profile_to_entry_record(p, i as u64 + 1, scenario.now))
        .collect();

    let expected_ids: Vec<u64> = scenario
        .expected_ordering
        .iter()
        .map(|&idx| idx as u64 + 1)
        .collect();

    assert_confidence_ordering(&records, &expected_ids, scenario.now);
}

// ---------------------------------------------------------------------------
// T-CAL-03: Freshness dominance ordering
// ---------------------------------------------------------------------------

#[test]
fn test_freshness_dominance_ordering() {
    let scenario = freshness_dominance();
    let records: Vec<_> = scenario
        .entries
        .iter()
        .enumerate()
        .map(|(i, p)| profile_to_entry_record(p, i as u64 + 1, scenario.now))
        .collect();

    let expected_ids: Vec<u64> = scenario
        .expected_ordering
        .iter()
        .map(|&idx| idx as u64 + 1)
        .collect();

    assert_confidence_ordering(&records, &expected_ids, scenario.now);
}

// ---------------------------------------------------------------------------
// T-CAL-04: Weight sensitivity (+/-10% perturbation)
// ---------------------------------------------------------------------------

/// Manually recompute confidence with a perturbed weight.
fn confidence_with_adjusted_weight(
    entry: &unimatrix_core::EntryRecord,
    now: u64,
    weight_index: usize,
    delta: f64,
) -> f64 {
    let weights = [W_BASE, W_USAGE, W_FRESH, W_HELP, W_CORR, W_TRUST];
    let scores = [
        base_score(entry.status),
        usage_score(entry.access_count),
        freshness_score(entry.last_accessed_at, entry.created_at, now),
        helpfulness_score(entry.helpful_count, entry.unhelpful_count),
        correction_score(entry.correction_count),
        trust_score(&entry.trust_source),
    ];

    let mut adjusted = weights.to_vec();
    adjusted[weight_index] *= 1.0 + delta;

    adjusted
        .iter()
        .zip(scores.iter())
        .map(|(w, s)| w * s)
        .sum::<f64>()
        .clamp(0.0, 1.0)
}

#[test]
fn test_weight_sensitivity() {
    let scenario = standard_ranking();
    let records: Vec<_> = scenario
        .entries
        .iter()
        .enumerate()
        .map(|(i, p)| profile_to_entry_record(p, i as u64 + 1, scenario.now))
        .collect();

    // Original ranking
    let mut original_scored: Vec<(u64, f64)> = records
        .iter()
        .map(|e| (e.id, compute_confidence(e, scenario.now, 3.0, 3.0)))
        .collect();
    original_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let original_ranking: Vec<u64> = original_scored.iter().map(|(id, _)| *id).collect();

    // Perturb each weight by +10% and -10%
    for weight_idx in 0..6 {
        for &delta in &[0.1, -0.1] {
            let mut perturbed_scored: Vec<(u64, f64)> = records
                .iter()
                .map(|e| {
                    (
                        e.id,
                        confidence_with_adjusted_weight(e, scenario.now, weight_idx, delta),
                    )
                })
                .collect();
            perturbed_scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let perturbed_ranking: Vec<u64> = perturbed_scored.iter().map(|(id, _)| *id).collect();

            let tau = kendall_tau(&original_ranking, &perturbed_ranking);
            assert!(
                tau > 0.6,
                "weight[{weight_idx}] delta={delta}: tau={tau:.4}, expected > 0.6"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T-ABL-01 through T-ABL-06: Signal ablation
// ---------------------------------------------------------------------------

/// Create a pair of entries that differ only in one signal dimension.
/// Entry A maximizes the signal, Entry B minimizes it.
fn ablation_pair(
    signal: &str,
    now: u64,
) -> (unimatrix_core::EntryRecord, unimatrix_core::EntryRecord) {
    // Base: all signals at moderate level
    let base = EntryProfile {
        label: "ablation-base",
        status: Status::Active,
        access_count: 20,
        last_accessed_at: now - 7 * 24 * 3600,
        created_at: now - 30 * 24 * 3600,
        helpful_count: 5,
        unhelpful_count: 1,
        correction_count: 1,
        trust_source: "agent",
        category: "decision",
    };

    let (mut high, mut low) = (base.clone(), base);

    match signal {
        "base" => {
            high.status = Status::Active; // 0.5
            low.status = Status::Quarantined; // 0.1
        }
        "usage" => {
            high.access_count = 50; // ~1.0
            low.access_count = 0; // 0.0
        }
        "freshness" => {
            high.last_accessed_at = now - 60; // very fresh ~1.0
            low.last_accessed_at = now - 365 * 24 * 3600; // very stale ~0.0
        }
        "helpfulness" => {
            high.helpful_count = 100;
            high.unhelpful_count = 0; // high Wilson
            low.helpful_count = 0;
            low.unhelpful_count = 100; // low Wilson
        }
        "correction" => {
            high.correction_count = 1; // 0.8
            low.correction_count = 10; // 0.3
        }
        "trust" => {
            high.trust_source = "human"; // 1.0
            low.trust_source = "unknown"; // 0.3
        }
        _ => panic!("unknown signal: {signal}"),
    }

    let entry_high = profile_to_entry_record(&high, 1, now);
    let entry_low = profile_to_entry_record(&low, 2, now);
    (entry_high, entry_low)
}

use unimatrix_core::Status;

macro_rules! ablation_test {
    ($name:ident, $signal:literal, $test_id:literal) => {
        #[test]
        fn $name() {
            let now = CANONICAL_NOW;
            let (high, low) = ablation_pair($signal, now);
            let conf_high = compute_confidence(&high, now, 3.0, 3.0);
            let conf_low = compute_confidence(&low, now, 3.0, 3.0);
            assert!(
                conf_high > conf_low,
                "{}: high={conf_high:.6}, low={conf_low:.6} — expected high > low",
                $test_id,
            );
        }
    };
}

ablation_test!(test_signal_ablation_base, "base", "T-ABL-01");
ablation_test!(test_signal_ablation_usage, "usage", "T-ABL-02");
ablation_test!(test_signal_ablation_freshness, "freshness", "T-ABL-03");
ablation_test!(test_signal_ablation_helpfulness, "helpfulness", "T-ABL-04");
ablation_test!(test_signal_ablation_correction, "correction", "T-ABL-05");
ablation_test!(test_signal_ablation_trust, "trust", "T-ABL-06");

// ---------------------------------------------------------------------------
// T-CAL-05: Boundary entries
// ---------------------------------------------------------------------------

#[test]
fn test_boundary_all_zero() {
    let profile = EntryProfile {
        label: "all-zero",
        status: Status::Active,
        access_count: 0,
        last_accessed_at: 0,
        created_at: 0,
        helpful_count: 0,
        unhelpful_count: 0,
        correction_count: 0,
        trust_source: "",
        category: "decision",
    };
    let entry = profile_to_entry_record(&profile, 1, CANONICAL_NOW);
    let conf = compute_confidence(&entry, CANONICAL_NOW, 3.0, 3.0);
    assert!(
        (0.0..=1.0).contains(&conf),
        "all-zero confidence {conf} out of range"
    );
}

#[test]
fn test_boundary_all_max() {
    let profile = EntryProfile {
        label: "all-max",
        status: Status::Active,
        access_count: 1000,
        last_accessed_at: CANONICAL_NOW,
        created_at: CANONICAL_NOW,
        helpful_count: 100,
        unhelpful_count: 0,
        correction_count: 1,
        trust_source: "human",
        category: "decision",
    };
    let entry = profile_to_entry_record(&profile, 1, CANONICAL_NOW);
    let conf = compute_confidence(&entry, CANONICAL_NOW, 3.0, 3.0);
    assert!(
        (0.0..=1.0).contains(&conf),
        "all-max confidence {conf} out of range"
    );
    assert!(
        conf > 0.7,
        "all-max should have high confidence, got {conf}"
    );
}
