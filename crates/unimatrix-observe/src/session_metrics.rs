//! Per-session activity profile computation and cross-session context reload rate (col-020).
//!
//! Pure computation on `ObservationRecord` arrays. No database access.

use std::collections::{HashMap, HashSet};

use crate::types::{HookType, ObservationRecord, SessionSummary};

/// Compute per-session activity profiles from observation records.
///
/// Groups records by `session_id`, computes tool distribution (PreToolUse only),
/// top file zones, agents spawned, and knowledge flow counts. Returns summaries
/// sorted by `started_at` ascending with lexicographic `session_id` tiebreaker.
pub fn compute_session_summaries(records: &[ObservationRecord]) -> Vec<SessionSummary> {
    // Group records by session_id
    let mut groups: HashMap<&str, Vec<&ObservationRecord>> = HashMap::new();
    for record in records {
        groups
            .entry(record.session_id.as_str())
            .or_default()
            .push(record);
    }

    let mut summaries: Vec<SessionSummary> = groups
        .into_iter()
        .map(|(session_id, session_records)| build_session_summary(session_id, &session_records))
        .collect();

    // Sort by started_at ascending, session_id lexicographic tiebreaker (FR-01.9)
    summaries.sort_by(|a, b| {
        a.started_at
            .cmp(&b.started_at)
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    summaries
}

/// Compute the fraction of file reads in sessions N+1..N that overlap with any prior session.
///
/// Requires `summaries` to be sorted by `started_at` (as returned by `compute_session_summaries`).
/// Returns 0.0 for single-session or empty input, and when no files are read after the first session.
pub fn compute_context_reload_pct(
    summaries: &[SessionSummary],
    records: &[ObservationRecord],
) -> f64 {
    if summaries.len() <= 1 {
        return 0.0;
    }

    // Build per-session file sets from observation records
    let mut session_files: HashMap<String, HashSet<String>> = HashMap::new();
    for record in records {
        if record.hook != HookType::PreToolUse {
            continue;
        }
        let path = record
            .tool
            .as_deref()
            .zip(record.input.as_ref())
            .and_then(|(tool, input)| extract_file_path(tool, input));
        if let Some(path) = path {
            session_files
                .entry(record.session_id.clone())
                .or_default()
                .insert(path);
        }
    }

    // Walk sessions in chronological order, tracking cumulative prior files
    let mut prior_files: HashSet<String> = HashSet::new();
    let mut total_files_in_subsequent: u64 = 0;
    let mut reload_files: u64 = 0;

    for summary in summaries {
        let current_files = session_files
            .get(&summary.session_id)
            .cloned()
            .unwrap_or_default();

        if !prior_files.is_empty() {
            for file in &current_files {
                total_files_in_subsequent += 1;
                if prior_files.contains(file) {
                    reload_files += 1;
                }
            }
        }

        // Add current session's files to prior set
        prior_files.extend(current_files);
    }

    // Division by zero guard (R-13)
    if total_files_in_subsequent == 0 {
        return 0.0;
    }

    reload_files as f64 / total_files_in_subsequent as f64
}

/// Build a single session summary from grouped records.
fn build_session_summary(
    session_id: &str,
    session_records: &[&ObservationRecord],
) -> SessionSummary {
    // Timestamps
    let min_ts = session_records.iter().map(|r| r.ts).min().unwrap_or(0);
    let max_ts = session_records.iter().map(|r| r.ts).max().unwrap_or(0);
    let duration_secs = (max_ts.saturating_sub(min_ts)) / 1000;

    // Tool distribution: only PreToolUse events (FR-01.2)
    let mut tool_distribution: HashMap<String, u64> = HashMap::new();
    for record in session_records {
        if record.hook != HookType::PreToolUse {
            continue;
        }
        let tool_name = record.tool.as_deref().unwrap_or("");
        let category = classify_tool(tool_name);
        *tool_distribution.entry(category.to_string()).or_default() += 1;
    }

    // File zones: only PreToolUse events for file-touching tools
    let mut file_counts: HashMap<String, u64> = HashMap::new();
    for record in session_records {
        if record.hook != HookType::PreToolUse {
            continue;
        }
        let path = record
            .tool
            .as_deref()
            .zip(record.input.as_ref())
            .and_then(|(tool, input)| extract_file_path(tool, input));
        if let Some(path) = path {
            let zone = extract_directory_zone(&path);
            *file_counts.entry(zone).or_default() += 1;
        }
    }

    // Top 5 file zones sorted by count descending, then alphabetically for ties
    let mut top_file_zones: Vec<(String, u64)> = file_counts.into_iter().collect();
    top_file_zones.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top_file_zones.truncate(5);

    // Agents spawned: SubagentStart events (FR-01.5)
    let mut agents_spawned: Vec<String> = Vec::new();
    for record in session_records {
        if record.hook != HookType::SubagentStart {
            continue;
        }
        if let Some(tool_name) = &record.tool {
            agents_spawned.push(tool_name.clone());
        }
    }

    // Knowledge flow: PreToolUse events only
    let knowledge_in = session_records
        .iter()
        .filter(|r| {
            r.hook == HookType::PreToolUse
                && matches!(
                    r.tool.as_deref(),
                    Some("context_search") | Some("context_lookup") | Some("context_get")
                )
        })
        .count() as u64;

    let knowledge_out = session_records
        .iter()
        .filter(|r| r.hook == HookType::PreToolUse && r.tool.as_deref() == Some("context_store"))
        .count() as u64;

    SessionSummary {
        session_id: session_id.to_string(),
        started_at: min_ts,
        duration_secs,
        tool_distribution,
        top_file_zones,
        agents_spawned,
        knowledge_in,
        knowledge_out,
        outcome: None, // populated by handler from SessionRecord
    }
}

/// Classify a tool name into a category for tool distribution.
fn classify_tool(tool: &str) -> &'static str {
    match tool {
        "Read" | "Glob" | "Grep" => "read",
        "Edit" | "Write" => "write",
        "Bash" => "execute",
        "context_search" | "context_lookup" | "context_get" => "search",
        "context_store" => "store",
        "SubagentStart" => "spawn",
        _ => "other",
    }
}

/// Extract a file path from a tool's input JSON per ADR-004 mapping.
fn extract_file_path(tool: &str, input: &serde_json::Value) -> Option<String> {
    match tool {
        "Read" | "Edit" | "Write" => input.get("file_path")?.as_str().map(String::from),
        "Glob" | "Grep" => input.get("path")?.as_str().map(String::from),
        _ => None,
    }
}

/// Extract the directory zone (first 3 path components from workspace root).
///
/// For `/workspaces/unimatrix/crates/unimatrix-store/src/read.rs`, returns `crates/unimatrix-store/src`.
fn extract_directory_zone(path: &str) -> String {
    // Strip common workspace prefix if present
    let stripped = if let Some(rest) = path.strip_prefix("/workspaces/unimatrix/") {
        rest
    } else if let Some(rest) = path.strip_prefix('/') {
        rest
    } else {
        path
    };

    let components: Vec<&str> = stripped.split('/').filter(|c| !c.is_empty()).collect();

    // If the path ends with '/' it refers to a directory, so all components are dirs.
    // Otherwise the last component is the filename and we take up to 3 dir components.
    let is_directory = stripped.ends_with('/');
    let dir_count = if is_directory {
        std::cmp::min(3, components.len())
    } else if components.len() > 1 {
        std::cmp::min(3, components.len() - 1)
    } else {
        components.len()
    };

    components[..dir_count].join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Helper to create an ObservationRecord with common defaults.
    fn make_record(
        session_id: &str,
        ts: u64,
        hook: HookType,
        tool: Option<&str>,
        input: Option<serde_json::Value>,
    ) -> ObservationRecord {
        ObservationRecord {
            ts,
            hook,
            session_id: session_id.to_string(),
            tool: tool.map(String::from),
            input,
            response_size: None,
            response_snippet: None,
        }
    }

    fn pre_tool(session_id: &str, ts: u64, tool: &str) -> ObservationRecord {
        make_record(session_id, ts, HookType::PreToolUse, Some(tool), None)
    }

    fn pre_tool_with_input(
        session_id: &str,
        ts: u64,
        tool: &str,
        input: serde_json::Value,
    ) -> ObservationRecord {
        make_record(
            session_id,
            ts,
            HookType::PreToolUse,
            Some(tool),
            Some(input),
        )
    }

    // ---- compute_session_summaries tests ----

    #[test]
    fn test_session_summaries_groups_by_session_id() {
        let records = vec![
            pre_tool("s1", 1000, "Read"),
            pre_tool("s1", 2000, "Read"),
            pre_tool("s1", 3000, "Edit"),
            pre_tool("s2", 4000, "Bash"),
            pre_tool("s2", 5000, "Read"),
            pre_tool("s2", 6000, "Read"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].session_id, "s1");
        assert_eq!(summaries[1].session_id, "s2");
    }

    #[test]
    fn test_session_summaries_empty_input() {
        let summaries = compute_session_summaries(&[]);
        assert!(summaries.is_empty());
    }

    #[test]
    fn test_session_summaries_single_record() {
        let records = vec![pre_tool("s1", 5000, "Read")];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].duration_secs, 0);
    }

    #[test]
    fn test_session_summaries_ordered_by_started_at() {
        let records = vec![
            pre_tool("s3", 300_000, "Read"),
            pre_tool("s1", 100_000, "Read"),
            pre_tool("s2", 200_000, "Read"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].session_id, "s1");
        assert_eq!(summaries[1].session_id, "s2");
        assert_eq!(summaries[2].session_id, "s3");
    }

    #[test]
    fn test_session_summaries_tiebreak_by_session_id() {
        let records = vec![
            pre_tool("beta", 1000, "Read"),
            pre_tool("alpha", 1000, "Read"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].session_id, "alpha");
        assert_eq!(summaries[1].session_id, "beta");
    }

    #[test]
    fn test_session_summaries_tool_distribution_categories() {
        let records = vec![
            pre_tool("s1", 1000, "Read"),
            pre_tool("s1", 1001, "Edit"),
            pre_tool("s1", 1002, "Bash"),
            pre_tool("s1", 1003, "context_search"),
            pre_tool("s1", 1004, "context_store"),
            pre_tool("s1", 1005, "SubagentStart"),
            pre_tool("s1", 1006, "UnknownTool"),
        ];
        let summaries = compute_session_summaries(&records);
        let dist = &summaries[0].tool_distribution;
        assert_eq!(dist.get("read"), Some(&1));
        assert_eq!(dist.get("write"), Some(&1));
        assert_eq!(dist.get("execute"), Some(&1));
        assert_eq!(dist.get("search"), Some(&1));
        assert_eq!(dist.get("store"), Some(&1));
        assert_eq!(dist.get("spawn"), Some(&1));
        assert_eq!(dist.get("other"), Some(&1));
    }

    #[test]
    fn test_session_summaries_filters_pretooluse_only() {
        let records = vec![
            pre_tool("s1", 1000, "Read"),
            pre_tool("s1", 2000, "Read"),
            make_record("s1", 3000, HookType::PostToolUse, Some("Read"), None),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].tool_distribution.get("read"), Some(&2));
    }

    #[test]
    fn test_session_summaries_knowledge_in_out() {
        let records = vec![
            pre_tool("s1", 1000, "context_search"),
            pre_tool("s1", 1001, "context_search"),
            pre_tool("s1", 1002, "context_search"),
            pre_tool("s1", 1003, "context_search"),
            pre_tool("s1", 1004, "context_search"),
            pre_tool("s1", 1005, "context_lookup"),
            pre_tool("s1", 1006, "context_lookup"),
            pre_tool("s1", 1007, "context_get"),
            pre_tool("s1", 1008, "context_store"),
            pre_tool("s1", 1009, "context_store"),
            pre_tool("s1", 1010, "context_store"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].knowledge_in, 8);
        assert_eq!(summaries[0].knowledge_out, 3);
    }

    #[test]
    fn test_session_summaries_agents_spawned() {
        let records = vec![
            make_record("s1", 1000, HookType::SubagentStart, Some("agent-a"), None),
            make_record("s1", 2000, HookType::SubagentStart, Some("agent-b"), None),
            make_record("s1", 3000, HookType::SubagentStart, Some("agent-c"), None),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].agents_spawned.len(), 3);
        assert!(summaries[0].agents_spawned.contains(&"agent-a".to_string()));
        assert!(summaries[0].agents_spawned.contains(&"agent-b".to_string()));
        assert!(summaries[0].agents_spawned.contains(&"agent-c".to_string()));
    }

    #[test]
    fn test_session_summaries_top_file_zones_max_5() {
        // Create records touching 7 distinct zones
        let records = vec![
            pre_tool_with_input(
                "s1",
                1000,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/a/src/lib.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1001,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/b/src/lib.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1002,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/c/src/lib.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1003,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/d/src/lib.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1004,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/e/src/lib.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1005,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/f/src/lib.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1006,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/g/src/lib.rs"}),
            ),
            // Extra hits for zones a and b to ensure ordering
            pre_tool_with_input(
                "s1",
                1007,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/a/src/main.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1008,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/a/src/types.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1009,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/b/src/main.rs"}),
            ),
        ];
        let summaries = compute_session_summaries(&records);
        let zones = &summaries[0].top_file_zones;
        assert_eq!(zones.len(), 5);
        // "crates/a/src" has 3 hits, should be first
        assert_eq!(zones[0].0, "crates/a/src");
        assert_eq!(zones[0].1, 3);
        // "crates/b/src" has 2 hits, should be second
        assert_eq!(zones[1].0, "crates/b/src");
        assert_eq!(zones[1].1, 2);
    }

    #[test]
    fn test_session_summaries_started_at_and_duration() {
        let records = vec![
            pre_tool("s1", 1000, "Read"),
            pre_tool("s1", 2000, "Read"),
            pre_tool("s1", 5000, "Read"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].started_at, 1000);
        assert_eq!(summaries[0].duration_secs, 4); // (5000 - 1000) / 1000
    }

    // ---- extract_file_path tests ----

    #[test]
    fn test_extract_file_path_read() {
        let input = json!({"file_path": "/foo/bar.rs"});
        assert_eq!(
            extract_file_path("Read", &input),
            Some("/foo/bar.rs".to_string())
        );
    }

    #[test]
    fn test_extract_file_path_edit() {
        let input = json!({"file_path": "/foo/bar.rs", "old_string": "x"});
        assert_eq!(
            extract_file_path("Edit", &input),
            Some("/foo/bar.rs".to_string())
        );
    }

    #[test]
    fn test_extract_file_path_write() {
        let input = json!({"file_path": "/foo/bar.rs", "content": "x"});
        assert_eq!(
            extract_file_path("Write", &input),
            Some("/foo/bar.rs".to_string())
        );
    }

    #[test]
    fn test_extract_file_path_glob() {
        let input = json!({"path": "/foo"});
        assert_eq!(extract_file_path("Glob", &input), Some("/foo".to_string()));
    }

    #[test]
    fn test_extract_file_path_grep() {
        let input = json!({"path": "/foo", "pattern": "test"});
        assert_eq!(extract_file_path("Grep", &input), Some("/foo".to_string()));
    }

    #[test]
    fn test_extract_file_path_unknown_tool() {
        let input = json!({"file_path": "/foo"});
        assert_eq!(extract_file_path("NewTool", &input), None);
    }

    #[test]
    fn test_extract_file_path_missing_field() {
        let input = json!({"other_field": "value"});
        assert_eq!(extract_file_path("Read", &input), None);
    }

    #[test]
    fn test_extract_file_path_non_string_value() {
        let input = json!({"file_path": 42});
        assert_eq!(extract_file_path("Read", &input), None);
    }

    // ---- classify_tool tests ----

    #[test]
    fn test_classify_tool_all_categories() {
        assert_eq!(classify_tool("Read"), "read");
        assert_eq!(classify_tool("Glob"), "read");
        assert_eq!(classify_tool("Grep"), "read");
        assert_eq!(classify_tool("Edit"), "write");
        assert_eq!(classify_tool("Write"), "write");
        assert_eq!(classify_tool("Bash"), "execute");
        assert_eq!(classify_tool("context_search"), "search");
        assert_eq!(classify_tool("context_lookup"), "search");
        assert_eq!(classify_tool("context_get"), "search");
        assert_eq!(classify_tool("context_store"), "store");
        assert_eq!(classify_tool("SubagentStart"), "spawn");
        assert_eq!(classify_tool("anything_else"), "other");
        assert_eq!(classify_tool(""), "other");
    }

    // ---- extract_directory_zone tests ----

    #[test]
    fn test_extract_directory_zone_absolute_path() {
        let zone = extract_directory_zone("/workspaces/unimatrix/crates/store/src/lib.rs");
        assert_eq!(zone, "crates/store/src");
    }

    #[test]
    fn test_extract_directory_zone_relative_path() {
        let zone = extract_directory_zone("crates/store/src/lib.rs");
        assert_eq!(zone, "crates/store/src");
    }

    #[test]
    fn test_extract_directory_zone_short_path() {
        let zone = extract_directory_zone("src/lib.rs");
        assert_eq!(zone, "src");
    }

    #[test]
    fn test_extract_directory_zone_trailing_slash() {
        let zone = extract_directory_zone("/workspaces/unimatrix/crates/store/src/");
        assert_eq!(zone, "crates/store/src");
    }

    // ---- compute_context_reload_pct tests ----

    #[test]
    fn test_reload_pct_basic() {
        // Session 1 reads files A, B, C. Session 2 reads B, C, D.
        let records = vec![
            pre_tool_with_input(
                "s1",
                1000,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/a.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1001,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/b.rs"}),
            ),
            pre_tool_with_input(
                "s1",
                1002,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/c.rs"}),
            ),
            pre_tool_with_input(
                "s2",
                2000,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/b.rs"}),
            ),
            pre_tool_with_input(
                "s2",
                2001,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/c.rs"}),
            ),
            pre_tool_with_input(
                "s2",
                2002,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/d.rs"}),
            ),
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        // B and C reloaded out of 3 files in session 2 = 2/3
        let expected = 2.0 / 3.0;
        assert!((pct - expected).abs() < 1e-10);
    }

    #[test]
    fn test_reload_pct_single_session() {
        let records = vec![pre_tool_with_input(
            "s1",
            1000,
            "Read",
            json!({"file_path": "/a.rs"}),
        )];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn test_reload_pct_no_files_in_later_sessions() {
        let records = vec![
            pre_tool_with_input("s1", 1000, "Read", json!({"file_path": "/a.rs"})),
            pre_tool_with_input("s1", 1001, "Read", json!({"file_path": "/b.rs"})),
            pre_tool("s2", 2000, "Bash"), // no file reads
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn test_reload_pct_full_overlap() {
        let records = vec![
            pre_tool_with_input("s1", 1000, "Read", json!({"file_path": "/a.rs"})),
            pre_tool_with_input("s1", 1001, "Read", json!({"file_path": "/b.rs"})),
            pre_tool_with_input("s2", 2000, "Read", json!({"file_path": "/a.rs"})),
            pre_tool_with_input("s2", 2001, "Read", json!({"file_path": "/b.rs"})),
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert_eq!(pct, 1.0);
    }

    #[test]
    fn test_reload_pct_no_overlap() {
        let records = vec![
            pre_tool_with_input("s1", 1000, "Read", json!({"file_path": "/a.rs"})),
            pre_tool_with_input("s1", 1001, "Read", json!({"file_path": "/b.rs"})),
            pre_tool_with_input("s2", 2000, "Read", json!({"file_path": "/c.rs"})),
            pre_tool_with_input("s2", 2001, "Read", json!({"file_path": "/d.rs"})),
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn test_reload_pct_range() {
        // Verify result is always in [0.0, 1.0]
        let records = vec![
            pre_tool_with_input("s1", 1000, "Read", json!({"file_path": "/a.rs"})),
            pre_tool_with_input("s2", 2000, "Read", json!({"file_path": "/a.rs"})),
            pre_tool_with_input("s2", 2001, "Read", json!({"file_path": "/b.rs"})),
            pre_tool_with_input("s3", 3000, "Read", json!({"file_path": "/a.rs"})),
            pre_tool_with_input("s3", 3001, "Read", json!({"file_path": "/c.rs"})),
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert!(pct >= 0.0);
        assert!(pct <= 1.0);
    }
}
