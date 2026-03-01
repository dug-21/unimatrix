//! Hotspot detection framework with extensible rule trait.
//!
//! Ships 3 rules: PermissionRetries, SessionTimeout, SleepWorkarounds.

use std::collections::HashMap;

use crate::types::{
    EvidenceRecord, HookType, HotspotCategory, HotspotFinding, ObservationRecord, Severity,
};

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

/// Return the default set of detection rules.
pub fn default_rules() -> Vec<Box<dyn DetectionRule>> {
    vec![
        Box::new(PermissionRetriesRule),
        Box::new(SessionTimeoutRule),
        Box::new(SleepWorkaroundsRule),
    ]
}

// -- Rule 1: PermissionRetriesRule (FR-06.1) --

struct PermissionRetriesRule;

impl DetectionRule for PermissionRetriesRule {
    fn name(&self) -> &str {
        "permission_retries"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Friction
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut pre_counts: HashMap<String, u64> = HashMap::new();
        let mut post_counts: HashMap<String, u64> = HashMap::new();
        let mut evidence_records: HashMap<String, Vec<EvidenceRecord>> = HashMap::new();

        for record in records {
            if let Some(tool) = &record.tool {
                match record.hook {
                    HookType::PreToolUse => {
                        *pre_counts.entry(tool.clone()).or_default() += 1;
                        evidence_records
                            .entry(tool.clone())
                            .or_default()
                            .push(EvidenceRecord {
                                description: format!("PreToolUse for {tool}"),
                                ts: record.ts,
                                tool: Some(tool.clone()),
                                detail: format!("Pre-use event at ts={}", record.ts),
                            });
                    }
                    HookType::PostToolUse => {
                        *post_counts.entry(tool.clone()).or_default() += 1;
                    }
                    _ => {}
                }
            }
        }

        let threshold = 2.0;
        let mut findings = Vec::new();

        for (tool, pre_count) in &pre_counts {
            let post_count = post_counts.get(tool).copied().unwrap_or(0);
            let retries = pre_count.saturating_sub(post_count);
            if retries > threshold as u64 {
                findings.push(HotspotFinding {
                    category: HotspotCategory::Friction,
                    severity: Severity::Warning,
                    rule_name: "permission_retries".to_string(),
                    claim: format!(
                        "Tool '{tool}' had {retries} permission retries (Pre-Post differential)"
                    ),
                    measured: retries as f64,
                    threshold,
                    evidence: evidence_records.remove(tool).unwrap_or_default(),
                });
            }
        }

        findings
    }
}

// -- Rule 2: SessionTimeoutRule (FR-06.2) --

struct SessionTimeoutRule;

/// 2 hours in milliseconds.
const TIMEOUT_GAP_MS: u64 = 2 * 60 * 60 * 1000;

impl DetectionRule for SessionTimeoutRule {
    fn name(&self) -> &str {
        "session_timeout"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Session
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut by_session: HashMap<&str, Vec<&ObservationRecord>> = HashMap::new();
        for record in records {
            by_session
                .entry(&record.session_id)
                .or_default()
                .push(record);
        }

        let mut findings = Vec::new();

        for (session_id, session_records) in &by_session {
            let mut sorted = session_records.clone();
            sorted.sort_by_key(|r| r.ts);

            for window in sorted.windows(2) {
                let gap = window[1].ts.saturating_sub(window[0].ts);
                if gap > TIMEOUT_GAP_MS {
                    let gap_hours = gap as f64 / (1000.0 * 60.0 * 60.0);
                    findings.push(HotspotFinding {
                        category: HotspotCategory::Session,
                        severity: Severity::Warning,
                        rule_name: "session_timeout".to_string(),
                        claim: format!(
                            "Session '{session_id}' had a {gap_hours:.1}h gap"
                        ),
                        measured: gap_hours,
                        threshold: 2.0,
                        evidence: vec![
                            EvidenceRecord {
                                description: "Gap start".to_string(),
                                ts: window[0].ts,
                                tool: window[0].tool.clone(),
                                detail: "Last event before gap".to_string(),
                            },
                            EvidenceRecord {
                                description: "Gap end".to_string(),
                                ts: window[1].ts,
                                tool: window[1].tool.clone(),
                                detail: "First event after gap".to_string(),
                            },
                        ],
                    });
                }
            }
        }

        findings
    }
}

// -- Rule 3: SleepWorkaroundsRule (FR-06.3) --

struct SleepWorkaroundsRule;

impl DetectionRule for SleepWorkaroundsRule {
    fn name(&self) -> &str {
        "sleep_workarounds"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Friction
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut evidence = Vec::new();

        for record in records {
            if record.tool.as_deref() == Some("Bash") {
                if let Some(input) = &record.input {
                    let input_str = input_to_command_string(input);
                    if contains_sleep_command(&input_str) {
                        evidence.push(EvidenceRecord {
                            description: "Sleep command in Bash input".to_string(),
                            ts: record.ts,
                            tool: Some("Bash".to_string()),
                            detail: truncate(&input_str, 200),
                        });
                    }
                }
            }
        }

        if evidence.is_empty() {
            vec![]
        } else {
            let count = evidence.len();
            vec![HotspotFinding {
                category: HotspotCategory::Friction,
                severity: Severity::Info,
                rule_name: "sleep_workarounds".to_string(),
                claim: format!("Found {count} sleep workaround(s) in Bash commands"),
                measured: count as f64,
                threshold: 1.0,
                evidence,
            }]
        }
    }
}

// -- Helpers --

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

/// Check if a string contains a sleep command (not part of another word).
pub(crate) fn contains_sleep_command(s: &str) -> bool {
    s.split(|c: char| c == ';' || c == '|' || c == '&' || c == '\n')
        .any(|segment| {
            let trimmed = segment.trim();
            trimmed.starts_with("sleep ") || trimmed == "sleep"
        })
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookType;

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

    // -- PermissionRetriesRule --

    #[test]
    fn test_permission_retries_exceeds_threshold() {
        // 5 Pre + 2 Post for tool X -> retries=3, exceeds threshold of 2
        let records = vec![
            make_pre(1000, "Read"),
            make_pre(2000, "Read"),
            make_pre(3000, "Read"),
            make_pre(4000, "Read"),
            make_pre(5000, "Read"),
            make_post(1500, "Read"),
            make_post(2500, "Read"),
        ];

        let rule = PermissionRetriesRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 3.0);
        assert_eq!(findings[0].category, HotspotCategory::Friction);
        assert_eq!(findings[0].rule_name, "permission_retries");
    }

    #[test]
    fn test_permission_retries_equal_pre_post() {
        // 3 Pre + 3 Post -> no finding
        let records = vec![
            make_pre(1000, "Read"),
            make_pre(2000, "Read"),
            make_pre(3000, "Read"),
            make_post(1500, "Read"),
            make_post(2500, "Read"),
            make_post(3500, "Read"),
        ];

        let rule = PermissionRetriesRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_permission_retries_multiple_tools_one_exceeds() {
        let records = vec![
            make_pre(1000, "Read"),
            make_pre(2000, "Read"),
            make_pre(3000, "Read"),
            make_pre(4000, "Read"),
            make_pre(5000, "Read"),
            make_post(1500, "Read"),
            make_post(2500, "Read"),
            // Write tool: balanced
            make_pre(6000, "Write"),
            make_post(6500, "Write"),
        ];

        let rule = PermissionRetriesRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].claim.contains("Read"));
    }

    #[test]
    fn test_permission_retries_empty_records() {
        let rule = PermissionRetriesRule;
        let findings = rule.detect(&[]);
        assert!(findings.is_empty());
    }

    // -- SessionTimeoutRule --

    #[test]
    fn test_session_timeout_three_hour_gap() {
        let three_hours_ms = 3 * 60 * 60 * 1000;
        let records = vec![
            make_record_in_session(1000, "sess-1"),
            make_record_in_session(1000 + three_hours_ms, "sess-1"),
        ];

        let rule = SessionTimeoutRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > 2.9);
        assert_eq!(findings[0].category, HotspotCategory::Session);
    }

    #[test]
    fn test_session_timeout_one_hour_gap() {
        let one_hour_ms = 60 * 60 * 1000;
        let records = vec![
            make_record_in_session(1000, "sess-1"),
            make_record_in_session(1000 + one_hour_ms, "sess-1"),
        ];

        let rule = SessionTimeoutRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_session_timeout_empty_records() {
        let rule = SessionTimeoutRule;
        let findings = rule.detect(&[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_session_timeout_single_record() {
        let records = vec![make_record_in_session(1000, "sess-1")];
        let rule = SessionTimeoutRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    // -- SleepWorkaroundsRule --

    #[test]
    fn test_sleep_workaround_detected() {
        let records = vec![make_bash_with_input(1000, "sleep 5")];

        let rule = SleepWorkaroundsRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 1.0);
        assert_eq!(findings[0].category, HotspotCategory::Friction);
        assert_eq!(findings[0].rule_name, "sleep_workarounds");
    }

    #[test]
    fn test_sleep_workaround_in_pipeline() {
        let records = vec![make_bash_with_input(1000, "ls -la; sleep 10; echo done")];

        let rule = SleepWorkaroundsRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn test_sleep_workaround_no_bash() {
        let records = vec![make_pre(1000, "Read")];

        let rule = SleepWorkaroundsRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_sleep_workaround_bash_without_sleep() {
        let records = vec![make_bash_with_input(1000, "cargo test")];

        let rule = SleepWorkaroundsRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_sleep_workaround_multiple() {
        let records = vec![
            make_bash_with_input(1000, "sleep 5"),
            make_bash_with_input(2000, "sleep 10"),
            make_bash_with_input(3000, "sleep 1"),
        ];

        let rule = SleepWorkaroundsRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 3.0);
        assert_eq!(findings[0].evidence.len(), 3);
    }

    // -- contains_sleep_command helper --

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

        let rules = default_rules();
        let findings = detect_hotspots(&records, &rules);
        assert!(findings.len() >= 3, "expected at least 3 findings, got {}", findings.len());
    }

    // -- Custom rule test (AC-18, R-05) --

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
    fn test_default_rules_has_three_rules() {
        let rules = default_rules();
        assert_eq!(rules.len(), 3);
    }

    #[test]
    fn test_default_rules_names() {
        let rules = default_rules();
        let names: Vec<&str> = rules.iter().map(|r| r.name()).collect();
        assert!(names.contains(&"permission_retries"));
        assert!(names.contains(&"session_timeout"));
        assert!(names.contains(&"sleep_workarounds"));
    }
}
