//! Retrieval arithmetic tests: re-rank blend, penalties, boosts.
//!
//! Validates the pure-function components of search result re-ranking
//! without requiring a running server or ONNX model.

use unimatrix_engine::coaccess::MAX_CO_ACCESS_BOOST;
use unimatrix_engine::confidence::{PROVENANCE_BOOST, rerank_score};
use unimatrix_engine::graph::{
    CLEAN_REPLACEMENT_PENALTY, FALLBACK_PENALTY, ORPHAN_PENALTY, PARTIAL_SUPERSESSION_PENALTY,
};

// ---------------------------------------------------------------------------
// T-RET-01: Re-rank blend ordering
// ---------------------------------------------------------------------------

#[test]
fn test_rerank_blend_ordering() {
    // High similarity + moderate confidence should beat moderate similarity + high confidence.
    // Use initial server-start confidence_weight = 0.184 (adaptive weight at spread 0.1471).
    let cw = 0.184_f64;
    let score_high_sim = rerank_score(0.95, 0.50, cw);
    let score_high_conf = rerank_score(0.70, 1.0, cw);

    // At cw=0.184: similarity_weight = 0.816
    // high_sim:  0.816 * 0.95 + 0.184 * 0.50 = 0.8692
    // high_conf: 0.816 * 0.70 + 0.184 * 1.00 = 0.7552
    assert!(
        score_high_sim > score_high_conf,
        "similarity-dominant entry ({score_high_sim:.4}) should beat \
         confidence-dominant entry ({score_high_conf:.4}) at confidence_weight={cw}"
    );
}

// ---------------------------------------------------------------------------
// T-RET-02: Status penalty ordering (crt-014 topology-derived constants)
// ---------------------------------------------------------------------------

#[test]
fn test_status_penalty_ordering() {
    let base_score = rerank_score(0.90, 0.60, 0.184);

    // crt-014 topology ordering (ADR-004):
    // active (no penalty) > orphan deprecated (ORPHAN_PENALTY=0.75) >
    // clean-replacement superseded (CLEAN_REPLACEMENT_PENALTY=0.40)
    let active_score = base_score * 1.0; // no penalty
    let deprecated_score = base_score * ORPHAN_PENALTY; // 0.75 — orphan deprecated
    let superseded_score = base_score * CLEAN_REPLACEMENT_PENALTY; // 0.40 — depth-1 superseded

    assert!(
        active_score > deprecated_score,
        "active ({active_score:.4}) should beat orphan deprecated ({deprecated_score:.4})"
    );
    assert!(
        deprecated_score > superseded_score,
        "orphan deprecated ({deprecated_score:.4}) should beat clean-replacement superseded ({superseded_score:.4})"
    );
}

// ---------------------------------------------------------------------------
// T-RET-02b: Topology penalty ordering — behavioral assertions (crt-014)
// ---------------------------------------------------------------------------

#[test]
fn test_topology_penalty_behavioral_ordering() {
    // Verify constant ordering semantics without needing a live graph.
    // Higher multiplier = softer penalty (less suppression).
    // Ordering from harshest to softest (ADR-004, ADR-006):
    // CLEAN_REPLACEMENT_PENALTY (0.40) < PARTIAL_SUPERSESSION_PENALTY (0.60) <
    // FALLBACK_PENALTY (0.70) < ORPHAN_PENALTY (0.75) < 1.0 (active, no penalty)
    assert!(
        CLEAN_REPLACEMENT_PENALTY < PARTIAL_SUPERSESSION_PENALTY,
        "clean replacement ({CLEAN_REPLACEMENT_PENALTY}) must be harsher than \
         partial supersession ({PARTIAL_SUPERSESSION_PENALTY})"
    );
    assert!(
        PARTIAL_SUPERSESSION_PENALTY < FALLBACK_PENALTY,
        "partial supersession ({PARTIAL_SUPERSESSION_PENALTY}) must be harsher than \
         fallback ({FALLBACK_PENALTY})"
    );
    assert!(
        FALLBACK_PENALTY < ORPHAN_PENALTY,
        "fallback ({FALLBACK_PENALTY}) must be harsher than orphan ({ORPHAN_PENALTY})"
    );
    assert!(
        ORPHAN_PENALTY < 1.0,
        "orphan ({ORPHAN_PENALTY}) must be less than 1.0 (active entry, no penalty)"
    );
}

// ---------------------------------------------------------------------------
// T-RET-03: Provenance boost effect
// ---------------------------------------------------------------------------

#[test]
fn test_provenance_boost_effect() {
    let base_score = rerank_score(0.85, 0.60, 0.184);
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

    let cw = 0.184_f64; // initial confidence_weight
    let score_a = rerank_score(sim_a, conf, cw) * 1.0 + PROVENANCE_BOOST + 0.02; // active + provenance + co-access
    // crt-014: deprecated orphan entry receives ORPHAN_PENALTY (0.75), no boosts
    let score_b = rerank_score(sim_b, conf, cw) * ORPHAN_PENALTY; // deprecated orphan
    let score_c = rerank_score(sim_c, conf, cw) * 1.0; // active, no boosts

    assert!(
        score_a > score_c,
        "A ({score_a:.4}) with boosts should beat C ({score_c:.4}) without"
    );
    assert!(
        score_c > score_b,
        "C ({score_c:.4}) active should beat B ({score_b:.4}) deprecated"
    );
}
