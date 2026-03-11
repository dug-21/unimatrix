//! Session hotspot detection rules (5 rules).
//!
//! 1 existing from col-002 (SessionTimeout) + 4 new (ColdRestart, CoordinatorRespawns,
//! PostCompletionWork, ReworkEvents).

use std::collections::{HashMap, HashSet};

use crate::types::{
    EvidenceRecord, HookType, HotspotCategory, HotspotFinding, ObservationRecord, Severity,
};

use super::{DetectionRule, find_completion_boundary, input_to_file_path};

// -- Rule 1: SessionTimeoutRule (moved from col-002 detection.rs) --

pub(crate) struct SessionTimeoutRule;

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
                        claim: format!("Session '{session_id}' had a {gap_hours:.1}h gap"),
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

// -- Rule 2: ColdRestartRule (FR-03.1) --

pub(crate) struct ColdRestartRule;

const COLD_RESTART_GAP_MS: u64 = 30 * 60 * 1000; // 30 minutes
const BURST_WINDOW_MS: u64 = 5 * 60 * 1000; // 5 minutes

impl DetectionRule for ColdRestartRule {
    fn name(&self) -> &str {
        "cold_restart"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Session
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        if records.is_empty() {
            return vec![];
        }

        let mut sorted: Vec<&ObservationRecord> = records.iter().collect();
        sorted.sort_by_key(|r| r.ts);

        let mut files_read_before: HashSet<String> = HashSet::new();
        let mut findings = Vec::new();
        let mut prev_ts = sorted[0].ts;

        for (idx, record) in sorted.iter().enumerate() {
            let gap = record.ts.saturating_sub(prev_ts);
            if gap > COLD_RESTART_GAP_MS {
                // Collect reads in the 5-minute burst window after the gap
                let burst_window_end = record.ts + BURST_WINDOW_MS;
                let mut overlap = Vec::new();

                for burst_record in &sorted[idx..] {
                    if burst_record.ts > burst_window_end {
                        break;
                    }
                    if burst_record.tool.as_deref() == Some("Read") {
                        if let Some(input) = &burst_record.input {
                            if let Some(path) = input_to_file_path(input) {
                                if files_read_before.contains(&path) {
                                    overlap.push(path);
                                }
                            }
                        }
                    }
                }

                if !overlap.is_empty() {
                    let gap_mins = gap as f64 / 60000.0;
                    let mut evidence = vec![EvidenceRecord {
                        description: "Gap detected".to_string(),
                        ts: record.ts,
                        tool: None,
                        detail: format!("{gap_mins:.0}-minute gap before re-reads"),
                    }];
                    for path in &overlap {
                        evidence.push(EvidenceRecord {
                            description: "Re-read after gap".to_string(),
                            ts: record.ts,
                            tool: Some("Read".to_string()),
                            detail: path.clone(),
                        });
                    }

                    findings.push(HotspotFinding {
                        category: HotspotCategory::Session,
                        severity: Severity::Warning,
                        rule_name: "cold_restart".to_string(),
                        claim: format!(
                            "{gap_mins:.0}-minute gap followed by {} re-reads of previously accessed files",
                            overlap.len()
                        ),
                        measured: gap_mins,
                        threshold: COLD_RESTART_GAP_MS as f64 / 60000.0,
                        evidence,
                    });
                }
            }

            // Track reads
            if record.tool.as_deref() == Some("Read") {
                if let Some(input) = &record.input {
                    if let Some(path) = input_to_file_path(input) {
                        files_read_before.insert(path);
                    }
                }
            }

            prev_ts = record.ts;
        }

        findings
    }
}

// -- Rule 3: CoordinatorRespawnsRule (FR-03.2) --

pub(crate) struct CoordinatorRespawnsRule;

const COORDINATOR_RESPAWN_THRESHOLD: f64 = 3.0;

impl DetectionRule for CoordinatorRespawnsRule {
    fn name(&self) -> &str {
        "coordinator_respawns"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Session
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut coordinator_spawns = 0u64;
        let mut evidence = Vec::new();

        for record in records {
            if record.hook == HookType::SubagentStart {
                if let Some(agent_type) = &record.tool {
                    let lower = agent_type.to_lowercase();
                    if lower.contains("scrum-master")
                        || lower.contains("coordinator")
                        || lower.contains("lead")
                    {
                        coordinator_spawns += 1;
                        evidence.push(EvidenceRecord {
                            description: "Coordinator spawn".to_string(),
                            ts: record.ts,
                            tool: Some(agent_type.clone()),
                            detail: format!("Coordinator agent '{agent_type}' spawned"),
                        });
                    }
                }
            }
        }

        if coordinator_spawns as f64 > COORDINATOR_RESPAWN_THRESHOLD {
            vec![HotspotFinding {
                category: HotspotCategory::Session,
                severity: Severity::Warning,
                rule_name: "coordinator_respawns".to_string(),
                claim: format!("{coordinator_spawns} coordinator respawns detected"),
                measured: coordinator_spawns as f64,
                threshold: COORDINATOR_RESPAWN_THRESHOLD,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 4: PostCompletionWorkRule (FR-03.3) --

pub(crate) struct PostCompletionWorkRule;

const POST_COMPLETION_THRESHOLD_PCT: f64 = 8.0;

impl DetectionRule for PostCompletionWorkRule {
    fn name(&self) -> &str {
        "post_completion_work"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Session
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        if records.is_empty() {
            return vec![];
        }

        let boundary_ts = match find_completion_boundary(records) {
            Some(ts) => ts,
            None => return vec![],
        };

        let total = records.len();
        let post_count = records.iter().filter(|r| r.ts > boundary_ts).count();

        let pct = (post_count as f64 / total as f64) * 100.0;
        if pct > POST_COMPLETION_THRESHOLD_PCT {
            let evidence = vec![
                EvidenceRecord {
                    description: "Completion boundary".to_string(),
                    ts: boundary_ts,
                    tool: Some("TaskUpdate".to_string()),
                    detail: format!("Last task completion at ts={boundary_ts}"),
                },
                EvidenceRecord {
                    description: "Post-completion summary".to_string(),
                    ts: boundary_ts,
                    tool: None,
                    detail: format!("{post_count} of {total} records after completion"),
                },
            ];

            vec![HotspotFinding {
                category: HotspotCategory::Session,
                severity: Severity::Info,
                rule_name: "post_completion_work".to_string(),
                claim: format!(
                    "{pct:.1}% of tool calls occurred after task completion ({post_count}/{total})"
                ),
                measured: pct,
                threshold: POST_COMPLETION_THRESHOLD_PCT,
                evidence,
            }]
        } else {
            vec![]
        }
    }
}

// -- Rule 5: ReworkEventsRule (FR-03.4) --

pub(crate) struct ReworkEventsRule;

impl DetectionRule for ReworkEventsRule {
    fn name(&self) -> &str {
        "rework_events"
    }

    fn category(&self) -> HotspotCategory {
        HotspotCategory::Session
    }

    fn detect(&self, records: &[ObservationRecord]) -> Vec<HotspotFinding> {
        let mut task_states: HashMap<String, String> = HashMap::new();
        let mut rework_evidence = Vec::new();

        let mut sorted: Vec<&ObservationRecord> = records.iter().collect();
        sorted.sort_by_key(|r| r.ts);

        for record in &sorted {
            if record.tool.as_deref() == Some("TaskUpdate") {
                if let Some(input) = &record.input {
                    let status = input.get("status").and_then(|v| v.as_str());
                    let task_id = input
                        .get("taskId")
                        .or_else(|| input.get("subject"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    if let Some(status) = status {
                        let prev = task_states.get(task_id);
                        if prev.map(|s| s.as_str()) == Some("completed") && status == "in_progress"
                        {
                            rework_evidence.push(EvidenceRecord {
                                description: "Task rework: completed -> in_progress".to_string(),
                                ts: record.ts,
                                tool: Some("TaskUpdate".to_string()),
                                detail: format!("Task '{task_id}' reopened"),
                            });
                        }
                        task_states.insert(task_id.to_string(), status.to_string());
                    }
                }
            }
        }

        if rework_evidence.is_empty() {
            vec![]
        } else {
            let count = rework_evidence.len();
            vec![HotspotFinding {
                category: HotspotCategory::Session,
                severity: Severity::Warning,
                rule_name: "rework_events".to_string(),
                claim: format!("{count} task rework event(s) detected"),
                measured: count as f64,
                threshold: 1.0,
                evidence: rework_evidence,
            }]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn make_read_with_path(ts: u64, path: &str) -> ObservationRecord {
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

    fn make_subagent_start(ts: u64, agent_type: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::SubagentStart,
            session_id: "sess-1".to_string(),
            tool: Some(agent_type.to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_task_update(ts: u64, task_id: &str, status: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
            session_id: "sess-1".to_string(),
            tool: Some("TaskUpdate".to_string()),
            input: Some(serde_json::json!({"taskId": task_id, "status": status})),
            response_size: None,
            response_snippet: None,
        }
    }

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
    }

    #[test]
    fn test_session_timeout_one_hour_gap() {
        let one_hour_ms = 60 * 60 * 1000;
        let records = vec![
            make_record_in_session(1000, "sess-1"),
            make_record_in_session(1000 + one_hour_ms, "sess-1"),
        ];
        let rule = SessionTimeoutRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_session_timeout_empty() {
        let rule = SessionTimeoutRule;
        assert!(rule.detect(&[]).is_empty());
    }

    #[test]
    fn test_session_timeout_single_record() {
        let records = vec![make_record_in_session(1000, "sess-1")];
        let rule = SessionTimeoutRule;
        assert!(rule.detect(&records).is_empty());
    }

    // -- ColdRestartRule --

    #[test]
    fn test_cold_restart_fires_with_rereads() {
        let gap = 35 * 60 * 1000; // 35 minutes
        let records = vec![
            make_read_with_path(1000, "/tmp/a.rs"),
            make_read_with_path(2000, "/tmp/b.rs"),
            // Gap from ts=2000 to ts=1000+gap = (gap-1000) ms
            make_read_with_path(1000 + gap, "/tmp/a.rs"), // re-read
            make_read_with_path(2000 + gap, "/tmp/b.rs"), // re-read
        ];
        let rule = ColdRestartRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        // Gap is from 2000 to 2101000 = 2099000ms = ~34.98 minutes, above 30-min threshold
        assert!(findings[0].measured > 30.0);
    }

    #[test]
    fn test_cold_restart_silent_new_files() {
        let gap = 35 * 60 * 1000;
        let records = vec![
            make_read_with_path(1000, "/tmp/a.rs"),
            make_read_with_path(2000, "/tmp/b.rs"),
            // 35-minute gap, reading NEW files
            make_read_with_path(1000 + gap, "/tmp/c.rs"),
            make_read_with_path(2000 + gap, "/tmp/d.rs"),
        ];
        let rule = ColdRestartRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_cold_restart_silent_short_gap() {
        let gap = 25 * 60 * 1000; // 25 minutes (below threshold)
        let records = vec![
            make_read_with_path(1000, "/tmp/a.rs"),
            make_read_with_path(1000 + gap, "/tmp/a.rs"),
        ];
        let rule = ColdRestartRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_cold_restart_empty() {
        let rule = ColdRestartRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- CoordinatorRespawnsRule --

    #[test]
    fn test_coordinator_respawns_fires() {
        let records = vec![
            make_subagent_start(1000, "uni-scrum-master"),
            make_subagent_start(2000, "uni-scrum-master"),
            make_subagent_start(3000, "uni-scrum-master"),
            make_subagent_start(4000, "uni-scrum-master"),
        ];
        let rule = CoordinatorRespawnsRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > COORDINATOR_RESPAWN_THRESHOLD);
    }

    #[test]
    fn test_coordinator_respawns_silent() {
        let records = vec![
            make_subagent_start(1000, "uni-scrum-master"),
            make_subagent_start(2000, "uni-scrum-master"),
        ];
        let rule = CoordinatorRespawnsRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_coordinator_respawns_non_coordinator() {
        let records = vec![
            make_subagent_start(1000, "uni-rust-dev"),
            make_subagent_start(2000, "uni-rust-dev"),
            make_subagent_start(3000, "uni-rust-dev"),
            make_subagent_start(4000, "uni-rust-dev"),
        ];
        let rule = CoordinatorRespawnsRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_coordinator_respawns_empty() {
        let rule = CoordinatorRespawnsRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- PostCompletionWorkRule --

    #[test]
    fn test_post_completion_fires() {
        // 80 records before completion, 20 after = 20%
        let mut records: Vec<ObservationRecord> =
            (0..80).map(|i| make_pre(i * 100, "Read")).collect();
        records.push(make_task_update(8000, "1", "completed"));
        for i in 0..20 {
            records.push(make_pre(8100 + i * 100, "Read"));
        }
        let rule = PostCompletionWorkRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].measured > POST_COMPLETION_THRESHOLD_PCT);
    }

    #[test]
    fn test_post_completion_silent() {
        let mut records: Vec<ObservationRecord> =
            (0..98).map(|i| make_pre(i * 100, "Read")).collect();
        records.push(make_task_update(9800, "1", "completed"));
        records.push(make_pre(9900, "Read"));
        let rule = PostCompletionWorkRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_post_completion_no_task_update() {
        let records: Vec<ObservationRecord> = (0..10).map(|i| make_pre(i * 100, "Read")).collect();
        let rule = PostCompletionWorkRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_post_completion_empty() {
        let rule = PostCompletionWorkRule;
        assert!(rule.detect(&[]).is_empty());
    }

    // -- ReworkEventsRule --

    #[test]
    fn test_rework_events_fires() {
        let records = vec![
            make_task_update(1000, "task-1", "in_progress"),
            make_task_update(2000, "task-1", "completed"),
            make_task_update(3000, "task-1", "in_progress"), // rework
        ];
        let rule = ReworkEventsRule;
        let findings = rule.detect(&records);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].measured, 1.0);
    }

    #[test]
    fn test_rework_events_normal_flow() {
        let records = vec![
            make_task_update(1000, "task-1", "in_progress"),
            make_task_update(2000, "task-1", "completed"),
        ];
        let rule = ReworkEventsRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_rework_events_completed_only() {
        let records = vec![make_task_update(1000, "task-1", "completed")];
        let rule = ReworkEventsRule;
        assert!(rule.detect(&records).is_empty());
    }

    #[test]
    fn test_rework_events_empty() {
        let rule = ReworkEventsRule;
        assert!(rule.detect(&[]).is_empty());
    }
}
