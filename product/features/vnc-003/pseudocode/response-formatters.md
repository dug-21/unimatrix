# Pseudocode: C3 Response Formatting Extensions

## File: `crates/unimatrix-server/src/response.rs`

### New Data Structures

```rust
/// Aggregated health metrics for format_status_report.
pub struct StatusReport {
    pub total_active: u64,
    pub total_deprecated: u64,
    pub total_proposed: u64,
    pub category_distribution: Vec<(String, u64)>,
    pub topic_distribution: Vec<(String, u64)>,
    pub entries_with_supersedes: u64,
    pub entries_with_superseded_by: u64,
    pub total_correction_count: u64,
    pub trust_source_distribution: Vec<(String, u64)>,
    pub entries_without_attribution: u64,
}

/// Assembled briefing for format_briefing.
pub struct Briefing {
    pub role: String,
    pub task: String,
    pub conventions: Vec<EntryRecord>,
    pub duties: Vec<EntryRecord>,
    pub relevant_context: Vec<(EntryRecord, f32)>,
    pub search_available: bool,
}
```

### New Function: `format_correct_success`

```
pub fn format_correct_success(
    original: &EntryRecord,
    correction: &EntryRecord,
    format: ResponseFormat,
) -> CallToolResult:
    match format:
        Summary:
            text = "Corrected #{original.id} -> #{correction.id} | {correction.title} | {correction.category}"
            return CallToolResult::success(vec![Content::text(text)])

        Markdown:
            text = "## Correction Applied\n\n"
            text += "**Original (deprecated):** #{original.id} - {original.title}\n"
            text += "**Correction:** #{correction.id} - {correction.title}\n\n"
            text += "### Corrected Entry\n"
            text += format_entry_markdown_section(1, correction, None)
            return CallToolResult::success(vec![Content::text(text)])

        Json:
            obj = {
                "corrected": true,
                "original": entry_to_json(original),  // includes status: "deprecated"
                "correction": entry_to_json(correction)
            }
            return CallToolResult::success(vec![Content::text(to_string_pretty(obj))])
```

### New Function: `format_deprecate_success`

```
pub fn format_deprecate_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult:
    match format:
        Summary:
            text = "Deprecated #{entry.id} | {entry.title}"
            return CallToolResult::success(vec![Content::text(text)])

        Markdown:
            text = "## Entry Deprecated\n\n"
            text += "**Entry:** #{entry.id} - {entry.title}\n"
            text += "**Status:** deprecated\n"
            if reason.is_some():
                text += "**Reason:** {reason}\n"
            return CallToolResult::success(vec![Content::text(text)])

        Json:
            obj = {
                "deprecated": true,
                "entry": entry_to_json(entry),
                "reason": reason or null
            }
            return CallToolResult::success(vec![Content::text(to_string_pretty(obj))])
```

### New Function: `format_status_report`

```
pub fn format_status_report(
    report: &StatusReport,
    format: ResponseFormat,
) -> CallToolResult:
    match format:
        Summary:
            text = "Active: {total_active} | Deprecated: {total_deprecated} | Proposed: {total_proposed} | Corrections: {total_correction_count}"
            return CallToolResult::success(vec![Content::text(text)])

        Markdown:
            text = "## Knowledge Base Status\n\n"
            text += "### Entry Counts\n"
            text += "| Status | Count |\n|--------|-------|\n"
            text += "| Active | {total_active} |\n"
            text += "| Deprecated | {total_deprecated} |\n"
            text += "| Proposed | {total_proposed} |\n\n"

            text += "### Category Distribution\n"
            text += "| Category | Count |\n|----------|-------|\n"
            for (cat, count) in category_distribution:
                text += "| {cat} | {count} |\n"

            text += "\n### Topic Distribution\n"
            text += "| Topic | Count |\n|-------|-------|\n"
            for (topic, count) in topic_distribution:
                text += "| {topic} | {count} |\n"

            text += "\n### Correction Chains\n"
            text += "- Entries with supersedes: {entries_with_supersedes}\n"
            text += "- Entries with superseded_by: {entries_with_superseded_by}\n"
            text += "- Total correction count: {total_correction_count}\n"

            text += "\n### Security Metrics\n"
            text += "| Trust Source | Count |\n|-------------|-------|\n"
            for (source, count) in trust_source_distribution:
                text += "| {source} | {count} |\n"
            text += "\n- Entries without attribution: {entries_without_attribution}\n"

            return CallToolResult::success(vec![Content::text(text)])

        Json:
            obj = {
                "total_active": ...,
                "total_deprecated": ...,
                "total_proposed": ...,
                "category_distribution": { cat: count, ... },
                "topic_distribution": { topic: count, ... },
                "correction_chains": {
                    "entries_with_supersedes": ...,
                    "entries_with_superseded_by": ...,
                    "total_correction_count": ...
                },
                "security": {
                    "trust_source_distribution": { source: count, ... },
                    "entries_without_attribution": ...
                }
            }
            return CallToolResult::success(vec![Content::text(to_string_pretty(obj))])
```

### New Function: `format_briefing`

```
pub fn format_briefing(
    briefing: &Briefing,
    format: ResponseFormat,
) -> CallToolResult:
    match format:
        Summary:
            lines = []
            lines.push("Briefing for {role}: {task}")
            lines.push("Conventions: {conventions.len()} | Duties: {duties.len()} | Context: {relevant_context.len()}")
            if !search_available:
                lines.push("[search unavailable - lookup only]")
            return CallToolResult::success(vec![Content::text(lines.join("\n"))])

        Markdown:
            text = "## Briefing: {role}\n\n"
            text += "**Task:** {task}\n\n"

            if !search_available:
                text += "> Note: Semantic search unavailable. Showing lookup results only.\n\n"

            text += "### Conventions\n\n"
            if conventions.is_empty():
                text += "No conventions found for this role.\n\n"
            else:
                for entry in conventions:
                    text += "- **{entry.title}**: {entry.content}\n"
                text += "\n"

            text += "### Duties\n\n"
            if duties.is_empty():
                text += "No duties found for this role.\n\n"
            else:
                for entry in duties:
                    text += "- **{entry.title}**: {entry.content}\n"
                text += "\n"

            text += "### Relevant Context\n\n"
            if relevant_context.is_empty():
                text += "No relevant context found.\n\n"
            else:
                for (entry, score) in relevant_context:
                    text += "- **{entry.title}** ({score:.2}): {entry.content}\n"

            return CallToolResult::success(vec![Content::text(text)])

        Json:
            obj = {
                "role": role,
                "task": task,
                "search_available": search_available,
                "conventions": conventions.map(entry_to_json),
                "duties": duties.map(entry_to_json),
                "relevant_context": relevant_context.map(|(e,s)| entry_to_json_with_similarity(e,s))
            }
            return CallToolResult::success(vec![Content::text(to_string_pretty(obj))])
```

### Helper: `entry_to_json` update

The existing `entry_to_json` function needs to include correction chain fields:

```
fn entry_to_json(entry: &EntryRecord) -> serde_json::Value:
    // Add to existing json!({}) macro:
    "supersedes": entry.supersedes,
    "superseded_by": entry.superseded_by,
    "correction_count": entry.correction_count,
```
