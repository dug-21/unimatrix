//! Hotspot detection framework with extensible rule trait.
//!
//! Ships 21 rules across 4 categories: agent (7), friction (4), session (5), scope (5).

pub mod agent;
pub mod friction;
pub mod scope;
pub mod session;

use crate::types::{HotspotCategory, HotspotFinding, MetricVector, ObservationRecord};

/// Trait for implementing hotspot detection rules.
///
/// Rules inspect observation records and produce findings when patterns exceed thresholds.
pub trait DetectionRule: Send {
    /// Unique rule name.
    fn name(&self) -> &str;
    /// Hotspot category this rule targets.
    fn category(&self) -> HotspotCategory;
    /// Analyze records and return findings.
    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding>;
}

/// Run all detection rules against a set of records.
///
/// Collects findings from all rules without short-circuiting.
pub fn detect_hotspots(
    records: &[ObservationRecord],
    rules: &[Box<dyn DetectionRule>],
) -> Vec<HotspotFinding> {
    let mut findings = Vec::new();
    for rule in rules {
        findings.extend(rule.detect(records));
    }
    findings
}

/// Return the default set of detection rules (21 total).
///
/// The `history` parameter provides historical MetricVectors for the
/// PhaseDurationOutlierRule (ADR-001: constructor injection).
pub fn default_rules(history: Option<&[MetricVector]>) -> Vec<Box<dyn DetectionRule>> {
    vec![
        // Friction (4)
        Box::new(friction::PermissionRetriesRule),
        Box::new(friction::SleepWorkaroundsRule),
        Box::new(friction::SearchViaBashRule),
        Box::new(friction::OutputParsingStruggleRule),
        // Session (5)
        Box::new(session::SessionTimeoutRule),
        Box::new(session::ColdRestartRule),
        Box::new(session::CoordinatorRespawnsRule),
        Box::new(session::PostCompletionWorkRule),
        Box::new(session::ReworkEventsRule),
        // Agent (7)
        Box::new(agent::ContextLoadRule),
        Box::new(agent::LifespanRule),
        Box::new(agent::FileBreadthRule),
        Box::new(agent::RereadRateRule),
        Box::new(agent::MutationSpreadRule),
        Box::new(agent::CompileCyclesRule),
        Box::new(agent::EditBloatRule),
        // Scope (5)
        Box::new(scope::SourceFileCountRule),
        Box::new(scope::DesignArtifactCountRule),
        Box::new(scope::AdrCountRule),
        Box::new(scope::PostDeliveryIssuesRule),
        Box::new(scope::PhaseDurationOutlierRule::new(history)),
    ]
}

// -- Helpers shared across detection modules --

/// Extract command string from tool input Value.
pub(crate) fn input_to_command_string(input: &serde_json::Value) -> String {
    match input {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Object(map) => map
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        _ => String::new(),
    }
}

/// Extract file_path from tool input Value (Read, Write, Edit tools).
pub(crate) fn input_to_file_path(input: &serde_json::Value) -> Option<String> {
    match input {
        serde_json::Value::Object(map) => map
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// Check if a string contains a sleep command (not part of another word).
pub(crate) fn contains_sleep_command(s: &str) -> bool {
    s.split(|c: char| c == ';' || c == '|' || c == '&' || c == '\n')
        .any(|segment| {
            let trimmed = segment.trim();
            trimmed.starts_with("sleep ") || trimmed == "sleep"
        })
}

/// Truncate a string to a maximum length, appending "..." if truncated.
pub(crate) fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Find the timestamp of the LAST TaskUpdate with "completed" status.
///
/// Shared by PostCompletionWorkRule and PostDeliveryIssuesRule.
pub(crate) fn find_completion_boundary(records: &[ObservationRecord]) -> Option<u64> {
    let mut last_completion_ts: Option<u64> = None;

    for record in records {
        if record.tool.as_deref() == Some("TaskUpdate") {
            if let Some(input) = &record.input {
                if let Some(status) = input.get("status").and_then(|v| v.as_str()) {
                    if status == "completed" {
                        match last_completion_ts {
                            None => last_completion_ts = Some(record.ts),
                            Some(prev) if record.ts > prev => {
                                last_completion_ts = Some(record.ts);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    last_completion_ts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EvidenceRecord, HookType, Severity};

    fn make_pre(ts: u64, tool: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some(tool.to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_post(ts: u64, tool: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PostToolUse,
            session_id: "sess-1".to_string(),
            tool: Some(tool.to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_bash_with_input(ts: u64, command: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Bash".to_string()),
            input: Some(serde_json::json!({"command": command})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_record_in_session(ts: u64, session: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: session.to_string(),
            tool: Some("Read".to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    // -- detect_hotspots engine --

    #[test]
    fn test_detect_hotspots_collects_from_all_rules() {
        let three_hours_ms = 3 * 60 * 60 * 1000;
        let records = vec![
            // Trigger permission retries
            make_pre(1000, "Read"),
            make_pre(2000, "Read"),
            make_pre(3000, "Read"),
            make_pre(4000, "Read"),
            make_pre(5000, "Read"),
            make_post(1500, "Read"),
            make_post(2500, "Read"),
            // Trigger session timeout
            make_record_in_session(10000, "sess-2"),
            make_record_in_session(10000 + three_hours_ms, "sess-2"),
            // Trigger sleep workaround
            make_bash_with_input(20000, "sleep 5"),
        ];

        let rules = default_rules(None);
        let findings = detect_hotspots(&records, &rules);
        assert!(
            findings.len() >= 3,
            "expected at least 3 findings, got {}",
            findings.len()
        );
    }

    // -- Custom rule test --

    struct CustomRule;
    impl DetectionRule for CustomRule {
        fn name(&self) -> &str {
            "custom_test"
        }
        fn category(&self) -> HotspotCategory {
            HotspotCategory::Agent
        }
        fn detect(&self, _records: &[ObservationRecord]) -> Vec<HotspotFinding> {
            vec![HotspotFinding {
                category: HotspotCategory::Agent,
                severity: Severity::Info,
                rule_name: "custom_test".to_string(),
                claim: "custom detection".to_string(),
                measured: 1.0,
                threshold: 0.0,
                evidence: vec![],
            }]
        }
    }

    #[test]
    fn test_custom_rule_engine_runs_it() {
        let rules: Vec<Box<dyn DetectionRule>> = vec![Box::new(CustomRule)];
        let findings = detect_hotspots(&[], &rules);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "custom_test");
        assert_eq!(findings[0].category, HotspotCategory::Agent);
    }

    // -- default_rules --

    #[test]
    fn test_default_rules_has_21_rules() {
        let rules = default_rules(None);
        assert_eq!(rules.len(), 21);
    }

    #[test]
    fn test_default_rules_names() {
        let rules = default_rules(None);
        let names: Vec<&str> = rules.iter().map(|r| r.name()).collect();
        assert!(names.contains(&"permission_retries"));
        assert!(names.contains(&"session_timeout"));
        assert!(names.contains(&"sleep_workarounds"));
        assert!(names.contains(&"context_load"));
        assert!(names.contains(&"lifespan"));
        assert!(names.contains(&"file_breadth"));
        assert!(names.contains(&"reread_rate"));
        assert!(names.contains(&"mutation_spread"));
        assert!(names.contains(&"compile_cycles"));
        assert!(names.contains(&"edit_bloat"));
        assert!(names.contains(&"search_via_bash"));
        assert!(names.contains(&"output_parsing_struggle"));
        assert!(names.contains(&"cold_restart"));
        assert!(names.contains(&"coordinator_respawns"));
        assert!(names.contains(&"post_completion_work"));
        assert!(names.contains(&"rework_events"));
        assert!(names.contains(&"source_file_count"));
        assert!(names.contains(&"design_artifact_count"));
        assert!(names.contains(&"adr_count"));
        assert!(names.contains(&"post_delivery_issues"));
        assert!(names.contains(&"phase_duration_outlier"));
    }

    #[test]
    fn test_default_rules_with_history() {
        use crate::types::PhaseMetrics;
        use std::collections::BTreeMap;

        let mut phases = BTreeMap::new();
        phases.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: 100,
                tool_call_count: 10,
            },
        );

        let mvs: Vec<MetricVector> = (0..3)
            .map(|_| MetricVector {
                computed_at: 0,
                universal: Default::default(),
                phases: phases.clone(),
            })
            .collect();

        let rules = default_rules(Some(&mvs));
        assert_eq!(rules.len(), 21);
    }

    // -- Helper tests --

    #[test]
    fn test_input_to_file_path_object() {
        let input = serde_json::json!({"file_path": "/tmp/test.rs"});
        assert_eq!(input_to_file_path(&input), Some("/tmp/test.rs".to_string()));
    }

    #[test]
    fn test_input_to_file_path_missing() {
        let input = serde_json::json!({"command": "cargo test"});
        assert_eq!(input_to_file_path(&input), None);
    }

    #[test]
    fn test_input_to_file_path_non_object() {
        let input = serde_json::json!("just a string");
        assert_eq!(input_to_file_path(&input), None);
    }

    #[test]
    fn test_contains_sleep_standalone() {
        assert!(contains_sleep_command("sleep 5"));
        assert!(contains_sleep_command("sleep"));
    }

    #[test]
    fn test_contains_sleep_in_pipeline() {
        assert!(contains_sleep_command("echo x; sleep 5"));
        assert!(contains_sleep_command("echo x | sleep 5"));
        assert!(contains_sleep_command("echo x && sleep 5"));
    }

    #[test]
    fn test_contains_sleep_not_standalone() {
        assert!(!contains_sleep_command("sleeping"));
        assert!(!contains_sleep_command("nosleep"));
    }

    #[test]
    fn test_contains_sleep_empty() {
        assert!(!contains_sleep_command(""));
    }

    #[test]
    fn test_find_completion_boundary_found() {
        let records = vec![ObservationRecord {
            ts: 5000,
            hook: HookType::PreToolUse,
            session_id: "s1".to_string(),
            tool: Some("TaskUpdate".to_string()),
            input: Some(serde_json::json!({"status": "completed", "taskId": "1"})),
            response_size: None,
            response_snippet: None,
        }];
        assert_eq!(find_completion_boundary(&records), Some(5000));
    }

    #[test]
    fn test_find_completion_boundary_last_used() {
        let records = vec![
            ObservationRecord {
                ts: 3000,
                hook: HookType::PreToolUse,
                session_id: "s1".to_string(),
                tool: Some("TaskUpdate".to_string()),
                input: Some(serde_json::json!({"status": "completed", "taskId": "1"})),
                response_size: None,
                response_snippet: None,
            },
            ObservationRecord {
                ts: 8000,
                hook: HookType::PreToolUse,
                session_id: "s1".to_string(),
                tool: Some("TaskUpdate".to_string()),
                input: Some(serde_json::json!({"status": "completed", "taskId": "2"})),
                response_size: None,
                response_snippet: None,
            },
        ];
        assert_eq!(find_completion_boundary(&records), Some(8000));
    }

    #[test]
    fn test_find_completion_boundary_not_found() {
        let records = vec![make_pre(1000, "Read")];
        assert_eq!(find_completion_boundary(&records), None);
    }

    #[test]
    fn test_find_completion_boundary_non_completed() {
        let records = vec![ObservationRecord {
            ts: 5000,
            hook: HookType::PreToolUse,
            session_id: "s1".to_string(),
            tool: Some("TaskUpdate".to_string()),
            input: Some(serde_json::json!({"status": "in_progress", "taskId": "1"})),
            response_size: None,
            response_snippet: None,
        }];
        assert_eq!(find_completion_boundary(&records), None);
    }
}
