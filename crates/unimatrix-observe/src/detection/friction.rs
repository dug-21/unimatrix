//! Friction hotspot detection rules (5 rules).
//!
//! 2 existing from col-002 (PermissionRetries, SleepWorkarounds) + 2 new (SearchViaBash, OutputParsingStruggle) + 1 new col-027 (ToolFailure).

use std::collections::{HashMap, HashSet};

use unimatrix_core::observation::hook_type;

use crate::types::{EvidenceRecord, HotspotCategory, HotspotFinding, ObservationRecord, Severity};

use super::{DetectionRule, contains_sleep_command, input_to_command_string, truncate};

// col-027: ToolFailureRule threshold.
// Fires when a single tool accumulates strictly more than this many PostToolUseFailure records.
// ADR-005: hardcoded constant for col-027; extract to named constant for future configurability.
const TOOL_FAILURE_THRESHOLD: u64 = 3;

// -- Rule 1: PermissionRetriesRule (moved from col-002 detection.rs) --

pub(crate) struct PermissionRetriesRule;

impl DetectionRule for PermissionRetriesRule {
    fn name(&self) -> &str {
        "permission_retries"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Friction
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let records: Vec<&ObservationRecord> = records
            .iter()
            .filter(|r| r.source_domain == "claude-code")
            .collect();

        let mut pre_counts: HashMap<String, u64> = HashMap::new();
        // CHANGED (col-027, ADR-004): renamed from post_counts to terminal_counts.
        // Semantics widened: both PostToolUse and PostToolUseFailure are terminal events.
        let mut terminal_counts: HashMap<String, u64> = HashMap::new();
        let mut evidence_records: HashMap<String, Vec<EvidenceRecord>> = HashMap::new();

        for record in &records {
            if let Some(tool) = &record.tool {
                if record.event_type == "PreToolUse" {
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
                } else if record.event_type == "PostToolUse" {
                    // unchanged: PostToolUse is a terminal event
                    *terminal_counts.entry(tool.clone()).or_default() += 1;
                } else if record.event_type == hook_type::POSTTOOLUSEFAILURE {
                    // NEW (col-027): PostToolUseFailure is also a terminal event.
                    // A failed call is still a resolved call — not a retried/blocked call.
                    *terminal_counts.entry(tool.clone()).or_default() += 1;
                }
            }
        }

        let threshold = 2.0;
        let mut findings = Vec::new();

        for (tool, pre_count) in &pre_counts {
            // CHANGED (col-027): use terminal_counts instead of post_counts
            let terminal_count = terminal_counts.get(tool).copied().unwrap_or(0);
            let retries = pre_count.saturating_sub(terminal_count);
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

// -- Rule 5: ToolFailureRule (col-027) --

// col-027: New rule to surface per-tool failure counts (ADR-005).
pub(crate) struct ToolFailureRule;

impl DetectionRule for ToolFailureRule {
    fn name(&self) -> &str {
        "tool_failure_hotspot" // ADR-005: canonical name; do not use "tool_failures"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Friction
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        // Pre-filter: source_domain == "claude-code" only (ADR-005, R-07)
        // Consistent with all other friction rules.
        let records: Vec<&ObservationRecord> = records
            .iter()
            .filter(|r| r.source_domain == "claude-code")
            .collect();

        // Count PostToolUseFailure records per tool and collect evidence.
        let mut failure_counts: HashMap<String, u64> = HashMap::new();
        let mut evidence_map: HashMap<String, Vec<EvidenceRecord>> = HashMap::new();

        for record in &records {
            // Only count PostToolUseFailure events (exact event_type match).
            if record.event_type != hook_type::POSTTOOLUSEFAILURE {
                continue;
            }
            // Skip records with no tool (tool_name absent in payload).
            let tool = match record.tool.as_ref() {
                Some(t) => t,
                None => continue,
            };

            *failure_counts.entry(tool.clone()).or_default() += 1;

            // Collect evidence: one EvidenceRecord per failure event (FR-07.7).
            evidence_map
                .entry(tool.clone())
                .or_default()
                .push(EvidenceRecord {
                    description: format!("PostToolUseFailure for {tool}"),
                    ts: record.ts,
                    tool: Some(tool.clone()),
                    // detail = response_snippet (the error string) if present (ADR-005).
                    detail: record.response_snippet.clone().unwrap_or_default(),
                });
        }

        // Emit one finding per tool that exceeds TOOL_FAILURE_THRESHOLD.
        // Threshold is STRICTLY greater than (ADR-005): fires at count > 3, i.e., 4+.
        let mut findings = Vec::new();
        for (tool, count) in &failure_counts {
            if *count > TOOL_FAILURE_THRESHOLD {
                findings.push(HotspotFinding {
                    category: HotspotCategory::Friction,
                    severity: Severity::Warning,
                    rule_name: "tool_failure_hotspot".to_string(),
                    // Claim format: "Tool 'X' failed N times" (FR-07.4).
                    claim: format!("Tool '{tool}' failed {count} times"),
                    measured: *count as f64,
                    threshold: TOOL_FAILURE_THRESHOLD as f64,
                    evidence: evidence_map.remove(tool).unwrap_or_default(),
                });
            }
        }

        findings
    }
}

// -- Rule 2: SleepWorkaroundsRule (moved from col-002 detection.rs) --

pub(crate) struct SleepWorkaroundsRule;

impl DetectionRule for SleepWorkaroundsRule {
    fn name(&self) -> &str {
        "sleep_workarounds"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Friction
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let records: Vec<&ObservationRecord> = records
            .iter()
            .filter(|r| r.source_domain == "claude-code")
            .collect();

        let mut evidence = Vec::new();

        for record in &records {
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

// -- Rule 3: SearchViaBashRule (FR-02.1) --

pub(crate) struct SearchViaBashRule;

const SEARCH_VIA_BASH_THRESHOLD_PCT: f64 = 5.0;

fn is_search_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    for segment in trimmed.split(|c: char| c == ';' || c == '\n') {
        let seg = segment.trim();
        if seg.starts_with("find ")
            || seg == "find"
            || seg.starts_with("grep ")
            || seg == "grep"
            || seg.starts_with("rg ")
            || seg == "rg"
            || seg.starts_with("ag ")
            || seg == "ag"
        {
            return true;
        }
    }
    false
}

impl DetectionRule for SearchViaBashRule {
    fn name(&self) -> &str {
        "search_via_bash"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Friction
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let records: Vec<&ObservationRecord> = records
            .iter()
            .filter(|r| r.source_domain == "claude-code")
            .collect();

        let mut total_bash = 0u64;
        let mut search_bash = 0u64;
        let mut evidence = Vec::new();

        for record in &records {
            if record.tool.as_deref() == Some("Bash") && record.event_type == "PreToolUse" {
                total_bash += 1;
                if let Some(input) = &record.input {
                    let cmd = input_to_command_string(input);
                    if is_search_command(&cmd) {
                        search_bash += 1;
                        evidence.push(EvidenceRecord {
                            description: "Search command via Bash".to_string(),
                            ts: record.ts,
                            tool: Some("Bash".to_string()),
                            detail: truncate(&cmd, 200),
                        });
                    }
                }
            }
        }

        if total_bash == 0 {
            return vec![];
        }

        let pct = (search_bash as f64 / total_bash as f64) * 100.0;
        if pct > SEARCH_VIA_BASH_THRESHOLD_PCT {
            vec![HotspotFinding {
                category: HotspotCategory::Friction,
                severity: Severity::Info,
                rule_name: "search_via_bash".to_string(),
                claim: format!(
                    "{pct:.1}% of Bash calls are search commands ({search_bash}/{total_bash})"
                ),
                measured: pct,
                threshold: SEARCH_VIA_BASH_THRESHOLD_PCT,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 4: OutputParsingStruggleRule (FR-02.2) --

pub(crate) struct OutputParsingStruggleRule;

const OUTPUT_PARSING_THRESHOLD: f64 = 2.0;
const OUTPUT_PARSING_WINDOW_MS: u64 = 3 * 60 * 1000; // 3 minutes

impl DetectionRule for OutputParsingStruggleRule {
    fn name(&self) -> &str {
        "output_parsing_struggle"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Friction
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let records: Vec<&ObservationRecord> = records
            .iter()
            .filter(|r| r.source_domain == "claude-code")
            .collect();

        // Collect Bash commands with pipes: (ts, base_command, full_command)
        let mut piped_commands: Vec<(u64, String, String)> = Vec::new();

        let mut sorted: Vec<&ObservationRecord> = records.clone();
        sorted.sort_by_key(|r| r.ts);

        for record in &sorted {
            if record.tool.as_deref() == Some("Bash") && record.event_type == "PreToolUse" {
                if let Some(input) = &record.input {
                    let cmd = input_to_command_string(input);
                    if cmd.contains('|') {
                        let base = cmd.split('|').next().unwrap_or("").trim().to_string();
                        if !base.is_empty() {
                            piped_commands.push((record.ts, base, cmd));
                        }
                    }
                }
            }
        }

        // Group by base command
        let mut by_base: HashMap<String, Vec<(u64, String)>> = HashMap::new();
        for (ts, base, full) in piped_commands {
            by_base.entry(base).or_default().push((ts, full));
        }

        let mut findings = Vec::new();
        let mut found_bases: HashSet<String> = HashSet::new();

        for (base, entries) in &by_base {
            let mut sorted_entries: Vec<&(u64, String)> = entries.iter().collect();
            sorted_entries.sort_by_key(|(ts, _)| *ts);

            for i in 0..sorted_entries.len() {
                if found_bases.contains(base) {
                    break;
                }

                let window_start = sorted_entries[i].0;
                let window_end = window_start + OUTPUT_PARSING_WINDOW_MS;

                let in_window: Vec<&(u64, String)> = sorted_entries
                    .iter()
                    .skip(i)
                    .take_while(|(ts, _)| *ts <= window_end)
                    .copied()
                    .collect();

                // Extract distinct filter suffixes (part after first pipe)
                let filters: HashSet<String> = in_window
                    .iter()
                    .filter_map(|(_, full)| {
                        let after_pipe = full
                            .split_once('|')
                            .map(|(_, rest)| rest.trim().to_string());
                        after_pipe
                    })
                    .collect();

                if filters.len() as f64 > OUTPUT_PARSING_THRESHOLD {
                    let evidence: Vec<EvidenceRecord> = in_window
                        .iter()
                        .map(|(ts, full)| EvidenceRecord {
                            description: "Piped command variation".to_string(),
                            ts: *ts,
                            tool: Some("Bash".to_string()),
                            detail: truncate(full, 200),
                        })
                        .collect();

                    findings.push(HotspotFinding {
                        category: HotspotCategory::Friction,
                        severity: Severity::Info,
                        rule_name: "output_parsing_struggle".to_string(),
                        claim: format!(
                            "Command '{}' piped through {} different filters within 3 minutes",
                            truncate(base, 60),
                            filters.len()
                        ),
                        measured: filters.len() as f64,
                        threshold: OUTPUT_PARSING_THRESHOLD,
                        evidence,
                    });
                    found_bases.insert(base.clone());
                }
            }
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pre(ts: u64, tool: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
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
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "sess-1".to_string(),
            tool: Some(tool.to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_failure(ts: u64, tool: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: hook_type::POSTTOOLUSEFAILURE.to_string(),
            source_domain: "claude-code".to_string(),
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
            event_type: "PreToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "sess-1".to_string(),
            tool: Some("Bash".to_string()),
            input: Some(serde_json::json!({"command": command})),
            response_size: None,
            response_snippet: None,
        }
    }

    // -- PermissionRetriesRule --

    #[test]
    fn test_permission_retries_exceeds_threshold() {
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
    }

    #[test]
    fn test_permission_retries_equal_pre_post() {
        let records = vec![
            make_pre(1000, "Read"),
            make_pre(2000, "Read"),
            make_pre(3000, "Read"),
            make_post(1500, "Read"),
            make_post(2500, "Read"),
            make_post(3500, "Read"),
        ];
        let rule = PermissionRetriesRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_permission_retries_empty_records() {
        let rule = PermissionRetriesRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- SleepWorkaroundsRule --

    #[test]
    fn test_sleep_workaround_detected() {
        let records = vec![make_bash_with_input(1000, "sleep 5")];
        let rule = SleepWorkaroundsRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 1.0);
    }

    #[test]
    fn test_sleep_workaround_no_sleep() {
        let records = vec![make_bash_with_input(1000, "cargo test")];
        let rule = SleepWorkaroundsRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_sleep_workaround_empty() {
        let rule = SleepWorkaroundsRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- SearchViaBashRule --

    #[test]
    fn test_search_via_bash_fires() {
        let mut records = Vec::new();
        // 18 non-search + 2 search = 10%
        for i in 0..18 {
            records.push(make_bash_with_input(i * 1000, "cargo test"));
        }
        records.push(make_bash_with_input(18000, "find . -name '*.rs'"));
        records.push(make_bash_with_input(19000, "grep -r 'pattern' ."));
        let rule = SearchViaBashRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > SEARCH_VIA_BASH_THRESHOLD_PCT);
    }

    #[test]
    fn test_search_via_bash_silent() {
        let records: Vec<ObservationRecord> = (0..20)
            .map(|i| make_bash_with_input(i * 1000, "cargo test"))
            .collect();
        let rule = SearchViaBashRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_search_via_bash_not_search() {
        let records = vec![make_bash_with_input(1000, "echo \"finding\"")];
        let rule = SearchViaBashRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_search_via_bash_empty() {
        let rule = SearchViaBashRule;
        assert!(rule.detect(&[]).is_empty());
    }

    #[test]
    fn test_is_search_command_variants() {
        assert!(is_search_command("find . -name '*.rs'"));
        assert!(is_search_command("grep -r pattern ."));
        assert!(is_search_command("rg pattern"));
        assert!(is_search_command("ag pattern"));
        assert!(!is_search_command("cargo test"));
        assert!(!is_search_command("echo finding"));
    }

    // -- OutputParsingStruggleRule --

    #[test]
    fn test_output_parsing_fires() {
        let records = vec![
            make_bash_with_input(1000, "cargo test | grep FAIL"),
            make_bash_with_input(2000, "cargo test | tail -20"),
            make_bash_with_input(3000, "cargo test | head -5"),
        ];
        let rule = OutputParsingStruggleRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > OUTPUT_PARSING_THRESHOLD);
    }

    #[test]
    fn test_output_parsing_different_base_commands() {
        let records = vec![
            make_bash_with_input(1000, "cargo test | grep FAIL"),
            make_bash_with_input(2000, "cargo build | tail -20"),
        ];
        let rule = OutputParsingStruggleRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_output_parsing_same_filter() {
        let records = vec![
            make_bash_with_input(1000, "cargo test | grep FAIL"),
            make_bash_with_input(2000, "cargo test | grep FAIL"),
        ];
        let rule = OutputParsingStruggleRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_output_parsing_outside_window() {
        let records = vec![
            make_bash_with_input(1000, "cargo test | grep FAIL"),
            make_bash_with_input(1000 + 4 * 60 * 1000, "cargo test | tail -20"),
            make_bash_with_input(1000 + 5 * 60 * 1000, "cargo test | head -5"),
        ];
        let rule = OutputParsingStruggleRule;
        // First and second are > 3 min apart, but second and third are within 3 min
        // Window from record 2: has records 2 and 3 = 2 filters, not > 2
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_output_parsing_empty() {
        let rule = OutputParsingStruggleRule;
        assert!(rule.detect(&[]).is_empty());
    }

    #[test]
    fn test_output_parsing_no_pipes() {
        let records = vec![
            make_bash_with_input(1000, "cargo test"),
            make_bash_with_input(2000, "cargo build"),
        ];
        let rule = OutputParsingStruggleRule;
        assert!(rule.detect(&records).is_empty());
    }

    // -- PermissionRetriesRule fix tests (col-027, ADR-004) --

    #[test]
    fn test_permission_retries_failure_as_terminal_no_finding() {
        // T-FM-01: 5 Pre + 5 Failure -> retries = 0, no finding (AC-05, R-04)
        let records = vec![
            make_pre(1, "Bash"),
            make_pre(2, "Bash"),
            make_pre(3, "Bash"),
            make_pre(4, "Bash"),
            make_pre(5, "Bash"),
            make_failure(6, "Bash"),
            make_failure(7, "Bash"),
            make_failure(8, "Bash"),
            make_failure(9, "Bash"),
            make_failure(10, "Bash"),
        ];
        let rule = PermissionRetriesRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_permission_retries_mixed_post_and_failure_balanced() {
        // T-FM-02: 4 Pre + 2 Post + 2 Failure -> retries = 0, no finding (AC-05 extension)
        let records = vec![
            make_pre(1, "Read"),
            make_pre(2, "Read"),
            make_pre(3, "Read"),
            make_pre(4, "Read"),
            make_post(5, "Read"),
            make_post(6, "Read"),
            make_failure(7, "Read"),
            make_failure(8, "Read"),
        ];
        let rule = PermissionRetriesRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_permission_retries_genuine_imbalance_with_failures() {
        // T-FM-03: 5 Pre + 2 Post + 0 Failure -> retries = 3 > threshold(2) -> 1 finding (AC-06)
        let records = vec![
            make_pre(1, "Write"),
            make_pre(2, "Write"),
            make_pre(3, "Write"),
            make_pre(4, "Write"),
            make_pre(5, "Write"),
            make_post(6, "Write"),
            make_post(7, "Write"),
        ];
        let rule = PermissionRetriesRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 3.0);
    }

    // -- ToolFailureRule tests (col-027, ADR-005) --

    #[test]
    fn test_tool_failure_rule_at_threshold_no_finding() {
        // T-FM-11: exactly 3 failures == threshold -> no finding (AC-09, R-06)
        let records = vec![
            make_failure(1, "Read"),
            make_failure(2, "Read"),
            make_failure(3, "Read"),
        ];
        let rule = ToolFailureRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_tool_failure_rule_above_threshold_fires() {
        // T-FM-12: 4 failures > threshold(3) -> 1 finding (AC-08, R-06)
        let records = vec![
            make_failure(1, "Bash"),
            make_failure(2, "Bash"),
            make_failure(3, "Bash"),
            make_failure(4, "Bash"),
        ];
        let rule = ToolFailureRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "tool_failure_hotspot");
        assert_eq!(findings[0].measured, 4.0);
        assert_eq!(findings[0].threshold, 3.0);
        assert_eq!(findings[0].claim, "Tool 'Bash' failed 4 times");
        assert_eq!(findings[0].category, HotspotCategory::Friction);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn test_tool_failure_rule_multiple_tools_independent() {
        // T-FM-13: 4 Bash + 3 Read + 2 Write -> only Bash exceeds threshold
        let records = vec![
            make_failure(1, "Bash"),
            make_failure(2, "Bash"),
            make_failure(3, "Bash"),
            make_failure(4, "Bash"),
            make_failure(5, "Read"),
            make_failure(6, "Read"),
            make_failure(7, "Read"),
            make_failure(8, "Write"),
            make_failure(9, "Write"),
        ];
        let rule = ToolFailureRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].claim.contains("'Bash'"));
        assert_eq!(findings[0].measured, 4.0);
    }

    #[test]
    fn test_tool_failure_rule_multiple_tools_multiple_findings() {
        // T-FM-14: 5 Bash + 4 Read -> both exceed threshold -> 2 findings
        let records = vec![
            make_failure(1, "Bash"),
            make_failure(2, "Bash"),
            make_failure(3, "Bash"),
            make_failure(4, "Bash"),
            make_failure(5, "Bash"),
            make_failure(6, "Read"),
            make_failure(7, "Read"),
            make_failure(8, "Read"),
            make_failure(9, "Read"),
        ];
        let rule = ToolFailureRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 2);
        let bash_finding = findings.iter().find(|f| f.claim.contains("'Bash'")).unwrap();
        assert_eq!(bash_finding.measured, 5.0);
        let read_finding = findings.iter().find(|f| f.claim.contains("'Read'")).unwrap();
        assert_eq!(read_finding.measured, 4.0);
    }

    #[test]
    fn test_tool_failure_rule_empty_records() {
        // T-FM-15: empty input -> no findings, no panic
        let rule = ToolFailureRule;
        assert!(rule.detect(&[]).is_empty());
    }

    #[test]
    fn test_tool_failure_rule_non_claude_code_excluded() {
        // T-FM-16: 5 failures from non-claude-code domain -> no finding (R-07)
        let records: Vec<ObservationRecord> = (1u64..=5)
            .map(|ts| ObservationRecord {
                ts,
                event_type: hook_type::POSTTOOLUSEFAILURE.to_string(),
                source_domain: "sre".to_string(),
                session_id: "sess-1".to_string(),
                tool: Some("Bash".to_string()),
                input: None,
                response_size: None,
                response_snippet: None,
            })
            .collect();
        let rule = ToolFailureRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_tool_failure_rule_mixed_domains() {
        // T-FM-17: 4 claude-code + 5 sre for same tool -> only claude-code counted -> 1 finding
        let mut records: Vec<ObservationRecord> = (1u64..=4).map(|ts| make_failure(ts, "Bash")).collect();
        records.extend((5u64..=9).map(|ts| ObservationRecord {
            ts,
            event_type: hook_type::POSTTOOLUSEFAILURE.to_string(),
            source_domain: "other-agent".to_string(),
            session_id: "sess-1".to_string(),
            tool: Some("Bash".to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }));
        let rule = ToolFailureRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 4.0);
    }

    #[test]
    fn test_tool_failure_rule_evidence_records() {
        // T-FM-18: 4 failures with response_snippet -> evidence populated
        let records: Vec<ObservationRecord> = (1u64..=4)
            .map(|ts| ObservationRecord {
                ts,
                event_type: hook_type::POSTTOOLUSEFAILURE.to_string(),
                source_domain: "claude-code".to_string(),
                session_id: "sess-1".to_string(),
                tool: Some("Bash".to_string()),
                input: None,
                response_size: None,
                response_snippet: Some("permission denied".to_string()),
            })
            .collect();
        let rule = ToolFailureRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].evidence.len(), 4);
        assert!(findings[0].evidence[0].description.contains("PostToolUseFailure for Bash"));
        assert_eq!(findings[0].evidence[0].detail, "permission denied");
    }

    #[test]
    fn test_tool_failure_rule_no_tool_records_skipped() {
        // Edge case: records with tool == None are skipped gracefully
        let records = vec![
            ObservationRecord {
                ts: 1,
                event_type: hook_type::POSTTOOLUSEFAILURE.to_string(),
                source_domain: "claude-code".to_string(),
                session_id: "sess-1".to_string(),
                tool: None, // no tool
                input: None,
                response_size: None,
                response_snippet: None,
            },
        ];
        let rule = ToolFailureRule;
        assert!(rule.detect(&records).is_empty());
    }

    // -- Two-site coherence tests (T-FM-08/09/10, ADR-004) --

    #[test]
    fn test_two_site_agreement_balanced_failure_and_post() {
        // T-FM-08: 4 Pre + 2 Post + 2 Failure -> both sites agree: 0 imbalance (R-02)
        use crate::metrics::compute_metric_vector;
        let records = vec![
            make_pre(1000, "Bash"),
            make_pre(2000, "Bash"),
            make_pre(3000, "Bash"),
            make_pre(4000, "Bash"),
            make_post(1500, "Bash"),
            make_post(2500, "Bash"),
            make_failure(3500, "Bash"),
            make_failure(4500, "Bash"),
        ];
        // Site 1: metrics
        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.permission_friction_events, 0);
        // Site 2: rule
        let findings = PermissionRetriesRule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_two_site_agreement_genuine_imbalance() {
        // T-FM-09: 5 Pre + 2 Post + 1 Failure -> terminal=3, retries=2 (at threshold, no finding)
        // Both sites must agree: friction_events==2 AND findings empty (threshold > 2)
        use crate::metrics::compute_metric_vector;
        let records = vec![
            make_pre(1, "Read"),
            make_pre(2, "Read"),
            make_pre(3, "Read"),
            make_pre(4, "Read"),
            make_pre(5, "Read"),
            make_post(6, "Read"),
            make_post(7, "Read"),
            make_failure(8, "Read"),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.permission_friction_events, 2);
        let findings = PermissionRetriesRule.detect(&records);
        // retries = 2, threshold = 2 -> 2 > 2 is false -> no finding
        assert!(findings.is_empty());
    }

    #[test]
    fn test_two_site_agreement_failure_only_no_post() {
        // T-FM-10: 5 Pre + 0 Post + 5 Failure -> terminal=5, retries=0
        // Both sites must agree: friction_events==0 AND findings empty
        use crate::metrics::compute_metric_vector;
        let records = vec![
            make_pre(1, "Read"),
            make_pre(2, "Read"),
            make_pre(3, "Read"),
            make_pre(4, "Read"),
            make_pre(5, "Read"),
            make_failure(6, "Read"),
            make_failure(7, "Read"),
            make_failure(8, "Read"),
            make_failure(9, "Read"),
            make_failure(10, "Read"),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.permission_friction_events, 0);
        let findings = PermissionRetriesRule.detect(&records);
        assert!(findings.is_empty());
    }
}
