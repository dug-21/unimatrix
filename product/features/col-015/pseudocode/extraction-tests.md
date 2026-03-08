# Pseudocode: extraction-tests

## Purpose

Validate extraction pipeline: rules, quality gate, neural enhancer.

## File: tests/extraction_pipeline.rs

```
use unimatrix_observe::extraction::*;
use unimatrix_observe::types::ObservationRecord;
use unimatrix_store::test_helpers::TestDb;

// T-EXT-01: Rule firing with seeded observations
fn test_extraction_rules_fire()
    let db = TestDb::new()
    // Seed store with observations spanning 3+ feature cycles
    // Seed entries that match knowledge-gap pattern (accessed but no knowledge stored)
    let rules = default_extraction_rules()
    let proposals = run_extraction_rules(&observations, db.store(), &rules)
    assert !proposals.is_empty()

// T-EXT-02: Quality gate accepts valid entry
fn test_quality_gate_accepts_valid()
    let entry = ProposedEntry {
        title: "Valid long enough title here",
        content: "Valid content with enough length for quality gate",
        category: "convention",
        source_rule: "knowledge-gap",
        source_features: vec!["s1", "s2"],
        extraction_confidence: 0.5,
        ...
    }
    let mut ctx = ExtractionContext::new()
    assert quality_gate(&entry, &mut ctx) == QualityGateResult::Accept

// T-EXT-03: Quality gate rejects short title
fn test_quality_gate_rejects_short_title()
    // title.len() < 10

// T-EXT-04: Quality gate rejects insufficient features
fn test_quality_gate_rejects_insufficient_features()
    // implicit-convention with only 2 features (needs 3)

// T-EXT-05: Neural enhancer shadow mode
fn test_neural_enhancer_shadow_mode()
    let enhancer = NeuralEnhancer::new(
        SignalClassifier::new_with_baseline(),
        ConventionScorer::new_with_baseline(),
        EnhancerMode::Shadow,
    )
    let entry = make_test_entry()
    let prediction = enhancer.enhance(&entry)
    // Assert prediction produced, entry unchanged
    assert prediction.convention_score >= 0.0
    assert enhancer.mode() == EnhancerMode::Shadow

// T-EXT-06: Cross-rule feature minimums
fn test_cross_rule_feature_minimums()
    assert min_features_for_rule("knowledge-gap") == 2
    assert min_features_for_rule("implicit-convention") == 3
    assert min_features_for_rule("recurring-friction") == 3
    assert min_features_for_rule("file-dependency") == 3
    assert min_features_for_rule("dead-knowledge") == 5
```
