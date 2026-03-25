//! MetricVector computation from analyzed ObservationRecords and HotspotFindings.

use std::collections::{BTreeMap, HashMap, HashSet};

use unimatrix_core::observation::hook_type;

use crate::detection::{contains_sleep_command, input_to_command_string};
use crate::types::{
    HotspotCategory, HotspotFinding, MetricVector, ObservationRecord, PhaseMetrics,
    UniversalMetrics,
};

/// Compute a MetricVector from analyzed records and hotspot findings.
pub fn compute_metric_vector(
    records: &[ObservationRecord],
    hotspots: &[HotspotFinding],
    computed_at: u64,
) -> MetricVector {
    let universal = compute_universal(records, hotspots);
    let phases = compute_phases(records);
    let domain_metrics = compute_domain_metrics(records);

    MetricVector {
        computed_at,
        universal,
        phases,
        domain_metrics,
    }
}

/// Compute universal metrics from claude-code records only (IR-03).
/// Records with `source_domain != "claude-code"` are excluded from all
/// UniversalMetrics computations — they contain domain-specific events
/// that are not comparable to claude-code session metrics.
fn compute_universal(
    records: &[ObservationRecord],
    hotspots: &[HotspotFinding],
) -> UniversalMetrics {
    // IR-03: pre-filter to claude-code domain only.
    let records: Vec<&ObservationRecord> = records
        .iter()
        .filter(|r| r.source_domain == "claude-code")
        .collect();

    let mut m = UniversalMetrics::default();

    // Count tool calls (PreToolUse events)
    m.total_tool_calls = records
        .iter()
        .filter(|r| r.event_type == hook_type::PRETOOLUSE)
        .count() as u64;

    // Total duration: max_ts - min_ts (in seconds, from millis)
    if let (Some(first), Some(last)) = (
        records.iter().map(|r| r.ts).min(),
        records.iter().map(|r| r.ts).max(),
    ) {
        m.total_duration_secs = last.saturating_sub(first) / 1000;
    }

    // Session count: distinct session_ids
    let sessions: HashSet<&str> = records.iter().map(|r| r.session_id.as_str()).collect();
    m.session_count = sessions.len() as u64;

    // Permission friction: sum of (pre - terminal) per tool, only positive values.
    // "terminal" = PostToolUse OR PostToolUseFailure (col-027, ADR-004).
    // A failed call is a resolved call — not a retried/blocked call.
    let mut pre_counts: HashMap<&str, u64> = HashMap::new();
    // Widened (col-027): counts both PostToolUse and PostToolUseFailure as terminal events.
    // Variable renamed from post_counts to terminal_counts for clarity (matches friction.rs).
    let mut terminal_counts: HashMap<&str, u64> = HashMap::new();
    for r in &records {
        if let Some(tool) = &r.tool {
            if r.event_type == hook_type::PRETOOLUSE {
                *pre_counts.entry(tool).or_default() += 1;
            } else if r.event_type == hook_type::POSTTOOLUSE {
                // unchanged: PostToolUse is a terminal event
                *terminal_counts.entry(tool.as_str()).or_default() += 1;
            } else if r.event_type == hook_type::POSTTOOLUSEFAILURE {
                // NEW (col-027): PostToolUseFailure is also a terminal event
                *terminal_counts.entry(tool.as_str()).or_default() += 1;
            }
        }
    }
    m.permission_friction_events = pre_counts
        .iter()
        .map(|(tool, &pre)| pre.saturating_sub(*terminal_counts.get(*tool).unwrap_or(&0)))
        .sum();

    // Sleep workaround count
    m.sleep_workaround_count = records
        .iter()
        .filter(|r| r.tool.as_deref() == Some("Bash"))
        .filter(|r| {
            r.input.as_ref().is_some_and(|input| {
                let s = input_to_command_string(input);
                contains_sleep_command(&s)
            })
        })
        .count() as u64;

    // Bash for search count (using grep/find via Bash instead of dedicated tools)
    m.bash_for_search_count = records
        .iter()
        .filter(|r| r.tool.as_deref() == Some("Bash"))
        .filter(|r| {
            r.input.as_ref().is_some_and(|input| {
                let s = input_to_command_string(input);
                contains_search_pattern(&s)
            })
        })
        .count() as u64;

    // Search miss rate: Grep/Glob PostToolUse records with empty results
    {
        let search_posts: Vec<&&ObservationRecord> = records
            .iter()
            .filter(|r| r.event_type == hook_type::POSTTOOLUSE)
            .filter(|r| {
                r.tool
                    .as_deref()
                    .is_some_and(|t| t == "Grep" || t == "Glob")
            })
            .collect();
        let total_search = search_posts.len();
        if total_search > 0 {
            let misses = search_posts
                .iter()
                .filter(|r| {
                    r.response_snippet.as_ref().is_some_and(|s| {
                        s.contains("\"numFiles\": 0") || s.contains("\"content\": \"\"")
                    })
                })
                .count();
            m.search_miss_rate = misses as f64 / total_search as f64;
        }
    }

    // Context loaded: sum response_size from all PostToolUse records
    let total_response_bytes: u64 = records
        .iter()
        .filter(|r| r.event_type == hook_type::POSTTOOLUSE)
        .filter_map(|r| r.response_size)
        .sum();
    m.total_context_loaded_kb = total_response_bytes as f64 / 1024.0;

    // Edit bloat total KB: sum response_size from Edit PostToolUse records
    {
        let edit_response_bytes: u64 = records
            .iter()
            .filter(|r| r.event_type == hook_type::POSTTOOLUSE)
            .filter(|r| r.tool.as_deref() == Some("Edit"))
            .filter_map(|r| r.response_size)
            .sum();
        m.edit_bloat_total_kb = edit_response_bytes as f64 / 1024.0;
    }

    // Edit bloat ratio: edit PostToolUse response_size / total PostToolUse response_size
    if total_response_bytes > 0 {
        let edit_response_bytes: u64 = records
            .iter()
            .filter(|r| r.event_type == hook_type::POSTTOOLUSE)
            .filter(|r| r.tool.as_deref() == Some("Edit"))
            .filter_map(|r| r.response_size)
            .sum();
        m.edit_bloat_ratio = edit_response_bytes as f64 / total_response_bytes as f64;
    }

    // Context load before first write: sum Read PostToolUse response_size until first Write/Edit PreToolUse
    {
        let mut sorted_records: Vec<&&ObservationRecord> = records.iter().collect();
        sorted_records.sort_by_key(|r| r.ts);

        let mut read_bytes: u64 = 0;
        for r in &sorted_records {
            if r.event_type == hook_type::PRETOOLUSE {
                if let Some(tool) = &r.tool {
                    if tool == "Write" || tool == "Edit" {
                        break;
                    }
                }
            }
            if r.event_type == hook_type::POSTTOOLUSE && r.tool.as_deref() == Some("Read") {
                read_bytes += r.response_size.unwrap_or(0);
            }
        }
        m.context_load_before_first_write_kb = read_bytes as f64 / 1024.0;
    }

    // Cold restart events: count timestamp gaps > 30 minutes between consecutive records
    {
        let mut sorted_records: Vec<&&ObservationRecord> = records.iter().collect();
        sorted_records.sort_by_key(|r| r.ts);

        let cold_restart_threshold_ms: u64 = 1_800_000; // 30 minutes
        let mut count: u64 = 0;
        for window in sorted_records.windows(2) {
            let gap = window[1].ts.saturating_sub(window[0].ts);
            if gap > cold_restart_threshold_ms {
                count += 1;
            }
        }
        m.cold_restart_events = count;
    }

    // Parallel call rate: PreToolUse records sharing same timestamp / total PreToolUse
    {
        let pre_records: Vec<&&ObservationRecord> = records
            .iter()
            .filter(|r| r.event_type == hook_type::PRETOOLUSE)
            .collect();
        let total_pre = pre_records.len();
        if total_pre > 0 {
            let mut ts_counts: HashMap<u64, u64> = HashMap::new();
            for r in &pre_records {
                *ts_counts.entry(r.ts).or_default() += 1;
            }
            let parallel_count: u64 = ts_counts.values().filter(|&&count| count > 1).sum::<u64>();
            m.parallel_call_rate = parallel_count as f64 / total_pre as f64;
        }
    }

    // Post-completion work %: PreToolUse records after last TaskUpdate with status "completed"
    {
        let mut sorted_records: Vec<&&ObservationRecord> = records.iter().collect();
        sorted_records.sort_by_key(|r| r.ts);

        let total_pre = records
            .iter()
            .filter(|r| r.event_type == hook_type::PRETOOLUSE)
            .count();

        // Find the last TaskUpdate PreToolUse with status "completed" in input
        let mut last_completion_ts: Option<u64> = None;
        for r in sorted_records.iter() {
            if r.event_type == hook_type::PRETOOLUSE && r.tool.as_deref() == Some("TaskUpdate") {
                if let Some(input) = &r.input {
                    if let Some(obj) = input.as_object() {
                        if obj.get("status").and_then(|v| v.as_str()) == Some("completed") {
                            last_completion_ts = Some(r.ts);
                        }
                    }
                }
            }
        }

        if let Some(completion_ts) = last_completion_ts {
            if total_pre > 0 {
                let post_work = records
                    .iter()
                    .filter(|r| r.event_type == hook_type::PRETOOLUSE && r.ts > completion_ts)
                    .count();
                m.post_completion_work_pct = post_work as f64 / total_pre as f64 * 100.0;
            }
        }
    }

    // Follow-up issues created: Bash PreToolUse records with "gh issue create"
    m.follow_up_issues_created = records
        .iter()
        .filter(|r| r.event_type == hook_type::PRETOOLUSE)
        .filter(|r| r.tool.as_deref() == Some("Bash"))
        .filter(|r| {
            r.input.as_ref().is_some_and(|input| {
                let s = input_to_command_string(input);
                s.contains("gh issue create")
            })
        })
        .count() as u64;

    // Coordinator respawn count: SubagentStart for coordinator-like agents
    m.coordinator_respawn_count = records
        .iter()
        .filter(|r| r.event_type == hook_type::SUBAGENTSTART)
        .filter(|r| {
            r.tool
                .as_deref()
                .is_some_and(|t| t.contains("scrum-master") || t.contains("coordinator"))
        })
        .count() as u64;

    // Knowledge entries stored: context_store calls
    m.knowledge_entries_stored = records
        .iter()
        .filter(|r| r.event_type == hook_type::PRETOOLUSE)
        .filter(|r| {
            r.tool
                .as_deref()
                .is_some_and(|t| t.contains("context_store"))
        })
        .count() as u64;

    // Hotspot counts by category
    m.agent_hotspot_count = hotspots
        .iter()
        .filter(|h| h.category == HotspotCategory::Agent)
        .count() as u64;
    m.friction_hotspot_count = hotspots
        .iter()
        .filter(|h| h.category == HotspotCategory::Friction)
        .count() as u64;
    m.session_hotspot_count = hotspots
        .iter()
        .filter(|h| h.category == HotspotCategory::Session)
        .count() as u64;
    m.scope_hotspot_count = hotspots
        .iter()
        .filter(|h| h.category == HotspotCategory::Scope)
        .count() as u64;

    m
}

fn compute_phases(records: &[ObservationRecord]) -> BTreeMap<String, PhaseMetrics> {
    let mut phases: BTreeMap<String, Vec<&ObservationRecord>> = BTreeMap::new();
    let mut current_phase: Option<String> = None;

    for record in records {
        // Check TaskCreate/TaskUpdate PreToolUse records for phase transitions (FR-07.3).
        // Phase names come from task subject prefixes using "{phase-id}: {description}".
        if record.event_type == hook_type::PRETOOLUSE {
            if let Some(tool) = &record.tool {
                if tool == "TaskCreate" || tool == "TaskUpdate" {
                    if let Some(input) = &record.input {
                        if let Some(phase) = extract_phase_name(input) {
                            current_phase = Some(phase);
                        }
                    }
                }
            }
        }

        if let Some(ref phase) = current_phase {
            phases.entry(phase.clone()).or_default().push(record);
        }
    }

    let mut result = BTreeMap::new();
    for (phase, phase_records) in &phases {
        let tool_call_count = phase_records
            .iter()
            .filter(|r| r.event_type == hook_type::PRETOOLUSE)
            .count() as u64;

        let duration_secs = if let (Some(first), Some(last)) = (
            phase_records.iter().map(|r| r.ts).min(),
            phase_records.iter().map(|r| r.ts).max(),
        ) {
            last.saturating_sub(first) / 1000
        } else {
            0
        };

        result.insert(
            phase.clone(),
            PhaseMetrics {
                duration_secs,
                tool_call_count,
            },
        );
    }

    result
}

/// Compute domain-specific extension metrics.
/// Returns an empty map for W1-5 (extension point for W3-1 and later domain packs).
fn compute_domain_metrics(_records: &[ObservationRecord]) -> HashMap<String, f64> {
    HashMap::new()
}

/// Extract phase name from TaskCreate/TaskUpdate input (FR-07.3).
///
/// Expects a JSON object with a `"subject"` field whose value follows
/// the `"{phase-id}: {description}"` convention. Splits on the first `:`
/// and returns the trimmed prefix as the phase name.
///
/// Also accepts a plain `Value::String` as a fallback (useful in tests).
fn extract_phase_name(input: &serde_json::Value) -> Option<String> {
    let s = match input {
        serde_json::Value::Object(map) => map.get("subject")?.as_str()?,
        serde_json::Value::String(s) => s.as_str(),
        _ => return None,
    };

    let colon_pos = s.find(':')?;
    let prefix = s[..colon_pos].trim();

    if prefix.is_empty() {
        return None;
    }

    Some(prefix.to_string())
}

/// Check if a Bash command contains search-like patterns.
fn contains_search_pattern(s: &str) -> bool {
    let patterns = ["grep ", "rg ", "find ", "ack "];
    patterns.iter().any(|p| s.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{HotspotCategory, HotspotFinding, Severity};

    fn make_pre(ts: u64, tool: &str, session: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: hook_type::PRETOOLUSE.to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session.to_string(),
            tool: Some(tool.to_string()),
            input: None,
            response_size: None,
            response_snippet: None,
        }
    }

    fn make_post(ts: u64, tool: &str, session: &str, response_size: u64) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: hook_type::POSTTOOLUSE.to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session.to_string(),
            tool: Some(tool.to_string()),
            input: None,
            response_size: Some(response_size),
            response_snippet: None,
        }
    }

    fn make_task_create(ts: u64, session: &str, subject: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: hook_type::PRETOOLUSE.to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session.to_string(),
            tool: Some("TaskCreate".to_string()),
            input: Some(serde_json::json!({"subject": subject})),
            response_size: None,
            response_snippet: None,
        }
    }

    #[test]
    fn test_compute_metric_vector_basic() {
        let records = vec![
            make_pre(1000, "Read", "s1"),
            make_pre(2000, "Write", "s1"),
            make_post(1500, "Read", "s1", 1024),
            make_post(2500, "Write", "s1", 2048),
        ];

        let mv = compute_metric_vector(&records, &[], 9999);
        assert_eq!(mv.computed_at, 9999);
        assert_eq!(mv.universal.total_tool_calls, 2);
        assert_eq!(mv.universal.session_count, 1);
        assert_eq!(mv.universal.total_duration_secs, 1); // (2500-1000)/1000 = 1
        assert!(mv.universal.total_context_loaded_kb > 0.0);
    }

    #[test]
    fn test_compute_metric_vector_empty_records() {
        let mv = compute_metric_vector(&[], &[], 5000);
        assert_eq!(mv.computed_at, 5000);
        assert_eq!(mv.universal.total_tool_calls, 0);
        assert_eq!(mv.universal.session_count, 0);
        assert_eq!(mv.universal.total_duration_secs, 0);
    }

    #[test]
    fn test_compute_metric_vector_multiple_sessions() {
        let records = vec![
            make_pre(1000, "Read", "s1"),
            make_pre(2000, "Read", "s2"),
            make_pre(3000, "Read", "s3"),
        ];

        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.session_count, 3);
    }

    #[test]
    fn test_compute_metric_vector_with_hotspots() {
        let hotspots = vec![
            HotspotFinding {
                category: HotspotCategory::Friction,
                severity: Severity::Warning,
                rule_name: "test".to_string(),
                claim: "test".to_string(),
                measured: 1.0,
                threshold: 0.0,
                evidence: vec![],
            },
            HotspotFinding {
                category: HotspotCategory::Session,
                severity: Severity::Warning,
                rule_name: "test".to_string(),
                claim: "test".to_string(),
                measured: 1.0,
                threshold: 0.0,
                evidence: vec![],
            },
        ];

        let mv = compute_metric_vector(&[], &hotspots, 0);
        assert_eq!(mv.universal.friction_hotspot_count, 1);
        assert_eq!(mv.universal.session_hotspot_count, 1);
        assert_eq!(mv.universal.agent_hotspot_count, 0);
        assert_eq!(mv.universal.scope_hotspot_count, 0);
    }

    #[test]
    fn test_compute_metric_vector_permission_friction() {
        let records = vec![
            make_pre(1000, "Read", "s1"),
            make_pre(2000, "Read", "s1"),
            make_pre(3000, "Read", "s1"),
            make_post(1500, "Read", "s1", 100),
        ];

        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.permission_friction_events, 2);
    }

    #[test]
    fn test_compute_metric_vector_context_loaded() {
        let records = vec![
            make_post(1000, "Read", "s1", 1024),
            make_post(2000, "Read", "s1", 2048),
        ];

        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.total_context_loaded_kb - 3.0).abs() < 0.01);
    }

    // -- Phase extraction tests --

    #[test]
    fn test_extract_phase_name_standard() {
        let input = serde_json::Value::String("3a: Pseudocode".to_string());
        assert_eq!(extract_phase_name(&input), Some("3a".to_string()));
    }

    #[test]
    fn test_extract_phase_name_no_colon() {
        let input = serde_json::Value::String("Just a description".to_string());
        assert_eq!(extract_phase_name(&input), None);
    }

    #[test]
    fn test_extract_phase_name_multiple_colons() {
        let input = serde_json::Value::String("3b: Code: implement parser".to_string());
        assert_eq!(extract_phase_name(&input), Some("3b".to_string()));
    }

    #[test]
    fn test_extract_phase_name_empty_prefix() {
        let input = serde_json::Value::String(": Just a description".to_string());
        assert_eq!(extract_phase_name(&input), None);
    }

    #[test]
    fn test_extract_phase_name_object_with_subject() {
        let input = serde_json::json!({"subject": "Phase 1: Spawn researcher for crt-006 scope exploration"});
        assert_eq!(extract_phase_name(&input), Some("Phase 1".to_string()));
    }

    #[test]
    fn test_extract_phase_name_object_no_subject() {
        let input = serde_json::json!({"key": "value"});
        assert_eq!(extract_phase_name(&input), None);
    }

    #[test]
    fn test_extract_phase_name_object_subject_no_colon() {
        let input = serde_json::json!({"subject": "Just a description"});
        assert_eq!(extract_phase_name(&input), None);
    }

    #[test]
    fn test_extract_phase_name_not_string_or_object() {
        let input = serde_json::json!(42);
        assert_eq!(extract_phase_name(&input), None);
    }

    #[test]
    fn test_compute_phases_with_transitions() {
        let records = vec![
            make_task_create(1000, "s1", "3a: Pseudocode design"),
            make_pre(2000, "Read", "s1"),
            make_pre(3000, "Write", "s1"),
            make_task_create(4000, "s1", "3b: Code implementation"),
            make_pre(5000, "Read", "s1"),
            make_pre(6000, "Write", "s1"),
            make_pre(7000, "Bash", "s1"),
        ];

        let phases = compute_phases(&records);
        assert_eq!(phases.len(), 2);
        assert!(phases.contains_key("3a"));
        assert!(phases.contains_key("3b"));
        // 3a: the TaskCreate record itself + 2 subsequent PreToolUse = 3 total PreToolUse
        assert_eq!(phases["3a"].tool_call_count, 3);
        // 3b: the TaskCreate record itself + 3 subsequent PreToolUse = 4 total PreToolUse
        assert_eq!(phases["3b"].tool_call_count, 4);
    }

    #[test]
    fn test_compute_phases_no_task_create() {
        let records = vec![make_pre(1000, "Read", "s1"), make_pre(2000, "Write", "s1")];

        let phases = compute_phases(&records);
        assert!(phases.is_empty());
    }

    // -- contains_search_pattern --

    #[test]
    fn test_contains_search_pattern_grep() {
        assert!(contains_search_pattern("grep -r 'pattern' ."));
    }

    #[test]
    fn test_contains_search_pattern_rg() {
        assert!(contains_search_pattern("rg 'pattern' src/"));
    }

    #[test]
    fn test_contains_search_pattern_find() {
        assert!(contains_search_pattern("find . -name '*.rs'"));
    }

    #[test]
    fn test_contains_search_pattern_no_match() {
        assert!(!contains_search_pattern("cargo test"));
    }

    // -- Bash search count --

    #[test]
    fn test_bash_for_search_count() {
        let records = vec![
            ObservationRecord {
                ts: 1000,
                event_type: hook_type::PRETOOLUSE.to_string(),
                source_domain: "claude-code".to_string(),
                session_id: "s1".to_string(),
                tool: Some("Bash".to_string()),
                input: Some(serde_json::json!({"command": "grep -r 'test' ."})),
                response_size: None,
                response_snippet: None,
            },
            ObservationRecord {
                ts: 2000,
                event_type: hook_type::PRETOOLUSE.to_string(),
                source_domain: "claude-code".to_string(),
                session_id: "s1".to_string(),
                tool: Some("Bash".to_string()),
                input: Some(serde_json::json!({"command": "cargo build"})),
                response_size: None,
                response_snippet: None,
            },
        ];

        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.bash_for_search_count, 1);
    }

    // -- Search miss rate --

    fn make_post_with_snippet(
        ts: u64,
        tool: &str,
        session: &str,
        response_size: u64,
        snippet: &str,
    ) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: hook_type::POSTTOOLUSE.to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session.to_string(),
            tool: Some(tool.to_string()),
            input: None,
            response_size: Some(response_size),
            response_snippet: Some(snippet.to_string()),
        }
    }

    fn make_pre_with_input(
        ts: u64,
        tool: &str,
        session: &str,
        input: serde_json::Value,
    ) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: hook_type::PRETOOLUSE.to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session.to_string(),
            tool: Some(tool.to_string()),
            input: Some(input),
            response_size: None,
            response_snippet: None,
        }
    }

    #[test]
    fn test_search_miss_rate_all_misses() {
        let records = vec![
            make_post_with_snippet(1000, "Grep", "s1", 50, "{\"numFiles\": 0}"),
            make_post_with_snippet(2000, "Glob", "s1", 50, "{\"content\": \"\"}"),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.search_miss_rate - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_search_miss_rate_half_misses() {
        let records = vec![
            make_post_with_snippet(1000, "Grep", "s1", 50, "{\"numFiles\": 0}"),
            make_post_with_snippet(
                2000,
                "Grep",
                "s1",
                50,
                "{\"numFiles\": 5, \"results\": [...]}",
            ),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.search_miss_rate - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_search_miss_rate_no_search_records() {
        let records = vec![make_post(1000, "Read", "s1", 100)];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.search_miss_rate - 0.0).abs() < 0.001);
    }

    // -- Edit bloat total KB --

    #[test]
    fn test_edit_bloat_total_kb() {
        let records = vec![
            make_post(1000, "Edit", "s1", 2048),
            make_post(2000, "Edit", "s1", 1024),
            make_post(3000, "Read", "s1", 4096),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.edit_bloat_total_kb - 3.0).abs() < 0.01); // (2048+1024)/1024 = 3.0
    }

    #[test]
    fn test_edit_bloat_total_kb_no_edits() {
        let records = vec![make_post(1000, "Read", "s1", 4096)];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.edit_bloat_total_kb - 0.0).abs() < 0.01);
    }

    // -- Edit bloat ratio --

    #[test]
    fn test_edit_bloat_ratio() {
        let records = vec![
            make_post(1000, "Edit", "s1", 1024),
            make_post(2000, "Read", "s1", 3072),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        // Edit is 1024 out of total 4096 = 0.25
        assert!((mv.universal.edit_bloat_ratio - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_edit_bloat_ratio_no_responses() {
        let records = vec![make_pre(1000, "Read", "s1")];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.edit_bloat_ratio - 0.0).abs() < 0.001);
    }

    // -- Context load before first write KB --

    #[test]
    fn test_context_load_before_first_write() {
        let records = vec![
            make_post(1000, "Read", "s1", 2048), // Read response before any write
            make_post(2000, "Read", "s1", 1024), // Another read before write
            make_pre(3000, "Edit", "s1"),        // First write/edit - stops counting
            make_post(4000, "Read", "s1", 8192), // After write, not counted
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        // (2048 + 1024) / 1024 = 3.0
        assert!((mv.universal.context_load_before_first_write_kb - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_context_load_before_first_write_no_writes() {
        let records = vec![
            make_post(1000, "Read", "s1", 2048),
            make_post(2000, "Read", "s1", 1024),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        // No write/edit, so all reads are counted: (2048 + 1024) / 1024 = 3.0
        assert!((mv.universal.context_load_before_first_write_kb - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_context_load_before_first_write_write_tool() {
        let records = vec![
            make_post(1000, "Read", "s1", 4096),
            make_pre(2000, "Write", "s1"), // Write stops counting
            make_post(3000, "Read", "s1", 4096),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.context_load_before_first_write_kb - 4.0).abs() < 0.01);
    }

    // -- Cold restart events --

    #[test]
    fn test_cold_restart_events() {
        // Gap of 31 minutes between record 2 and 3
        let records = vec![
            make_pre(1_000_000, "Read", "s1"),
            make_pre(1_060_000, "Read", "s1"), // 1 min later
            make_pre(1_060_000 + 1_860_000, "Read", "s1"), // 31 min gap
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.cold_restart_events, 1);
    }

    #[test]
    fn test_cold_restart_events_multiple_gaps() {
        let records = vec![
            make_pre(1_000_000, "Read", "s1"),
            make_pre(3_000_000, "Read", "s1"), // 2000s = 33min gap
            make_pre(5_000_000, "Read", "s1"), // 2000s = 33min gap
            make_pre(5_060_000, "Read", "s1"), // 60s = 1min gap (no restart)
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.cold_restart_events, 2);
    }

    #[test]
    fn test_cold_restart_events_no_gaps() {
        let records = vec![
            make_pre(1_000_000, "Read", "s1"),
            make_pre(1_060_000, "Read", "s1"), // 1 min gap
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.cold_restart_events, 0);
    }

    // -- Parallel call rate --

    #[test]
    fn test_parallel_call_rate() {
        // 3 PreToolUse at ts=1000, 1 at ts=2000 => 3 parallel out of 4 total
        let records = vec![
            make_pre(1000, "Read", "s1"),
            make_pre(1000, "Grep", "s1"),
            make_pre(1000, "Glob", "s1"),
            make_pre(2000, "Write", "s1"),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        // 3 parallel / 4 total = 0.75
        assert!((mv.universal.parallel_call_rate - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_parallel_call_rate_no_parallel() {
        let records = vec![make_pre(1000, "Read", "s1"), make_pre(2000, "Write", "s1")];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.parallel_call_rate - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_parallel_call_rate_no_pre_records() {
        let records = vec![make_post(1000, "Read", "s1", 100)];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.parallel_call_rate - 0.0).abs() < 0.001);
    }

    // -- Post completion work percentage --

    #[test]
    fn test_post_completion_work_pct() {
        // 5 PreToolUse total: 3 before completion, 1 is the TaskUpdate, 1 after
        let records = vec![
            make_pre(1000, "Read", "s1"),
            make_pre(2000, "Write", "s1"),
            make_pre(3000, "Bash", "s1"),
            make_pre_with_input(
                4000,
                "TaskUpdate",
                "s1",
                serde_json::json!({"taskId": "1", "status": "completed"}),
            ),
            make_pre(5000, "Read", "s1"), // after completion
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        // 1 post-completion record / 5 total * 100 = 20.0
        assert!((mv.universal.post_completion_work_pct - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_post_completion_work_pct_no_completion() {
        let records = vec![make_pre(1000, "Read", "s1"), make_pre(2000, "Write", "s1")];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!((mv.universal.post_completion_work_pct - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_post_completion_work_pct_uses_last_completion() {
        // Two completions: should use the last one
        let records = vec![
            make_pre(1000, "Read", "s1"),
            make_pre_with_input(
                2000,
                "TaskUpdate",
                "s1",
                serde_json::json!({"taskId": "1", "status": "completed"}),
            ),
            make_pre(3000, "Read", "s1"), // after first completion
            make_pre_with_input(
                4000,
                "TaskUpdate",
                "s1",
                serde_json::json!({"taskId": "2", "status": "completed"}),
            ),
            make_pre(5000, "Read", "s1"), // after last completion
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        // 1 record after last completion / 5 total * 100 = 20.0
        assert!((mv.universal.post_completion_work_pct - 20.0).abs() < 0.1);
    }

    // -- Follow-up issues created --

    #[test]
    fn test_follow_up_issues_created() {
        let records = vec![
            make_pre_with_input(
                1000,
                "Bash",
                "s1",
                serde_json::json!({"command": "gh issue create --label bug --title 'Fix thing'"}),
            ),
            make_pre_with_input(
                2000,
                "Bash",
                "s1",
                serde_json::json!({"command": "gh issue create --label enhancement"}),
            ),
            make_pre_with_input(
                3000,
                "Bash",
                "s1",
                serde_json::json!({"command": "cargo test"}),
            ),
        ];
        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.follow_up_issues_created, 2);
    }

    #[test]
    fn test_follow_up_issues_created_none() {
        let records = vec![make_pre_with_input(
            1000,
            "Bash",
            "s1",
            serde_json::json!({"command": "gh pr create --title 'PR'"}),
        )];
        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.follow_up_issues_created, 0);
    }

    // -- Sleep workaround count in metrics --

    #[test]
    fn test_sleep_workaround_count_in_metrics() {
        let records = vec![ObservationRecord {
            ts: 1000,
            event_type: hook_type::PRETOOLUSE.to_string(),
            source_domain: "claude-code".to_string(),
            session_id: "s1".to_string(),
            tool: Some("Bash".to_string()),
            input: Some(serde_json::json!({"command": "sleep 5"})),
            response_size: None,
            response_snippet: None,
        }];

        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.sleep_workaround_count, 1);
    }

    // -- Coordinator respawn count --

    #[test]
    fn test_coordinator_respawn_count() {
        let records = vec![
            ObservationRecord {
                ts: 1000,
                event_type: hook_type::SUBAGENTSTART.to_string(),
                source_domain: "claude-code".to_string(),
                session_id: "s1".to_string(),
                tool: Some("uni-scrum-master".to_string()),
                input: None,
                response_size: None,
                response_snippet: None,
            },
            ObservationRecord {
                ts: 2000,
                event_type: hook_type::SUBAGENTSTART.to_string(),
                source_domain: "claude-code".to_string(),
                session_id: "s1".to_string(),
                tool: Some("uni-pseudocode".to_string()),
                input: None,
                response_size: None,
                response_snippet: None,
            },
        ];

        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(mv.universal.coordinator_respawn_count, 1);
    }

    // -------------------------------------------------------------------------
    // T-MET-10: Non-claude-code records produce zero universal metrics (IR-03)
    // -------------------------------------------------------------------------
    #[test]
    fn test_compute_universal_zeros_for_non_claude_code_domain() {
        // Records with source_domain != "claude-code" must be excluded entirely.
        let records = vec![
            ObservationRecord {
                ts: 1000,
                event_type: hook_type::PRETOOLUSE.to_string(),
                source_domain: "sre".to_string(),
                session_id: "s1".to_string(),
                tool: Some("Read".to_string()),
                input: None,
                response_size: None,
                response_snippet: None,
            },
            ObservationRecord {
                ts: 2000,
                event_type: hook_type::POSTTOOLUSE.to_string(),
                source_domain: "sre".to_string(),
                session_id: "s1".to_string(),
                tool: Some("Read".to_string()),
                input: None,
                response_size: Some(4096),
                response_snippet: None,
            },
        ];

        let mv = compute_metric_vector(&records, &[], 0);
        assert_eq!(
            mv.universal.total_tool_calls, 0,
            "non-claude-code must not count as tool calls"
        );
        assert_eq!(
            mv.universal.session_count, 0,
            "non-claude-code must not count sessions"
        );
        assert_eq!(
            mv.universal.total_context_loaded_kb, 0.0,
            "non-claude-code response bytes must be excluded"
        );
    }

    // -------------------------------------------------------------------------
    // T-MET-11: Mixed slice — only claude-code records contribute (IR-03)
    // -------------------------------------------------------------------------
    #[test]
    fn test_compute_universal_filters_mixed_domain_slice() {
        let records = vec![
            // claude-code: should be counted
            make_pre(1000, "Read", "s1"),
            make_pre(2000, "Write", "s1"),
            // sre domain: must be excluded
            ObservationRecord {
                ts: 3000,
                event_type: hook_type::PRETOOLUSE.to_string(),
                source_domain: "sre".to_string(),
                session_id: "s2".to_string(),
                tool: Some("Bash".to_string()),
                input: None,
                response_size: None,
                response_snippet: None,
            },
            ObservationRecord {
                ts: 4000,
                event_type: hook_type::PRETOOLUSE.to_string(),
                source_domain: "sre".to_string(),
                session_id: "s2".to_string(),
                tool: Some("Read".to_string()),
                input: None,
                response_size: None,
                response_snippet: None,
            },
        ];

        let mv = compute_metric_vector(&records, &[], 0);
        // Only the 2 claude-code PreToolUse records count
        assert_eq!(
            mv.universal.total_tool_calls, 2,
            "only claude-code tool calls should count"
        );
        // Only claude-code session "s1" counts
        assert_eq!(
            mv.universal.session_count, 1,
            "only claude-code sessions should count"
        );
    }

    // -------------------------------------------------------------------------
    // T-MET-12: Empty slice produces zero MetricVector (baseline)
    // -------------------------------------------------------------------------
    #[test]
    fn test_compute_metric_vector_empty_slice_all_zeros() {
        let mv = compute_metric_vector(&[], &[], 42);
        assert_eq!(mv.computed_at, 42);
        assert_eq!(mv.universal.total_tool_calls, 0);
        assert_eq!(mv.universal.session_count, 0);
        assert_eq!(mv.universal.total_duration_secs, 0);
        assert_eq!(mv.universal.search_miss_rate, 0.0);
        assert_eq!(mv.universal.parallel_call_rate, 0.0);
        assert_eq!(mv.universal.post_completion_work_pct, 0.0);
        assert!(mv.phases.is_empty());
        assert!(mv.domain_metrics.is_empty());
    }

    // -------------------------------------------------------------------------
    // T-MET-domain: compute_domain_metrics returns empty map (W1-5 extension point)
    // -------------------------------------------------------------------------
    #[test]
    fn test_compute_domain_metrics_returns_empty_map() {
        let records = vec![make_pre(1000, "Read", "s1")];
        let mv = compute_metric_vector(&records, &[], 0);
        assert!(
            mv.domain_metrics.is_empty(),
            "domain_metrics must be empty HashMap in W1-5"
        );
    }
}
