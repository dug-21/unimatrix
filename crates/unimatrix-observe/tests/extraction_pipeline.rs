//! Extraction pipeline integration tests.
//!
//! Tests rule firing, quality gate, and neural enhancer behavior.

use unimatrix_learn::models::{ConventionScorer, SignalClassifier};
use unimatrix_observe::extraction::neural::{EnhancerMode, NeuralEnhancer};
use unimatrix_observe::extraction::{
    ExtractionContext, ProposedEntry, QualityGateResult, default_extraction_rules, quality_gate,
    run_extraction_rules,
};
use unimatrix_observe::types::ObservationRecord;
use unimatrix_store::test_helpers::TestDb;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_search_obs(session_id: &str, query: &str) -> ObservationRecord {
    ObservationRecord {
        ts: 1_700_000_000_000,
        event_type: "PostToolUse".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: session_id.to_string(),
        tool: Some("mcp__unimatrix__context_search".to_string()),
        input: Some(serde_json::json!({"query": query})),
        response_size: Some(0),
        response_snippet: Some("No results found".to_string()),
    }
}

fn make_valid_entry() -> ProposedEntry {
    ProposedEntry {
        title: "Valid long enough title for gate".to_string(),
        content: "This is valid content with enough length for the quality gate pipeline checks"
            .to_string(),
        category: "convention".to_string(),
        topic: "test-extraction".to_string(),
        tags: vec!["auto-extracted".to_string()],
        source_rule: "knowledge-gap".to_string(),
        source_features: vec!["session-1".to_string(), "session-2".to_string()],
        extraction_confidence: 0.5,
    }
}

// ---------------------------------------------------------------------------
// T-EXT-01: Rule firing with seeded observations
// ---------------------------------------------------------------------------

#[test]
fn test_extraction_rules_fire() {
    let db = TestDb::new();

    // Seed observations matching knowledge-gap pattern:
    // Same query searched in 2+ distinct sessions with zero results
    let observations = vec![
        make_search_obs("session-alpha", "deployment rollback procedure"),
        make_search_obs("session-beta", "deployment rollback procedure"),
        make_search_obs("session-gamma", "deployment rollback procedure"),
    ];

    let rules = default_extraction_rules();
    let proposals = run_extraction_rules(&observations, db.store(), &rules);

    assert!(
        !proposals.is_empty(),
        "expected at least one proposal from seeded observations"
    );

    // Verify the proposal comes from knowledge-gap rule
    let gap_proposals: Vec<_> = proposals
        .iter()
        .filter(|p| p.source_rule == "knowledge-gap")
        .collect();
    assert!(
        !gap_proposals.is_empty(),
        "expected knowledge-gap rule to fire"
    );
}

// ---------------------------------------------------------------------------
// T-EXT-02: Quality gate accepts valid entry
// ---------------------------------------------------------------------------

#[test]
fn test_quality_gate_accepts_valid() {
    let entry = make_valid_entry();
    let mut ctx = ExtractionContext::new();
    let result = quality_gate(&entry, &mut ctx);
    assert_eq!(
        result,
        QualityGateResult::Accept,
        "valid entry should be accepted"
    );
}

// ---------------------------------------------------------------------------
// T-EXT-03: Quality gate rejects short title
// ---------------------------------------------------------------------------

#[test]
fn test_quality_gate_rejects_short_title() {
    let mut entry = make_valid_entry();
    entry.title = "Short".to_string(); // 5 chars, min is 10

    let mut ctx = ExtractionContext::new();
    let result = quality_gate(&entry, &mut ctx);
    match result {
        QualityGateResult::Reject { check_name, .. } => {
            assert_eq!(check_name, "content_validation");
        }
        QualityGateResult::Accept => panic!("short title should be rejected"),
    }
}

// ---------------------------------------------------------------------------
// T-EXT-04: Quality gate rejects insufficient features
// ---------------------------------------------------------------------------

#[test]
fn test_quality_gate_rejects_insufficient_features() {
    let mut entry = make_valid_entry();
    entry.source_rule = "implicit-convention".to_string();
    entry.source_features = vec!["s1".to_string(), "s2".to_string()]; // needs 3

    let mut ctx = ExtractionContext::new();
    let result = quality_gate(&entry, &mut ctx);
    match result {
        QualityGateResult::Reject { check_name, .. } => {
            assert_eq!(check_name, "cross_feature");
        }
        QualityGateResult::Accept => panic!("insufficient features should be rejected"),
    }
}

// ---------------------------------------------------------------------------
// T-EXT-05: Neural enhancer shadow mode
// ---------------------------------------------------------------------------

#[test]
fn test_neural_enhancer_shadow_mode() {
    let enhancer = NeuralEnhancer::new(
        SignalClassifier::new_with_baseline(),
        ConventionScorer::new_with_baseline(),
        EnhancerMode::Shadow,
    );

    let entry = make_valid_entry();
    let prediction = enhancer.enhance(&entry);

    // Prediction produced
    assert!(prediction.convention_score >= 0.0);
    assert!(prediction.convention_score <= 1.0);
    assert_eq!(prediction.classification.probabilities.len(), 5);

    // Mode is shadow
    assert_eq!(enhancer.mode(), EnhancerMode::Shadow);

    // Entry is unchanged (original values still valid)
    assert_eq!(entry.title, "Valid long enough title for gate");
    assert_eq!(entry.extraction_confidence, 0.5);
}

// ---------------------------------------------------------------------------
// T-EXT-06: Cross-rule feature minimums
// ---------------------------------------------------------------------------

#[test]
fn test_cross_rule_feature_minimums() {
    // Verify documented minimum feature counts by testing quality gate rejections
    let base_entry = make_valid_entry();
    // Note: "dead-knowledge" was removed from the extraction pipeline (GH #351).
    // It now falls through to the default minimum (3) in min_features_for_rule.
    let test_cases = vec![
        ("knowledge-gap", 2),
        ("implicit-convention", 3),
        ("recurring-friction", 3),
        ("file-dependency", 3),
    ];

    for (rule_name, min_features) in test_cases {
        // Entry with one fewer than minimum features should be rejected
        let mut insufficient = base_entry.clone();
        insufficient.source_rule = rule_name.to_string();
        insufficient.source_features = (0..min_features - 1)
            .map(|i| format!("session-{i}"))
            .collect();

        let mut ctx = ExtractionContext::new();
        let result = quality_gate(&insufficient, &mut ctx);
        assert!(
            matches!(result, QualityGateResult::Reject { .. }),
            "rule '{rule_name}' should reject {}/{min_features} features",
            min_features - 1
        );

        // Entry with exactly minimum features should pass cross-feature check
        let mut sufficient = base_entry.clone();
        sufficient.source_rule = rule_name.to_string();
        sufficient.source_features = (0..min_features).map(|i| format!("session-{i}")).collect();

        let mut ctx2 = ExtractionContext::new();
        let result2 = quality_gate(&sufficient, &mut ctx2);
        assert_eq!(
            result2,
            QualityGateResult::Accept,
            "rule '{rule_name}' should accept {min_features}/{min_features} features"
        );
    }
}
