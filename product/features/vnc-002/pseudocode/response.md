# Pseudocode: response (C3)

## File: `crates/unimatrix-server/src/response.rs`

### Types

```
enum ResponseFormat:
    Summary
    Markdown
    Json
```

### Format Parsing

```
fn parse_format(format: &Option<String>) -> Result<ResponseFormat, ServerError>:
    match format:
        None => Ok(ResponseFormat::Summary)
        Some(f) => match f.to_lowercase().as_str():
            "summary" => Ok(Summary)
            "markdown" => Ok(Markdown)
            "json" => Ok(Json)
            _ => Err(InvalidInput { field: "format", reason: "must be summary, markdown, or json" })
```

### Timestamp Formatting

```
fn format_timestamp_iso(unix_secs: u64) -> String:
    // Convert unix seconds to "YYYY-MM-DD HH:MM:SS UTC"
    // Use chrono-free manual calculation or just format as seconds
    // For simplicity: format as ISO-ish string from unix timestamp
    // Actually: use time arithmetic to produce readable date
    // Implementation: manual UTC conversion (no chrono dependency)
```

### Response Builders

```
fn format_single_entry(entry: &EntryRecord, format: ResponseFormat) -> CallToolResult:
    match format:
        Summary =>
            // Full content for single-entry get (summary would be pointless per FR-11h)
            line = "#{id} | {title} | {category} | [{tags}]"
            CallToolResult::success(vec![Content::text(line)])
        Markdown =>
            text = "## Context: {title}\n"
            text += "**Topic:** {topic} | **Category:** {category} | **Tags:** {tags}\n"
            text += "**Confidence:** {confidence} | **Status:** {status}\n\n"
            text += "[KNOWLEDGE DATA]\n"
            text += "{content}\n"
            text += "[/KNOWLEDGE DATA]\n\n"
            text += "*Entry #{id} | Created {iso_date} | Updated {iso_date}*"
            CallToolResult::success(vec![Content::text(text)])
        Json =>
            obj = serde_json::json!({
                "id": entry.id,
                "title": entry.title,
                "content": entry.content,
                "topic": entry.topic,
                "category": entry.category,
                "tags": entry.tags,
                "status": status_str(entry.status),
                "confidence": entry.confidence,
                "created_at": entry.created_at,
                "updated_at": entry.updated_at,
                "created_by": entry.created_by
            })
            CallToolResult::success(vec![Content::text(serde_json::to_string_pretty(&obj))])

fn format_search_results(results: &[(EntryRecord, f32)], format: ResponseFormat) -> CallToolResult:
    if results.is_empty():
        return format_empty_results("context_search", format)
    match format:
        Summary =>
            lines = results.iter().map(|(e, sim)|
                "#{id} | {title} | {category} | [{tags}] | {sim:.2}"
            ).join("\n")
            CallToolResult::success(vec![Content::text(lines)])
        Markdown =>
            text = results.iter().enumerate().map(|(i, (e, sim))|
                format_entry_markdown_section(i+1, e, Some(*sim))
            ).join("\n\n---\n\n")
            CallToolResult::success(vec![Content::text(text)])
        Json =>
            arr = results.iter().map(|(e, sim)| json!({
                "id": e.id,
                "title": e.title,
                "content": e.content,
                "topic": e.topic,
                "category": e.category,
                "tags": e.tags,
                "status": status_str(e.status),
                "confidence": e.confidence,
                "similarity": sim,
                "created_at": e.created_at,
                "created_by": e.created_by
            })).collect::<Vec<_>>()
            CallToolResult::success(vec![Content::text(serde_json::to_string_pretty(&arr))])

fn format_lookup_results(entries: &[EntryRecord], format: ResponseFormat) -> CallToolResult:
    if entries.is_empty():
        return format_empty_results("context_lookup", format)
    match format:
        Summary =>
            lines = entries.iter().map(|e|
                "#{id} | {title} | {category} | [{tags}]"
            ).join("\n")
            CallToolResult::success(vec![Content::text(lines)])
        Markdown =>
            text = entries.iter().enumerate().map(|(i, e)|
                format_entry_markdown_section(i+1, e, None)
            ).join("\n\n---\n\n")
            CallToolResult::success(vec![Content::text(text)])
        Json =>
            arr = entries.iter().map(|e| entry_to_json(e)).collect()
            CallToolResult::success(vec![Content::text(serde_json::to_string_pretty(&arr))])

fn format_store_success(entry: &EntryRecord, format: ResponseFormat) -> CallToolResult:
    match format:
        Summary =>
            text = "Stored #{id} | {title} | {category}"
            CallToolResult::success(vec![Content::text(text)])
        Markdown =>
            text = "## Stored: {title}\n\n"
            text += format_entry_markdown_body(entry)
            CallToolResult::success(vec![Content::text(text)])
        Json =>
            obj = json!({ "stored": true, "entry": entry_to_json(entry) })
            CallToolResult::success(vec![Content::text(serde_json::to_string_pretty(&obj))])

fn format_duplicate_found(existing: &EntryRecord, similarity: f32, format: ResponseFormat) -> CallToolResult:
    match format:
        Summary =>
            text = "Duplicate of #{id} | {title} | similarity: {sim:.2} | duplicate: true"
            CallToolResult::success(vec![Content::text(text)])
        Markdown =>
            text = "## Near-Duplicate Detected (similarity: {sim:.2})\n\n"
            text += "Existing entry matches your content. No new entry created.\n\n"
            text += format_entry_markdown_body(existing)
            CallToolResult::success(vec![Content::text(text)])
        Json =>
            obj = json!({
                "duplicate": true,
                "similarity": similarity,
                "existing_entry": entry_to_json(existing)
            })
            CallToolResult::success(vec![Content::text(serde_json::to_string_pretty(&obj))])

fn format_empty_results(tool: &str, format: ResponseFormat) -> CallToolResult:
    match format:
        Summary =>
            text = "No results. Try broadening your filters."
            CallToolResult::success(vec![Content::text(text)])
        Markdown =>
            text = "No matching entries found. Try broadening your search filters or using different terms."
            CallToolResult::success(vec![Content::text(text)])
        Json =>
            CallToolResult::success(vec![Content::text("[]")])
```

### Internal Helpers

```
fn status_str(status: Status) -> &'static str:
    match status:
        Active => "active"
        Deprecated => "deprecated"
        Proposed => "proposed"

fn tags_str(tags: &[String]) -> String:
    tags.join(", ")

fn format_entry_markdown_section(num: usize, entry: &EntryRecord, similarity: Option<f32>) -> String:
    text = "## {num}. {title}"
    if let Some(sim) = similarity:
        text += " (similarity: {sim:.2})"
    text += "\n"
    text += "**Topic:** {topic} | **Category:** {category} | **Tags:** {tags}\n"
    text += "**Confidence:** {confidence} | **Status:** {status}\n\n"
    text += "[KNOWLEDGE DATA]\n"
    text += "{content}\n"
    text += "[/KNOWLEDGE DATA]\n\n"
    text += "*Entry #{id} | Created {created_at} | Updated {updated_at}*"
    text

fn entry_to_json(entry: &EntryRecord) -> serde_json::Value:
    json!({
        "id": entry.id,
        "title": entry.title,
        "content": entry.content,
        "topic": entry.topic,
        "category": entry.category,
        "tags": entry.tags,
        "status": status_str(entry.status),
        "confidence": entry.confidence,
        "created_at": entry.created_at,
        "updated_at": entry.updated_at,
        "created_by": entry.created_by
    })
```

### Key Constraints
- Single Content::text() block per response (NOT dual-format)
- Output framing markers only in markdown format
- Summary format: one compact line per entry, no full content
- JSON format: serde_json::to_string_pretty for readability
- Empty results: helpful message in summary/markdown, `[]` in json
- Timestamps formatted as unix seconds in JSON, human-readable in markdown
