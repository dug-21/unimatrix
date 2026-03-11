//! Retrieval arithmetic tests: re-rank blend, penalties, boosts.
//!
//! Validates the pure-function components of search result re-ranking
//! without requiring a running server or ONNX model.

use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST;
use unimatrix_engine::confidence::{
    DEPRECATED_PENALTY, PROVENANCE_BOOST, SEARCH_SIMILARITY_WEIGHT, SUPERSEDED_PENALTY,
    rerank_score,
};

// ---------------------------------------------------------------------------
// T-RET-01: Re-rank blend ordering
// ---------------------------------------------------------------------------

#[test]
fn test_rerank_blend_ordering() {
    // High similarity + moderate confidence should beat moderate similarity + high confidence
    // because SEARCH_SIMILARITY_WEIGHT = 0.85
    let score_high_sim = rerank_score(0.95, 0.50);
    let score_high_conf = rerank_score(0.70, 1.0);

    assert!(
        score_high_sim > score_high_conf,
        "similarity-dominant entry ({score_high_sim:.4}) should beat \
         confidence-dominant entry ({score_high_conf:.4}) at weight={SEARCH_SIMILARITY_WEIGHT}"
    );
}

// ---------------------------------------------------------------------------
// T-RET-02: Status penalty ordering
// ---------------------------------------------------------------------------

#[test]
fn test_status_penalty_ordering() {
    let base_score = rerank_score(0.90, 0.60);

    let active_score = base_score * 1.0; // no penalty
    let deprecated_score = base_score * DEPRECATED_PENALTY;
    let superseded_score = base_score * SUPERSEDED_PENALTY;

    assert!(
        active_score > deprecated_score,
        "active ({active_score:.4}) should beat deprecated ({deprecated_score:.4})"
    );
    assert!(
        deprecated_score > superseded_score,
        "deprecated ({deprecated_score:.4}) should beat superseded ({superseded_score:.4})"
    );
}

// ---------------------------------------------------------------------------
// T-RET-03: Provenance boost effect
// ---------------------------------------------------------------------------

#[test]
fn test_provenance_boost_effect() {
    let base_score = rerank_score(0.85, 0.60);
    let boosted_score = base_score + PROVENANCE_BOOST;

    assert!(
        boosted_score > base_score,
        "boosted ({boosted_score:.4}) should beat unboosted ({base_score:.4})"
    );
    assert!(
        (boosted_score - base_score - PROVENANCE_BOOST).abs() < f64::EPSILON,
        "boost should be exactly {PROVENANCE_BOOST}"
    );
}

// ---------------------------------------------------------------------------
// T-RET-04: Co-access boost monotonic and capped
// ---------------------------------------------------------------------------

#[test]
fn test_co_access_boost_monotonic_and_capped() {
    // The co_access_boost function is private, but we can verify the constant
    // and the formula properties through the public MAX_CO_ACCESS_BOOST.
    // We test the log formula directly: raw = ln(1+count)/ln(1+20), boost = min(raw,1.0)*0.03

    let max_meaningful = 20.0_f64;
    let max_boost = MAX_CO_ACCESS_BOOST;

    let mut prev_boost = 0.0_f64;
    for count in 0..=50u32 {
        let boost = if count == 0 {
            0.0
        } else {
            let raw = (1.0 + count as f64).ln() / (1.0 + max_meaningful).ln();
            raw.min(1.0) * max_boost
        };

        // Monotonically non-decreasing
        assert!(
            boost >= prev_boost,
            "boost at count={count} ({boost:.6}) < boost at count={} ({prev_boost:.6})",
            count - 1
        );

        // Capped at MAX_CO_ACCESS_BOOST
        assert!(
            boost <= max_boost + f64::EPSILON,
            "boost at count={count} ({boost:.6}) exceeds max ({max_boost})"
        );

        prev_boost = boost;
    }
}

// ---------------------------------------------------------------------------
// T-RET-05: Combined interaction ordering
// ---------------------------------------------------------------------------

#[test]
fn test_combined_interaction_ordering() {
    // Entry A: high similarity, active, lesson-learned (provenance boost), co-access boost
    // Entry B: similar similarity, deprecated, no boosts
    // Entry C: slightly higher similarity, active, no boosts
    // Expected: A > C > B

    let sim_a = 0.88;
    let sim_b = 0.88;
    let sim_c = 0.90;
    let conf = 0.60;

    let score_a = rerank_score(sim_a, conf) * 1.0 + PROVENANCE_BOOST + 0.02; // active + provenance + co-access
    let score_b = rerank_score(sim_b, conf) * DEPRECATED_PENALTY; // deprecated, no boosts
    let score_c = rerank_score(sim_c, conf) * 1.0; // active, no boosts

    assert!(
        score_a > score_c,
        "A ({score_a:.4}) with boosts should beat C ({score_c:.4}) without"
    );
    assert!(
        score_c > score_b,
        "C ({score_c:.4}) active should beat B ({score_b:.4}) deprecated"
    );
}
