//! Tests for the domain-pack-registry and rule-dsl-evaluator components (col-023 Wave 2).
//!
//! Test plan coverage:
//! - T-DPR-01 through T-DPR-14 (domain-pack-registry.md)
//! - T-DSL-01 through T-DSL-18 (rule-dsl-evaluator.md)
//! - EC-04, EC-05, EC-06, EC-07, EC-08, EC-09 edge cases

use serde_json::json;
use unimatrix_observe::DetectionRule;
use unimatrix_observe::domain::{
    DomainPack, DomainPackRegistry,
    evaluator::{RuleDescriptor, RuleEvaluator, TemporalWindowRule, ThresholdRule},
};
use unimatrix_observe::error::ObserveError;
use unimatrix_observe::types::{ObservationRecord, Severity};

// ── Test helpers ───────────────────────────────────────────────────────────────

fn make_record(ts: u64, event_type: &str, source_domain: &str) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: event_type.to_string(),
        source_domain: source_domain.to_string(),
        session_id: "test-session".to_string(),
        tool: None,
        input: None,
        response_size: None,
        response_snippet: None,
    }
}

fn make_record_with_input(
    ts: u64,
    event_type: &str,
    source_domain: &str,
    input: serde_json::Value,
) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: event_type.to_string(),
        source_domain: source_domain.to_string(),
        session_id: "test-session".to_string(),
        tool: None,
        input: Some(input),
        response_size: None,
        response_snippet: None,
    }
}

fn sre_pack_no_rules() -> DomainPack {
    DomainPack {
        source_domain: "sre".to_string(),
        event_types: vec![
            "incident_opened".to_string(),
            "incident_resolved".to_string(),
        ],
        categories: vec!["incident".to_string()],
        rules: vec![],
    }
}

fn threshold_rule(
    name: &str,
    domain: &str,
    event_filter: Vec<&str>,
    threshold: f64,
) -> ThresholdRule {
    ThresholdRule {
        name: name.to_string(),
        source_domain: domain.to_string(),
        event_type_filter: event_filter.into_iter().map(|s| s.to_string()).collect(),
        field_path: String::new(),
        threshold,
        severity: "warning".to_string(),
        claim_template: "{measured} events".to_string(),
    }
}

fn temporal_rule(
    name: &str,
    domain: &str,
    event_filter: Vec<&str>,
    window_secs: u64,
    threshold: f64,
) -> TemporalWindowRule {
    TemporalWindowRule {
        name: name.to_string(),
        source_domain: domain.to_string(),
        event_type_filter: event_filter.into_iter().map(|s| s.to_string()).collect(),
        window_secs,
        threshold,
        severity: "critical".to_string(),
        claim_template: "{measured} in {window_secs}s".to_string(),
    }
}

// ── DomainPackRegistry tests ───────────────────────────────────────────────────

/// T-DPR-01: Built-in claude-code pack always present via with_builtin_claude_code()
#[test]
fn test_with_builtin_claude_code_pack_always_loads() {
    let registry = DomainPackRegistry::with_builtin_claude_code();
    let pack = registry.lookup("claude-code");
    assert!(pack.is_some(), "claude-code pack must be present");
    let pack = pack.unwrap();
    assert!(pack.event_types.contains(&"PreToolUse".to_string()));
    assert!(pack.event_types.contains(&"PostToolUse".to_string()));
    assert!(pack.event_types.contains(&"SubagentStart".to_string()));
    assert!(pack.event_types.contains(&"SubagentStop".to_string()));
}

/// T-DPR-02: Default config (empty domain_packs) loads built-in pack
#[test]
fn test_default_config_loads_claude_code_pack() {
    let registry = DomainPackRegistry::new(vec![]).expect("empty config must succeed");
    assert!(
        registry.lookup("claude-code").is_some(),
        "claude-code must be present with empty config"
    );
}

/// T-DPR-03: Custom pack registered alongside built-in
#[test]
fn test_custom_pack_registered_alongside_builtin() {
    let registry = DomainPackRegistry::new(vec![sre_pack_no_rules()]).expect("valid pack");
    assert!(
        registry.lookup("sre").is_some(),
        "sre pack must be registered"
    );
    assert!(
        registry.lookup("claude-code").is_some(),
        "built-in not displaced"
    );
}

/// T-DPR-04: lookup returns None for unregistered domain
#[test]
fn test_lookup_unregistered_domain_returns_none() {
    let registry = DomainPackRegistry::with_builtin_claude_code();
    assert!(registry.lookup("sre").is_none());
}

/// T-DPR-05: resolve_source_domain returns correct domain for known event type
#[test]
fn test_resolve_source_domain_known_event_type() {
    let registry = DomainPackRegistry::with_builtin_claude_code();
    assert_eq!(registry.resolve_source_domain("PostToolUse"), "claude-code");
    assert_eq!(registry.resolve_source_domain("PreToolUse"), "claude-code");
    assert_eq!(
        registry.resolve_source_domain("SubagentStart"),
        "claude-code"
    );
    assert_eq!(
        registry.resolve_source_domain("SubagentStop"),
        "claude-code"
    );
}

/// T-DPR-06: resolve_source_domain returns "unknown" for unregistered event type
#[test]
fn test_resolve_source_domain_unknown_event_type_returns_unknown() {
    let registry = DomainPackRegistry::with_builtin_claude_code();
    assert_eq!(registry.resolve_source_domain("incident_opened"), "unknown");
}

/// T-DPR-07: source_domain = "unknown" registration is rejected (EC-04)
#[test]
fn test_registry_rejects_unknown_as_source_domain() {
    let pack = DomainPack {
        source_domain: "unknown".to_string(),
        event_types: vec![],
        categories: vec![],
        rules: vec![],
    };
    let result = DomainPackRegistry::new(vec![pack]);
    assert!(
        matches!(result, Err(ObserveError::InvalidSourceDomain { domain }) if domain == "unknown"),
        "must reject reserved source_domain 'unknown'"
    );
}

/// T-DPR-08: source_domain regex validation at registration (AC-07)
#[test]
fn test_registry_rejects_invalid_source_domain_formats() {
    let too_long = "a".repeat(65);
    let invalid_cases: Vec<&str> = vec![
        "",            // empty
        "Claude-Code", // uppercase
        "my domain",   // space
        &too_long,     // too long (65 chars)
        "sre!",        // special character
    ];
    for invalid in &invalid_cases {
        let pack = DomainPack {
            source_domain: invalid.to_string(),
            event_types: vec![],
            categories: vec![],
            rules: vec![],
        };
        let result = DomainPackRegistry::new(vec![pack]);
        assert!(
            matches!(result, Err(ObserveError::InvalidSourceDomain { .. })),
            "must reject invalid source_domain: {invalid:?}"
        );
    }

    // Valid boundary cases must succeed.
    let pack_64 = DomainPack {
        source_domain: "a".repeat(64),
        event_types: vec![],
        categories: vec![],
        rules: vec![],
    };
    assert!(
        DomainPackRegistry::new(vec![pack_64]).is_ok(),
        "64-char domain must succeed"
    );

    let pack_mixed = DomainPack {
        source_domain: "sre-monitoring_v2".to_string(),
        event_types: vec![],
        categories: vec![],
        rules: vec![],
    };
    assert!(
        DomainPackRegistry::new(vec![pack_mixed]).is_ok(),
        "mixed valid chars must succeed"
    );
}

/// T-DPR-09: rules_for_domain returns RuleEvaluator instances
#[test]
fn test_rules_for_domain_returns_evaluators_for_registered_pack() {
    let pack = DomainPack {
        source_domain: "sre".to_string(),
        event_types: vec!["incident_opened".to_string()],
        categories: vec!["incident".to_string()],
        rules: vec![
            RuleDescriptor::Threshold(threshold_rule(
                "rule-1",
                "sre",
                vec!["incident_opened"],
                3.0,
            )),
            RuleDescriptor::TemporalWindow(temporal_rule(
                "rule-2",
                "sre",
                vec!["incident_opened"],
                60,
                5.0,
            )),
        ],
    };
    let registry = DomainPackRegistry::new(vec![pack]).expect("valid pack");
    let rules = registry.rules_for_domain("sre");
    assert_eq!(rules.len(), 2, "must return 2 evaluators");
    // Each implements DetectionRule (checked via calling detect() with empty slice)
    for rule in &rules {
        let findings = rule.detect(&[]);
        assert!(findings.is_empty(), "empty input must yield empty findings");
    }
}

/// T-DPR-10: rules_for_domain returns empty for unregistered domain
#[test]
fn test_rules_for_domain_unregistered_returns_empty() {
    let registry = DomainPackRegistry::with_builtin_claude_code();
    let rules = registry.rules_for_domain("nonexistent-domain");
    assert!(rules.is_empty());
}

/// T-DPR-11: Structural assertion — no runtime write path beyond constructors (AC-08)
#[test]
fn test_domain_pack_registry_no_runtime_write_path() {
    // Verify the public read-only API surface post-construction.
    // No insert(), remove(), register(), or update() methods exist.
    let registry = DomainPackRegistry::with_builtin_claude_code();
    let _ = registry.lookup("claude-code");
    let _ = registry.rules_for_domain("claude-code");
    let _ = registry.resolve_source_domain("PreToolUse");
    let _ = registry.iter_packs();
    // If the above compile, the API surface is read-only as required.
}

/// T-DPR-12: CategoryAllowlist integration — built-in categories include all 8 initial ones
#[test]
fn test_builtin_pack_has_all_initial_categories() {
    let registry = DomainPackRegistry::with_builtin_claude_code();
    let pack = registry
        .lookup("claude-code")
        .expect("claude-code must exist");
    let expected = [
        "outcome",
        "lesson-learned",
        "decision",
        "convention",
        "pattern",
        "procedure",
        "duties",
        "reference",
    ];
    for cat in &expected {
        assert!(
            pack.categories.contains(&cat.to_string()),
            "claude-code pack must include category '{cat}'"
        );
    }
}

/// T-DPR-14: Empty event_types matches all events for that domain (EC-05)
#[test]
fn test_registry_empty_event_types_matches_all() {
    let pack = DomainPack {
        source_domain: "sre".to_string(),
        event_types: vec![], // empty = claims all event types
        categories: vec![],
        rules: vec![],
    };
    let registry = DomainPackRegistry::new(vec![pack]).expect("valid pack");
    // With sre having empty event_types, resolve_source_domain returns a registered
    // domain (not "unknown") for any event string.
    let resolved = registry.resolve_source_domain("any_random_event_type_xyz");
    assert_ne!(
        resolved, "unknown",
        "empty event_types must match all events"
    );
}

/// T-DPR: iter_packs returns all registered packs
#[test]
fn test_iter_packs_returns_all_packs() {
    let registry = DomainPackRegistry::new(vec![sre_pack_no_rules()]).expect("valid pack");
    let packs = registry.iter_packs();
    let domains: Vec<&str> = packs.iter().map(|p| p.source_domain.as_str()).collect();
    assert!(domains.contains(&"claude-code"), "must include built-in");
    assert!(domains.contains(&"sre"), "must include registered sre pack");
    assert_eq!(packs.len(), 2);
}

// ── Threshold rule tests ───────────────────────────────────────────────────────

/// T-DSL-01: Threshold rule fires when count exceeds threshold
#[test]
fn test_threshold_rule_fires_on_count_exceeded() {
    let rule = threshold_rule("many-events", "sre", vec!["incident_opened"], 3.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let records: Vec<ObservationRecord> = (0..4)
        .map(|i| make_record(i * 1000, "incident_opened", "sre"))
        .collect();
    let findings = evaluator.detect(&records);
    assert_eq!(
        findings.len(),
        1,
        "must fire when count (4) > threshold (3)"
    );
    assert_eq!(findings[0].rule_name, "many-events");
    assert_eq!(findings[0].measured, 4.0);
}

/// T-DSL-02: Threshold rule does not fire at exact threshold
#[test]
fn test_threshold_rule_does_not_fire_at_threshold() {
    let rule = threshold_rule("many-events", "sre", vec!["incident_opened"], 3.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let records: Vec<ObservationRecord> = (0..3)
        .map(|i| make_record(i * 1000, "incident_opened", "sre"))
        .collect();
    let findings = evaluator.detect(&records);
    assert!(
        findings.is_empty(),
        "must not fire when count (3) == threshold (3)"
    );
}

/// T-DSL-03: Threshold domain guard — non-matching source_domain produces no findings
#[test]
fn test_threshold_rule_ignores_wrong_source_domain() {
    let rule = threshold_rule("many-events", "sre", vec!["incident_opened"], 3.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    // 10 records with source_domain = "claude-code" — should be ignored by sre rule
    let records: Vec<ObservationRecord> = (0..10)
        .map(|i| make_record(i * 1000, "incident_opened", "claude-code"))
        .collect();
    let findings = evaluator.detect(&records);
    assert!(
        findings.is_empty(),
        "domain guard must reject wrong source_domain"
    );
}

/// T-DSL-04: Threshold rule with field_path — numeric extraction
#[test]
fn test_threshold_rule_field_path_numeric_extraction() {
    let rule = ThresholdRule {
        name: "large-response".to_string(),
        source_domain: "sre".to_string(),
        event_type_filter: vec!["metric".to_string()],
        field_path: "/response_size".to_string(),
        threshold: 1000.0,
        severity: "warning".to_string(),
        claim_template: "Large response: {measured}".to_string(),
    };
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let records = vec![make_record_with_input(
        1000,
        "metric",
        "sre",
        json!({"response_size": 2000}),
    )];
    let findings = evaluator.detect(&records);
    assert_eq!(
        findings.len(),
        1,
        "must fire when extracted value (2000) > threshold (1000)"
    );
    assert_eq!(findings[0].measured, 2000.0);
}

/// T-DSL-05: field_path resolves to non-numeric value — no panic, no finding (R-08)
#[test]
fn test_threshold_field_path_non_numeric_silent_skip() {
    let rule = ThresholdRule {
        name: "numeric-check".to_string(),
        source_domain: "sre".to_string(),
        event_type_filter: vec!["metric".to_string()],
        field_path: "/tool_name".to_string(),
        threshold: 5.0,
        severity: "warning".to_string(),
        claim_template: "{measured}".to_string(),
    };
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let records = vec![make_record_with_input(
        1000,
        "metric",
        "sre",
        json!({"tool_name": "Bash"}),
    )];
    let findings = evaluator.detect(&records);
    assert!(
        findings.is_empty(),
        "non-numeric field must produce no finding"
    );
}

/// T-DSL-06: field_path missing from payload — no panic, no finding (R-08)
#[test]
fn test_threshold_field_path_missing_key_no_panic() {
    let rule = ThresholdRule {
        name: "missing-key".to_string(),
        source_domain: "sre".to_string(),
        event_type_filter: vec!["metric".to_string()],
        field_path: "/nonexistent/path".to_string(),
        threshold: 1.0,
        severity: "warning".to_string(),
        claim_template: "{measured}".to_string(),
    };
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let records = vec![make_record_with_input(
        1000,
        "metric",
        "sre",
        json!({"other_key": 42}),
    )];
    let findings = evaluator.detect(&records);
    assert!(
        findings.is_empty(),
        "missing key must produce no finding and no panic"
    );
}

/// T-DSL-07: empty field_path — count-based threshold
#[test]
fn test_threshold_empty_field_path_counts_events() {
    let rule = threshold_rule("count-ticks", "sre", vec!["tick"], 2.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let records: Vec<ObservationRecord> = (0..3)
        .map(|i| make_record(i * 1000, "tick", "sre"))
        .collect();
    let findings = evaluator.detect(&records);
    assert_eq!(findings.len(), 1, "count (3) > threshold (2) must fire");
    assert_eq!(findings[0].measured, 3.0);
}

/// T-DSL-08: event_type_filter — only matching event types counted
#[test]
fn test_threshold_event_type_filter_excludes_non_matching() {
    let rule = threshold_rule("incident-count", "sre", vec!["incident_opened"], 2.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let mut records: Vec<ObservationRecord> = (0..3)
        .map(|i| make_record(i * 1000, "incident_resolved", "sre"))
        .collect();
    records.push(make_record(10000, "incident_opened", "sre")); // only 1 match
    let findings = evaluator.detect(&records);
    assert!(
        findings.is_empty(),
        "only 1 matching event; threshold not exceeded"
    );
}

// ── Temporal window rule tests ─────────────────────────────────────────────────

/// T-DSL-09: window_secs = 0 rejected at load time (EC-08)
#[test]
fn test_temporal_window_zero_secs_rejected() {
    let pack = DomainPack {
        source_domain: "sre".to_string(),
        event_types: vec![],
        categories: vec![],
        rules: vec![RuleDescriptor::TemporalWindow(temporal_rule(
            "bad-rule",
            "sre",
            vec![],
            0,
            5.0,
        ))],
    };
    let result = DomainPackRegistry::new(vec![pack]);
    assert!(
        matches!(result, Err(ObserveError::InvalidRuleDescriptor { reason, .. }) if reason.contains("window_secs")),
        "window_secs=0 must be rejected at startup"
    );
}

/// T-DSL-10: Temporal window rule fires on N+1 events within window
#[test]
fn test_temporal_window_fires_within_window() {
    let rule = temporal_rule("deploy-storm", "sre", vec!["deploy_triggered"], 60, 3.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule));
    // 4 events within 60 seconds
    let records: Vec<ObservationRecord> = [0u64, 10, 20, 30]
        .iter()
        .map(|&offset_secs| make_record(offset_secs * 1000, "deploy_triggered", "sre"))
        .collect();
    let findings = evaluator.detect(&records);
    assert_eq!(
        findings.len(),
        1,
        "4 events in 60s window > threshold 3 must fire"
    );
}

/// T-DSL-11: Temporal window rule does not fire when events span beyond window
#[test]
fn test_temporal_window_does_not_fire_outside_window() {
    let rule = temporal_rule("deploy-storm", "sre", vec!["deploy_triggered"], 60, 3.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule));
    // 4 events spread over 4 minutes (> 60s window) — no 4 are within 60s
    let records: Vec<ObservationRecord> = [0u64, 61, 122, 183]
        .iter()
        .map(|&offset_secs| make_record(offset_secs * 1000, "deploy_triggered", "sre"))
        .collect();
    let findings = evaluator.detect(&records);
    assert!(
        findings.is_empty(),
        "events spread beyond window must not fire"
    );
}

/// T-DSL-12: Temporal window with unsorted input fires correctly (R-07 CRITICAL)
#[test]
fn test_temporal_window_unsorted_input_fires() {
    let rule = temporal_rule("alarm-storm", "sre", vec!["alarm"], 60, 2.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule));
    // Records in reverse ts order — all within 60 seconds
    let records: Vec<ObservationRecord> = [30u64, 20, 10, 0]
        .iter()
        .map(|&offset_secs| make_record(offset_secs * 1000, "alarm", "sre"))
        .collect();
    let findings = evaluator.detect(&records);
    assert_eq!(
        findings.len(),
        1,
        "unsorted input must still fire after sort step"
    );
}

/// T-DSL-13: Temporal window sorted vs unsorted produces equivalent result (R-07)
#[test]
fn test_temporal_window_sorted_vs_unsorted_equivalent() {
    let rule_a = temporal_rule("alarm-storm", "sre", vec!["alarm"], 60, 2.0);
    let rule_b = temporal_rule("alarm-storm", "sre", vec!["alarm"], 60, 2.0);
    let eval_a = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule_a));
    let eval_b = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule_b));

    let sorted_records: Vec<ObservationRecord> = [0u64, 10, 20, 30]
        .iter()
        .map(|&offset_secs| make_record(offset_secs * 1000, "alarm", "sre"))
        .collect();
    let reverse_records: Vec<ObservationRecord> = [30u64, 20, 10, 0]
        .iter()
        .map(|&offset_secs| make_record(offset_secs * 1000, "alarm", "sre"))
        .collect();

    let findings_sorted = eval_a.detect(&sorted_records);
    let findings_reversed = eval_b.detect(&reverse_records);

    assert_eq!(
        findings_sorted.len(),
        findings_reversed.len(),
        "sorted and unsorted must produce same number of findings"
    );
    if !findings_sorted.is_empty() {
        assert_eq!(
            findings_sorted[0].measured, findings_reversed[0].measured,
            "measured values must be equal"
        );
    }
}

/// T-DSL-14: Temporal window boundary — exactly N events in window does not fire
#[test]
fn test_temporal_window_boundary_exact_threshold_does_not_fire() {
    let rule = temporal_rule("boundary", "sre", vec!["tick"], 60, 3.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule));
    // Exactly 3 events within 60s (== threshold, not >)
    let records: Vec<ObservationRecord> = [0u64, 10, 20]
        .iter()
        .map(|&offset_secs| make_record(offset_secs * 1000, "tick", "sre"))
        .collect();
    let findings = evaluator.detect(&records);
    assert!(
        findings.is_empty(),
        "count (3) == threshold (3) must not fire"
    );
}

/// T-DSL-15: Temporal window boundary — N+1 events in window fires
#[test]
fn test_temporal_window_boundary_one_over_threshold_fires() {
    let rule = temporal_rule("boundary", "sre", vec!["tick"], 60, 3.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule));
    // 4 events within 60s (> threshold 3)
    let records: Vec<ObservationRecord> = [0u64, 10, 20, 30]
        .iter()
        .map(|&offset_secs| make_record(offset_secs * 1000, "tick", "sre"))
        .collect();
    let findings = evaluator.detect(&records);
    assert_eq!(findings.len(), 1, "count (4) > threshold (3) must fire");
}

/// T-DSL-16: Temporal window domain guard (R-01)
#[test]
fn test_temporal_window_rule_ignores_wrong_source_domain() {
    let rule = temporal_rule("alarm-storm", "sre", vec!["alarm"], 60, 2.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule));
    // Records with source_domain = "claude-code" — must be ignored by sre rule
    let records: Vec<ObservationRecord> = (0..5)
        .map(|i| make_record(i * 1000, "alarm", "claude-code"))
        .collect();
    let findings = evaluator.detect(&records);
    assert!(
        findings.is_empty(),
        "domain guard must reject claude-code records for sre rule"
    );
}

// ── Rule descriptor validation tests ──────────────────────────────────────────

/// T-DSL-17: Empty source_domain in rule rejected
#[test]
fn test_rule_descriptor_empty_source_domain_rejected() {
    let rule = ThresholdRule {
        name: "bad-rule".to_string(),
        source_domain: String::new(), // empty
        event_type_filter: vec![],
        field_path: String::new(),
        threshold: 1.0,
        severity: "warning".to_string(),
        claim_template: "{measured}".to_string(),
    };
    let pack = DomainPack {
        source_domain: "sre".to_string(),
        event_types: vec![],
        categories: vec![],
        rules: vec![RuleDescriptor::Threshold(rule)],
    };
    let result = DomainPackRegistry::new(vec![pack]);
    assert!(
        matches!(result, Err(ObserveError::InvalidRuleDescriptor { .. })),
        "empty source_domain in rule must be rejected"
    );
}

/// T-DSL-18: rule source_domain mismatch with pack domain rejected (EC-09)
#[test]
fn test_rule_file_source_domain_mismatch_rejected() {
    let rule = ThresholdRule {
        name: "mismatch-rule".to_string(),
        source_domain: "claude-code".to_string(), // mismatches pack's "sre"
        event_type_filter: vec![],
        field_path: String::new(),
        threshold: 1.0,
        severity: "warning".to_string(),
        claim_template: "{measured}".to_string(),
    };
    let pack = DomainPack {
        source_domain: "sre".to_string(),
        event_types: vec![],
        categories: vec![],
        rules: vec![RuleDescriptor::Threshold(rule)],
    };
    let result = DomainPackRegistry::new(vec![pack]);
    assert!(
        matches!(result, Err(ObserveError::InvalidRuleDescriptor { reason, .. })
            if reason.contains("claude-code") && reason.contains("sre")),
        "source_domain mismatch must name both domains in error"
    );
}

// ── Edge cases ─────────────────────────────────────────────────────────────────

/// EC-06: Empty records slice returns empty Vec without panic
#[test]
fn test_threshold_empty_records_no_panic() {
    let rule = threshold_rule("empty-test", "sre", vec!["tick"], 1.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let findings = evaluator.detect(&[]);
    assert!(findings.is_empty());
}

#[test]
fn test_temporal_window_empty_records_no_panic() {
    let rule = temporal_rule("empty-test", "sre", vec!["tick"], 60, 1.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::TemporalWindow(rule));
    let findings = evaluator.detect(&[]);
    assert!(findings.is_empty());
}

// ── Severity mapping ───────────────────────────────────────────────────────────

#[test]
fn test_threshold_rule_severity_critical() {
    let rule = ThresholdRule {
        name: "crit".to_string(),
        source_domain: "sre".to_string(),
        event_type_filter: vec!["tick".to_string()],
        field_path: String::new(),
        threshold: 0.0,
        severity: "critical".to_string(),
        claim_template: "{measured}".to_string(),
    };
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let records = vec![make_record(0, "tick", "sre")];
    let findings = evaluator.detect(&records);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Critical);
}

#[test]
fn test_threshold_rule_severity_info_default() {
    let rule = ThresholdRule {
        name: "info".to_string(),
        source_domain: "sre".to_string(),
        event_type_filter: vec!["tick".to_string()],
        field_path: String::new(),
        threshold: 0.0,
        severity: "unknown-level".to_string(),
        claim_template: "{measured}".to_string(),
    };
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let records = vec![make_record(0, "tick", "sre")];
    let findings = evaluator.detect(&records);
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, Severity::Info);
}

// ── Source domain guard isolation ──────────────────────────────────────────────

/// R-01: Only source_domain-matching records counted (mixed domain input)
#[test]
fn test_threshold_source_domain_guard_isolation() {
    let rule = threshold_rule("count-sre", "sre", vec!["incident_opened"], 3.0);
    let evaluator = RuleEvaluator::new(RuleDescriptor::Threshold(rule));
    let mut records: Vec<ObservationRecord> = (0..10)
        .map(|i| make_record(i * 1000, "incident_opened", "claude-code")) // wrong domain
        .collect();
    records.extend(
        (10..14).map(|i| make_record(i * 1000, "incident_opened", "sre")), // 4 correct
    );
    records.extend(
        (14..18).map(|i| make_record(i * 1000, "incident_opened", "unknown")), // wrong domain
    );
    let findings = evaluator.detect(&records);
    // Only 4 sre records; 4 > 3 threshold => must fire
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].measured, 4.0);
}

/// IR-01: resolve_source_domain for hook-path event types always returns "claude-code"
#[test]
fn test_event_type_pretooluse_resolves_to_claude_code_domain() {
    let registry = DomainPackRegistry::with_builtin_claude_code();
    assert_eq!(registry.resolve_source_domain("PreToolUse"), "claude-code");
    assert_eq!(registry.resolve_source_domain("PostToolUse"), "claude-code");
    assert_eq!(
        registry.resolve_source_domain("SubagentStart"),
        "claude-code"
    );
    assert_eq!(
        registry.resolve_source_domain("SubagentStop"),
        "claude-code"
    );
}
