//! Confidence score computation for knowledge entries.
//!
//! Implements a six-component additive weighted composite formula that
//! transforms raw usage signals into a single quality score in [0.0, 1.0].
//! All component functions are pure (no I/O, deterministic given inputs).
//! Internal computation uses f64 for numerical stability (ADR-002).

use unimatrix_core::{EntryRecord, Status};

// -- Weight constants (must sum to exactly 1.0) --

/// Weight for base quality (status-dependent).
pub const W_BASE: f32 = 0.20;
/// Weight for usage frequency.
pub const W_USAGE: f32 = 0.15;
/// Weight for freshness (recency of access).
pub const W_FRESH: f32 = 0.20;
/// Weight for helpfulness (Wilson score).
pub const W_HELP: f32 = 0.15;
/// Weight for correction chain quality.
pub const W_CORR: f32 = 0.15;
/// Weight for creator trust level.
pub const W_TRUST: f32 = 0.15;

/// Access counts beyond this contribute negligible signal.
pub const MAX_MEANINGFUL_ACCESS: f64 = 50.0;

/// Freshness half-life in hours (1 week).
pub const FRESHNESS_HALF_LIFE_HOURS: f64 = 168.0;

/// Minimum votes (helpful + unhelpful) before Wilson score deviates from neutral.
pub const MINIMUM_SAMPLE_SIZE: u32 = 5;

/// Wilson score z-value for 95% confidence interval.
pub const WILSON_Z: f64 = 1.96;

/// Similarity weight for search re-ranking blend.
pub const SEARCH_SIMILARITY_WEIGHT: f32 = 0.85;

/// Base quality proxy from entry lifecycle status.
///
/// Active entries = 0.5, Proposed = 0.5, Deprecated = 0.2.
/// Uses exhaustive match so new Status variants cause a compile error.
pub fn base_score(status: Status) -> f64 {
    match status {
        Status::Active => 0.5,
        Status::Proposed => 0.5,
        Status::Deprecated => 0.2,
    }
}

/// Log-transformed access frequency, clamped to [0.0, 1.0].
///
/// `usage_score(0) = 0.0`, `usage_score(50) ~= 1.0`, `usage_score(500) = 1.0` (clamped).
pub fn usage_score(access_count: u32) -> f64 {
    if access_count == 0 {
        return 0.0;
    }
    let numerator = (1.0 + access_count as f64).ln();
    let denominator = (1.0 + MAX_MEANINGFUL_ACCESS).ln();
    let result = numerator / denominator;
    result.min(1.0)
}

/// Exponential decay from reference timestamp, returning a value in [0.0, 1.0].
///
/// Uses `last_accessed_at` if > 0, otherwise falls back to `created_at`.
/// Returns 0.0 when both timestamps are 0 (no reference).
/// Returns 1.0 on clock skew (reference in the future).
pub fn freshness_score(last_accessed_at: u64, created_at: u64, now: u64) -> f64 {
    let reference = if last_accessed_at > 0 {
        last_accessed_at
    } else {
        created_at
    };

    if reference == 0 {
        return 0.0;
    }

    if now <= reference {
        return 1.0;
    }

    let age_seconds = now - reference;
    let age_hours = age_seconds as f64 / 3600.0;
    (-age_hours / FRESHNESS_HALF_LIFE_HOURS).exp()
}

/// Helpfulness score using Wilson score lower bound with minimum sample guard.
///
/// Returns 0.5 (neutral prior) when total votes < `MINIMUM_SAMPLE_SIZE`.
/// Otherwise returns Wilson lower bound at z = 1.96.
pub fn helpfulness_score(helpful_count: u32, unhelpful_count: u32) -> f64 {
    let total = helpful_count as u64 + unhelpful_count as u64;
    if total < MINIMUM_SAMPLE_SIZE as u64 {
        return 0.5;
    }

    wilson_lower_bound(helpful_count as f64, total as f64)
}

/// Wilson score lower bound at 95% confidence (z = 1.96).
///
/// Formula: `(p_hat + z^2/(2n) - z * sqrt(p_hat*(1-p_hat)/n + z^2/(4n^2))) / (1 + z^2/n)`
///
/// Only called when total >= MINIMUM_SAMPLE_SIZE (>= 5).
fn wilson_lower_bound(positive: f64, total: f64) -> f64 {
    let z = WILSON_Z;
    let p_hat = positive / total;
    let z_sq = z * z;

    let numerator = p_hat + z_sq / (2.0 * total)
        - z * (p_hat * (1.0 - p_hat) / total + z_sq / (4.0 * total * total)).sqrt();
    let denominator = 1.0 + z_sq / total;

    let result = numerator / denominator;
    result.clamp(0.0, 1.0)
}

/// Correction chain quality signal.
///
/// 0 corrections = 0.5, 1-2 = 0.8, 3-5 = 0.6, 6+ = 0.3.
pub fn correction_score(correction_count: u32) -> f64 {
    match correction_count {
        0 => 0.5,
        1..=2 => 0.8,
        3..=5 => 0.6,
        _ => 0.3,
    }
}

/// Trust source of creator.
///
/// "human" = 1.0, "system" = 0.7, "agent" = 0.5, any other = 0.3.
/// Case-sensitive matching.
pub fn trust_score(trust_source: &str) -> f64 {
    match trust_source {
        "human" => 1.0,
        "system" => 0.7,
        "agent" => 0.5,
        _ => 0.3,
    }
}

/// Compute confidence for an entry at the given timestamp.
///
/// Returns f32 in [0.0, 1.0]. All intermediate computation uses f64.
/// The function is pure: given the same inputs, it always returns the same output.
pub fn compute_confidence(entry: &EntryRecord, now: u64) -> f32 {
    let b = base_score(entry.status);
    let u = usage_score(entry.access_count);
    let f = freshness_score(entry.last_accessed_at, entry.created_at, now);
    let h = helpfulness_score(entry.helpful_count, entry.unhelpful_count);
    let c = correction_score(entry.correction_count);
    let t = trust_score(&entry.trust_source);

    let composite = W_BASE as f64 * b
        + W_USAGE as f64 * u
        + W_FRESH as f64 * f
        + W_HELP as f64 * h
        + W_CORR as f64 * c
        + W_TRUST as f64 * t;

    composite.clamp(0.0, 1.0) as f32
}

/// Blend similarity and confidence for search result re-ranking.
///
/// `final_score = SEARCH_SIMILARITY_WEIGHT * similarity + (1 - SEARCH_SIMILARITY_WEIGHT) * confidence`
pub fn rerank_score(similarity: f32, confidence: f32) -> f32 {
    SEARCH_SIMILARITY_WEIGHT * similarity + (1.0 - SEARCH_SIMILARITY_WEIGHT) * confidence
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- T-01: Weight sum invariant (R-05, AC-02) --

    #[test]
    fn weight_sum_invariant() {
        assert_eq!(W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST, 1.0);
    }

    // -- T-02: base_score values (AC-08) --

    #[test]
    fn base_score_active() {
        assert_eq!(base_score(Status::Active), 0.5);
    }

    #[test]
    fn base_score_proposed() {
        assert_eq!(base_score(Status::Proposed), 0.5);
    }

    #[test]
    fn base_score_deprecated() {
        assert_eq!(base_score(Status::Deprecated), 0.2);
    }

    // -- T-03: usage_score values (R-08, AC-03) --

    #[test]
    fn usage_score_zero() {
        assert_eq!(usage_score(0), 0.0);
    }

    #[test]
    fn usage_score_one() {
        let result = usage_score(1);
        assert!(result > 0.0 && result < 0.5);
    }

    #[test]
    fn usage_score_at_max() {
        let result = usage_score(50);
        assert!((result - 1.0).abs() < 0.01);
    }

    #[test]
    fn usage_score_above_max_clamped() {
        assert_eq!(usage_score(500), 1.0);
    }

    #[test]
    fn usage_score_u32_max_clamped() {
        assert_eq!(usage_score(u32::MAX), 1.0);
    }

    // -- T-04: freshness_score values (R-07, AC-04) --

    #[test]
    fn freshness_just_accessed() {
        let now = 1_000_000u64;
        let result = freshness_score(now, now, now);
        assert!((result - 1.0).abs() < 0.001);
    }

    #[test]
    fn freshness_one_week_ago() {
        let now = 1_000_000u64;
        let one_week_ago = now - 168 * 3600;
        let result = freshness_score(one_week_ago, 0, now);
        assert!((result - 0.3679).abs() < 0.01);
    }

    #[test]
    fn freshness_fallback_to_created_at() {
        let now = 1_000_000u64;
        let result = freshness_score(0, now, now);
        assert!((result - 1.0).abs() < 0.001);
    }

    #[test]
    fn freshness_both_timestamps_zero() {
        let now = 1_000_000u64;
        assert_eq!(freshness_score(0, 0, now), 0.0);
    }

    #[test]
    fn freshness_clock_skew() {
        let now = 1_000_000u64;
        assert_eq!(freshness_score(now + 100, 0, now), 1.0);
    }

    #[test]
    fn freshness_very_old_entry() {
        let now = 100_000_000u64;
        let very_old = now - 365 * 24 * 3600;
        let result = freshness_score(very_old, 0, now);
        assert!(result >= 0.0 && result < 0.001);
    }

    // -- T-05: helpfulness_score minimum sample guard (R-01, AC-05, AC-21) --

    #[test]
    fn helpfulness_no_votes() {
        assert_eq!(helpfulness_score(0, 0), 0.5);
    }

    #[test]
    fn helpfulness_below_minimum_three_helpful() {
        assert_eq!(helpfulness_score(3, 0), 0.5);
    }

    #[test]
    fn helpfulness_below_minimum_two_each() {
        assert_eq!(helpfulness_score(2, 2), 0.5);
    }

    #[test]
    fn helpfulness_below_minimum_four_total() {
        assert_eq!(helpfulness_score(4, 0), 0.5);
    }

    #[test]
    fn helpfulness_at_minimum_wilson_kicks_in() {
        let result = helpfulness_score(5, 0);
        assert_ne!(result, 0.5);
        assert!(result < 1.0);
    }

    #[test]
    fn helpfulness_all_helpful() {
        let result = helpfulness_score(100, 0);
        assert!(result > 0.5 && result < 1.0);
    }

    #[test]
    fn helpfulness_all_unhelpful() {
        let result = helpfulness_score(0, 100);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn helpfulness_mixed_mostly_helpful() {
        let result = helpfulness_score(80, 20);
        assert!(result > 0.5);
    }

    // -- T-06: Wilson score reference values (R-01, AC-15) --

    #[test]
    fn wilson_reference_n100_p80() {
        let result = wilson_lower_bound(80.0, 100.0);
        // Exact Wilson lower bound at z=1.96: ~0.7112
        assert!((result - 0.7112).abs() < 0.002);
    }

    #[test]
    fn wilson_reference_n10_p80() {
        let result = wilson_lower_bound(8.0, 10.0);
        assert!((result - 0.494).abs() < 0.02);
    }

    #[test]
    fn wilson_reference_large_n_p50() {
        let result = wilson_lower_bound(50000.0, 100000.0);
        assert!((result - 0.497).abs() < 0.005);
    }

    // -- T-07: correction_score values (AC-06) --

    #[test]
    fn correction_score_values() {
        assert_eq!(correction_score(0), 0.5);
        assert_eq!(correction_score(1), 0.8);
        assert_eq!(correction_score(2), 0.8);
        assert_eq!(correction_score(3), 0.6);
        assert_eq!(correction_score(4), 0.6);
        assert_eq!(correction_score(5), 0.6);
        assert_eq!(correction_score(6), 0.3);
        assert_eq!(correction_score(100), 0.3);
    }

    // -- T-08: trust_score values (AC-07) --

    #[test]
    fn trust_score_values() {
        assert_eq!(trust_score("human"), 1.0);
        assert_eq!(trust_score("system"), 0.7);
        assert_eq!(trust_score("agent"), 0.5);
        assert_eq!(trust_score(""), 0.3);
        assert_eq!(trust_score("unknown"), 0.3);
        assert_eq!(trust_score("Human"), 0.3); // case-sensitive
    }

    // -- T-09: compute_confidence composite (AC-01, AC-02, R-05) --

    fn make_test_entry(
        status: Status,
        access_count: u32,
        last_accessed_at: u64,
        created_at: u64,
        helpful_count: u32,
        unhelpful_count: u32,
        correction_count: u32,
        trust_source: &str,
    ) -> EntryRecord {
        EntryRecord {
            id: 1,
            title: String::new(),
            content: String::new(),
            topic: String::new(),
            category: String::new(),
            tags: vec![],
            source: String::new(),
            status,
            confidence: 0.0,
            created_at,
            updated_at: 0,
            last_accessed_at,
            access_count,
            supersedes: None,
            superseded_by: None,
            correction_count,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: trust_source.to_string(),
            helpful_count,
            unhelpful_count,
        }
    }

    #[test]
    fn compute_confidence_all_defaults() {
        let entry = make_test_entry(Status::Active, 0, 0, 0, 0, 0, 0, "");
        let result = compute_confidence(&entry, 1_000_000);
        // base=0.5, usage=0.0, fresh=0.0, help=0.5, corr=0.5, trust=0.3
        let expected =
            0.20 * 0.5 + 0.15 * 0.0 + 0.20 * 0.0 + 0.15 * 0.5 + 0.15 * 0.5 + 0.15 * 0.3;
        assert!((result as f64 - expected).abs() < 0.001);
    }

    #[test]
    fn compute_confidence_all_max() {
        let now = 1_000_000u64;
        let entry = make_test_entry(Status::Active, 1000, now, now, 100, 0, 1, "human");
        let result = compute_confidence(&entry, now);
        assert!(result > 0.8);
        assert!(result <= 1.0);
    }

    // -- T-10: compute_confidence range (AC-01, R-12) --

    #[test]
    fn compute_confidence_range_active_defaults() {
        let entry = make_test_entry(Status::Active, 0, 0, 0, 0, 0, 0, "");
        let result = compute_confidence(&entry, 1_000_000);
        assert!(result >= 0.0);
        assert!(result <= 1.0);
    }

    #[test]
    fn compute_confidence_range_deprecated_max_values() {
        let now = 1_000_000u64;
        let entry =
            make_test_entry(Status::Deprecated, u32::MAX, now, now, u32::MAX, 0, 100, "human");
        let result = compute_confidence(&entry, now);
        assert!(result >= 0.0);
        assert!(result <= 1.0);
    }

    #[test]
    fn compute_confidence_range_extreme_timestamps() {
        let entry = make_test_entry(Status::Active, 0, u64::MAX, 0, 0, 0, 0, "agent");
        let result = compute_confidence(&entry, 0);
        assert!(result >= 0.0);
        assert!(result <= 1.0);
    }

    #[test]
    fn compute_confidence_range_all_unhelpful() {
        let entry = make_test_entry(Status::Active, 50, 0, 0, 0, u32::MAX, 0, "");
        let result = compute_confidence(&entry, 1_000_000);
        assert!(result >= 0.0);
        assert!(result <= 1.0);
    }

    // -- T-11: rerank_score blend (AC-13, AC-14) --

    #[test]
    fn rerank_score_both_max() {
        assert_eq!(rerank_score(1.0, 1.0), 1.0);
    }

    #[test]
    fn rerank_score_both_zero() {
        assert_eq!(rerank_score(0.0, 0.0), 0.0);
    }

    #[test]
    fn rerank_score_similarity_only() {
        let result = rerank_score(1.0, 0.0);
        assert!((result - 0.85).abs() < 0.001);
    }

    #[test]
    fn rerank_score_confidence_only() {
        let result = rerank_score(0.0, 1.0);
        assert!((result - 0.15).abs() < 0.001);
    }

    #[test]
    fn rerank_score_confidence_tiebreaker() {
        assert!(rerank_score(0.90, 0.80) > rerank_score(0.90, 0.20));
    }

    #[test]
    fn rerank_score_similarity_dominant() {
        assert!(rerank_score(0.95, 0.0) > rerank_score(0.70, 1.0));
    }
}
