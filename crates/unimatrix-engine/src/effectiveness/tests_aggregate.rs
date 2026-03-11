//! Tests for build_calibration_buckets, aggregate_by_source, and build_report.

use super::*;

// -- Test helpers --

fn make_entry(
    entry_id: u64,
    trust_source: &str,
    category: EffectivenessCategory,
    injection_count: u32,
    success_rate: f64,
) -> EntryEffectiveness {
    EntryEffectiveness {
        entry_id,
        title: format!("entry-{entry_id}"),
        topic: "test-topic".to_string(),
        trust_source: trust_source.to_string(),
        category,
        injection_count,
        success_rate,
        helpfulness_ratio: 0.0,
    }
}

fn empty_window() -> DataWindow {
    DataWindow {
        session_count: 0,
        earliest_session_at: None,
        latest_session_at: None,
    }
}

// -- build_calibration_buckets tests --

#[test]
fn test_calibration_confidence_zero_in_first_bucket() {
    // E-07
    let buckets = build_calibration_buckets(&[(0.0, true)]);
    assert_eq!(buckets[0].entry_count, 1);
    assert!((buckets[0].actual_success_rate - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_calibration_confidence_0_1_in_second_bucket() {
    // E-08
    let buckets = build_calibration_buckets(&[(0.1, false)]);
    assert_eq!(buckets[1].entry_count, 1);
    assert!((buckets[1].actual_success_rate).abs() < f64::EPSILON);
}

#[test]
fn test_calibration_confidence_0_9_in_last_bucket() {
    // E-09
    let buckets = build_calibration_buckets(&[(0.9, true)]);
    assert_eq!(buckets[9].entry_count, 1);
}

#[test]
fn test_calibration_confidence_1_0_in_last_bucket() {
    // E-10
    let buckets = build_calibration_buckets(&[(1.0, true)]);
    assert_eq!(buckets[9].entry_count, 1);
}

#[test]
fn test_calibration_confidence_just_below_0_1_in_first_bucket() {
    // E-11
    let buckets = build_calibration_buckets(&[(0.09999999, false)]);
    assert_eq!(buckets[0].entry_count, 1);
}

#[test]
fn test_calibration_confidence_0_5_in_sixth_bucket() {
    // E-12
    let buckets = build_calibration_buckets(&[(0.5, true)]);
    assert_eq!(buckets[5].entry_count, 1);
}

#[test]
fn test_calibration_empty_data_produces_10_empty_buckets() {
    // E-13
    let buckets = build_calibration_buckets(&[]);
    assert_eq!(buckets.len(), 10);
    for bucket in &buckets {
        assert_eq!(bucket.entry_count, 0);
        assert!((bucket.actual_success_rate).abs() < f64::EPSILON);
    }
}

#[test]
fn test_calibration_negative_confidence_clamped_to_first() {
    let buckets = build_calibration_buckets(&[(-0.5, true)]);
    assert_eq!(buckets[0].entry_count, 1);
}

#[test]
fn test_calibration_above_1_clamped_to_last() {
    let buckets = build_calibration_buckets(&[(1.5, false)]);
    assert_eq!(buckets[9].entry_count, 1);
}

// -- aggregate_by_source tests --

#[test]
fn test_aggregate_zero_injection_source_utility_zero() {
    // E-22: all unmatched (zero injections) => aggregate_utility = 0.0
    let entries = vec![
        make_entry(1, "human", EffectivenessCategory::Unmatched, 0, 0.0),
        make_entry(2, "human", EffectivenessCategory::Unmatched, 0, 0.0),
    ];
    let result = aggregate_by_source(&entries);
    assert_eq!(result.len(), 1);
    assert!((result[0].aggregate_utility).abs() < f64::EPSILON);
}

#[test]
fn test_aggregate_mixed_trust_sources() {
    // E-23
    let entries = vec![
        make_entry(1, "auto", EffectivenessCategory::Noisy, 3, 0.0),
        make_entry(2, "auto", EffectivenessCategory::Noisy, 2, 0.0),
        make_entry(3, "auto", EffectivenessCategory::Effective, 5, 0.8),
        make_entry(4, "human", EffectivenessCategory::Effective, 4, 0.9),
        make_entry(5, "human", EffectivenessCategory::Settled, 2, 0.6),
    ];
    let result = aggregate_by_source(&entries);
    assert_eq!(result.len(), 2);

    let auto = &result[0]; // "auto" sorts first
    assert_eq!(auto.trust_source, "auto");
    assert_eq!(auto.noisy_count, 2);
    assert_eq!(auto.effective_count, 1);
    assert_eq!(auto.total_entries, 3);

    let human = &result[1];
    assert_eq!(human.trust_source, "human");
    assert_eq!(human.effective_count, 1);
    assert_eq!(human.settled_count, 1);
}

#[test]
fn test_aggregate_empty_entries() {
    // E-24
    let result = aggregate_by_source(&[]);
    assert!(result.is_empty());
}

// -- build_report tests --

#[test]
fn test_report_top_10_ineffective_cap() {
    // E-25: 15 ineffective => top_ineffective capped at 10
    let entries: Vec<EntryEffectiveness> = (0..15)
        .map(|i| {
            make_entry(
                i as u64,
                "human",
                EffectivenessCategory::Ineffective,
                (15 - i) as u32, // descending injection_count
                0.1,
            )
        })
        .collect();
    let report = build_report(entries, &[], empty_window());
    assert_eq!(report.top_ineffective.len(), 10);
    // Verify sorted by injection_count descending
    for w in report.top_ineffective.windows(2) {
        assert!(w[0].injection_count >= w[1].injection_count);
    }
}

#[test]
fn test_report_all_noisy_entries_listed() {
    // E-26: 20 noisy => all listed (no cap)
    let entries: Vec<EntryEffectiveness> = (0..20)
        .map(|i| make_entry(i as u64, "auto", EffectivenessCategory::Noisy, 1, 0.0))
        .collect();
    let report = build_report(entries, &[], empty_window());
    assert_eq!(report.noisy_entries.len(), 20);
}

#[test]
fn test_report_top_10_unmatched_cap() {
    // E-27: 15 unmatched => capped at 10
    let entries: Vec<EntryEffectiveness> = (0..15)
        .map(|i| make_entry(i as u64, "human", EffectivenessCategory::Unmatched, 0, 0.0))
        .collect();
    let report = build_report(entries, &[], empty_window());
    assert_eq!(report.unmatched_entries.len(), 10);
}

#[test]
fn test_report_empty_data_produces_valid_report() {
    // E-28
    let report = build_report(vec![], &[], empty_window());

    // All five categories with count 0
    assert_eq!(report.by_category.len(), 5);
    for (_cat, count) in &report.by_category {
        assert_eq!(*count, 0);
    }

    // 10 empty calibration buckets
    assert_eq!(report.calibration.len(), 10);
    for bucket in &report.calibration {
        assert_eq!(bucket.entry_count, 0);
    }

    assert!(report.top_ineffective.is_empty());
    assert!(report.noisy_entries.is_empty());
    assert!(report.unmatched_entries.is_empty());
    assert!(report.by_source.is_empty());
}
