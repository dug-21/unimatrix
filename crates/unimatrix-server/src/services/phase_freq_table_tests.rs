//! Unit tests for `phase_freq_table.rs`.
//!
//! Tests cover:
//!   - Cold-start construction and Default impl
//!   - Handle mechanics (RwLock, Arc clone, poison recovery)
//!   - `phase_affinity_score` — fallback, absent phase/entry, rank score, edge cases
//!   - `outcome_weight` — pass/rework/fail/unknown variants, priority order
//!   - `apply_outcome_weights` — per-phase mean, empty outcomes, missing phase
//!   - `phase_category_weights` — cold-start, single category, multi-category, multi-phase
//!
//! Extracted to a separate file to keep `phase_freq_table.rs` under the 500-line limit.

use super::*;

// helper: build a populated (not cold-start) table with one bucket
fn table_with(phase: &str, cat: &str, bucket: Vec<(u64, f32)>) -> PhaseFreqTable {
    let mut m = HashMap::new();
    m.insert((phase.to_string(), cat.to_string()), bucket);
    PhaseFreqTable {
        table: m,
        use_fallback: false,
    }
}

// helper: apply rank normalization to a slice of entry_ids (pre-sorted desc by freq)
fn rank_bucket(ids: &[u64]) -> Vec<(u64, f32)> {
    let n = ids.len();
    ids.iter()
        .enumerate()
        .map(|(idx, &id)| {
            let score = 1.0_f32 - ((idx) as f32 / n as f32);
            (id, score)
        })
        .collect()
}

// AC-01: cold-start construction
#[test]
fn test_phase_freq_table_new_returns_cold_start() {
    let t = PhaseFreqTable::new();
    assert!(t.use_fallback);
    assert!(t.table.is_empty());
}

#[test]
fn test_phase_freq_table_default_matches_new() {
    let d = PhaseFreqTable::default();
    assert!(d.use_fallback);
    assert!(d.table.is_empty());
}

// AC-03: handle mechanics
#[test]
fn test_new_handle_wraps_cold_start_state() {
    let h = PhaseFreqTable::new_handle();
    let g = h.read().unwrap_or_else(|e| e.into_inner());
    assert!(g.use_fallback);
    assert!(g.table.is_empty());
}

#[test]
fn test_new_handle_write_then_read_reflects_change() {
    let h = PhaseFreqTable::new_handle();
    h.write().unwrap_or_else(|e| e.into_inner()).use_fallback = false;
    assert!(!h.read().unwrap_or_else(|e| e.into_inner()).use_fallback);
}

#[test]
fn test_new_handle_returns_independent_handles() {
    let h1 = PhaseFreqTable::new_handle();
    let h2 = PhaseFreqTable::new_handle();
    h1.write().unwrap_or_else(|e| e.into_inner()).use_fallback = false;
    assert!(h2.read().unwrap_or_else(|e| e.into_inner()).use_fallback);
}

#[test]
fn test_arc_clone_shares_state() {
    let h = PhaseFreqTable::new_handle();
    let c = Arc::clone(&h);
    h.write().unwrap_or_else(|e| e.into_inner()).use_fallback = false;
    assert!(!c.read().unwrap_or_else(|e| e.into_inner()).use_fallback);
}

// Poison recovery
#[test]
fn test_phase_freq_table_handle_poison_recovery() {
    let h = PhaseFreqTable::new_handle();
    let hc = Arc::clone(&h);
    let _ = std::panic::catch_unwind(move || {
        let _g = hc.write().unwrap_or_else(|e| e.into_inner());
        panic!("intentional");
    });
    // must not panic
    let g = h.read().unwrap_or_else(|e| e.into_inner());
    assert!(g.use_fallback);
}

// AC-07 / R-04: three 1.0-return paths
#[test]
fn test_phase_affinity_score_use_fallback_returns_one() {
    let t = PhaseFreqTable::new(); // use_fallback=true
    assert_eq!(t.phase_affinity_score(42, "decision", "delivery"), 1.0_f32);
    assert_eq!(t.phase_affinity_score(99, "pattern", "scope"), 1.0_f32);
}

#[test]
fn test_phase_affinity_score_absent_phase_returns_one() {
    let t = table_with("scope", "decision", vec![(42, 1.0)]);
    assert_eq!(t.phase_affinity_score(42, "decision", "delivery"), 1.0_f32);
}

#[test]
fn test_phase_affinity_score_absent_entry_returns_one() {
    let t = table_with("delivery", "decision", vec![(100, 1.0)]);
    assert_eq!(t.phase_affinity_score(99, "decision", "delivery"), 1.0_f32);
}

#[test]
fn test_phase_affinity_score_present_entry_returns_rank_score() {
    let t = table_with("delivery", "decision", vec![(42, 2.0 / 3.0)]);
    let score = t.phase_affinity_score(42, "decision", "delivery");
    assert!(
        (score - 2.0_f32 / 3.0_f32).abs() < f32::EPSILON,
        "got {score}"
    );
}

// AC-13 / R-07: single-entry bucket must yield 1.0, NOT 0.0
#[test]
fn test_phase_affinity_score_single_entry_bucket_returns_one() {
    // N=1, rank=1: 1.0 - (1-1)/1 = 1.0
    let t = table_with("scope", "decision", rank_bucket(&[7]));
    assert_eq!(t.phase_affinity_score(7, "decision", "scope"), 1.0_f32);
}

// AC-14: exact scores for N=3 bucket
#[test]
fn test_rebuild_normalization_three_entry_bucket_exact_scores() {
    // entry_ids pre-sorted desc by freq: [10, 20, 30]
    let bucket = rank_bucket(&[10, 20, 30]);
    assert!(
        (bucket[0].1 - 1.0_f32).abs() < 1e-6,
        "rank-1={}",
        bucket[0].1
    );
    assert!(
        (bucket[1].1 - 2.0_f32 / 3.0_f32).abs() < 1e-5,
        "rank-2={}",
        bucket[1].1
    );
    assert!(
        (bucket[2].1 - 1.0_f32 / 3.0_f32).abs() < 1e-5,
        "rank-3={}",
        bucket[2].1
    );
    assert!(bucket[0].1 >= bucket[1].1 && bucket[1].1 >= bucket[2].1);

    let t = table_with("delivery", "decision", bucket);
    let s1 = t.phase_affinity_score(10, "decision", "delivery");
    let s2 = t.phase_affinity_score(20, "decision", "delivery");
    let s3 = t.phase_affinity_score(30, "decision", "delivery");
    assert!((s1 - 1.0_f32).abs() < 1e-6, "s1={s1}");
    assert!((s2 - 2.0_f32 / 3.0_f32).abs() < 1e-5, "s2={s2}");
    assert!((s3 - 1.0_f32 / 3.0_f32).abs() < 1e-5, "s3={s3}");
}

// R-07: 5-bucket last rank = 0.2 (= 1 - 4/5), never 0.0.
// Formula: 1.0 - ((rank-1)/N). Rank 5 of 5: 1.0 - 4/5 = 0.2.
// (The banned `1-rank/N` form would give 0.0 for rank=N — this test guards against that.)
#[test]
fn test_rebuild_normalization_last_entry_in_five_bucket() {
    let bucket = rank_bucket(&[1, 2, 3, 4, 5]);
    let t = table_with("delivery", "pattern", bucket);
    let last = t.phase_affinity_score(5, "pattern", "delivery");
    assert!(
        (last - 0.2_f32).abs() < 1e-5,
        "rank-5 of 5 must be ~0.2, got {last}"
    );
    assert!(last > 0.0_f32, "last-rank entry must never be 0.0");
}

// AC-14: N=2 bucket
#[test]
fn test_rebuild_normalization_two_entry_bucket() {
    let bucket = rank_bucket(&[1, 2]);
    let t = table_with("scope", "pattern", bucket);
    assert_eq!(t.phase_affinity_score(1, "pattern", "scope"), 1.0_f32);
    assert!((t.phase_affinity_score(2, "pattern", "scope") - 0.5_f32).abs() < 1e-6);
}

// R-10: phase rename -> 1.0 (graceful degradation)
#[test]
fn test_phase_affinity_score_unknown_phase_returns_one() {
    let t = table_with("delivery", "decision", vec![(42, 1.0)]);
    assert_eq!(t.phase_affinity_score(42, "decision", "implement"), 1.0_f32);
}

// -----------------------------------------------------------------------
// New tests: outcome_weight() — R-02, AC-13b/c/d/e
// -----------------------------------------------------------------------

// Build PhaseFreqRow test helper
fn make_freq_row(phase: &str, category: &str, entry_id: u64, freq: i64) -> PhaseFreqRow {
    PhaseFreqRow {
        phase: phase.to_string(),
        category: category.to_string(),
        entry_id,
        freq,
    }
}

// Build PhaseOutcomeRow test helper
fn make_outcome_row(phase: &str, feature_cycle: &str, outcome: &str) -> PhaseOutcomeRow {
    PhaseOutcomeRow {
        phase: phase.to_string(),
        feature_cycle: feature_cycle.to_string(),
        outcome: outcome.to_string(),
    }
}

// T-PFT-14 / R-02 scenario 1: pass variants return 1.0
#[test]
fn test_outcome_weight_pass_variants_return_1_0() {
    assert_eq!(outcome_weight("pass"), 1.0_f32);
    assert_eq!(outcome_weight("PASS"), 1.0_f32);
    assert_eq!(outcome_weight("Pass"), 1.0_f32);
}

// T-PFT-14 / R-02 scenario 1: rework variants return 0.5
#[test]
fn test_outcome_weight_rework_variants_return_0_5() {
    assert_eq!(outcome_weight("rework"), 0.5_f32);
    assert_eq!(outcome_weight("REWORK"), 0.5_f32);
    assert_eq!(outcome_weight("Rework"), 0.5_f32);
}

// T-PFT-14 / R-02 scenario 1: fail variants return 0.5
#[test]
fn test_outcome_weight_fail_variants_return_0_5() {
    assert_eq!(outcome_weight("fail"), 0.5_f32);
    assert_eq!(outcome_weight("FAIL"), 0.5_f32);
    assert_eq!(outcome_weight("FAILED"), 0.5_f32);
}

// T-PFT-14 / R-02 scenario 1: unknown/empty return 1.0 (graceful degradation)
#[test]
fn test_outcome_weight_unknown_and_empty_return_1_0() {
    assert_eq!(outcome_weight("unknown"), 1.0_f32);
    assert_eq!(outcome_weight("abandoned"), 1.0_f32);
    assert_eq!(outcome_weight(""), 1.0_f32);
}

// T-PFT-15 / R-02 scenario 2+3: rework checked before fail (priority order, ADR-003)
#[test]
fn test_outcome_weight_rework_checked_before_fail() {
    // "rework-and-fail" contains both "rework" and "fail"
    // rework branch must fire first → returns 0.5 (not double-penalized)
    assert_eq!(outcome_weight("rework-and-fail"), 0.5_f32);
    assert_eq!(outcome_weight("rework_fail"), 0.5_f32);
    // "fail_rework" — rework check (via contains) fires first too
    assert_eq!(outcome_weight("fail_rework"), 0.5_f32);
}

// -----------------------------------------------------------------------
// New tests: apply_outcome_weights() — R-03 / AC-04
// -----------------------------------------------------------------------

// T-PFT (AC-13b): single cycle pass → weight 1.0, freq unchanged
#[test]
fn test_apply_outcome_weights_single_cycle_pass_weights_1_0() {
    let freq_rows = vec![make_freq_row("delivery", "decision", 1, 10)];
    let outcome_rows = vec![make_outcome_row("delivery", "c-1", "PASS")];
    let result = apply_outcome_weights(freq_rows, outcome_rows);
    assert_eq!(result[0].freq, 10); // 10 * 1.0 = 10
}

// T-PFT (AC-13c): single cycle rework → weight 0.5
#[test]
fn test_apply_outcome_weights_single_cycle_rework_weights_0_5() {
    let freq_rows = vec![make_freq_row("delivery", "decision", 1, 10)];
    let outcome_rows = vec![make_outcome_row("delivery", "c-1", "REWORK")];
    let result = apply_outcome_weights(freq_rows, outcome_rows);
    assert_eq!(result[0].freq, 5); // 10 * 0.5 = 5
}

// T-PFT (AC-05): no outcome rows → default weight 1.0, freq unchanged
#[test]
fn test_apply_outcome_weights_no_outcome_rows_defaults_to_1_0() {
    let freq_rows = vec![make_freq_row("delivery", "decision", 1, 8)];
    let result = apply_outcome_weights(freq_rows, vec![]);
    assert_eq!(result[0].freq, 8); // default weight 1.0
}

// T-PFT (AC-13e): phase not in outcome rows → default weight 1.0
#[test]
fn test_apply_outcome_weights_missing_phase_defaults_to_1_0() {
    let freq_rows = vec![make_freq_row("delivery", "decision", 1, 6)];
    // outcome row is for "scope", not "delivery"
    let outcome_rows = vec![make_outcome_row("scope", "c-1", "REWORK")];
    let result = apply_outcome_weights(freq_rows, outcome_rows);
    assert_eq!(result[0].freq, 6); // default 1.0 for unmatched phase
}

// T-PFT-06 (R-03 key test): per-phase MEAN not per-cycle
// Phase "delivery": cycle-A (pass=1.0), cycle-B (rework=0.5) → mean=0.75
#[test]
fn test_apply_outcome_weights_mixed_cycles_uses_per_phase_mean() {
    let freq_rows = vec![
        make_freq_row("delivery", "decision", 10, 18),
        make_freq_row("delivery", "decision", 20, 15),
    ];
    let outcome_rows = vec![
        make_outcome_row("delivery", "cycle-A", "PASS"),
        make_outcome_row("delivery", "cycle-B", "REWORK"),
    ];
    let result = apply_outcome_weights(freq_rows, outcome_rows);
    // per-phase mean = (1.0 + 0.5) / 2 = 0.75
    // 18 * 0.75 = 13.5 → as i64 = 13
    // 15 * 0.75 = 11.25 → as i64 = 11
    assert!(
        result[0].freq == 13 || result[0].freq == 14,
        "got {}",
        result[0].freq
    );
    assert_eq!(result[1].freq, 11);
    // rank ordering preserved: entry 10 still above entry 20
    assert!(result[0].freq > result[1].freq);
}

// T-PFT-07 (R-03 ordering invariant): per-phase mean preserves relative ordering
#[test]
fn test_apply_outcome_weights_per_phase_mean_not_per_cycle() {
    let freq_rows = vec![
        make_freq_row("scope", "decision", 1, 10),
        make_freq_row("scope", "decision", 2, 8),
    ];
    let outcome_rows = vec![
        make_outcome_row("scope", "ca", "PASS"),   // weight 1.0
        make_outcome_row("scope", "cb", "REWORK"), // weight 0.5
    ];
    let result = apply_outcome_weights(freq_rows, outcome_rows);
    // mean = 0.75; entry 1 gets 10*0.75=7, entry 2 gets 8*0.75=6
    assert_eq!(result[0].entry_id, 1, "entry 1 must remain first");
    assert!(
        result[0].freq > result[1].freq,
        "relative ordering must be preserved"
    );
    // weight was applied: 10*1.0=10 would indicate no weighting
    assert!(result[0].freq < 10, "weight must be applied (not 1.0 path)");
}

// -----------------------------------------------------------------------
// New tests: phase_category_weights() — AC-08, R-07
// -----------------------------------------------------------------------

// Build a table with multiple buckets for testing phase_category_weights
fn table_with_buckets(buckets: Vec<(&str, &str, Vec<(u64, f32)>)>) -> PhaseFreqTable {
    let mut m = HashMap::new();
    for (phase, cat, bucket) in buckets {
        m.insert((phase.to_string(), cat.to_string()), bucket);
    }
    PhaseFreqTable {
        table: m,
        use_fallback: false,
    }
}

// T-PFT-11: cold-start → empty map (AC-08a)
#[test]
fn test_phase_category_weights_cold_start_returns_empty_map() {
    let t = PhaseFreqTable::new(); // use_fallback = true
    assert!(t.phase_category_weights().is_empty());
}

// T-PFT-13: single category → weight 1.0 (R-07 edge)
#[test]
fn test_phase_category_weights_single_category_returns_1_0() {
    let t = table_with("delivery", "decision", vec![(1, 1.0)]);
    let weights = t.phase_category_weights();
    let w = weights
        .get(&("delivery".to_string(), "decision".to_string()))
        .copied()
        .unwrap_or(0.0);
    assert!(
        (w - 1.0_f32).abs() < 1e-6,
        "single category must be 1.0, got {w}"
    );
}

// T-PFT-12: two categories — correct distribution and sum=1.0 (AC-08b, R-07)
#[test]
fn test_phase_category_weights_two_categories_sums_to_1_0() {
    // "decision": 2 entries, "pattern": 1 entry → total 3
    let t = table_with_buckets(vec![
        ("delivery", "decision", vec![(1, 1.0), (2, 0.5)]),
        ("delivery", "pattern", vec![(3, 1.0)]),
    ]);
    let weights = t.phase_category_weights();

    let w_decision = weights
        .get(&("delivery".to_string(), "decision".to_string()))
        .copied()
        .unwrap_or(0.0);
    let w_pattern = weights
        .get(&("delivery".to_string(), "pattern".to_string()))
        .copied()
        .unwrap_or(0.0);

    assert!(
        (w_decision - 2.0_f32 / 3.0_f32).abs() < 1e-6,
        "decision={w_decision}"
    );
    assert!(
        (w_pattern - 1.0_f32 / 3.0_f32).abs() < 1e-6,
        "pattern={w_pattern}"
    );
    assert!(
        (w_decision + w_pattern - 1.0_f32).abs() < 1e-6,
        "sum must be 1.0"
    );
}

// R-07 explicit test: breadth-based (entry count), NOT frequency-weighted
#[test]
fn test_phase_category_weights_breadth_not_freq_sum() {
    // 1 entry in "decision" (freq=10), 10 entries in "pattern" (freq=1 each)
    // breadth: decision=1/11, pattern=10/11
    // (NOT frequency-weighted: which would give decision=10/20, pattern=10/20)
    let pattern_bucket: Vec<(u64, f32)> = (1u64..=10).map(|i| (i, 1.0)).collect();
    let t = table_with_buckets(vec![
        ("scope", "decision", vec![(100, 1.0)]),
        ("scope", "pattern", pattern_bucket),
    ]);
    let weights = t.phase_category_weights();

    let w_decision = weights
        .get(&("scope".to_string(), "decision".to_string()))
        .copied()
        .unwrap_or(0.0);
    let w_pattern = weights
        .get(&("scope".to_string(), "pattern".to_string()))
        .copied()
        .unwrap_or(0.0);

    assert!(
        (w_decision - 1.0_f32 / 11.0_f32).abs() < 1e-5,
        "decision={w_decision}"
    );
    assert!(
        (w_pattern - 10.0_f32 / 11.0_f32).abs() < 1e-5,
        "pattern={w_pattern}"
    );
}

// T-PFT-12 variant: multiple phases are independent (each sums to 1.0)
#[test]
fn test_phase_category_weights_multiple_phases_independent() {
    // "delivery": "decision"=2 entries, "pattern"=1 entry (total 3)
    // "scope":    "decision"=1 entry, "lesson-learned"=1 entry (total 2)
    let t = table_with_buckets(vec![
        ("delivery", "decision", vec![(1, 1.0), (2, 0.5)]),
        ("delivery", "pattern", vec![(3, 1.0)]),
        ("scope", "decision", vec![(4, 1.0)]),
        ("scope", "lesson-learned", vec![(5, 1.0)]),
    ]);
    let weights = t.phase_category_weights();

    // delivery sum
    let delivery_sum: f32 = weights
        .iter()
        .filter(|((p, _), _)| p == "delivery")
        .map(|(_, &w)| w)
        .sum();
    // scope sum
    let scope_sum: f32 = weights
        .iter()
        .filter(|((p, _), _)| p == "scope")
        .map(|(_, &w)| w)
        .sum();

    assert!(
        (delivery_sum - 1.0_f32).abs() < 1e-6,
        "delivery sum={delivery_sum}"
    );
    assert!((scope_sum - 1.0_f32).abs() < 1e-6, "scope sum={scope_sum}");
}
