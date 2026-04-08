//! Coherence gate: composite health metric (lambda) for the knowledge base.
//!
//! Computes three dimension scores that combine into a single coherence value
//! in [0.0, 1.0]. All functions are pure (no I/O, deterministic given inputs).
//! Used by context_status to report knowledge base health and generate
//! maintenance recommendations.

/// Staleness threshold for confidence refresh: 24 hours in seconds.
///
/// Used by run_maintenance() in services/status.rs to identify entries eligible
/// for confidence score re-computation. NOT a Lambda input — the Lambda freshness
/// dimension was removed in crt-048.
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;

/// Stale node ratio above which graph compaction is recommended.
pub const DEFAULT_STALE_RATIO_TRIGGER: f64 = 0.10;

/// Lambda threshold below which maintenance recommendations are generated.
pub const DEFAULT_LAMBDA_THRESHOLD: f64 = 0.8;

/// Maximum entries to refresh per context_status call.
pub const MAX_CONFIDENCE_REFRESH_BATCH: usize = 500;

/// Weights for the three coherence dimensions.
pub struct CoherenceWeights {
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}

/// Default weights (ADR-001 crt-048): graph 0.46, contradiction 0.31, embedding 0.23.
/// Derived by proportional re-normalization of original 0.30:0.20:0.15 structural
/// ratio (2:1.33:1). Freshness dimension removed in crt-048 (see GH #520).
pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    graph_quality: 0.46,
    embedding_consistency: 0.23,
    contradiction_density: 0.31,
};

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

/// Contradiction density dimension: complement of contradiction pair ratio.
///
/// Returns 1.0 if `total_active` is zero (empty database guard).
/// Returns 1.0 if `contradiction_pair_count` is zero (cold-start or no contradictions
/// detected — optimistic default until the scan produces evidence).
/// Score is `1.0 - contradiction_pair_count / total_active`, clamped to [0.0, 1.0].
/// When `contradiction_pair_count > total_active` (degenerate: many pairs from a
/// small active set), the clamp produces 0.0.
///
/// `contradiction_pair_count` comes from `ContradictionScanCacheHandle` read in Phase 2
/// of `compute_report()`. It reflects detected contradiction pairs from the background
/// heuristic scan (HNSW nearest-neighbour + negation/directive/sentiment signals).
/// The cache is rebuilt approximately every 60 minutes. A stale cache is a known
/// limitation (SR-07); this function is not responsible for cache freshness.
pub fn contradiction_density_score(contradiction_pair_count: usize, total_active: u64) -> f64 {
    if total_active == 0 {
        return 1.0;
    }
    let score = 1.0 - (contradiction_pair_count as f64 / total_active as f64);
    score.clamp(0.0, 1.0)
}

/// Compute the composite lambda coherence score.
///
/// When `embedding_consistency` is `None` (embedding check not run), the
/// embedding weight is excluded and remaining weights are re-normalized
/// per ADR-001 (crt-048).
pub fn compute_lambda(
    graph_quality: f64,
    embedding_consistency: Option<f64>,
    contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64 {
    match embedding_consistency {
        Some(embed_score) => {
            let lambda = weights.graph_quality * graph_quality
                + weights.embedding_consistency * embed_score
                + weights.contradiction_density * contradiction_density;
            lambda.clamp(0.0, 1.0)
        }
        None => {
            // Re-normalize over 2 remaining dimensions.
            // With DEFAULT_WEIGHTS: remaining = 0.46 + 0.31 = 0.77
            // graph effective weight:         0.46 / 0.77 ≈ 0.5974
            // contradiction effective weight: 0.31 / 0.77 ≈ 0.4026
            let remaining = weights.graph_quality + weights.contradiction_density;
            if remaining <= 0.0 {
                return 1.0;
            }
            let lambda = (weights.graph_quality * graph_quality
                + weights.contradiction_density * contradiction_density)
                / remaining;
            lambda.clamp(0.0, 1.0)
        }
    }
}

/// Generate actionable maintenance recommendations when lambda is below threshold.
///
/// Returns an empty vec when `lambda >= threshold` (healthy state).
pub fn generate_recommendations(
    lambda: f64,
    threshold: f64,
    graph_stale_ratio: f64,
    embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String> {
    if lambda >= threshold {
        return vec![];
    }

    let mut recs = Vec::new();

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
        // Empty database: any pair count with zero active entries returns 1.0.
        assert_eq!(contradiction_density_score(0_usize, 0_u64), 1.0);
    }

    #[test]
    fn contradiction_density_pairs_exceed_active() {
        // Degenerate: more detected pairs than active entries — clamped to 0.0.
        assert_eq!(contradiction_density_score(200_usize, 100_u64), 0.0);
    }

    #[test]
    fn contradiction_density_no_pairs() {
        // No detected pairs with active entries: maximum health score.
        assert_eq!(contradiction_density_score(0_usize, 100_u64), 1.0);
    }

    #[test]
    fn contradiction_density_cold_start_cache_absent() {
        // Simulates cold-start: scan cache is None, contradiction_count defaults to 0.
        // Active entries exist but no pairs detected. Optimistic default: score 1.0.
        let result = contradiction_density_score(0_usize, 50_u64);
        assert!((result - 1.0).abs() < 1e-10);
    }

    #[test]
    fn contradiction_density_cold_start_no_pairs_found() {
        // Simulates warm cache with zero pairs found
        // (contradiction_scan_performed: true, contradiction_count: 0).
        // Active entries exist but scan returned Some([]). Optimistic default: score 1.0.
        let result = contradiction_density_score(0_usize, 50_u64);
        assert!((result - 1.0).abs() < 1e-10);
    }

    #[test]
    fn contradiction_density_partial() {
        // Mid-range: 5 pairs in a 100-entry database.
        // Expected: 1.0 - 5/100 = 0.95
        let result = contradiction_density_score(5_usize, 100_u64);
        assert!(result > 0.0);
        assert!(result < 1.0);
        assert!((result - 0.95).abs() < 1e-10);
    }

    // -- compute_lambda tests --

    #[test]
    fn lambda_all_ones() {
        let lambda = compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS);
        assert!((lambda - 1.0).abs() < 0.001);
    }

    #[test]
    fn lambda_all_zeros() {
        let lambda = compute_lambda(0.0, Some(0.0), 0.0, &DEFAULT_WEIGHTS);
        assert_eq!(lambda, 0.0);
    }

    #[test]
    fn lambda_weighted_sum() {
        let lambda = compute_lambda(0.6, Some(0.7), 0.4, &DEFAULT_WEIGHTS);
        // 0.46*0.6 + 0.23*0.7 + 0.31*0.4 = 0.276 + 0.161 + 0.124 = 0.561
        assert!(
            (lambda - 0.561).abs() < 1e-10,
            "expected 0.561, got {lambda}"
        );
    }

    #[test]
    fn lambda_renormalization_without_embedding() {
        // Case 1 — trivial (AC-08): all-ones with None must give 1.0
        let lambda = compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS);
        // remaining = 0.46 + 0.31 = 0.77
        // lambda = (0.46*1.0 + 0.31*1.0) / 0.77 = 0.77/0.77 = 1.0
        assert!((lambda - 1.0).abs() < 0.001);

        // Case 2 — non-trivial (R-07): verify the 0.46/0.77 and 0.31/0.77 re-norm base
        let lambda2 = compute_lambda(0.8, None, 0.6, &DEFAULT_WEIGHTS);
        let expected = 0.8 * (0.46_f64 / 0.77_f64) + 0.6 * (0.31_f64 / 0.77_f64);
        assert!(
            (lambda2 - expected).abs() < 1e-10,
            "expected {expected}, got {lambda2}"
        );
    }

    #[test]
    fn lambda_renormalization_partial() {
        let lambda = compute_lambda(0.4, None, 0.9, &DEFAULT_WEIGHTS);
        // remaining = 0.46 + 0.31 = 0.77
        let expected = 0.4 * (0.46_f64 / 0.77_f64) + 0.9 * (0.31_f64 / 0.77_f64);
        assert!(
            (lambda - expected).abs() < 1e-10,
            "expected {expected}, got {lambda}"
        );
    }

    #[test]
    fn lambda_weight_sum_invariant() {
        let total = DEFAULT_WEIGHTS.graph_quality
            + DEFAULT_WEIGHTS.embedding_consistency
            + DEFAULT_WEIGHTS.contradiction_density;
        // Must use f64::EPSILON per NFR-04 — exact == is forbidden even though
        // 0.46 + 0.23 + 0.31 = 1.00 is exactly representable in IEEE 754.
        assert!(
            (total - 1.0_f64).abs() < f64::EPSILON,
            "DEFAULT_WEIGHTS sum deviates from 1.0 by {}",
            (total - 1.0_f64).abs()
        );
    }

    // -- generate_recommendations tests --

    #[test]
    fn recommendations_above_threshold_empty() {
        let recs = generate_recommendations(0.85, 0.8, 0.15, 2, 1);
        assert!(recs.is_empty());
    }

    #[test]
    fn recommendations_at_threshold_empty() {
        let recs = generate_recommendations(0.8, 0.8, 0.15, 2, 1);
        assert!(recs.is_empty());
    }

    #[test]
    fn recommendations_below_threshold_high_stale_ratio() {
        let recs = generate_recommendations(0.5, 0.8, 0.25, 0, 0);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("25%"));
        assert!(recs[0].contains("compact"));
    }

    #[test]
    fn recommendations_below_threshold_all_issues() {
        // stale-confidence branch removed — 3 recommendations max (graph, embedding, quarantined)
        let recs = generate_recommendations(0.3, 0.8, 0.15, 3, 2);
        assert_eq!(recs.len(), 3);
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

    // R-01: distinct-value test to detect positional transposition
    #[test]
    fn lambda_specific_three_dimensions() {
        let result = compute_lambda(0.8, Some(0.5), 0.3, &DEFAULT_WEIGHTS);
        // graph=0.8, embed=0.5, contradiction=0.3
        // 0.8*0.46 + 0.5*0.23 + 0.3*0.31 = 0.368 + 0.115 + 0.093 = 0.576
        assert!(
            (result - 0.576_f64).abs() < 1e-10,
            "expected 0.576, got {result}"
        );
    }

    // R-01 triangulation: vary one dimension at a time, assert distinct outputs
    #[test]
    fn lambda_single_dimension_deviation() {
        // Vary graph: graph=0.5, embed=1.0, contradiction=1.0
        let r1 = compute_lambda(0.5, Some(1.0), 1.0, &DEFAULT_WEIGHTS);
        // 0.46*0.5 + 0.23*1.0 + 0.31*1.0 = 0.23 + 0.23 + 0.31 = 0.77
        assert!(
            (r1 - 0.77).abs() < 0.001,
            "vary graph: expected 0.77, got {r1}"
        );

        // Vary embedding: graph=1.0, embed=0.5, contradiction=1.0
        let r2 = compute_lambda(1.0, Some(0.5), 1.0, &DEFAULT_WEIGHTS);
        // 0.46*1.0 + 0.23*0.5 + 0.31*1.0 = 0.46 + 0.115 + 0.31 = 0.885
        assert!(
            (r2 - 0.885).abs() < 0.001,
            "vary embed: expected 0.885, got {r2}"
        );

        // Vary contradiction: graph=1.0, embed=1.0, contradiction=0.5
        let r3 = compute_lambda(1.0, Some(1.0), 0.5, &DEFAULT_WEIGHTS);
        // 0.46*1.0 + 0.23*1.0 + 0.31*0.5 = 0.46 + 0.23 + 0.155 = 0.845
        assert!(
            (r3 - 0.845).abs() < 0.001,
            "vary contradiction: expected 0.845, got {r3}"
        );

        // Assert all three results are distinct
        assert!(
            r1 != r2 && r2 != r3 && r1 != r3,
            "each dimension deviation must produce a distinct result: {r1}, {r2}, {r3}"
        );
    }

    // UT-C4-19: re-normalized effective weights sum to 1.0
    #[test]
    fn lambda_renormalized_weights_sum_to_one() {
        let w_graph = DEFAULT_WEIGHTS.graph_quality;
        let w_contra = DEFAULT_WEIGHTS.contradiction_density;
        // remaining = 0.46 + 0.31 = 0.77
        let sum = w_graph / (w_graph + w_contra) + w_contra / (w_graph + w_contra);
        assert!(
            (sum - 1.0_f64).abs() < f64::EPSILON,
            "re-normalized weights should sum to 1.0, got {sum}"
        );
    }

    // Non-trivial 2-of-3 re-normalization with specific values
    #[test]
    fn lambda_embedding_excluded_specific() {
        let lambda = compute_lambda(0.7, None, 0.8, &DEFAULT_WEIGHTS);
        // remaining = 0.46 + 0.31 = 0.77
        let expected = 0.7 * (0.46_f64 / 0.77_f64) + 0.8 * (0.31_f64 / 0.77_f64);
        assert!(
            (lambda - expected).abs() < 1e-10,
            "expected {expected}, got {lambda}"
        );
    }

    // UT-C4-23: custom weights with zero embedding weight
    #[test]
    fn lambda_custom_weights_zero_embedding() {
        let weights = CoherenceWeights {
            graph_quality: 0.3,
            embedding_consistency: 0.0,
            contradiction_density: 0.2,
        };
        // embed=None, remaining = 0.3 + 0.2 = 0.5
        // graph=0.6, contradiction=0.4
        // weighted_sum = 0.3*0.6 + 0.2*0.4 = 0.18 + 0.08 = 0.26
        // lambda = 0.26 / 0.5 = 0.52
        let lambda = compute_lambda(0.6, None, 0.4, &weights);
        assert!((lambda - 0.52).abs() < 0.001, "expected 0.52, got {lambda}");
    }

    // UT-C4-30: recommendations with embedding inconsistencies
    #[test]
    fn recommendations_below_threshold_embedding_inconsistencies() {
        let recs = generate_recommendations(0.5, 0.8, 0.05, 3, 0);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("3 embedding inconsistencies"));
    }

    // UT-C4-31: recommendations with quarantined entries
    #[test]
    fn recommendations_below_threshold_quarantined() {
        let recs = generate_recommendations(0.5, 0.8, 0.05, 0, 5);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("5 entries quarantined"));
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
