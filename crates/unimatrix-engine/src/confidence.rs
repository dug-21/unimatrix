//! Confidence score computation for knowledge entries.
//!
//! Implements a six-component additive weighted composite formula that
//! transforms raw usage signals into a single quality score in [0.0, 1.0].
//! All component functions are pure (no I/O, deterministic given inputs).
//! Internal computation uses f64 for numerical stability (ADR-002).

use unimatrix_core::{EntryRecord, Status};

// -- Weight constants (stored factors must sum to exactly 0.92) --
//
// Six stored weights sum to exactly 0.92.
// The remaining 0.08 was previously reserved for co-access affinity (W_COAC)
// but was never integrated into stored confidence computation.
// Removed in crt-013 (dead code cleanup). See ADR-001.
//
// crt-019: W_BASE 0.18->0.16, W_USAGE 0.14->0.16, W_HELP 0.14->0.12,
//          W_TRUST 0.14->0.16. Sum remains 0.92 exactly (IEEE 754 verified).

/// Weight for base quality (status + trust_source dependent).
pub const W_BASE: f64 = 0.16;
/// Weight for usage frequency.
pub const W_USAGE: f64 = 0.16;
/// Weight for freshness (recency of access).
pub const W_FRESH: f64 = 0.18;
/// Weight for helpfulness (Bayesian Beta-Binomial posterior).
pub const W_HELP: f64 = 0.12;
/// Weight for correction chain quality.
pub const W_CORR: f64 = 0.14;
/// Weight for creator trust level.
pub const W_TRUST: f64 = 0.16;

/// Access counts beyond this contribute negligible signal.
pub const MAX_MEANINGFUL_ACCESS: f64 = 50.0;

/// Freshness half-life in hours (1 week).
pub const FRESHNESS_HALF_LIFE_HOURS: f64 = 168.0;

/// Cold-start default for Bayesian prior positive pseudo-votes.
///
/// Documentation constant — the value is passed as an argument to
/// `compute_confidence` and `helpfulness_score`, not read from this
/// constant in the formula itself.
pub const COLD_START_ALPHA: f64 = 3.0;

/// Cold-start default for Bayesian prior negative pseudo-votes.
///
/// Documentation constant — the value is passed as an argument to
/// `compute_confidence` and `helpfulness_score`, not read from this
/// constant in the formula itself.
pub const COLD_START_BETA: f64 = 3.0;

/// Query-time boost for `lesson-learned` category entries (col-010b).
/// Applied in search re-ranking alongside co-access affinity.
/// Does NOT modify the stored confidence formula invariant (0.92).
pub const PROVENANCE_BOOST: f64 = 0.02;

/// Cosine similarity between two f32 vectors, returned as f64 for scoring precision.
///
/// Returns 0.0 for zero-length, mismatched dimensions, or zero-norm vectors.
/// Result is clamped to [0.0, 1.0] to guard against floating-point edge cases.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot: f64 = 0.0;
    let mut norm_a_sq: f64 = 0.0;
    let mut norm_b_sq: f64 = 0.0;

    for i in 0..a.len() {
        let ai = a[i] as f64;
        let bi = b[i] as f64;
        dot += ai * bi;
        norm_a_sq += ai * ai;
        norm_b_sq += bi * bi;
    }

    let norm_a = norm_a_sq.sqrt();
    let norm_b = norm_b_sq.sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    let result = dot / (norm_a * norm_b);
    result.clamp(0.0, 1.0)
}

/// Base quality proxy from entry lifecycle status and trust source.
///
/// Active entries: "auto" source returns 0.35, all other sources return 0.5.
/// Proposed = 0.5 (ALWAYS, regardless of trust_source — preserves T-REG-01).
/// Deprecated = 0.2, Quarantined = 0.1.
///
/// The trust_source differentiation applies ONLY to Status::Active (ADR-003, C-03).
/// Uses exhaustive match so new Status variants cause a compile error.
pub fn base_score(status: Status, trust_source: &str) -> f64 {
    match status {
        Status::Active => {
            if trust_source == "auto" {
                0.35
            } else {
                0.5
            }
        }
        Status::Proposed => 0.5,
        Status::Deprecated => 0.2,
        Status::Quarantined => 0.1,
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

/// Parameters controlling all aspects of confidence computation.
///
/// `Default` reproduces the compiled constants exactly — the `collaborative`
/// preset and all code paths that do not configure a preset produce identical
/// results to the pre-dsn-001 binary.
///
/// The six `w_*` fields carry the per-domain weight vector set by the active
/// preset (or `custom` weights from `[confidence]`). W3-1 will add
/// `Option<LearnedWeights>` here without touching any call site that uses
/// `Default`.
///
/// Weight sum invariant: w_base + w_usage + w_fresh + w_help + w_corr + w_trust == 0.92
/// (tolerance: (sum - 0.92).abs() < 1e-9). Enforced by validate_config for custom
/// presets; asserted by the SR-10 test for named presets.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfidenceParams {
    /// Weight for base quality (status + trust_source). Default: W_BASE (0.16)
    pub w_base: f64,
    /// Weight for usage frequency. Default: W_USAGE (0.16)
    pub w_usage: f64,
    /// Weight for freshness (recency of access). Default: W_FRESH (0.18)
    pub w_fresh: f64,
    /// Weight for helpfulness (Bayesian posterior). Default: W_HELP (0.12)
    pub w_help: f64,
    /// Weight for correction chain quality. Default: W_CORR (0.14)
    pub w_corr: f64,
    /// Weight for creator trust level. Default: W_TRUST (0.16)
    pub w_trust: f64,
    /// Freshness half-life in hours. Default: FRESHNESS_HALF_LIFE_HOURS (168.0)
    pub freshness_half_life_hours: f64,
    /// Bayesian prior positive pseudo-votes. Default: COLD_START_ALPHA (3.0)
    pub alpha0: f64,
    /// Bayesian prior negative pseudo-votes. Default: COLD_START_BETA (3.0)
    pub beta0: f64,
}

impl Default for ConfidenceParams {
    fn default() -> Self {
        ConfidenceParams {
            w_base: W_BASE,
            w_usage: W_USAGE,
            w_fresh: W_FRESH,
            w_help: W_HELP,
            w_corr: W_CORR,
            w_trust: W_TRUST,
            freshness_half_life_hours: FRESHNESS_HALF_LIFE_HOURS,
            alpha0: COLD_START_ALPHA,
            beta0: COLD_START_BETA,
        }
    }
}

/// Exponential decay from reference timestamp, returning a value in [0.0, 1.0].
///
/// Uses `last_accessed_at` if > 0, otherwise falls back to `created_at`.
/// Returns 0.0 when both timestamps are 0 (no reference).
/// Returns 1.0 on clock skew (reference in the future).
/// Uses `params.freshness_half_life_hours` for the decay rate.
pub fn freshness_score(
    last_accessed_at: u64,
    created_at: u64,
    now: u64,
    params: &ConfidenceParams,
) -> f64 {
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
    // params.freshness_half_life_hours replaces the compiled FRESHNESS_HALF_LIFE_HOURS const.
    // When params == ConfidenceParams::default(), behavior is identical to pre-dsn-001.
    (-age_hours / params.freshness_half_life_hours).exp()
}

/// Helpfulness score using Bayesian Beta-Binomial posterior mean.
///
/// Returns `(helpful + alpha0) / (helpful + unhelpful + alpha0 + beta0)`,
/// clamped to [0.0, 1.0].
///
/// The `alpha0`/`beta0` parameters are the prior pseudo-vote counts. The
/// cold-start defaults are `COLD_START_ALPHA = 3.0` and `COLD_START_BETA = 3.0`,
/// which return 0.5 when no votes are present (symmetric neutral prior).
///
/// Unlike the previous Wilson score, this responds immediately to any vote
/// without a minimum sample size floor. The prior provides regularization.
///
/// u32 counts are cast to f64 before arithmetic to prevent overflow (EC-03).
pub fn helpfulness_score(helpful: u32, unhelpful: u32, alpha0: f64, beta0: f64) -> f64 {
    let h = helpful as f64;
    let u = unhelpful as f64;
    let total = h + u;

    // Bayesian posterior mean: (helpful + alpha0) / (total_votes + alpha0 + beta0)
    let score = (h + alpha0) / (total + alpha0 + beta0);

    // Clamp to [0.0, 1.0] as defense against degenerate prior inputs (R-12).
    // NaN inputs from a degenerate prior are guarded explicitly.
    if score.is_nan() {
        return 0.5;
    }
    score.clamp(0.0, 1.0)
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
        "neural" => 0.40,
        "auto" => 0.35,
        _ => 0.3,
    }
}

/// Compute confidence for an entry at the given timestamp.
///
/// Returns f64 in [0.0, 1.0]. All computation uses f64 natively.
/// The function is pure: given the same inputs, it always returns the same output.
///
/// `params.w_*` control the six weight factors; `params.alpha0`/`params.beta0` are
/// the Bayesian prior parameters for helpfulness scoring.
/// Use `&ConfidenceParams::default()` to reproduce the pre-dsn-001 behavior exactly.
pub fn compute_confidence(entry: &EntryRecord, now: u64, params: &ConfidenceParams) -> f64 {
    let b = base_score(entry.status, &entry.trust_source);
    let u = usage_score(entry.access_count);
    let f = freshness_score(entry.last_accessed_at, entry.created_at, now, params);
    let h = helpfulness_score(
        entry.helpful_count,
        entry.unhelpful_count,
        params.alpha0,
        params.beta0,
    );
    let c = correction_score(entry.correction_count);
    let t = trust_score(&entry.trust_source);

    // params.w_* replace the compiled weight constants W_BASE, W_USAGE, etc.
    // When params == ConfidenceParams::default(), the results are identical to
    // the pre-dsn-001 formula — behavioral backward compatibility is guaranteed.
    let composite = params.w_base * b
        + params.w_usage * u
        + params.w_fresh * f
        + params.w_help * h
        + params.w_corr * c
        + params.w_trust * t;

    composite.clamp(0.0, 1.0)
}

/// Blend similarity and confidence for search result re-ranking.
///
/// `confidence_weight` is the runtime-adaptive blend weight (from `ConfidenceState`).
/// `similarity_weight = 1.0 - confidence_weight`.
///
/// The `confidence_weight` is clamped to [0.15, 0.25] by `adaptive_confidence_weight`
/// upstream, so the result is in [0.0, 1.0] by construction.
pub fn rerank_score(similarity: f64, confidence: f64, confidence_weight: f64) -> f64 {
    let similarity_weight = 1.0 - confidence_weight;
    similarity_weight * similarity + confidence_weight * confidence
}

/// Compute adaptive blend weight from observed confidence spread.
///
/// Formula: `clamp(observed_spread * 1.25, 0.15, 0.25)`.
///
/// As the active confidence population spreads out (higher `observed_spread`),
/// the confidence dimension carries more weight in search re-ranking.
/// Clamped to [0.15, 0.25] to prevent extreme shifts.
pub fn adaptive_confidence_weight(observed_spread: f64) -> f64 {
    (observed_spread * 1.25).clamp(0.15, 0.25)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- T-01: Weight sum invariant (crt-005, crt-019) --

    #[test]
    fn weight_sum_invariant_f64() {
        let stored_sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
        assert_eq!(stored_sum, 0.92_f64, "stored weight sum should be 0.92");
    }

    #[test]
    fn weight_constants_values() {
        assert_eq!(W_BASE, 0.16_f64, "W_BASE");
        assert_eq!(W_USAGE, 0.16_f64, "W_USAGE");
        assert_eq!(W_FRESH, 0.18_f64, "W_FRESH");
        assert_eq!(W_HELP, 0.12_f64, "W_HELP");
        assert_eq!(W_CORR, 0.14_f64, "W_CORR");
        assert_eq!(W_TRUST, 0.16_f64, "W_TRUST");
    }

    // -- T-02: base_score two-parameter signature (AC-05, ADR-003) --

    #[test]
    fn base_score_active_agent() {
        assert_eq!(base_score(Status::Active, "agent"), 0.5);
    }

    #[test]
    fn base_score_active_human() {
        assert_eq!(base_score(Status::Active, "human"), 0.5);
    }

    #[test]
    fn base_score_active_system() {
        assert_eq!(base_score(Status::Active, "system"), 0.5);
    }

    #[test]
    fn base_score_active_auto() {
        assert_eq!(base_score(Status::Active, "auto"), 0.35);
    }

    // R-10: Proposed + auto must still return 0.5 — ADR-003 constraint (T-REG-01 anchor)
    #[test]
    fn auto_proposed_base_score_unchanged() {
        assert_eq!(
            base_score(Status::Proposed, "auto"),
            0.5,
            "Proposed/auto must retain 0.5 to preserve T-REG-01 ordering"
        );
    }

    #[test]
    fn base_score_deprecated_any_trust() {
        assert_eq!(base_score(Status::Deprecated, "auto"), 0.2);
        assert_eq!(base_score(Status::Deprecated, "human"), 0.2);
    }

    #[test]
    fn base_score_quarantined_any_trust() {
        assert_eq!(base_score(Status::Quarantined, "auto"), 0.1);
        assert_eq!(base_score(Status::Quarantined, "human"), 0.1);
    }

    // Active/auto strictly less than Active/agent (drives AC-12)
    #[test]
    fn base_score_auto_less_than_agent_for_active() {
        assert!(base_score(Status::Active, "auto") < base_score(Status::Active, "agent"));
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
        let result = freshness_score(now, now, now, &ConfidenceParams::default());
        assert!((result - 1.0).abs() < 0.001);
    }

    #[test]
    fn freshness_one_week_ago() {
        let now = 1_000_000u64;
        let one_week_ago = now - 168 * 3600;
        let result = freshness_score(one_week_ago, 0, now, &ConfidenceParams::default());
        assert!((result - 0.3679).abs() < 0.01);
    }

    #[test]
    fn freshness_fallback_to_created_at() {
        let now = 1_000_000u64;
        let result = freshness_score(0, now, now, &ConfidenceParams::default());
        assert!((result - 1.0).abs() < 0.001);
    }

    #[test]
    fn freshness_both_timestamps_zero() {
        let now = 1_000_000u64;
        assert_eq!(
            freshness_score(0, 0, now, &ConfidenceParams::default()),
            0.0
        );
    }

    #[test]
    fn freshness_clock_skew() {
        let now = 1_000_000u64;
        assert_eq!(
            freshness_score(now + 100, 0, now, &ConfidenceParams::default()),
            1.0
        );
    }

    #[test]
    fn freshness_very_old_entry() {
        let now = 100_000_000u64;
        let very_old = now - 365 * 24 * 3600;
        let result = freshness_score(very_old, 0, now, &ConfidenceParams::default());
        assert!(result >= 0.0 && result < 0.001);
    }

    // -- T-05: Bayesian helpfulness score (AC-02, replaces Wilson tests) --

    // AC-02 exact assertions — cold-start prior alpha0=3, beta0=3
    #[test]
    fn bayesian_helpfulness_cold_start_neutral() {
        // (0 + 3) / (0 + 3 + 3) = 3/6 = 0.5 exactly
        assert_eq!(helpfulness_score(0, 0, 3.0, 3.0), 0.5);
    }

    #[test]
    fn bayesian_helpfulness_two_unhelpful_votes() {
        // (0 + 3) / (2 + 3 + 3) = 3/8 = 0.375 exactly
        assert_eq!(helpfulness_score(0, 2, 3.0, 3.0), 0.375);
    }

    #[test]
    fn bayesian_helpfulness_balanced_votes_exact_half() {
        // (2 + 3) / (4 + 3 + 3) = 5/10 = 0.5 exactly
        // R-14: corrected from SCOPE which said > 0.5; SPEC AC-02 says == 0.5
        assert_eq!(helpfulness_score(2, 2, 3.0, 3.0), 0.5);
    }

    #[test]
    fn bayesian_helpfulness_two_helpful_votes_above_neutral() {
        // (2 + 3) / (2 + 3 + 3) = 5/8 = 0.625 > 0.5
        assert!(helpfulness_score(2, 0, 3.0, 3.0) > 0.5);
    }

    // Immediate responsiveness: 2 unhelpful votes lower the score below neutral
    // even without a 5-vote floor — confirms Wilson floor is gone
    #[test]
    fn bayesian_helpfulness_immediate_response_no_floor() {
        let score = helpfulness_score(0, 2, 3.0, 3.0);
        assert!(
            score < 0.5,
            "two unhelpful votes should lower score below 0.5, got {score}"
        );
    }

    // All helpful with large n — should be high but < 1.0
    #[test]
    fn bayesian_helpfulness_all_helpful_large_n() {
        let result = helpfulness_score(100, 0, 3.0, 3.0);
        assert!(
            result > 0.9,
            "100 helpful votes should give score > 0.9, got {result}"
        );
        assert!(result < 1.0);
    }

    // All unhelpful with large n — should approach 0 but clamped >= 0
    #[test]
    fn bayesian_helpfulness_all_unhelpful_large_n() {
        let result = helpfulness_score(0, 100, 3.0, 3.0);
        assert!(result >= 0.0);
        assert!(
            result < 0.1,
            "100 unhelpful votes should give score < 0.1, got {result}"
        );
    }

    // R-12 defense-in-depth: NaN inputs must not produce NaN output
    #[test]
    fn bayesian_helpfulness_nan_inputs_clamped() {
        let result = helpfulness_score(0, 0, f64::NAN, f64::NAN);
        assert!(!result.is_nan(), "NaN inputs must not produce NaN output");
        assert!(result >= 0.0 && result <= 1.0);
    }

    // EC-03: u32 counts must be cast to f64 before arithmetic
    #[test]
    fn bayesian_helpfulness_u32_max_does_not_overflow() {
        // u32::MAX as f64 is representable; addition in f64 space
        let result = helpfulness_score(u32::MAX, 0, 3.0, 3.0);
        assert!(
            result >= 0.0 && result <= 1.0,
            "result out of range: {result}"
        );
    }

    // Asymmetric prior test — non-default alpha0/beta0
    #[test]
    fn bayesian_helpfulness_asymmetric_prior() {
        // alpha0=2.0, beta0=8.0 → cold-start = 2/10 = 0.2; with 0,0 votes stays 0.2
        let score = helpfulness_score(0, 0, 2.0, 8.0);
        assert!(
            (score - 0.2).abs() < 1e-10,
            "expected 0.2 with alpha=2 beta=8, got {score}"
        );
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
        assert_eq!(trust_score("neural"), 0.40);
        assert_eq!(trust_score("auto"), 0.35);
        assert_eq!(trust_score(""), 0.3);
        assert_eq!(trust_score("unknown"), 0.3);
        assert_eq!(trust_score("Human"), 0.3); // case-sensitive
    }

    // -- col-013: trust_score("auto") dedicated tests --

    #[test]
    fn trust_score_auto_value() {
        assert!((trust_score("auto") - 0.35).abs() < f64::EPSILON);
    }

    #[test]
    fn trust_score_auto_between_agent_and_fallback() {
        let auto = trust_score("auto");
        let agent = trust_score("agent");
        let fallback = trust_score("unknown");
        assert!(
            auto > fallback,
            "auto ({auto}) should be > fallback ({fallback})"
        );
        assert!(auto < agent, "auto ({auto}) should be < agent ({agent})");
    }

    // -- crt-007: trust_score("neural") dedicated tests --

    #[test]
    fn trust_score_neural_value() {
        assert!((trust_score("neural") - 0.40).abs() < f64::EPSILON);
    }

    #[test]
    fn trust_score_neural_between_agent_and_auto() {
        let neural = trust_score("neural");
        let agent = trust_score("agent");
        let auto = trust_score("auto");
        assert!(neural > auto, "neural ({neural}) should be > auto ({auto})");
        assert!(
            neural < agent,
            "neural ({neural}) should be < agent ({agent})"
        );
    }

    // -- T-09: compute_confidence composite (AC-01, AC-02, crt-019) --

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
            pre_quarantine_status: None,
        }
    }

    #[test]
    fn compute_confidence_all_defaults_new_formula() {
        // Status::Active, trust_source="", all counts 0, timestamps 0
        // base_score(Active, "") = 0.5 (non-auto)
        // usage_score(0) = 0.0
        // freshness_score(0, 0, now) = 0.0
        // helpfulness_score(0, 0, 3.0, 3.0) = 0.5
        // correction_score(0) = 0.5
        // trust_score("") = 0.3
        // = 0.16*0.5 + 0.16*0.0 + 0.18*0.0 + 0.12*0.5 + 0.14*0.5 + 0.16*0.3
        // = 0.08 + 0.0 + 0.0 + 0.06 + 0.07 + 0.048 = 0.258
        let entry = make_test_entry(Status::Active, 0, 0, 0, 0, 0, 0, "");
        let result = compute_confidence(&entry, 1_000_000, &ConfidenceParams::default());
        let expected = 0.16 * 0.5 + 0.16 * 0.0 + 0.18 * 0.0 + 0.12 * 0.5 + 0.14 * 0.5 + 0.16 * 0.3;
        assert!(
            (result - expected).abs() < 0.001,
            "expected ~{expected:.4}, got {result:.4}"
        );
    }

    #[test]
    fn compute_confidence_all_max() {
        let now = 1_000_000u64;
        let entry = make_test_entry(Status::Active, 1000, now, now, 100, 0, 1, "human");
        let result = compute_confidence(&entry, now, &ConfidenceParams::default());
        assert!(result > 0.7, "expected > 0.7, got {result}");
        assert!(result <= 0.92, "expected <= 0.92, got {result}");
    }

    // Verify auto-active entry scores lower than agent-active entry (R-10 complement)
    #[test]
    fn compute_confidence_auto_active_lower_than_agent_active() {
        let now = 1_000_000u64;
        let auto_entry =
            make_test_entry(Status::Active, 20, now - 1000, now - 2000, 5, 1, 1, "auto");
        let agent_entry =
            make_test_entry(Status::Active, 20, now - 1000, now - 2000, 5, 1, 1, "agent");
        let conf_auto = compute_confidence(&auto_entry, now, &ConfidenceParams::default());
        let conf_agent = compute_confidence(&agent_entry, now, &ConfidenceParams::default());
        assert!(
            conf_auto < conf_agent,
            "auto active ({conf_auto:.4}) should be < agent active ({conf_agent:.4})"
        );
    }

    // -- T-10: compute_confidence range (AC-01, R-12) --

    #[test]
    fn compute_confidence_range_active_defaults() {
        let entry = make_test_entry(Status::Active, 0, 0, 0, 0, 0, 0, "");
        let result = compute_confidence(&entry, 1_000_000, &ConfidenceParams::default());
        assert!(result >= 0.0);
        assert!(result <= 1.0);
    }

    #[test]
    fn compute_confidence_range_deprecated_max_values() {
        let now = 1_000_000u64;
        let entry = make_test_entry(
            Status::Deprecated,
            u32::MAX,
            now,
            now,
            u32::MAX,
            0,
            100,
            "human",
        );
        let result = compute_confidence(&entry, now, &ConfidenceParams::default());
        assert!(result >= 0.0);
        assert!(result <= 1.0);
    }

    #[test]
    fn compute_confidence_range_extreme_timestamps() {
        let entry = make_test_entry(Status::Active, 0, u64::MAX, 0, 0, 0, 0, "agent");
        let result = compute_confidence(&entry, 0, &ConfidenceParams::default());
        assert!(result >= 0.0);
        assert!(result <= 1.0);
    }

    #[test]
    fn compute_confidence_range_all_unhelpful() {
        let entry = make_test_entry(Status::Active, 50, 0, 0, 0, u32::MAX, 0, "");
        let result = compute_confidence(&entry, 1_000_000, &ConfidenceParams::default());
        assert!(result >= 0.0);
        assert!(result <= 1.0);
    }

    // -- T-11: rerank_score three-parameter signature (AC-06) --

    #[test]
    fn rerank_score_both_max() {
        assert_eq!(rerank_score(1.0, 1.0, 0.15), 1.0);
    }

    #[test]
    fn rerank_score_both_zero() {
        assert_eq!(rerank_score(0.0, 0.0, 0.15), 0.0);
    }

    // Similarity-only case with floor confidence_weight (0.15)
    #[test]
    fn rerank_score_similarity_only_floor_weight() {
        // confidence_weight=0.15, similarity_weight=0.85
        let result = rerank_score(1.0, 0.0, 0.15);
        assert!((result - 0.85).abs() < f64::EPSILON);
    }

    // Similarity-only case with full confidence_weight (0.25)
    #[test]
    fn rerank_score_similarity_only_full_weight() {
        // confidence_weight=0.25, similarity_weight=0.75
        let result = rerank_score(1.0, 0.0, 0.25);
        assert!((result - 0.75).abs() < f64::EPSILON);
    }

    // Adaptive weight produces different result than fixed weight (R-02)
    #[test]
    fn rerank_score_adaptive_differs_from_fixed() {
        let fixed = rerank_score(0.9, 0.8, 0.15); // 0.85*0.9 + 0.15*0.8 = 0.885
        let adaptive = rerank_score(0.9, 0.8, 0.25); // 0.75*0.9 + 0.25*0.8 = 0.875
        assert_ne!(
            fixed, adaptive,
            "adaptive weight must produce different result than fixed"
        );
        assert!((fixed - 0.885).abs() < 1e-10, "fixed blend: {fixed}");
        assert!(
            (adaptive - 0.875).abs() < 1e-10,
            "adaptive blend: {adaptive}"
        );
    }

    #[test]
    fn rerank_score_confidence_tiebreaker() {
        // Higher confidence wins when similarity is equal
        assert!(rerank_score(0.90, 0.80, 0.15) > rerank_score(0.90, 0.20, 0.15));
    }

    // f64 precision round-trip
    #[test]
    fn rerank_score_f64_precision() {
        let sim = 0.123456789012345_f64;
        let conf = 0.987654321098765_f64;
        let cw = 0.25_f64;
        let result = rerank_score(sim, conf, cw);
        let expected = (1.0 - cw) * sim + cw * conf;
        assert_eq!(result, expected);
    }

    // -- T-NEW: adaptive_confidence_weight (AC-06) --

    #[test]
    fn adaptive_confidence_weight_at_target_spread() {
        // 0.20 * 1.25 = 0.25 — at full activation
        assert_eq!(adaptive_confidence_weight(0.20), 0.25);
    }

    #[test]
    fn adaptive_confidence_weight_floor() {
        // 0.10 * 1.25 = 0.125 < 0.15 — clamps to floor
        assert_eq!(adaptive_confidence_weight(0.10), 0.15);
    }

    #[test]
    fn adaptive_confidence_weight_cap() {
        // 0.30 * 1.25 = 0.375 > 0.25 — clamps to cap
        assert_eq!(adaptive_confidence_weight(0.30), 0.25);
    }

    #[test]
    fn adaptive_confidence_weight_initial_spread() {
        // Pre-crt-019 measured spread: 0.1471 * 1.25 = 0.183875
        // Between 0.15 and 0.25, so no clamping
        let result = adaptive_confidence_weight(0.1471);
        assert!(
            (result - 0.183875).abs() < 1e-10,
            "initial spread weight: {result}"
        );
        assert!(result > 0.15 && result < 0.25);
    }

    #[test]
    fn adaptive_confidence_weight_zero_spread() {
        // 0.0 * 1.25 = 0.0 — clamps to floor
        assert_eq!(adaptive_confidence_weight(0.0), 0.15);
    }

    #[test]
    fn adaptive_confidence_weight_one_spread() {
        // 1.0 * 1.25 = 1.25 — clamps to cap
        assert_eq!(adaptive_confidence_weight(1.0), 0.25);
    }

    // -- crt-005: f64 scoring precision tests --

    #[test]
    fn compute_confidence_f64_precision() {
        let now = 1_000_000u64;
        let entry = make_test_entry(
            Status::Active,
            500_000,
            now - 1000,
            now - 500,
            50,
            10,
            2,
            "agent",
        );
        let confidence = compute_confidence(&entry, now, &ConfidenceParams::default());
        assert!(
            confidence >= 0.0 && confidence <= 1.0,
            "confidence out of range: {confidence}"
        );
        let as_f32 = confidence as f32;
        let back_to_f64 = as_f32 as f64;
        let _: f64 = confidence;
        let _ = back_to_f64;
    }

    #[test]
    fn compute_confidence_high_inputs_in_range() {
        let now = 1_000_000u64;
        let entry = make_test_entry(Status::Active, 1000, now, now, 100, 0, 1, "human");
        let confidence = compute_confidence(&entry, now, &ConfidenceParams::default());
        assert!(
            confidence >= 0.0 && confidence <= 1.0,
            "confidence out of range: {confidence}"
        );
        assert!(
            confidence > 0.5,
            "high inputs should give confidence > 0.5, got {confidence}"
        );
    }

    #[test]
    fn compute_confidence_minimal_inputs_positive() {
        let now = 1_000_000u64;
        let entry = make_test_entry(Status::Active, 0, 0, 0, 0, 0, 0, "");
        let confidence = compute_confidence(&entry, now, &ConfidenceParams::default());
        assert!(
            confidence >= 0.0 && confidence <= 1.0,
            "confidence out of range: {confidence}"
        );
        let _: f64 = confidence;
    }

    // -- col-010b: PROVENANCE_BOOST tests (T-PB-01..04) --

    #[test]
    fn provenance_boost_value() {
        assert_eq!(PROVENANCE_BOOST, 0.02);
    }

    #[test]
    fn provenance_boost_less_than_scalar_boost_max() {
        // ADR-005: PROVENANCE_BOOST must be smaller than scalar co-access boost max (~0.03)
        assert!(PROVENANCE_BOOST < 0.03);
    }

    #[test]
    fn provenance_boost_score_difference() {
        // AC-09: lesson-learned vs convention with identical similarity and confidence
        let sim = 0.8;
        let conf = 0.6;
        let base = rerank_score(sim, conf, 0.15);
        let boosted_score = base + PROVENANCE_BOOST;
        assert!(
            (boosted_score - base - 0.02).abs() < f64::EPSILON,
            "boost should be exactly 0.02"
        );
    }

    #[test]
    fn provenance_boost_is_additive_tiebreaker() {
        // With identical scores, PROVENANCE_BOOST breaks the tie
        let sim = 0.9;
        let conf = 0.7;
        let base = rerank_score(sim, conf, 0.15);
        let boosted = base + PROVENANCE_BOOST;
        assert!(boosted > base);
        assert!((boosted - base - 0.02).abs() < f64::EPSILON);
    }

    #[test]
    fn weight_sum_invariant_is_0_92() {
        let stored_sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
        assert_eq!(
            stored_sum, 0.92_f64,
            "stored weight components must sum to 0.92"
        );
    }

    // -- crt-010: cosine_similarity tests (T-CS-01..08) --

    #[test]
    fn cosine_similarity_identical_normalized() {
        let v = vec![0.6_f32, 0.8];
        let result = cosine_similarity(&v, &v);
        assert!(
            (result - 1.0).abs() < 1e-6,
            "identical vectors should give ~1.0, got {result}"
        );
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        let result = cosine_similarity(&a, &b);
        assert!(
            result.abs() < 1e-6,
            "orthogonal vectors should give ~0.0, got {result}"
        );
    }

    #[test]
    fn cosine_similarity_zero_vector() {
        let a = vec![0.6_f32, 0.8];
        let b = vec![0.0_f32, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_similarity_mismatched_dimensions() {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![1.0_f32, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_similarity_empty_vectors() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_similarity_known_angle() {
        // 45 degrees: cos(pi/4) ~= 0.7071
        let a = vec![1.0_f32, 0.0];
        let b = vec![1.0_f32 / 2.0_f32.sqrt(), 1.0_f32 / 2.0_f32.sqrt()];
        let result = cosine_similarity(&a, &b);
        assert!(
            (result - 0.7071).abs() < 0.01,
            "expected ~0.7071, got {result}"
        );
    }

    #[test]
    fn cosine_similarity_returns_f64() {
        let a = vec![0.6_f32, 0.8];
        let b = vec![0.8_f32, 0.6];
        let result: f64 = cosine_similarity(&a, &b);
        assert!(result >= 0.0 && result <= 1.0);
    }

    #[test]
    fn cosine_similarity_clamped_for_denormalized() {
        // Large values that could produce > 1.0 due to floating point
        let a = vec![1e10_f32, 1e10];
        let b = vec![1e10_f32, 1e10];
        let result = cosine_similarity(&a, &b);
        assert!(
            result >= 0.0 && result <= 1.0,
            "result should be clamped, got {result}"
        );
    }

    // -- dsn-001: ConfidenceParams struct and load-bearing field tests --

    #[test]
    fn test_confidence_params_has_nine_fields() {
        // AC-27: If any field is missing or renamed, this test fails to compile.
        let _p = ConfidenceParams {
            w_base: 0.0,
            w_usage: 0.0,
            w_fresh: 0.0,
            w_help: 0.0,
            w_corr: 0.0,
            w_trust: 0.0,
            freshness_half_life_hours: 0.0,
            alpha0: 0.0,
            beta0: 0.0,
        };
    }

    #[test]
    fn test_confidence_params_default_values() {
        // AC-22: Default must reproduce compiled constants exactly.
        let p = ConfidenceParams::default();
        assert!((p.w_base - 0.16).abs() < 1e-9, "w_base  must be 0.16");
        assert!((p.w_usage - 0.16).abs() < 1e-9, "w_usage must be 0.16");
        assert!((p.w_fresh - 0.18).abs() < 1e-9, "w_fresh must be 0.18");
        assert!((p.w_help - 0.12).abs() < 1e-9, "w_help  must be 0.12");
        assert!((p.w_corr - 0.14).abs() < 1e-9, "w_corr  must be 0.14");
        assert!((p.w_trust - 0.16).abs() < 1e-9, "w_trust must be 0.16");
        assert!(
            (p.freshness_half_life_hours - 168.0).abs() < 1e-9,
            "freshness_half_life_hours must be 168.0"
        );
        assert!((p.alpha0 - 3.0).abs() < 1e-9, "alpha0 must be 3.0");
        assert!((p.beta0 - 3.0).abs() < 1e-9, "beta0  must be 3.0");
    }

    #[test]
    fn test_confidence_params_default_weight_sum_invariant() {
        // ConfidenceParams::default() sum must equal 0.92 (tolerance 1e-9).
        let p = ConfidenceParams::default();
        let sum = p.w_base + p.w_usage + p.w_fresh + p.w_help + p.w_corr + p.w_trust;
        assert!(
            (sum - 0.92).abs() < 1e-9,
            "ConfidenceParams::default() weight sum must equal 0.92, got {sum:.12}"
        );
    }

    #[test]
    fn test_compute_confidence_uses_params_w_fresh() {
        // R-01: compute_confidence must use params.w_fresh, not compiled W_FRESH.
        // A compiled-constant implementation would return identical scores.
        let now = 1_000_000u64;
        let age_hours = 48u64;
        let last = now - age_hours * 3600;
        let entry = make_test_entry(Status::Active, 10, last, last, 5, 0, 1, "agent");

        let params_default = ConfidenceParams::default(); // w_fresh = 0.18
        let params_empirical = ConfidenceParams {
            w_fresh: 0.34,
            ..Default::default()
        };

        let score_default = compute_confidence(&entry, now, &params_default);
        let score_empirical = compute_confidence(&entry, now, &params_empirical);

        assert!(
            (score_default - score_empirical).abs() > 0.01,
            "compute_confidence must use params.w_fresh; got near-identical scores: \
             default={:.6}, empirical={:.6}",
            score_default,
            score_empirical
        );
    }

    #[test]
    fn test_freshness_score_uses_params_half_life() {
        // R-01: freshness_score must use params.freshness_half_life_hours.
        let now = 1_000_000u64;
        let one_hour_secs = 3600u64;
        let age_hours = 24.0_f64;
        let last = now - (age_hours as u64) * one_hour_secs;

        let params_default = ConfidenceParams::default(); // half_life = 168.0h
        let params_short = ConfidenceParams {
            freshness_half_life_hours: 24.0,
            ..Default::default()
        };

        let score_168 = freshness_score(last, last, now, &params_default);
        let score_24 = freshness_score(last, last, now, &params_short);

        // At 24h age with half_life=168h: score = exp(-24/168) ≈ 0.867
        // At 24h age with half_life=24h:  score = exp(-1)      ≈ 0.368
        assert!(
            (score_168 - score_24).abs() > 0.1,
            "freshness_score must use params.freshness_half_life_hours; \
             score_168h={:.6}, score_24h={:.6}",
            score_168,
            score_24
        );

        // Verify expected exponential decay ratio.
        let expected_ratio = (-24.0 / 168.0_f64).exp() / (-24.0 / 24.0_f64).exp();
        let actual_ratio = score_168 / score_24;
        assert!(
            (actual_ratio - expected_ratio).abs() < 0.001,
            "ratio mismatch: expected {:.6}, got {:.6}",
            expected_ratio,
            actual_ratio
        );
    }

    #[test]
    fn test_freshness_score_configurable_half_life() {
        // AC-04: freshness_score with configurable half life.
        let one_hour = 3600u64;
        let now = 10_000_000u64;
        let age_hours = 168u64; // 1 week old
        let last = now - age_hours * one_hour;

        let p_168 = ConfidenceParams::default(); // half_life = 168h
        let p_24 = ConfidenceParams {
            freshness_half_life_hours: 24.0,
            ..Default::default()
        };

        let s_168 = freshness_score(last, last, now, &p_168);
        let s_24 = freshness_score(last, last, now, &p_24);

        // At 168h age with half_life=168h: score = exp(-1) ≈ 0.368 (one half-life elapsed).
        assert!(
            (s_168 - 0.368).abs() < 0.01,
            "168h old with 168h half_life must be ~0.368; got {:.6}",
            s_168
        );
        // At 168h age with half_life=24h: score = exp(-7) ≈ 0.0009 (7 half-lives).
        assert!(
            s_24 < 0.01,
            "168h old with 24h half_life must be near zero; got {:.6}",
            s_24
        );
        // The values must differ — compiled-constant impl would return same.
        assert!((s_168 - s_24).abs() > 0.3);
    }

    #[test]
    fn test_params_struct_update_syntax() {
        // Verify struct update syntax works (used in test migration pattern).
        let params = ConfidenceParams {
            w_trust: 0.22,
            ..Default::default()
        };
        assert!((params.w_trust - 0.22).abs() < 1e-9);
        // Other fields are unchanged from default.
        assert!((params.w_base - W_BASE).abs() < 1e-9);
        assert!((params.freshness_half_life_hours - FRESHNESS_HALF_LIFE_HOURS).abs() < 1e-9);
        // Score with modified w_trust must differ from default.
        let now = 1_000_000u64;
        let entry = make_test_entry(Status::Active, 10, now, now, 5, 0, 1, "human");
        let default_score = compute_confidence(&entry, now, &ConfidenceParams::default());
        let custom_score = compute_confidence(&entry, now, &params);
        assert_ne!(
            default_score, custom_score,
            "different w_trust must produce different score"
        );
    }
}
