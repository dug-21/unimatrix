//! MetricVector computation from analyzed ObservationRecords and HotspotFindings.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::detection::{contains_sleep_command, input_to_command_string};
use crate::types::{
    HookType, HotspotCategory, HotspotFinding, MetricVector, ObservationRecord, PhaseMetrics,
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

    MetricVector {
        computed_at,
        universal,
        phases,
    }
}

fn compute_universal(
    records: &[ObservationRecord],
    hotspots: &[HotspotFinding],
) -> UniversalMetrics {
    let mut m = UniversalMetrics::default();

    // Count tool calls (PreToolUse events)
    m.total_tool_calls = records
        .iter()
        .filter(|r| r.hook == HookType::PreToolUse)
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

    // Permission friction: sum of (pre - post) per tool, only positive values
    let mut pre_counts: HashMap<&str, u64> = HashMap::new();
    let mut post_counts: HashMap<&str, u64> = HashMap::new();
    for r in records {
        if let Some(tool) = &r.tool {
            match r.hook {
                HookType::PreToolUse => *pre_counts.entry(tool).or_default() += 1,
                HookType::PostToolUse => *post_counts.entry(tool).or_default() += 1,
                _ => {}
            }
        }
    }
    m.permission_friction_events = pre_counts
        .iter()
        .map(|(tool, &pre)| pre.saturating_sub(*post_counts.get(tool).unwrap_or(&0)))
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

    // Search miss rate: requires response analysis not available in records
    m.search_miss_rate = 0.0;

    // Context loaded: sum response_size from all PostToolUse records
    let total_response_bytes: u64 = records
        .iter()
        .filter(|r| r.hook == HookType::PostToolUse)
        .filter_map(|r| r.response_size)
        .sum();
    m.total_context_loaded_kb = total_response_bytes as f64 / 1024.0;

    // Coordinator respawn count: SubagentStart for coordinator-like agents
    m.coordinator_respawn_count = records
        .iter()
        .filter(|r| r.hook == HookType::SubagentStart)
        .filter(|r| {
            r.tool
                .as_deref()
                .is_some_and(|t| t.contains("scrum-master") || t.contains("coordinator"))
        })
        .count() as u64;

    // Knowledge entries stored: context_store calls
    m.knowledge_entries_stored = records
        .iter()
        .filter(|r| r.hook == HookType::PreToolUse)
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
        // Check SubagentStart records for phase transitions
        if record.hook == HookType::SubagentStart {
            if let Some(input) = &record.input {
                if let Some(phase) = extract_phase_name(input) {
                    current_phase = Some(phase);
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
            .filter(|r| r.hook == HookType::PreToolUse)
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

/// Extract phase name from SubagentStart input (FR-07.3).
///
/// Splits on first ":" and uses the prefix as phase name.
fn extract_phase_name(input: &serde_json::Value) -> Option<String> {
    let s = match input {
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
    use crate::types::{HookType, HotspotCategory, HotspotFinding, Severity};

    fn make_pre(ts: u64, tool: &str, session: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::PreToolUse,
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
            hook: HookType::PostToolUse,
            session_id: session.to_string(),
            tool: Some(tool.to_string()),
            input: None,
            response_size: Some(response_size),
            response_snippet: None,
        }
    }

    fn make_subagent_start(ts: u64, session: &str, prompt: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook: HookType::SubagentStart,
            session_id: session.to_string(),
            tool: Some("uni-pseudocode".to_string()),
            input: Some(serde_json::Value::String(prompt.to_string())),
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
    fn test_extract_phase_name_not_string() {
        let input = serde_json::json!({"key": "value"});
        assert_eq!(extract_phase_name(&input), None);
    }

    #[test]
    fn test_compute_phases_with_transitions() {
        let records = vec![
            make_subagent_start(1000, "s1", "3a: Pseudocode design"),
            make_pre(2000, "Read", "s1"),
            make_pre(3000, "Write", "s1"),
            make_subagent_start(4000, "s1", "3b: Code implementation"),
            make_pre(5000, "Read", "s1"),
            make_pre(6000, "Write", "s1"),
            make_pre(7000, "Bash", "s1"),
        ];

        let phases = compute_phases(&records);
        assert_eq!(phases.len(), 2);
        assert!(phases.contains_key("3a"));
        assert!(phases.contains_key("3b"));
        assert_eq!(phases["3a"].tool_call_count, 2);
        assert_eq!(phases["3b"].tool_call_count, 3);
    }

    #[test]
    fn test_compute_phases_no_subagent_start() {
        let records = vec![
            make_pre(1000, "Read", "s1"),
            make_pre(2000, "Write", "s1"),
        ];

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
                hook: HookType::PreToolUse,
                session_id: "s1".to_string(),
                tool: Some("Bash".to_string()),
                input: Some(serde_json::json!({"command": "grep -r 'test' ."})),
                response_size: None,
                response_snippet: None,
            },
            ObservationRecord {
                ts: 2000,
                hook: HookType::PreToolUse,
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

    // -- Sleep workaround count in metrics --

    #[test]
    fn test_sleep_workaround_count_in_metrics() {
        let records = vec![ObservationRecord {
            ts: 1000,
            hook: HookType::PreToolUse,
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
                hook: HookType::SubagentStart,
                session_id: "s1".to_string(),
                tool: Some("uni-scrum-master".to_string()),
                input: None,
                response_size: None,
                response_snippet: None,
            },
            ObservationRecord {
                ts: 2000,
                hook: HookType::SubagentStart,
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
}
