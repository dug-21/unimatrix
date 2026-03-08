# Pseudocode: regression-tests

## Purpose

Golden regression values that intentionally break when weights or formula change.

## File: tests/pipeline_regression.rs

```
use unimatrix_engine::test_scenarios::*;
use unimatrix_engine::confidence::*;

// T-REG-01: Golden confidence values (4 decimal places)
fn test_golden_confidence_values()
    let now = CANONICAL_NOW
    let expert = profile_to_entry_record(&expert_human_fresh(), 1, now)
    let good = profile_to_entry_record(&good_agent_entry(), 2, now)
    let auto = profile_to_entry_record(&auto_extracted_new(), 3, now)
    assert (compute_confidence(&expert, now) - EXPECTED_EXPERT).abs() < 0.0001
    assert (compute_confidence(&good, now) - EXPECTED_GOOD).abs() < 0.0001
    assert (compute_confidence(&auto, now) - EXPECTED_AUTO).abs() < 0.0001

// T-REG-02: Weight constants match expected
fn test_weight_constants()
    assert_eq W_BASE, 0.18
    assert_eq W_USAGE, 0.14
    assert_eq W_FRESH, 0.18
    assert_eq W_HELP, 0.14
    assert_eq W_CORR, 0.14
    assert_eq W_TRUST, 0.14
    assert (W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST - 0.92).abs() < 0.001

// T-REG-03: Ranking stability (tau = 1.0 against hardcoded ordering)
fn test_ranking_stability()
    let scenario = standard_ranking()
    compute confidences, rank by confidence
    assert_tau_above(&actual_ranking, &expected_ranking, 1.0)
```
