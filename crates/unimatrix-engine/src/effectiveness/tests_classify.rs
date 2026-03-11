//! Tests for classify_entry and utility_score functions.

use super::*;

fn default_noisy() -> &'static [&'static str] {
    &["auto"]
}

// -- utility_score tests --

#[test]
fn test_utility_score_zero_denominator_returns_zero() {
    // E-14
    assert_eq!(utility_score(0, 0, 0), 0.0);
}

#[test]
fn test_utility_score_pure_success_returns_one() {
    // E-15
    assert!((utility_score(10, 0, 0) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_utility_score_mixed_outcomes() {
    // E-16: (3*1.0 + 4*0.5 + 3*0.0) / 10 = 0.5
    assert!((utility_score(3, 4, 3) - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_utility_score_large_values_no_overflow() {
    // E-16b
    let result = utility_score(1_000_000, 1_000_000, 1_000_000);
    assert!((result - 0.5).abs() < 1e-10);
}

// -- classify_entry: priority chain tests --

#[test]
fn test_classify_noisy_over_ineffective_priority() {
    // E-01: auto + 0 helpful + 5 injections + 0% success => Noisy, not Ineffective
    let result = classify_entry(
        1,
        "test",
        "topic-a",
        "auto",
        0,
        0, // helpful, unhelpful
        5,
        0,
        0,
        5, // injection, success, rework, abandoned
        true,
        default_noisy(),
    );
    assert_eq!(result.category, EffectivenessCategory::Noisy);
}

#[test]
fn test_classify_ineffective_over_unmatched_priority() {
    // E-02: agent + 1 helpful + 4 injections + 0% success => Ineffective
    let result = classify_entry(
        2,
        "test",
        "topic-a",
        "agent",
        1,
        0,
        4,
        0,
        0,
        4,
        true,
        default_noisy(),
    );
    assert_eq!(result.category, EffectivenessCategory::Ineffective);
}

#[test]
fn test_classify_unmatched_over_settled_priority() {
    // E-03: zero injections + active topic => Unmatched
    let result = classify_entry(
        3,
        "test",
        "topic-a",
        "human",
        0,
        0,
        0,
        0,
        0,
        0,
        true,
        default_noisy(),
    );
    assert_eq!(result.category, EffectivenessCategory::Unmatched);
}

#[test]
fn test_classify_ineffective_boundary_at_min_injections() {
    // E-04: 3 injections, success_rate 33.3% (>= 30%) => NOT Ineffective
    let result = classify_entry(
        4,
        "test",
        "topic-a",
        "human",
        0,
        0,
        3,
        1,
        0,
        2, // utility = 1/3 = 0.333
        true,
        default_noisy(),
    );
    assert_ne!(result.category, EffectivenessCategory::Ineffective);

    // E-04: 10 injections, utility = (2*1.0 + 1*0.5) / 10 = 0.25 < 0.3 => Ineffective
    let result2 = classify_entry(
        5,
        "test",
        "topic-a",
        "human",
        0,
        0,
        10,
        2,
        1,
        7,
        true,
        default_noisy(),
    );
    assert_eq!(result2.category, EffectivenessCategory::Ineffective);
}

#[test]
fn test_classify_default_effective() {
    // E-05: good stats => Effective
    let result = classify_entry(
        6,
        "test",
        "topic-a",
        "human",
        5,
        0,
        5,
        4,
        1,
        0,
        true,
        default_noisy(),
    );
    assert_eq!(result.category, EffectivenessCategory::Effective);
}

#[test]
fn test_classify_empty_topic_mapped_to_unattributed() {
    // E-06
    let result = classify_entry(
        7,
        "test",
        "",
        "human",
        0,
        0,
        1,
        1,
        0,
        0,
        true,
        default_noisy(),
    );
    assert_eq!(result.topic, "(unattributed)");

    // Already "(unattributed)" is not double-wrapped
    let result2 = classify_entry(
        8,
        "test",
        "(unattributed)",
        "human",
        0,
        0,
        1,
        1,
        0,
        0,
        true,
        default_noisy(),
    );
    assert_eq!(result2.topic, "(unattributed)");
}

// -- classify_entry: settled tests --

#[test]
fn test_classify_settled_inactive_topic_with_success() {
    // E-17: inactive topic + success injection => Settled
    let result = classify_entry(
        9,
        "test",
        "old-topic",
        "human",
        0,
        0,
        2,
        1,
        1,
        0,
        false,
        default_noisy(),
    );
    assert_eq!(result.category, EffectivenessCategory::Settled);
}

#[test]
fn test_classify_settled_requires_success_injection() {
    // E-18: inactive topic + no success => NOT Settled (falls to Effective)
    let result = classify_entry(
        10,
        "test",
        "old-topic",
        "human",
        0,
        0,
        2,
        0,
        1,
        1,
        false,
        default_noisy(),
    );
    assert_ne!(result.category, EffectivenessCategory::Settled);
    assert_eq!(result.category, EffectivenessCategory::Effective);
}

#[test]
fn test_classify_inactive_topic_zero_injections() {
    // E-19: zero injections + inactive topic => Effective (default, not Settled)
    let result = classify_entry(
        11,
        "test",
        "old-topic",
        "human",
        0,
        0,
        0,
        0,
        0,
        0,
        false,
        default_noisy(),
    );
    assert_ne!(result.category, EffectivenessCategory::Settled);
    assert_eq!(result.category, EffectivenessCategory::Effective);
}

// -- classify_entry: noisy trust source tests --

#[test]
fn test_classify_noisy_matching_trust_source() {
    // E-20: auto + 0 helpful + injection => Noisy
    let result = classify_entry(
        12,
        "test",
        "topic-a",
        "auto",
        0,
        0,
        1,
        1,
        0,
        0,
        true,
        &["auto"],
    );
    assert_eq!(result.category, EffectivenessCategory::Noisy);
}

#[test]
fn test_classify_noisy_non_matching_trust_source() {
    // E-21: agent + 0 helpful + injection => NOT Noisy
    let result = classify_entry(
        13,
        "test",
        "topic-a",
        "agent",
        0,
        0,
        1,
        1,
        0,
        0,
        true,
        &["auto"],
    );
    assert_ne!(result.category, EffectivenessCategory::Noisy);
}

#[test]
fn test_classify_noisy_with_helpful_not_noisy() {
    // auto + 1 helpful + injection => NOT Noisy
    let result = classify_entry(
        14,
        "test",
        "topic-a",
        "auto",
        1,
        0,
        1,
        1,
        0,
        0,
        true,
        &["auto"],
    );
    assert_ne!(result.category, EffectivenessCategory::Noisy);
}

#[test]
fn test_classify_helpfulness_ratio_computed() {
    let result = classify_entry(
        15,
        "test",
        "topic-a",
        "human",
        3,
        7,
        5,
        3,
        1,
        1,
        true,
        default_noisy(),
    );
    assert!((result.helpfulness_ratio - 0.3).abs() < f64::EPSILON);
}
