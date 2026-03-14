//! Coherence gate: composite health metric (lambda) for the knowledge base.
//!
//! Computes four dimension scores that combine into a single coherence value
//! in [0.0, 1.0]. All functions are pure (no I/O, deterministic given inputs).
//! Used by context_status to report knowledge base health and generate
//! maintenance recommendations.

use unimatrix_store::EntryRecord;

/// Default staleness threshold for confidence freshness: 24 hours in seconds.
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;

/// Stale node ratio above which graph compaction is recommended.
pub const DEFAULT_STALE_RATIO_TRIGGER: f64 = 0.10;

/// Lambda threshold below which maintenance recommendations are generated.
pub const DEFAULT_LAMBDA_THRESHOLD: f64 = 0.8;

/// Maximum entries to refresh per context_status call.
pub const MAX_CONFIDENCE_REFRESH_BATCH: usize = 500;

/// Weights for the four coherence dimensions.
pub struct CoherenceWeights {
    pub confidence_freshness: f64,
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}

/// Default weights (ADR-001): freshness 0.35, graph 0.30, contradiction 0.20, embedding 0.15.
pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    confidence_freshness: 0.35,
    graph_quality: 0.30,
    embedding_consistency: 0.15,
    contradiction_density: 0.20,
};

/// Confidence freshness dimension: fraction of active entries with non-stale confidence.
///
/// Returns `(score, stale_count)` where score is in [0.0, 1.0].
/// An entry is stale if `max(updated_at, last_accessed_at)` is older than
/// `staleness_threshold_secs` from `now`, or if both timestamps are zero.
/// Returns (1.0, 0) for an empty entry set.
pub fn confidence_freshness_score(
    entries: &[EntryRecord],
    now: u64,
    staleness_threshold_secs: u64,
) -> (f64, u64) {
    if entries.is_empty() {
        return (1.0, 0);
    }

    let mut stale_count = 0u64;
    for entry in entries {
        let reference = entry.updated_at.max(entry.last_accessed_at);
        if reference == 0 {
            stale_count += 1;
            continue;
        }
        if now > reference && (now - reference) > staleness_threshold_secs {
            stale_count += 1;
        }
    }

    let total = entries.len() as u64;
    let score = (total - stale_count) as f64 / total as f64;
    (score, stale_count)
}

/// Graph quality dimension: fraction of non-stale HNSW nodes.
///
/// Returns 1.0 if `point_count` is zero (no graph to degrade).
/// Score is `1.0 - stale_count / point_count`, clamped to [0.0, 1.0].
pub fn graph_quality_score(stale_count: usize, point_count: usize) -> f64 {
    if point_count == 0 {
        return 1.0;
    }
    let score = 1.0 - (stale_count as f64 / point_count as f64);
    score.clamp(0.0, 1.0)
}

/// Embedding consistency dimension: fraction of consistent embeddings.
///
/// Returns 1.0 if `total_checked` is zero (nothing to verify).
/// Score is `1.0 - inconsistent_count / total_checked`, clamped to [0.0, 1.0].
pub fn embedding_consistency_score(inconsistent_count: usize, total_checked: usize) -> f64 {
    if total_checked == 0 {
        return 1.0;
    }
    let score = 1.0 - (inconsistent_count as f64 / total_checked as f64);
    score.clamp(0.0, 1.0)
}

/// Contradiction density dimension: complement of quarantined-to-active ratio.
///
/// Returns 1.0 if `total_active` is zero.
/// Score is `1.0 - total_quarantined / total_active`, clamped to [0.0, 1.0].
pub fn contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64 {
    if total_active == 0 {
        return 1.0;
    }
    let score = 1.0 - (total_quarantined as f64 / total_active as f64);
    score.clamp(0.0, 1.0)
}

/// Compute the composite lambda coherence score.
///
/// When `embedding_consistency` is `None` (embedding check not run), the
/// embedding weight is excluded and remaining weights are re-normalized
/// per ADR-003.
pub fn compute_lambda(
    freshness: f64,
    graph_quality: f64,
    embedding_consistency: Option<f64>,
    contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64 {
    match embedding_consistency {
        Some(embed_score) => {
            let lambda = weights.confidence_freshness * freshness
                + weights.graph_quality * graph_quality
                + weights.embedding_consistency * embed_score
                + weights.contradiction_density * contradiction_density;
            lambda.clamp(0.0, 1.0)
        }
        None => {
            let remaining = weights.confidence_freshness
                + weights.graph_quality
                + weights.contradiction_density;
            if remaining <= 0.0 {
                return 1.0;
            }
            let lambda = (weights.confidence_freshness * freshness
                + weights.graph_quality * graph_quality
                + weights.contradiction_density * contradiction_density)
                / remaining;
            lambda.clamp(0.0, 1.0)
        }
    }
}

/// Find the age (in seconds) of the oldest stale entry.
///
/// Returns 0 if no entries are stale.
pub fn oldest_stale_age(entries: &[EntryRecord], now: u64, staleness_threshold_secs: u64) -> u64 {
    let mut oldest = 0u64;
    for entry in entries {
        let reference = entry.updated_at.max(entry.last_accessed_at);
        let age = if reference == 0 && now > 0 {
            now
        } else if now > reference {
            now - reference
        } else {
            0
        };
        if age > staleness_threshold_secs {
            oldest = oldest.max(age);
        }
    }
    oldest
}

/// Generate actionable maintenance recommendations when lambda is below threshold.
///
/// Returns an empty vec when `lambda >= threshold` (healthy state).
pub fn generate_recommendations(
    lambda: f64,
    threshold: f64,
    stale_confidence_count: u64,
    oldest_stale_age_secs: u64,
    graph_stale_ratio: f64,
    embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String> {
    if lambda >= threshold {
        return vec![];
    }

    let mut recs = Vec::new();

    if stale_confidence_count > 0 {
        let days = oldest_stale_age_secs / 86400;
        recs.push(format!(
            "{stale_confidence_count} entries have stale confidence (oldest: {days} days) -- background maintenance will refresh automatically"
        ));
    }

    if graph_stale_ratio > DEFAULT_STALE_RATIO_TRIGGER {
        let pct = (graph_stale_ratio * 100.0) as u64;
        recs.push(format!(
            "HNSW graph has {pct}% stale nodes -- background maintenance will compact automatically"
        ));
    }

    if embedding_inconsistent_count > 0 {
        recs.push(format!(
            "{embedding_inconsistent_count} embedding inconsistencies detected"
        ));
    }

    if total_quarantined > 0 {
        recs.push(format!(
            "{total_quarantined} entries quarantined -- review for resolution"
        ));
    }

    recs
}

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_store::Status;

    fn make_entry_with_timestamps(updated_at: u64, last_accessed_at: u64) -> EntryRecord {
        EntryRecord {
            id: 1,
            title: String::new(),
            content: String::new(),
            topic: String::new(),
            category: String::new(),
            tags: vec![],
            source: String::new(),
            status: Status::Active,
            confidence: 0.0,
            created_at: 0,
            updated_at,
            last_accessed_at,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: String::new(),
            helpful_count: 0,
            unhelpful_count: 0,
            pre_quarantine_status: None,
        }
    }

    // -- confidence_freshness_score tests --

    #[test]
    fn freshness_empty_entries() {
        let (score, stale) = confidence_freshness_score(&[], 1000, 86400);
        assert_eq!(score, 1.0);
        assert_eq!(stale, 0);
    }

    #[test]
    fn freshness_all_stale() {
        let now = 200_000u64;
        let threshold = 86400u64; // 1 day
        let entries = vec![
            make_entry_with_timestamps(0, 0),    // both zero = stale
            make_entry_with_timestamps(1000, 0), // 199000 seconds old > threshold
        ];
        let (score, stale) = confidence_freshness_score(&entries, now, threshold);
        assert_eq!(score, 0.0);
        assert_eq!(stale, 2);
    }

    #[test]
    fn freshness_none_stale() {
        let now = 100_000u64;
        let threshold = 86400u64;
        let entries = vec![
            make_entry_with_timestamps(now - 100, 0), // 100 seconds old < threshold
            make_entry_with_timestamps(0, now - 50),  // 50 seconds old < threshold
        ];
        let (score, stale) = confidence_freshness_score(&entries, now, threshold);
        assert_eq!(score, 1.0);
        assert_eq!(stale, 0);
    }

    #[test]
    fn freshness_uses_max_of_timestamps() {
        let now = 200_000u64;
        let threshold = 86400u64;
        // updated_at is old but last_accessed_at is recent
        let entries = vec![make_entry_with_timestamps(1000, now - 100)];
        let (score, stale) = confidence_freshness_score(&entries, now, threshold);
        assert_eq!(score, 1.0);
        assert_eq!(stale, 0);
    }

    // -- graph_quality_score tests --

    #[test]
    fn graph_quality_zero_points() {
        assert_eq!(graph_quality_score(0, 0), 1.0);
    }

    #[test]
    fn graph_quality_no_stale() {
        assert_eq!(graph_quality_score(0, 100), 1.0);
    }

    #[test]
    fn graph_quality_stale_exceeds_total_clamped() {
        assert_eq!(graph_quality_score(200, 100), 0.0);
    }

    #[test]
    fn graph_quality_half_stale() {
        let score = graph_quality_score(50, 100);
        assert!((score - 0.5).abs() < 0.001);
    }

    // -- embedding_consistency_score tests --

    #[test]
    fn embedding_consistency_zero_checked() {
        assert_eq!(embedding_consistency_score(0, 0), 1.0);
    }

    #[test]
    fn embedding_consistency_all_inconsistent() {
        assert_eq!(embedding_consistency_score(10, 10), 0.0);
    }

    #[test]
    fn embedding_consistency_none_inconsistent() {
        assert_eq!(embedding_consistency_score(0, 100), 1.0);
    }

    // -- contradiction_density_score tests --

    #[test]
    fn contradiction_density_zero_active() {
        assert_eq!(contradiction_density_score(0, 0), 1.0);
    }

    #[test]
    fn contradiction_density_quarantined_exceeds_active() {
        assert_eq!(contradiction_density_score(200, 100), 0.0);
    }

    #[test]
    fn contradiction_density_no_quarantined() {
        assert_eq!(contradiction_density_score(0, 100), 1.0);
    }

    // -- compute_lambda tests --

    #[test]
    fn lambda_all_ones() {
        let lambda = compute_lambda(1.0, 1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS);
        assert!((lambda - 1.0).abs() < 0.001);
    }

    #[test]
    fn lambda_all_zeros() {
        let lambda = compute_lambda(0.0, 0.0, Some(0.0), 0.0, &DEFAULT_WEIGHTS);
        assert_eq!(lambda, 0.0);
    }

    #[test]
    fn lambda_weighted_sum() {
        let lambda = compute_lambda(0.5, 0.5, Some(0.5), 0.5, &DEFAULT_WEIGHTS);
        // 0.35*0.5 + 0.30*0.5 + 0.15*0.5 + 0.20*0.5 = 0.5
        assert!((lambda - 0.5).abs() < 0.001);
    }

    #[test]
    fn lambda_renormalization_without_embedding() {
        let lambda = compute_lambda(1.0, 1.0, None, 1.0, &DEFAULT_WEIGHTS);
        // remaining = 0.35 + 0.30 + 0.20 = 0.85
        // lambda = (0.35*1 + 0.30*1 + 0.20*1) / 0.85 = 0.85/0.85 = 1.0
        assert!((lambda - 1.0).abs() < 0.001);
    }

    #[test]
    fn lambda_renormalization_partial() {
        let lambda = compute_lambda(0.5, 0.5, None, 0.5, &DEFAULT_WEIGHTS);
        // remaining = 0.85, weighted_sum = 0.85*0.5 = 0.425
        // lambda = 0.425/0.85 = 0.5
        assert!((lambda - 0.5).abs() < 0.001);
    }

    #[test]
    fn lambda_weight_sum_invariant() {
        let total = DEFAULT_WEIGHTS.confidence_freshness
            + DEFAULT_WEIGHTS.graph_quality
            + DEFAULT_WEIGHTS.embedding_consistency
            + DEFAULT_WEIGHTS.contradiction_density;
        assert!(
            (total - 1.0).abs() < 0.001,
            "weight sum should be 1.0, got {total}"
        );
    }

    // -- oldest_stale_age tests --

    #[test]
    fn oldest_stale_no_stale() {
        let now = 100_000u64;
        let entries = vec![make_entry_with_timestamps(now - 100, 0)];
        assert_eq!(oldest_stale_age(&entries, now, 86400), 0);
    }

    #[test]
    fn oldest_stale_one_stale() {
        let now = 200_000u64;
        let entries = vec![make_entry_with_timestamps(1000, 0)]; // age = 199000
        let age = oldest_stale_age(&entries, now, 86400);
        assert_eq!(age, 199_000);
    }

    #[test]
    fn oldest_stale_both_timestamps_zero() {
        let now = 100_000u64;
        let entries = vec![make_entry_with_timestamps(0, 0)];
        let age = oldest_stale_age(&entries, now, 86400);
        assert_eq!(age, now); // age = now when reference is 0
    }

    // -- generate_recommendations tests --

    #[test]
    fn recommendations_above_threshold_empty() {
        let recs = generate_recommendations(0.85, 0.8, 10, 172800, 0.15, 2, 1);
        assert!(recs.is_empty());
    }

    #[test]
    fn recommendations_at_threshold_empty() {
        let recs = generate_recommendations(0.8, 0.8, 10, 172800, 0.15, 2, 1);
        assert!(recs.is_empty());
    }

    #[test]
    fn recommendations_below_threshold_stale_confidence() {
        let recs = generate_recommendations(0.5, 0.8, 15, 172800, 0.05, 0, 0);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("15 entries have stale confidence"));
        assert!(recs[0].contains("2 days"));
    }

    #[test]
    fn recommendations_below_threshold_high_stale_ratio() {
        let recs = generate_recommendations(0.5, 0.8, 0, 0, 0.25, 0, 0);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("25%"));
        assert!(recs[0].contains("compact"));
    }

    #[test]
    fn recommendations_below_threshold_all_issues() {
        let recs = generate_recommendations(0.3, 0.8, 5, 86400, 0.15, 3, 2);
        assert_eq!(recs.len(), 4);
    }

    // -- crt-005 Stage 3c: Additional tests from test plan --

    // UT-C4-06: recently accessed not stale
    #[test]
    fn freshness_recently_accessed_not_stale() {
        let now = 100_000u64;
        let threshold = 86400u64; // 24 hours
        // last_accessed_at = now - 1 hour (3600 secs), well within 24h threshold
        let entries = vec![make_entry_with_timestamps(0, now - 3600)];
        let (score, stale) = confidence_freshness_score(&entries, now, threshold);
        assert_eq!(score, 1.0, "recently accessed entry should not be stale");
        assert_eq!(stale, 0);
    }

    // UT-C4-07: both timestamps older than threshold
    #[test]
    fn freshness_both_timestamps_older_than_threshold() {
        let now = 300_000u64;
        let threshold = 86400u64;
        // updated_at = now - 48h, last_accessed_at = now - 36h
        let entries = vec![make_entry_with_timestamps(
            now - 48 * 3600, // updated 48h ago
            now - 36 * 3600, // accessed 36h ago
        )];
        let (score, stale) = confidence_freshness_score(&entries, now, threshold);
        assert_eq!(score, 0.0, "both timestamps older than threshold -> stale");
        assert_eq!(stale, 1);
    }

    // UT-C4-14: embedding consistency single entry
    #[test]
    fn embedding_consistency_single_entry_consistent() {
        assert_eq!(embedding_consistency_score(0, 1), 1.0);
    }

    #[test]
    fn embedding_consistency_single_entry_inconsistent() {
        assert_eq!(embedding_consistency_score(1, 1), 0.0);
    }

    // UT-C4-17: lambda with specific four dimensions
    #[test]
    fn lambda_specific_four_dimensions() {
        let lambda = compute_lambda(0.9, 0.8, Some(1.0), 0.7, &DEFAULT_WEIGHTS);
        // 0.35*0.9 + 0.30*0.8 + 0.15*1.0 + 0.20*0.7 = 0.315 + 0.24 + 0.15 + 0.14 = 0.845
        assert!(
            (lambda - 0.845).abs() < 0.001,
            "expected 0.845, got {lambda}"
        );
    }

    // UT-C4-18: lambda with embedding excluded (specific value)
    #[test]
    fn lambda_embedding_excluded_specific() {
        let lambda = compute_lambda(0.9, 0.8, None, 0.7, &DEFAULT_WEIGHTS);
        // remaining = 0.35 + 0.30 + 0.20 = 0.85
        // weighted_sum = 0.35*0.9 + 0.30*0.8 + 0.20*0.7 = 0.315 + 0.24 + 0.14 = 0.695
        // lambda = 0.695 / 0.85 = 0.81765...
        assert!(
            (lambda - 0.81765).abs() < 0.001,
            "expected ~0.81765, got {lambda}"
        );
    }

    // UT-C4-19: re-normalized effective weights sum to 1.0
    #[test]
    fn lambda_renormalized_weights_sum_to_one() {
        let remaining = DEFAULT_WEIGHTS.confidence_freshness
            + DEFAULT_WEIGHTS.graph_quality
            + DEFAULT_WEIGHTS.contradiction_density;
        let w_freshness = DEFAULT_WEIGHTS.confidence_freshness / remaining;
        let w_graph = DEFAULT_WEIGHTS.graph_quality / remaining;
        let w_contradiction = DEFAULT_WEIGHTS.contradiction_density / remaining;
        let sum = w_freshness + w_graph + w_contradiction;
        assert!(
            (sum - 1.0).abs() < f64::EPSILON * 10.0,
            "re-normalized weights should sum to 1.0, got {sum}"
        );
    }

    // UT-C4-22: lambda single dimension deviation
    #[test]
    fn lambda_single_dimension_deviation() {
        let lambda = compute_lambda(0.5, 1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS);
        // 0.35*0.5 + 0.30*1.0 + 0.15*1.0 + 0.20*1.0 = 0.175 + 0.30 + 0.15 + 0.20 = 0.825
        assert!(
            (lambda - 0.825).abs() < 0.001,
            "expected 0.825, got {lambda}"
        );
    }

    // UT-C4-23: custom weights with zero embedding weight
    #[test]
    fn lambda_custom_weights_zero_embedding() {
        let weights = CoherenceWeights {
            confidence_freshness: 0.5,
            graph_quality: 0.3,
            embedding_consistency: 0.0,
            contradiction_density: 0.2,
        };
        // embed=None, remaining = 0.5 + 0.3 + 0.2 = 1.0
        let lambda = compute_lambda(0.8, 0.6, None, 0.4, &weights);
        // (0.5*0.8 + 0.3*0.6 + 0.2*0.4) / 1.0 = 0.4 + 0.18 + 0.08 = 0.66
        assert!((lambda - 0.66).abs() < 0.001, "expected 0.66, got {lambda}");
    }

    // UT-C4-30: recommendations with embedding inconsistencies
    #[test]
    fn recommendations_below_threshold_embedding_inconsistencies() {
        let recs = generate_recommendations(0.5, 0.8, 0, 0, 0.05, 3, 0);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("3 embedding inconsistencies"));
    }

    // UT-C4-31: recommendations with quarantined entries
    #[test]
    fn recommendations_below_threshold_quarantined() {
        let recs = generate_recommendations(0.5, 0.8, 0, 0, 0.05, 0, 5);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("5 entries quarantined"));
    }

    // UT-C4-35: staleness threshold is named constant
    #[test]
    fn staleness_threshold_constant_value() {
        assert_eq!(
            DEFAULT_STALENESS_THRESHOLD_SECS, 86400,
            "staleness threshold should be 24 hours"
        );
    }

    // crt-019 AC-07: batch size increased to 500
    #[test]
    fn test_max_confidence_refresh_batch_is_500() {
        assert_eq!(
            MAX_CONFIDENCE_REFRESH_BATCH, 500,
            "MAX_CONFIDENCE_REFRESH_BATCH must be 500 after crt-019"
        );
    }
}
