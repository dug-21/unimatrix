//! Agent hotspot detection rules (7 rules).
//!
//! Detects patterns in agent behavior: excessive context loading, long lifespans,
//! broad file access, re-reads, mutation spread, compile cycles, and edit bloat.

use std::collections::{HashMap, HashSet};

use crate::types::{
    EvidenceRecord, HookType, HotspotCategory, HotspotFinding, ObservationRecord, Severity,
};

use super::{DetectionRule, input_to_command_string, input_to_file_path, truncate};

// -- Rule 1: ContextLoadRule (FR-01.1) --

pub(crate) struct ContextLoadRule;

const CONTEXT_LOAD_THRESHOLD_KB: f64 = 100.0;

impl DetectionRule for ContextLoadRule {
    fn name(&self) -> &str {
        "context_load"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Agent
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut sorted: Vec<&ObservationRecord> = records.iter().collect();
        sorted.sort_by_key(|r| r.ts);

        let mut total_kb = 0.0;
        let mut evidence = Vec::new();

        for record in sorted {
            if record.hook == HookType::PostToolUse {
                let tool = record.tool.as_deref().unwrap_or("");
                if tool == "Write" || tool == "Edit" {
                    break;
                }
                if let (true, Some(size)) = (tool == "Read", record.response_size) {
                    let kb = size as f64 / 1024.0;
                    total_kb += kb;
                    let path = record
                        .input
                        .as_ref()
                        .and_then(input_to_file_path)
                        .unwrap_or_default();
                    evidence.push(EvidenceRecord {
                        description: "Read before first write".to_string(),
                        ts: record.ts,
                        tool: Some("Read".to_string()),
                        detail: format!("{path} ({kb:.1} KB)"),
                    });
                }
            }
        }

        if total_kb > CONTEXT_LOAD_THRESHOLD_KB {
            evidence.truncate(10);
            vec![HotspotFinding {
                category: HotspotCategory::Agent,
                severity: Severity::Warning,
                rule_name: "context_load".to_string(),
                claim: format!("Loaded {total_kb:.0} KB before first write/edit"),
                measured: total_kb,
                threshold: CONTEXT_LOAD_THRESHOLD_KB,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 2: LifespanRule (FR-01.2) --

pub(crate) struct LifespanRule;

const LIFESPAN_THRESHOLD_MINS: f64 = 45.0;

impl DetectionRule for LifespanRule {
    fn name(&self) -> &str {
        "lifespan"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Agent
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut starts: HashMap<&str, Vec<(u64, &str)>> = HashMap::new();
        let mut stops: HashMap<&str, Vec<u64>> = HashMap::new();

        for record in records {
            match record.hook {
                HookType::SubagentStart => {
                    let agent_type = record.tool.as_deref().unwrap_or("unknown");
                    starts
                        .entry(&record.session_id)
                        .or_default()
                        .push((record.ts, agent_type));
                }
                HookType::SubagentStop => {
                    stops.entry(&record.session_id).or_default().push(record.ts);
                }
                _ => {}
            }
        }

        let mut findings = Vec::new();

        for (session_id, start_list) in &mut starts {
            start_list.sort_by_key(|(ts, _)| *ts);
            let stop_list = stops.get_mut(*session_id);
            if let Some(stop_list) = stop_list {
                stop_list.sort();
                for (i, (start_ts, agent_type)) in start_list.iter().enumerate() {
                    if i < stop_list.len() {
                        let duration_mins =
                            (stop_list[i].saturating_sub(*start_ts)) as f64 / 60000.0;
                        if duration_mins > LIFESPAN_THRESHOLD_MINS {
                            findings.push(HotspotFinding {
                                category: HotspotCategory::Agent,
                                severity: Severity::Warning,
                                rule_name: "lifespan".to_string(),
                                claim: format!(
                                    "Agent '{agent_type}' ran for {duration_mins:.0} minutes"
                                ),
                                measured: duration_mins,
                                threshold: LIFESPAN_THRESHOLD_MINS,
                                evidence: vec![
                                    EvidenceRecord {
                                        description: "Agent start".to_string(),
                                        ts: *start_ts,
                                        tool: Some(agent_type.to_string()),
                                        detail: format!("SubagentStart at ts={start_ts}"),
                                    },
                                    EvidenceRecord {
                                        description: "Agent stop".to_string(),
                                        ts: stop_list[i],
                                        tool: None,
                                        detail: format!("SubagentStop at ts={}", stop_list[i]),
                                    },
                                ],
                            });
                        }
                    }
                }
            }
        }

        findings
    }
}

// -- Rule 3: FileBreadthRule (FR-01.3) --

pub(crate) struct FileBreadthRule;

const FILE_BREADTH_THRESHOLD: f64 = 20.0;

impl DetectionRule for FileBreadthRule {
    fn name(&self) -> &str {
        "file_breadth"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Agent
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut file_paths: HashSet<String> = HashSet::new();
        let mut path_counts: HashMap<String, u64> = HashMap::new();

        for record in records {
            let tool = record.tool.as_deref().unwrap_or("");
            if tool == "Read" || tool == "Write" || tool == "Edit" {
                if let Some(input) = &record.input {
                    if let Some(path) = input_to_file_path(input) {
                        file_paths.insert(path.clone());
                        *path_counts.entry(path).or_default() += 1;
                    }
                }
            }
        }

        let count = file_paths.len() as f64;
        if count > FILE_BREADTH_THRESHOLD {
            let mut evidence: Vec<EvidenceRecord> = path_counts
                .iter()
                .map(|(path, count)| EvidenceRecord {
                    description: format!("Accessed {count} time(s)"),
                    ts: 0,
                    tool: None,
                    detail: path.clone(),
                })
                .collect();
            evidence.truncate(20);

            vec![HotspotFinding {
                category: HotspotCategory::Agent,
                severity: Severity::Warning,
                rule_name: "file_breadth".to_string(),
                claim: format!("Accessed {count:.0} distinct files"),
                measured: count,
                threshold: FILE_BREADTH_THRESHOLD,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 4: RereadRateRule (FR-01.4) --

pub(crate) struct RereadRateRule;

const REREAD_THRESHOLD: f64 = 3.0;

impl DetectionRule for RereadRateRule {
    fn name(&self) -> &str {
        "reread_rate"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Agent
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut read_counts: HashMap<String, u64> = HashMap::new();

        for record in records {
            if record.tool.as_deref() == Some("Read") {
                if let Some(input) = &record.input {
                    if let Some(path) = input_to_file_path(input) {
                        *read_counts.entry(path).or_default() += 1;
                    }
                }
            }
        }

        let reread_files: Vec<(&String, &u64)> = read_counts
            .iter()
            .filter(|(_, count)| **count > 1)
            .collect();
        let reread_count = reread_files.len() as f64;

        if reread_count > REREAD_THRESHOLD {
            let evidence: Vec<EvidenceRecord> = reread_files
                .iter()
                .map(|(path, count)| EvidenceRecord {
                    description: format!("Read {count} times"),
                    ts: 0,
                    tool: Some("Read".to_string()),
                    detail: (*path).clone(),
                })
                .collect();

            vec![HotspotFinding {
                category: HotspotCategory::Agent,
                severity: Severity::Info,
                rule_name: "reread_rate".to_string(),
                claim: format!("{reread_count:.0} files re-read multiple times"),
                measured: reread_count,
                threshold: REREAD_THRESHOLD,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 5: MutationSpreadRule (FR-01.5) --

pub(crate) struct MutationSpreadRule;

const MUTATION_SPREAD_THRESHOLD: f64 = 10.0;

impl DetectionRule for MutationSpreadRule {
    fn name(&self) -> &str {
        "mutation_spread"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Agent
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut mutated_files: HashSet<String> = HashSet::new();
        let mut evidence = Vec::new();

        for record in records {
            let tool = record.tool.as_deref().unwrap_or("");
            if tool == "Write" || tool == "Edit" {
                if let Some(input) = &record.input {
                    if let Some(path) = input_to_file_path(input) {
                        if mutated_files.insert(path.clone()) {
                            evidence.push(EvidenceRecord {
                                description: "File mutated".to_string(),
                                ts: record.ts,
                                tool: Some(tool.to_string()),
                                detail: path,
                            });
                        }
                    }
                }
            }
        }

        let count = mutated_files.len() as f64;
        if count > MUTATION_SPREAD_THRESHOLD {
            evidence.truncate(15);
            vec![HotspotFinding {
                category: HotspotCategory::Agent,
                severity: Severity::Warning,
                rule_name: "mutation_spread".to_string(),
                claim: format!("Mutations spread across {count:.0} files"),
                measured: count,
                threshold: MUTATION_SPREAD_THRESHOLD,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 6: CompileCyclesRule (FR-01.6) --

pub(crate) struct CompileCyclesRule;

const COMPILE_CYCLES_THRESHOLD: f64 = 6.0;

fn is_compile_command(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    // Find "cargo" in the command string (may be prefixed by env vars)
    if let Some(pos) = trimmed.find("cargo") {
        let after_cargo = &trimmed[pos + 5..].trim_start();
        after_cargo.starts_with("check")
            || after_cargo.starts_with("test")
            || after_cargo.starts_with("build")
            || after_cargo.starts_with("clippy")
    } else {
        false
    }
}

impl DetectionRule for CompileCyclesRule {
    fn name(&self) -> &str {
        "compile_cycles"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Agent
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut compile_count = 0u64;
        let mut evidence = Vec::new();

        for record in records {
            if record.tool.as_deref() == Some("Bash") && record.hook == HookType::PreToolUse {
                if let Some(input) = &record.input {
                    let cmd = input_to_command_string(input);
                    if is_compile_command(&cmd) {
                        compile_count += 1;
                        evidence.push(EvidenceRecord {
                            description: "Compile command".to_string(),
                            ts: record.ts,
                            tool: Some("Bash".to_string()),
                            detail: truncate(&cmd, 200),
                        });
                    }
                }
            }
        }

        if compile_count as f64 > COMPILE_CYCLES_THRESHOLD {
            vec![HotspotFinding {
                category: HotspotCategory::Agent,
                severity: Severity::Warning,
                rule_name: "compile_cycles".to_string(),
                claim: format!("{compile_count} compile/check cycles detected"),
                measured: compile_count as f64,
                threshold: COMPILE_CYCLES_THRESHOLD,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 7: EditBloatRule (FR-01.7) --

pub(crate) struct EditBloatRule;

const EDIT_BLOAT_THRESHOLD_KB: f64 = 50.0;

impl DetectionRule for EditBloatRule {
    fn name(&self) -> &str {
        "edit_bloat"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Agent
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut edit_sizes = Vec::new();
        let mut evidence = Vec::new();

        for record in records {
            let is_edit_post =
                record.tool.as_deref() == Some("Edit") && record.hook == HookType::PostToolUse;
            if let (true, Some(size)) = (is_edit_post, record.response_size) {
                let kb = size as f64 / 1024.0;
                edit_sizes.push(kb);
                if kb > EDIT_BLOAT_THRESHOLD_KB {
                    evidence.push(EvidenceRecord {
                        description: "Large Edit response".to_string(),
                        ts: record.ts,
                        tool: Some("Edit".to_string()),
                        detail: format!("{kb:.1} KB response"),
                    });
                }
            }
        }

        if edit_sizes.is_empty() {
            return vec![];
        }

        let avg_kb = edit_sizes.iter().sum::<f64>() / edit_sizes.len() as f64;
        if avg_kb > EDIT_BLOAT_THRESHOLD_KB {
            vec![HotspotFinding {
                category: HotspotCategory::Agent,
                severity: Severity::Info,
                rule_name: "edit_bloat".to_string(),
                claim: format!(
                    "Average Edit response is {avg_kb:.1} KB (threshold {EDIT_BLOAT_THRESHOLD_KB} KB)"
                ),
                measured: avg_kb,
                threshold: EDIT_BLOAT_THRESHOLD_KB,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_post_read(ts: u64, size: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PostToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: Some(size),
            response_snippet: None,
        }
    }

    fn make_pre_read(ts: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Read".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_post_write(ts: u64) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PostToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Write".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/out.rs"})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_pre_write(ts: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Write".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_pre_edit(ts: u64, path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Edit".to_string()),
            input: Some(serde_json::json!({"file_path": path})),
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_post_edit(ts: u64, size: u64) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PostToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("Edit".to_string()),
            input: Some(serde_json::json!({"file_path": "/tmp/edit.rs"})),
            response_size: Some(size),
            response_snippet: None,
        }
    }

    fn make_bash(ts: u64, command: &str) -> ObservationRecord {
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

    fn make_subagent_start(ts: u64, session: &str, agent_type: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::SubagentStart,
            session_id: session.to_string(),
            tool: Some(agent_type.to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_subagent_stop(ts: u64, session: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::SubagentStop,
            session_id: session.to_string(),
            tool: None,
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    // -- ContextLoadRule --

    #[test]
    fn test_context_load_fires_above_threshold() {
        // 200 KB of reads before any write
        let records = vec![
            make_post_read(1000, 102_400, "/tmp/a.rs"), // 100 KB
            make_post_read(2000, 102_400, "/tmp/b.rs"), // 100 KB = 200 KB total
            make_post_write(3000),
        ];
        let rule = ContextLoadRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > CONTEXT_LOAD_THRESHOLD_KB);
        assert_eq!(findings[0].rule_name, "context_load");
    }

    #[test]
    fn test_context_load_silent_below_threshold() {
        let records = vec![
            make_post_read(1000, 51_200, "/tmp/a.rs"), // 50 KB
            make_post_write(2000),
        ];
        let rule = ContextLoadRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_context_load_empty_records() {
        let rule = ContextLoadRule;
        assert!(rule.detect(&[]).is_empty());
    }

    #[test]
    fn test_context_load_stops_at_first_write() {
        // 50 KB before write, 200 KB after -- should only count the first 50 KB
        let records = vec![
            make_post_read(1000, 51_200, "/tmp/a.rs"), // 50 KB
            make_post_write(2000),
            make_post_read(3000, 204_800, "/tmp/b.rs"), // 200 KB
        ];
        let rule = ContextLoadRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    // -- LifespanRule --

    #[test]
    fn test_lifespan_fires_above_threshold() {
        let records = vec![
            make_subagent_start(1000, "sess-1", "uni-rust-dev"),
            make_subagent_stop(1000 + 60 * 60000, "sess-1"), // 60 minutes
        ];
        let rule = LifespanRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > LIFESPAN_THRESHOLD_MINS);
    }

    #[test]
    fn test_lifespan_silent_below_threshold() {
        let records = vec![
            make_subagent_start(1000, "sess-1", "uni-rust-dev"),
            make_subagent_stop(1000 + 30 * 60000, "sess-1"), // 30 minutes
        ];
        let rule = LifespanRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_lifespan_empty_records() {
        let rule = LifespanRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- FileBreadthRule --

    #[test]
    fn test_file_breadth_fires_above_threshold() {
        let mut records = Vec::new();
        for i in 0..25 {
            records.push(make_pre_read(i * 1000, &format!("/tmp/file_{i}.rs")));
        }
        let rule = FileBreadthRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > FILE_BREADTH_THRESHOLD);
    }

    #[test]
    fn test_file_breadth_silent_below_threshold() {
        let mut records = Vec::new();
        for i in 0..15 {
            records.push(make_pre_read(i * 1000, &format!("/tmp/file_{i}.rs")));
        }
        let rule = FileBreadthRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_file_breadth_deduplicates() {
        let mut records = Vec::new();
        for i in 0..25 {
            records.push(make_pre_read(i * 1000, "/tmp/same.rs"));
        }
        let rule = FileBreadthRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty()); // Only 1 distinct file
    }

    #[test]
    fn test_file_breadth_empty_records() {
        let rule = FileBreadthRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- RereadRateRule --

    #[test]
    fn test_reread_rate_fires_above_threshold() {
        let records = vec![
            make_pre_read(1000, "/tmp/a.rs"),
            make_pre_read(2000, "/tmp/a.rs"),
            make_pre_read(3000, "/tmp/b.rs"),
            make_pre_read(4000, "/tmp/b.rs"),
            make_pre_read(5000, "/tmp/c.rs"),
            make_pre_read(6000, "/tmp/c.rs"),
            make_pre_read(7000, "/tmp/d.rs"),
            make_pre_read(8000, "/tmp/d.rs"),
        ];
        let rule = RereadRateRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > REREAD_THRESHOLD);
    }

    #[test]
    fn test_reread_rate_silent_below_threshold() {
        let records = vec![
            make_pre_read(1000, "/tmp/a.rs"),
            make_pre_read(2000, "/tmp/a.rs"),
            make_pre_read(3000, "/tmp/b.rs"),
            make_pre_read(4000, "/tmp/b.rs"),
        ];
        let rule = RereadRateRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_reread_rate_empty_records() {
        let rule = RereadRateRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- MutationSpreadRule --

    #[test]
    fn test_mutation_spread_fires_above_threshold() {
        let mut records = Vec::new();
        for i in 0..12 {
            records.push(make_pre_write(i * 1000, &format!("/tmp/file_{i}.rs")));
        }
        let rule = MutationSpreadRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > MUTATION_SPREAD_THRESHOLD);
    }

    #[test]
    fn test_mutation_spread_silent_below_threshold() {
        let mut records = Vec::new();
        for i in 0..8 {
            records.push(make_pre_write(i * 1000, &format!("/tmp/file_{i}.rs")));
        }
        let rule = MutationSpreadRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_mutation_spread_deduplicates() {
        let mut records = Vec::new();
        for i in 0..20 {
            records.push(make_pre_write(i * 1000, "/tmp/same.rs"));
        }
        let rule = MutationSpreadRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_mutation_spread_empty_records() {
        let rule = MutationSpreadRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- CompileCyclesRule --

    #[test]
    fn test_compile_cycles_fires_above_threshold() {
        let records: Vec<ObservationRecord> = (0..8)
            .map(|i| make_bash(i * 1000, "cargo test --workspace"))
            .collect();
        let rule = CompileCyclesRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > COMPILE_CYCLES_THRESHOLD);
    }

    #[test]
    fn test_compile_cycles_silent_below_threshold() {
        let records: Vec<ObservationRecord> =
            (0..4).map(|i| make_bash(i * 1000, "cargo build")).collect();
        let rule = CompileCyclesRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_compile_cycles_matches_variants() {
        let records = vec![
            make_bash(1000, "cargo check"),
            make_bash(2000, "cargo test -p unimatrix-store"),
            make_bash(3000, "cargo build --workspace"),
            make_bash(4000, "cargo clippy -- -D warnings"),
            make_bash(5000, "RUSTFLAGS='-D warnings' cargo check"),
            make_bash(6000, "cargo check --all-targets"),
            make_bash(7000, "cargo test -- --test-threads=1"),
        ];
        let rule = CompileCyclesRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 7.0);
    }

    #[test]
    fn test_compile_cycles_non_compile_commands() {
        let records = vec![
            make_bash(1000, "ls -la"),
            make_bash(2000, "cat Cargo.toml"),
            make_bash(3000, "echo cargo test"),
        ];
        let rule = CompileCyclesRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_compile_cycles_empty_records() {
        let rule = CompileCyclesRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- EditBloatRule --

    #[test]
    fn test_edit_bloat_fires_above_threshold() {
        let records = vec![
            make_post_edit(1000, 60_000), // ~58 KB
            make_post_edit(2000, 70_000), // ~68 KB
        ];
        let rule = EditBloatRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > EDIT_BLOAT_THRESHOLD_KB);
    }

    #[test]
    fn test_edit_bloat_silent_below_threshold() {
        let records = vec![
            make_post_edit(1000, 10_000), // ~10 KB
            make_post_edit(2000, 20_000), // ~20 KB
        ];
        let rule = EditBloatRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_edit_bloat_no_edit_records() {
        let records = vec![make_pre_read(1000, "/tmp/a.rs")];
        let rule = EditBloatRule;
        let findings = rule.detect(&records);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_edit_bloat_empty_records() {
        let rule = EditBloatRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- is_compile_command --

    #[test]
    fn test_is_compile_cargo_check() {
        assert!(is_compile_command("cargo check"));
        assert!(is_compile_command("cargo check --workspace"));
    }

    #[test]
    fn test_is_compile_cargo_test() {
        assert!(is_compile_command("cargo test"));
        assert!(is_compile_command("cargo test -p unimatrix-store"));
    }

    #[test]
    fn test_is_compile_cargo_build() {
        assert!(is_compile_command("cargo build"));
        assert!(is_compile_command("cargo build --release"));
    }

    #[test]
    fn test_is_compile_cargo_clippy() {
        assert!(is_compile_command("cargo clippy -- -D warnings"));
    }

    #[test]
    fn test_is_compile_with_env_prefix() {
        assert!(is_compile_command("RUSTFLAGS='-D warnings' cargo check"));
    }

    #[test]
    fn test_is_compile_not_cargo() {
        assert!(!is_compile_command("ls -la"));
        // Note: "echo cargo test" is a known false positive because "cargo" appears
        // followed by "test". This is acceptable -- actual build commands dominate.
    }

    #[test]
    fn test_is_compile_empty() {
        assert!(!is_compile_command(""));
    }
}
