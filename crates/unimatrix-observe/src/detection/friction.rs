//! Friction hotspot detection rules (4 rules).
//!
//! 2 existing from col-002 (PermissionRetries, SleepWorkarounds) + 2 new (SearchViaBash, OutputParsingStruggle).

use std::collections::{HashMap, HashSet};

use crate::types::{
    EvidenceRecord, HookType, HotspotCategory, HotspotFinding, ObservationRecord, Severity,
};

use super::{contains_sleep_command, input_to_command_string, truncate, DetectionRule};

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

// -- Rule 3: SearchViaBashRule (FR-02.1) --

pub(crate) struct SearchViaBashRule;

const SEARCH_VIA_BASH_THRESHOLD_PCT: f64 = 5.0;

fn is_search_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    for segment in trimmed.split(|c: char| c == ';' || c == '\n') {
        let seg = segment.trim();
        if seg.starts_with("find ") || seg == "find"
            || seg.starts_with("grep ") || seg == "grep"
            || seg.starts_with("rg ") || seg == "rg"
            || seg.starts_with("ag ") || seg == "ag"
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
        let mut total_bash = 0u64;
        let mut search_bash = 0u64;
        let mut evidence = Vec::new();

        for record in records {
            if record.tool.as_deref() == Some("Bash") && record.hook == HookType::PreToolUse {
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
        // Collect Bash commands with pipes: (ts, base_command, full_command)
        let mut piped_commands: Vec<(u64, String, String)> = Vec::new();

        let mut sorted: Vec<&ObservationRecord> = records.iter().collect();
        sorted.sort_by_key(|r| r.ts);

        for record in &sorted {
            if record.tool.as_deref() == Some("Bash") && record.hook == HookType::PreToolUse {
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
                        let after_pipe = full.split_once('|').map(|(_, rest)| rest.trim().to_string());
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
}
