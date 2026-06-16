//! Per-session activity profile computation and cross-session context reload rate (col-020).
//!
//! Pure computation on `ObservationRecord` arrays. No database access.

use std::collections::HashMap;

use crate::types::{ObservationRecord, SessionSummary};

/// Compute per-session activity profiles from observation records.
///
/// Groups records by `session_id`, computes tool distribution (PostToolUse only),
/// top file zones, agents spawned, and knowledge flow counts. Returns summaries
/// sorted by `started_at` ascending with lexicographic `session_id` tiebreaker.
///
/// Per-tool aggregation reads `PostToolUse` events (the durable per-tool event
/// under the TS UDS client, ADR-004 / vnc-028). The retired non-cycle `PreToolUse`
/// event is no longer emitted by the active client (#750).
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

    // CROSS-SESSION caller of the one shared overlap primitive (crt-055 Component 4).
    // The fraction is reload_files / total_files_in_subsequent; the primitive owns the
    // file-set-intersection walk and is parameterized by [`ReloadWindow::CrossSession`].
    let counts = crate::reload_overlap::overlap_count(
        records,
        crate::reload_overlap::ReloadWindow::CrossSession,
        summaries,
    );

    // Division by zero guard (R-13)
    if counts.total == 0 {
        return 0.0;
    }

    counts.overlap as f64 / counts.total as f64
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

    // Tool distribution: only PostToolUse events (FR-01.2; #750 — PostToolUse is the
    // durable per-tool event under the TS UDS client, ADR-004 / vnc-028).
    let mut tool_distribution: HashMap<String, u64> = HashMap::new();
    for record in session_records {
        if record.event_type != "PostToolUse" {
            continue;
        }
        let tool_name = record.tool.as_deref().unwrap_or("");
        let category = classify_tool(tool_name);
        *tool_distribution.entry(category.to_string()).or_default() += 1;
    }

    // File zones: only PostToolUse events for file-touching tools (#750)
    let mut file_counts: HashMap<String, u64> = HashMap::new();
    for record in session_records {
        if record.event_type != "PostToolUse" {
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
        if record.event_type != "SubagentStart" {
            continue;
        }
        if let Some(tool_name) = &record.tool {
            agents_spawned.push(tool_name.clone());
        }
    }

    // Knowledge flow: PostToolUse events only (#750 — surviving per-tool event)
    let knowledge_served = session_records
        .iter()
        .filter(|r| {
            r.event_type == "PostToolUse"
                && r.tool.as_deref().map(normalize_tool_name).is_some_and(|t| {
                    matches!(t, "context_search" | "context_lookup" | "context_get")
                })
        })
        .count() as u64;

    let knowledge_stored = session_records
        .iter()
        .filter(|r| {
            r.event_type == "PostToolUse"
                && r.tool
                    .as_deref()
                    .map(normalize_tool_name)
                    .is_some_and(|t| t == "context_store")
        })
        .count() as u64;

    let knowledge_curated = session_records
        .iter()
        .filter(|r| {
            r.event_type == "PostToolUse"
                && r.tool.as_deref().map(normalize_tool_name).is_some_and(|t| {
                    matches!(
                        t,
                        "context_correct" | "context_deprecate" | "context_quarantine"
                    )
                })
        })
        .count() as u64;

    SessionSummary {
        session_id: session_id.to_string(),
        started_at: min_ts,
        duration_secs,
        tool_distribution,
        top_file_zones,
        agents_spawned,
        knowledge_served,
        knowledge_stored,
        knowledge_curated,
        outcome: None, // populated by handler from SessionRecord
    }
}

/// Strip MCP server prefix from tool names.
/// Returns the bare tool name for Unimatrix MCP tools,
/// or the input unchanged for Claude-native tools.
pub fn normalize_tool_name(tool: &str) -> &str {
    tool.strip_prefix("mcp__unimatrix__").unwrap_or(tool)
}

/// Classify a tool name into a category for tool distribution.
fn classify_tool(tool: &str) -> &'static str {
    let normalized = normalize_tool_name(tool);
    match normalized {
        "Read" | "Glob" | "Grep" => "read",
        "Edit" | "Write" => "write",
        "Bash" => "execute",
        "context_search" | "context_lookup" | "context_get" => "search",
        "context_store" => "store",
        "context_correct" | "context_deprecate" | "context_quarantine" => "curate",
        "SubagentStart" => "spawn",
        _ => "other",
    }
}

/// Extract a file path from a tool's input JSON per ADR-004 mapping.
pub(crate) fn extract_file_path(tool: &str, input: &serde_json::Value) -> Option<String> {
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
        event_type: &str,
        tool: Option<&str>,
        input: Option<serde_json::Value>,
    ) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: event_type.to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: tool.map(String::from),
            input,
            response_size: None,
            response_snippet: None,
        }
    }

    /// Per-tool event helper. Emits `PostToolUse` — the durable per-tool event under
    /// the TS UDS client (ADR-004 / vnc-028, #750). The retired non-cycle `PreToolUse`
    /// event is no longer emitted by the active client, and the aggregation read-path
    /// now filters on `PostToolUse`.
    fn post_tool(session_id: &str, ts: u64, tool: &str) -> ObservationRecord {
        make_record(session_id, ts, "PostToolUse", Some(tool), None)
    }

    fn post_tool_with_input(
        session_id: &str,
        ts: u64,
        tool: &str,
        input: serde_json::Value,
    ) -> ObservationRecord {
        make_record(session_id, ts, "PostToolUse", Some(tool), Some(input))
    }

    // ---- compute_session_summaries tests ----

    #[test]
    fn test_session_summaries_groups_by_session_id() {
        let records = vec![
            post_tool("s1", 1000, "Read"),
            post_tool("s1", 2000, "Read"),
            post_tool("s1", 3000, "Edit"),
            post_tool("s2", 4000, "Bash"),
            post_tool("s2", 5000, "Read"),
            post_tool("s2", 6000, "Read"),
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
        let records = vec![post_tool("s1", 5000, "Read")];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].duration_secs, 0);
    }

    #[test]
    fn test_session_summaries_ordered_by_started_at() {
        let records = vec![
            post_tool("s3", 300_000, "Read"),
            post_tool("s1", 100_000, "Read"),
            post_tool("s2", 200_000, "Read"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].session_id, "s1");
        assert_eq!(summaries[1].session_id, "s2");
        assert_eq!(summaries[2].session_id, "s3");
    }

    #[test]
    fn test_session_summaries_tiebreak_by_session_id() {
        let records = vec![
            post_tool("beta", 1000, "Read"),
            post_tool("alpha", 1000, "Read"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].session_id, "alpha");
        assert_eq!(summaries[1].session_id, "beta");
    }

    #[test]
    fn test_session_summaries_tool_distribution_categories() {
        let records = vec![
            post_tool("s1", 1000, "Read"),
            post_tool("s1", 1001, "Edit"),
            post_tool("s1", 1002, "Bash"),
            post_tool("s1", 1003, "context_search"),
            post_tool("s1", 1004, "context_store"),
            post_tool("s1", 1005, "SubagentStart"),
            post_tool("s1", 1006, "UnknownTool"),
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
    fn test_session_summaries_filters_posttooluse_only() {
        // #750: per-tool aggregation reads PostToolUse (the surviving event under the
        // TS UDS client). PreToolUse rows — which the active client no longer emits —
        // must NOT be counted, so a stray PreToolUse must be ignored.
        let records = vec![
            post_tool("s1", 1000, "Read"),
            post_tool("s1", 2000, "Read"),
            make_record("s1", 3000, "PreToolUse", Some("Read"), None),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].tool_distribution.get("read"), Some(&2));
    }

    #[test]
    fn test_session_summaries_knowledge_served_stored() {
        let records = vec![
            post_tool("s1", 1000, "context_search"),
            post_tool("s1", 1001, "context_search"),
            post_tool("s1", 1002, "context_search"),
            post_tool("s1", 1003, "context_search"),
            post_tool("s1", 1004, "context_search"),
            post_tool("s1", 1005, "context_lookup"),
            post_tool("s1", 1006, "context_lookup"),
            post_tool("s1", 1007, "context_get"),
            post_tool("s1", 1008, "context_store"),
            post_tool("s1", 1009, "context_store"),
            post_tool("s1", 1010, "context_store"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].knowledge_served, 8);
        assert_eq!(summaries[0].knowledge_stored, 3);
    }

    #[test]
    fn test_session_summaries_agents_spawned() {
        let records = vec![
            make_record("s1", 1000, "SubagentStart", Some("agent-a"), None),
            make_record("s1", 2000, "SubagentStart", Some("agent-b"), None),
            make_record("s1", 3000, "SubagentStart", Some("agent-c"), None),
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
            post_tool_with_input(
                "s1",
                1000,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/a/src/lib.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1001,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/b/src/lib.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1002,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/c/src/lib.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1003,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/d/src/lib.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1004,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/e/src/lib.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1005,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/f/src/lib.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1006,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/g/src/lib.rs"}),
            ),
            // Extra hits for zones a and b to ensure ordering
            post_tool_with_input(
                "s1",
                1007,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/a/src/main.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1008,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/a/src/types.rs"}),
            ),
            post_tool_with_input(
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
            post_tool("s1", 1000, "Read"),
            post_tool("s1", 2000, "Read"),
            post_tool("s1", 5000, "Read"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].started_at, 1000);
        assert_eq!(summaries[0].duration_secs, 4); // (5000 - 1000) / 1000
    }

    // ---- normalize_tool_name tests ----

    #[test]
    fn test_normalize_tool_name_standard_prefix() {
        assert_eq!(
            normalize_tool_name("mcp__unimatrix__context_search"),
            "context_search"
        );
    }

    #[test]
    fn test_normalize_tool_name_passthrough_bare() {
        assert_eq!(normalize_tool_name("context_search"), "context_search");
    }

    #[test]
    fn test_normalize_tool_name_passthrough_claude_native() {
        assert_eq!(normalize_tool_name("Read"), "Read");
    }

    #[test]
    fn test_normalize_tool_name_double_prefix() {
        assert_eq!(
            normalize_tool_name("mcp__unimatrix__mcp__unimatrix__context_search"),
            "mcp__unimatrix__context_search"
        );
    }

    #[test]
    fn test_normalize_tool_name_prefix_only() {
        assert_eq!(normalize_tool_name("mcp__unimatrix__"), "");
    }

    #[test]
    fn test_normalize_tool_name_empty_string() {
        assert_eq!(normalize_tool_name(""), "");
    }

    #[test]
    fn test_normalize_tool_name_case_sensitive() {
        assert_eq!(
            normalize_tool_name("MCP__UNIMATRIX__context_search"),
            "MCP__UNIMATRIX__context_search"
        );
    }

    #[test]
    fn test_normalize_tool_name_different_server() {
        assert_eq!(
            normalize_tool_name("mcp__other_server__context_search"),
            "mcp__other_server__context_search"
        );
    }

    // ---- classify_tool tests (extended) ----

    #[test]
    fn test_classify_tool_mcp_prefixed() {
        assert_eq!(classify_tool("mcp__unimatrix__context_search"), "search");
        assert_eq!(classify_tool("mcp__unimatrix__context_lookup"), "search");
        assert_eq!(classify_tool("mcp__unimatrix__context_get"), "search");
        assert_eq!(classify_tool("mcp__unimatrix__context_store"), "store");
        assert_eq!(classify_tool("mcp__unimatrix__context_correct"), "curate");
        assert_eq!(classify_tool("mcp__unimatrix__context_deprecate"), "curate");
        assert_eq!(
            classify_tool("mcp__unimatrix__context_quarantine"),
            "curate"
        );
    }

    #[test]
    fn test_classify_tool_admin_tools_are_other() {
        assert_eq!(classify_tool("context_briefing"), "other");
        assert_eq!(classify_tool("context_status"), "other");
        assert_eq!(classify_tool("context_enroll"), "other");
        assert_eq!(classify_tool("context_cycle_review"), "other");
        assert_eq!(classify_tool("mcp__unimatrix__context_briefing"), "other");
        assert_eq!(classify_tool("mcp__unimatrix__context_status"), "other");
    }

    // ---- knowledge curated counter tests ----

    #[test]
    fn test_session_summaries_mcp_prefixed_knowledge_flow() {
        let records = vec![
            post_tool("s1", 1000, "mcp__unimatrix__context_search"),
            post_tool("s1", 1001, "mcp__unimatrix__context_search"),
            post_tool("s1", 1002, "mcp__unimatrix__context_lookup"),
            post_tool("s1", 1003, "mcp__unimatrix__context_get"),
            post_tool("s1", 1004, "mcp__unimatrix__context_store"),
            post_tool("s1", 1005, "mcp__unimatrix__context_store"),
            post_tool("s1", 1006, "mcp__unimatrix__context_correct"),
            post_tool("s1", 1007, "mcp__unimatrix__context_deprecate"),
            post_tool("s1", 1008, "mcp__unimatrix__context_quarantine"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].knowledge_served, 4);
        assert_eq!(summaries[0].knowledge_stored, 2);
        assert_eq!(summaries[0].knowledge_curated, 3);
    }

    #[test]
    fn test_session_summaries_mixed_bare_and_prefixed() {
        let records = vec![
            post_tool("s1", 1000, "context_search"),
            post_tool("s1", 1001, "mcp__unimatrix__context_search"),
            post_tool("s1", 1002, "context_store"),
            post_tool("s1", 1003, "mcp__unimatrix__context_store"),
            post_tool("s1", 1004, "context_correct"),
            post_tool("s1", 1005, "mcp__unimatrix__context_correct"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].knowledge_served, 2);
        assert_eq!(summaries[0].knowledge_stored, 2);
        assert_eq!(summaries[0].knowledge_curated, 2);
    }

    #[test]
    fn test_session_summaries_curate_in_tool_distribution() {
        let records = vec![
            post_tool("s1", 1000, "mcp__unimatrix__context_correct"),
            post_tool("s1", 1001, "mcp__unimatrix__context_deprecate"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].tool_distribution.get("curate"), Some(&2));
    }

    #[test]
    fn test_session_summaries_no_curate_without_curation_tools() {
        let records = vec![
            post_tool("s1", 1000, "Read"),
            post_tool("s1", 1001, "context_search"),
        ];
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries[0].tool_distribution.get("curate"), None);
        assert_eq!(summaries[0].knowledge_curated, 0);
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
        assert_eq!(classify_tool("context_correct"), "curate");
        assert_eq!(classify_tool("context_deprecate"), "curate");
        assert_eq!(classify_tool("context_quarantine"), "curate");
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
            post_tool_with_input(
                "s1",
                1000,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/a.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1001,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/b.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1002,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/c.rs"}),
            ),
            post_tool_with_input(
                "s2",
                2000,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/b.rs"}),
            ),
            post_tool_with_input(
                "s2",
                2001,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/c.rs"}),
            ),
            post_tool_with_input(
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
        let records = vec![post_tool_with_input(
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
            post_tool_with_input("s1", 1000, "Read", json!({"file_path": "/a.rs"})),
            post_tool_with_input("s1", 1001, "Read", json!({"file_path": "/b.rs"})),
            post_tool("s2", 2000, "Bash"), // no file reads
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn test_reload_pct_full_overlap() {
        let records = vec![
            post_tool_with_input("s1", 1000, "Read", json!({"file_path": "/a.rs"})),
            post_tool_with_input("s1", 1001, "Read", json!({"file_path": "/b.rs"})),
            post_tool_with_input("s2", 2000, "Read", json!({"file_path": "/a.rs"})),
            post_tool_with_input("s2", 2001, "Read", json!({"file_path": "/b.rs"})),
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert_eq!(pct, 1.0);
    }

    #[test]
    fn test_reload_pct_no_overlap() {
        let records = vec![
            post_tool_with_input("s1", 1000, "Read", json!({"file_path": "/a.rs"})),
            post_tool_with_input("s1", 1001, "Read", json!({"file_path": "/b.rs"})),
            post_tool_with_input("s2", 2000, "Read", json!({"file_path": "/c.rs"})),
            post_tool_with_input("s2", 2001, "Read", json!({"file_path": "/d.rs"})),
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn test_reload_pct_range() {
        // Verify result is always in [0.0, 1.0]
        let records = vec![
            post_tool_with_input("s1", 1000, "Read", json!({"file_path": "/a.rs"})),
            post_tool_with_input("s2", 2000, "Read", json!({"file_path": "/a.rs"})),
            post_tool_with_input("s2", 2001, "Read", json!({"file_path": "/b.rs"})),
            post_tool_with_input("s3", 3000, "Read", json!({"file_path": "/a.rs"})),
            post_tool_with_input("s3", 3001, "Read", json!({"file_path": "/c.rs"})),
        ];
        let summaries = compute_session_summaries(&records);
        let pct = compute_context_reload_pct(&summaries, &records);
        assert!(pct >= 0.0);
        assert!(pct <= 1.0);
    }

    // ---- #750 believable-zero guard (ADR-009, #5007) ----

    /// Build a representative TS-client event stream: each tool call appears as a
    /// `PostToolUse` row carrying `tool` + `input.file_path`, exactly as the TS UDS
    /// client emits (verified live: Read/Edit/Write PostToolUse rows carry a populated
    /// `file_path`). The non-cycle `PreToolUse` event is NOT present — the active client
    /// retired it (ADR-004 / vnc-028). This is the stream that previously folded every
    /// per-session counter to a believable zero.
    fn ts_client_stream() -> Vec<ObservationRecord> {
        vec![
            // Session 1: reads two files, edits one, stores knowledge.
            post_tool_with_input(
                "s1",
                1000,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/store/src/lib.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1001,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/store/src/read.rs"}),
            ),
            post_tool_with_input(
                "s1",
                1002,
                "Edit",
                json!({"file_path": "/workspaces/unimatrix/crates/store/src/lib.rs"}),
            ),
            post_tool("s1", 1003, "context_store"),
            // Session 2 (later): re-reads one file from session 1 (reload) plus a new one.
            post_tool_with_input(
                "s2",
                2000,
                "Read",
                json!({"file_path": "/workspaces/unimatrix/crates/store/src/lib.rs"}),
            ),
            post_tool_with_input(
                "s2",
                2001,
                "Write",
                json!({"file_path": "/workspaces/unimatrix/crates/store/src/new.rs"}),
            ),
            post_tool("s2", 2002, "context_search"),
        ]
    }

    #[test]
    fn test_session_aggregation_counts_posttooluse_not_zero() {
        // Regression guard for #750: under the TS UDS client (PostToolUse-only),
        // per-session Calls/Tools/Knowledge and context_reload must be NON-ZERO.
        let records = ts_client_stream();
        let summaries = compute_session_summaries(&records);
        assert_eq!(summaries.len(), 2, "two sessions expected");

        // Session 1: Calls (tool_distribution non-empty), Tools categorized.
        let s1 = &summaries[0];
        assert_eq!(s1.session_id, "s1");
        let s1_calls: u64 = s1.tool_distribution.values().sum();
        assert!(
            s1_calls > 0,
            "per-session Calls must be non-zero (believable-zero guard, #750)"
        );
        assert!(
            !s1.tool_distribution.is_empty(),
            "Tools distribution must be non-empty"
        );
        assert_eq!(s1.tool_distribution.get("read"), Some(&2));
        assert_eq!(s1.tool_distribution.get("write"), Some(&1));
        assert_eq!(s1.tool_distribution.get("store"), Some(&1));
        assert!(
            !s1.top_file_zones.is_empty(),
            "top_file_zones must be populated from PostToolUse file_path"
        );
        assert_eq!(
            s1.knowledge_stored, 1,
            "knowledge_stored must count context_store"
        );

        // Session 2: knowledge_served counted from PostToolUse context_search.
        let s2 = &summaries[1];
        assert_eq!(
            s2.knowledge_served, 1,
            "knowledge_served must count context_search"
        );

        // context_reload: lib.rs read in s1 and re-read in s2 → non-zero overlap.
        let reload = compute_context_reload_pct(&summaries, &records);
        assert!(
            reload > 0.0,
            "context_reload must be non-zero on cross-session file overlap (#750), got {reload}"
        );
    }

    /// Contract guard: the `event_type` literal the per-session read-path filters on
    /// MUST be `PostToolUse` — the event the active TS UDS client emits per tool call
    /// (ADR-004 / vnc-028). If a future client retires/renames `PostToolUse`, the
    /// aggregation read-path filter must be re-audited rather than silently zeroing
    /// these metrics. A stream of ONLY the read-path's expected event type must
    /// produce non-zero aggregation; a stream of the retired `PreToolUse` event must not.
    #[test]
    fn test_read_path_filter_matches_active_client_event() {
        // The event_type the active client emits per tool call.
        const ACTIVE_CLIENT_PER_TOOL_EVENT: &str = "PostToolUse";

        let active = make_record("s1", 1000, ACTIVE_CLIENT_PER_TOOL_EVENT, Some("Read"), None);
        let active_summary = compute_session_summaries(std::slice::from_ref(&active));
        assert_eq!(
            active_summary[0].tool_distribution.get("read"),
            Some(&1),
            "read-path filter must count the active client's per-tool event type ({ACTIVE_CLIENT_PER_TOOL_EVENT})"
        );

        // The retired event must NOT be counted (it is no longer emitted; counting it
        // would re-ground the metric on a dead event class).
        let retired = make_record("s1", 1000, "PreToolUse", Some("Read"), None);
        let retired_summary = compute_session_summaries(std::slice::from_ref(&retired));
        assert_eq!(
            retired_summary[0].tool_distribution.get("read"),
            None,
            "retired PreToolUse event must not be counted by the read-path filter"
        );
    }
}
