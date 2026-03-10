# C1: Session Metrics (unimatrix-observe/src/session_metrics.rs)

## Purpose

Compute per-session activity profiles and cross-session context reload rate from ObservationRecord arrays. Pure computation -- no database access. New file in unimatrix-observe.

## Module Declaration

Add `pub mod session_metrics;` to `unimatrix-observe/src/lib.rs`.

Re-export public functions:
```
pub use session_metrics::{compute_session_summaries, compute_context_reload_pct};
```

## Function: compute_session_summaries

```
pub fn compute_session_summaries(records: &[ObservationRecord]) -> Vec<SessionSummary>
```

### Algorithm

```
function compute_session_summaries(records):
    // Group records by session_id
    let groups: HashMap<String, Vec<&ObservationRecord>> = group records by session_id

    let summaries: Vec<SessionSummary> = empty vec

    for each (session_id, session_records) in groups:
        // Timestamps
        let min_ts = minimum ts across session_records
        let max_ts = maximum ts across session_records
        let started_at = min_ts
        let duration_secs = (max_ts - min_ts) / 1000  // ts is epoch millis

        // Tool distribution: only PreToolUse events (FR-01.2)
        let tool_distribution: HashMap<String, u64> = empty
        for each record in session_records where record.hook == PreToolUse:
            let category = classify_tool(record.tool.as_deref().unwrap_or(""))
            tool_distribution[category] += 1

        // File zones: only PreToolUse events for file-touching tools
        let file_counts: HashMap<String, u64> = empty
        for each record in session_records where record.hook == PreToolUse:
            if let Some(tool_name) = &record.tool:
                if let Some(input) = &record.input:
                    if let Some(path) = extract_file_path(tool_name, input):
                        let zone = extract_directory_zone(&path)
                        file_counts[zone] += 1

        // Top 5 file zones sorted by count descending, then alphabetically for ties
        let mut top_file_zones: Vec<(String, u64)> = file_counts into sorted vec
        sort by count desc, then zone name asc
        truncate to 5

        // Agents spawned: SubagentStart events (FR-01.5)
        let agents_spawned: Vec<String> = empty
        for each record in session_records where record.hook == SubagentStart:
            if let Some(tool_name) = &record.tool:
                agents_spawned.push(tool_name.clone())

        // Knowledge flow: PreToolUse events only
        let knowledge_in = count of PreToolUse records where tool in
            ["context_search", "context_lookup", "context_get"]
        let knowledge_out = count of PreToolUse records where tool == "context_store"

        summaries.push(SessionSummary {
            session_id,
            started_at,
            duration_secs,
            tool_distribution,
            top_file_zones,
            agents_spawned,
            knowledge_in,
            knowledge_out,
            outcome: None,  // populated by handler from SessionRecord
        })

    // Sort by started_at ascending, session_id lexicographic tiebreaker (FR-01.9)
    summaries.sort_by(|a, b|
        a.started_at.cmp(&b.started_at)
            .then_with(|| a.session_id.cmp(&b.session_id))
    )

    return summaries
```

## Function: compute_context_reload_pct

```
pub fn compute_context_reload_pct(
    summaries: &[SessionSummary],
    records: &[ObservationRecord],
) -> f64
```

### Algorithm

```
function compute_context_reload_pct(summaries, records):
    // summaries must already be sorted by started_at (from compute_session_summaries)
    if summaries.len() <= 1:
        return 0.0  // single session or empty: no prior session to reload from

    // Build per-session file sets from observation records
    // Group records by session_id
    let session_files: HashMap<String, HashSet<String>> = empty
    for each record in records where record.hook == PreToolUse:
        if let Some(tool_name) = &record.tool:
            if let Some(input) = &record.input:
                if let Some(path) = extract_file_path(tool_name, input):
                    session_files[record.session_id].insert(path)

    // Walk sessions in chronological order
    // Track cumulative set of files seen in prior sessions
    let prior_files: HashSet<String> = empty
    let total_files_in_subsequent = 0u64
    let reload_files = 0u64

    for each summary in summaries:
        let current_files = session_files.get(&summary.session_id).unwrap_or_empty()

        if !prior_files.is_empty():
            // This is not the first session
            for each file in current_files:
                total_files_in_subsequent += 1
                if prior_files.contains(file):
                    reload_files += 1

        // Add current session's files to prior set
        prior_files.extend(current_files)

    // Division by zero guard (R-13)
    if total_files_in_subsequent == 0:
        return 0.0

    return reload_files as f64 / total_files_in_subsequent as f64
```

## Internal Helper: classify_tool

```
fn classify_tool(tool: &str) -> &'static str
```

```
function classify_tool(tool):
    match tool:
        "Read" | "Glob" | "Grep" => "read"
        "Edit" | "Write" => "write"
        "Bash" => "execute"
        "context_search" | "context_lookup" | "context_get" => "search"
        "context_store" => "store"
        "SubagentStart" => "spawn"
        _ => "other"
```

## Internal Helper: extract_file_path

```
fn extract_file_path(tool: &str, input: &serde_json::Value) -> Option<String>
```

Per ADR-004: explicit tool-to-field mapping.

```
function extract_file_path(tool, input):
    match tool:
        "Read" | "Edit" | "Write" =>
            input.get("file_path")?.as_str().map(String::from)
        "Glob" | "Grep" =>
            input.get("path")?.as_str().map(String::from)
        _ => None
```

## Internal Helper: extract_directory_zone

```
fn extract_directory_zone(path: &str) -> String
```

Extract first 3 path components from workspace root. Handles both absolute paths (strips workspace prefix) and relative paths.

```
function extract_directory_zone(path):
    // Strip common workspace prefix if present
    let stripped = if path starts with "/workspaces/unimatrix/":
        path after "/workspaces/unimatrix/"
    else if path starts with "/":
        path after first "/"
    else:
        path

    // Take first 3 path components (directories, not the file)
    let components: Vec<&str> = stripped.split('/').collect()

    // If the path has a file at the end (last component has a dot or is a filename),
    // take up to 3 directory components
    // Simple approach: take min(3, components.len() - 1) components
    // (the last component is typically the filename)
    let dir_count = if components.len() > 1:
        min(3, components.len() - 1)
    else:
        components.len()

    return components[..dir_count].join("/")
```

Note: The architecture says "first 3 path components from workspace root" (line 184). For `/workspaces/unimatrix/crates/unimatrix-store/src/read.rs`, the zone is `crates/unimatrix-store/src`.

## Error Handling

These are pure functions that cannot fail (no Result return type). Edge cases handled:
- Empty records: returns empty Vec / 0.0
- Missing tool name: treated as "other" category, no file path extracted
- Missing/non-string input fields: extract_file_path returns None, silently skipped
- Single session: reload_pct returns 0.0
- Sessions with no file reads: zero contribution to reload denominator

## Key Test Scenarios

1. **Multi-session grouping (AC-01)**: Records with 3 distinct session_ids produce 3 SessionSummary items.
2. **Tool distribution categories (AC-02)**: Records with Read, Edit, Bash, context_search tools produce correct category counts. Only PreToolUse events counted.
3. **PostToolUse filtering**: Records with PostToolUse hook are excluded from tool_distribution.
4. **Top file zones (AC-03)**: Records touching 7 directories, verify top 5 by frequency returned.
5. **Agents spawned (AC-04)**: SubagentStart records contribute tool names to agents_spawned.
6. **Knowledge flow (AC-05)**: Mix of search/store calls produces correct knowledge_in/knowledge_out.
7. **Session ordering (AC-16)**: Out-of-order records produce summaries sorted by started_at.
8. **Identical timestamps tiebreaker (R-07)**: Two sessions with same started_at ordered by session_id.
9. **Reload rate -- full overlap (AC-10)**: Two sessions reading same files: reload_pct = 1.0.
10. **Reload rate -- no overlap**: Two sessions reading different files: reload_pct = 0.0.
11. **Reload rate -- single session**: Returns 0.0 (FR-04.3).
12. **Reload rate -- zero files after first session (R-13)**: Returns 0.0, no division by zero.
13. **Empty records (R-10)**: Empty input produces empty Vec and 0.0.
14. **File path extraction -- all mapped tools (R-06)**: Read, Edit, Write, Glob, Grep all extract correctly.
15. **File path extraction -- unknown tool**: Returns None, no panic.
16. **File path extraction -- missing field**: Returns None.
17. **File path extraction -- non-string field (R-06)**: `file_path` is a number, returns None.
18. **Directory zone -- absolute path (R-15)**: `/workspaces/unimatrix/crates/store/src/lib.rs` -> `crates/store/src`.
19. **Directory zone -- relative path (R-15)**: `crates/store/src/lib.rs` -> `crates/store/src`.
20. **Duration calculation**: Session with ts range 1000ms to 5000ms has duration_secs = 4.
