# Pseudocode: calibration-tests

## Purpose

Validate confidence formula calibration, signal ablation, and retrieval arithmetic.

## File: tests/pipeline_calibration.rs

```
use unimatrix_engine::test_scenarios::*;
use unimatrix_engine::confidence::*;

// T-CAL-01: standard_ranking ordering holds
fn test_standard_ranking_ordering()
    let scenario = standard_ranking()
    let records = scenario.entries.iter().enumerate()
        .map(|(i, p)| profile_to_entry_record(p, i as u64 + 1, scenario.now))
    assert_confidence_ordering(&records, &expected_ids, scenario.now)

// T-CAL-02: trust_source_ordering holds
fn test_trust_source_ordering()
    // Same pattern as T-CAL-01 with trust_source_ordering()

// T-CAL-03: freshness_dominance holds
fn test_freshness_dominance_ordering()
    // Same pattern with freshness_dominance()

// T-CAL-04: Weight sensitivity +/-10%
fn test_weight_sensitivity()
    let profiles = standard_ranking()
    let original_confidences = compute for each
    let original_ranking = sort by confidence
    for each weight index 0..6:
        perturb weight by +10%, recompute manually
        compare tau to original ranking
        assert tau > 0.6

// T-ABL-01..06: Signal ablation
fn test_signal_ablation_base()
fn test_signal_ablation_usage()
fn test_signal_ablation_freshness()
fn test_signal_ablation_helpfulness()
fn test_signal_ablation_correction()
fn test_signal_ablation_trust()
    // For each signal: create two entries identical except for this signal
    // Entry A maximizes signal, Entry B minimizes it
    // Assert A ranks above B
    // Also compute tau for full population with/without signal

// T-CAL-05: Boundary entries
fn test_boundary_entries()
    // All-zero entry: confidence in [0.0, 1.0]
    // All-max entry: confidence in [0.0, 1.0]
    // Single-signal entries: confidence in [0.0, 1.0]
```

## File: tests/pipeline_retrieval.rs

```
use unimatrix_engine::confidence::*;
use unimatrix_engine::coaccess::*;

// T-RET-01: rerank blend ordering
fn test_rerank_blend_ordering()
    // high_sim + mod_conf vs mod_sim + high_conf
    // Assert similarity-dominant entry wins (SEARCH_SIMILARITY_WEIGHT=0.85)

// T-RET-02: status penalty ordering
fn test_status_penalty_ordering()
    // Same base score, apply penalties
    // active (1.0) > deprecated (0.7) > superseded (0.5)

// T-RET-03: provenance boost effect
fn test_provenance_boost_effect()
    // Two entries with same rerank_score
    // One gets PROVENANCE_BOOST
    // Assert boosted > unboosted

// T-RET-04: co-access boost monotonic and capped
fn test_co_access_boost_monotonic_capped()
    // Compute boost for counts 0..50
    // Assert monotonically non-decreasing
    // Assert all <= MAX_CO_ACCESS_BOOST

// T-RET-05: combined interaction ordering
fn test_combined_interaction_ordering()
    // Create scenario with co-access + provenance + status penalty all active
    // Assert combined effect produces expected ordering
```
