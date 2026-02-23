//! Format-selectable response builders for MCP tool results.
//!
//! Produces summary (default), markdown, or json format responses.
//! Output framing markers are applied in markdown format only.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::{EntryRecord, Status};

use crate::error::ServerError;

/// Format a unix timestamp (seconds) as a human-readable UTC string.
fn format_timestamp(ts: u64) -> String {
    let secs = ts % 60;
    let total_mins = ts / 60;
    let mins = total_mins % 60;
    let total_hours = total_mins / 60;
    let hours = total_hours % 24;
    let mut days = (total_hours / 24) as i64;

    // Convert days since epoch to year-month-day
    // Algorithm: civil_from_days (Howard Hinnant)
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{mins:02}:{secs:02}Z")
}

/// Response format enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    Summary,
    Markdown,
    Json,
}

/// Parse the optional format parameter.
pub fn parse_format(format: &Option<String>) -> Result<ResponseFormat, ServerError> {
    match format {
        None => Ok(ResponseFormat::Summary),
        Some(f) => match f.to_lowercase().as_str() {
            "summary" => Ok(ResponseFormat::Summary),
            "markdown" => Ok(ResponseFormat::Markdown),
            "json" => Ok(ResponseFormat::Json),
            _ => Err(ServerError::InvalidInput {
                field: "format".to_string(),
                reason: "must be summary, markdown, or json".to_string(),
            }),
        },
    }
}

fn status_str(status: Status) -> &'static str {
    match status {
        Status::Active => "active",
        Status::Deprecated => "deprecated",
        Status::Proposed => "proposed",
    }
}

fn tags_str(tags: &[String]) -> String {
    tags.join(", ")
}

fn entry_to_json(entry: &EntryRecord) -> serde_json::Value {
    serde_json::json!({
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
        "created_by": entry.created_by,
    })
}

fn entry_to_json_with_similarity(entry: &EntryRecord, similarity: f32) -> serde_json::Value {
    serde_json::json!({
        "id": entry.id,
        "title": entry.title,
        "content": entry.content,
        "topic": entry.topic,
        "category": entry.category,
        "tags": entry.tags,
        "status": status_str(entry.status),
        "confidence": entry.confidence,
        "similarity": similarity,
        "created_at": entry.created_at,
        "updated_at": entry.updated_at,
        "created_by": entry.created_by,
    })
}

fn format_entry_markdown_section(
    num: usize,
    entry: &EntryRecord,
    similarity: Option<f32>,
) -> String {
    let mut text = format!("## {}. {}", num, entry.title);
    if let Some(sim) = similarity {
        text.push_str(&format!(" (similarity: {sim:.2})"));
    }
    text.push('\n');
    text.push_str(&format!(
        "**Topic:** {} | **Category:** {} | **Tags:** {}\n",
        entry.topic,
        entry.category,
        tags_str(&entry.tags)
    ));
    text.push_str(&format!(
        "**Confidence:** {:.2} | **Status:** {}\n\n",
        entry.confidence,
        status_str(entry.status)
    ));
    text.push_str("[KNOWLEDGE DATA]\n");
    text.push_str(&entry.content);
    text.push_str("\n[/KNOWLEDGE DATA]\n\n");
    text.push_str(&format!(
        "*Entry #{} | Created {} | Updated {}*",
        entry.id,
        format_timestamp(entry.created_at),
        format_timestamp(entry.updated_at)
    ));
    text
}

/// Format a single entry (used by context_get and context_lookup with id).
pub fn format_single_entry(entry: &EntryRecord, format: ResponseFormat) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let line = format!(
                "#{} | {} | {} | [{}]",
                entry.id,
                entry.title,
                entry.category,
                tags_str(&entry.tags)
            );
            CallToolResult::success(vec![Content::text(line)])
        }
        ResponseFormat::Markdown => {
            let text = format_entry_markdown_section(1, entry, None);
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = entry_to_json(entry);
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}

/// Format search results with similarity scores.
pub fn format_search_results(
    results: &[(EntryRecord, f32)],
    format: ResponseFormat,
) -> CallToolResult {
    if results.is_empty() {
        return format_empty_results("context_search", format);
    }
    match format {
        ResponseFormat::Summary => {
            let lines: Vec<String> = results
                .iter()
                .map(|(e, sim)| {
                    format!(
                        "#{} | {} | {} | [{}] | {:.2}",
                        e.id,
                        e.title,
                        e.category,
                        tags_str(&e.tags),
                        sim
                    )
                })
                .collect();
            CallToolResult::success(vec![Content::text(lines.join("\n"))])
        }
        ResponseFormat::Markdown => {
            let sections: Vec<String> = results
                .iter()
                .enumerate()
                .map(|(i, (e, sim))| format_entry_markdown_section(i + 1, e, Some(*sim)))
                .collect();
            CallToolResult::success(vec![Content::text(sections.join("\n\n---\n\n"))])
        }
        ResponseFormat::Json => {
            let arr: Vec<serde_json::Value> = results
                .iter()
                .map(|(e, sim)| entry_to_json_with_similarity(e, *sim))
                .collect();
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&arr).unwrap_or_default(),
            )])
        }
    }
}

/// Format lookup results (no similarity scores).
pub fn format_lookup_results(entries: &[EntryRecord], format: ResponseFormat) -> CallToolResult {
    if entries.is_empty() {
        return format_empty_results("context_lookup", format);
    }
    match format {
        ResponseFormat::Summary => {
            let lines: Vec<String> = entries
                .iter()
                .map(|e| {
                    format!(
                        "#{} | {} | {} | [{}]",
                        e.id,
                        e.title,
                        e.category,
                        tags_str(&e.tags)
                    )
                })
                .collect();
            CallToolResult::success(vec![Content::text(lines.join("\n"))])
        }
        ResponseFormat::Markdown => {
            let sections: Vec<String> = entries
                .iter()
                .enumerate()
                .map(|(i, e)| format_entry_markdown_section(i + 1, e, None))
                .collect();
            CallToolResult::success(vec![Content::text(sections.join("\n\n---\n\n"))])
        }
        ResponseFormat::Json => {
            let arr: Vec<serde_json::Value> = entries.iter().map(entry_to_json).collect();
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&arr).unwrap_or_default(),
            )])
        }
    }
}

/// Format a store success response.
pub fn format_store_success(entry: &EntryRecord, format: ResponseFormat) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let text = format!("Stored #{} | {} | {}", entry.id, entry.title, entry.category);
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = format!("## Stored: {}\n\n", entry.title);
            text.push_str(&format!(
                "**Topic:** {} | **Category:** {} | **Tags:** {}\n\n",
                entry.topic,
                entry.category,
                tags_str(&entry.tags)
            ));
            text.push_str("[KNOWLEDGE DATA]\n");
            text.push_str(&entry.content);
            text.push_str("\n[/KNOWLEDGE DATA]\n\n");
            text.push_str(&format!("*Entry #{} | Version {}*", entry.id, entry.version));
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = serde_json::json!({
                "stored": true,
                "entry": entry_to_json(entry),
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}

/// Format a near-duplicate detection response.
pub fn format_duplicate_found(
    existing: &EntryRecord,
    similarity: f32,
    format: ResponseFormat,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let text = format!(
                "Duplicate of #{} | {} | similarity: {:.2} | duplicate: true",
                existing.id, existing.title, similarity
            );
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = format!(
                "## Near-Duplicate Detected (similarity: {:.2})\n\n",
                similarity
            );
            text.push_str("Existing entry matches your content. No new entry created.\n\n");
            text.push_str(&format_entry_markdown_section(1, existing, Some(similarity)));
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = serde_json::json!({
                "duplicate": true,
                "similarity": similarity,
                "existing_entry": entry_to_json(existing),
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}

/// Format empty results.
pub fn format_empty_results(_tool: &str, format: ResponseFormat) -> CallToolResult {
    match format {
        ResponseFormat::Summary => CallToolResult::success(vec![Content::text(
            "No results. Try broadening your filters.",
        )]),
        ResponseFormat::Markdown => CallToolResult::success(vec![Content::text(
            "No matching entries found. Try broadening your search filters or using different terms.",
        )]),
        ResponseFormat::Json => CallToolResult::success(vec![Content::text("[]")]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: u64, title: &str, content: &str) -> EntryRecord {
        EntryRecord {
            id,
            title: title.to_string(),
            content: content.to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: vec!["rust".to_string()],
            source: "test".to_string(),
            status: Status::Active,
            confidence: 0.85,
            created_at: 1700000000,
            updated_at: 1700001000,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: "test-agent".to_string(),
            modified_by: "test-agent".to_string(),
            content_hash: "abc".to_string(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
        }
    }

    fn result_text(result: &CallToolResult) -> String {
        result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.clone())
            .unwrap_or_default()
    }

    #[test]
    fn test_parse_format_none_defaults_to_summary() {
        assert_eq!(parse_format(&None).unwrap(), ResponseFormat::Summary);
    }

    #[test]
    fn test_parse_format_summary() {
        assert_eq!(
            parse_format(&Some("summary".to_string())).unwrap(),
            ResponseFormat::Summary
        );
    }

    #[test]
    fn test_parse_format_markdown() {
        assert_eq!(
            parse_format(&Some("markdown".to_string())).unwrap(),
            ResponseFormat::Markdown
        );
    }

    #[test]
    fn test_parse_format_json() {
        assert_eq!(
            parse_format(&Some("json".to_string())).unwrap(),
            ResponseFormat::Json
        );
    }

    #[test]
    fn test_parse_format_invalid() {
        assert!(parse_format(&Some("invalid".to_string())).is_err());
    }

    #[test]
    fn test_parse_format_case_insensitive() {
        assert_eq!(
            parse_format(&Some("JSON".to_string())).unwrap(),
            ResponseFormat::Json
        );
    }

    #[test]
    fn test_format_single_entry_summary() {
        let entry = make_entry(42, "Test Title", "content");
        let result = format_single_entry(&entry, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("#42"));
        assert!(text.contains("Test Title"));
        assert!(text.contains("convention"));
    }

    #[test]
    fn test_format_single_entry_markdown() {
        let entry = make_entry(42, "Test Title", "some content here");
        let result = format_single_entry(&entry, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("[KNOWLEDGE DATA]"));
        assert!(text.contains("[/KNOWLEDGE DATA]"));
        assert!(text.contains("some content here"));
    }

    #[test]
    fn test_format_single_entry_json() {
        let entry = make_entry(42, "Test Title", "content");
        let result = format_single_entry(&entry, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["id"], 42);
        assert_eq!(parsed["title"], "Test Title");
    }

    #[test]
    fn test_format_search_results_summary() {
        let results = vec![
            (make_entry(1, "Entry 1", "c1"), 0.95_f32),
            (make_entry(2, "Entry 2", "c2"), 0.88),
            (make_entry(3, "Entry 3", "c3"), 0.75),
        ];
        let result = format_search_results(&results, ResponseFormat::Summary);
        let text = result_text(&result);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(text.contains("0.95"));
    }

    #[test]
    fn test_format_search_results_markdown() {
        let results = vec![
            (make_entry(1, "Entry 1", "content 1"), 0.95_f32),
            (make_entry(2, "Entry 2", "content 2"), 0.88),
        ];
        let result = format_search_results(&results, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("[KNOWLEDGE DATA]"));
        assert!(text.contains("similarity: 0.95"));
    }

    #[test]
    fn test_format_search_results_json() {
        let results = vec![
            (make_entry(1, "E1", "c1"), 0.95_f32),
            (make_entry(2, "E2", "c2"), 0.88),
        ];
        let result = format_search_results(&results, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0]["similarity"].as_f64().is_some());
    }

    #[test]
    fn test_format_lookup_results_summary() {
        let entries = vec![make_entry(1, "E1", "c1"), make_entry(2, "E2", "c2")];
        let result = format_lookup_results(&entries, ResponseFormat::Summary);
        let text = result_text(&result);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        // No similarity in lookup
        assert!(!text.contains("0."));
    }

    #[test]
    fn test_format_lookup_results_json() {
        let entries = vec![make_entry(1, "E1", "c1")];
        let result = format_lookup_results(&entries, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.len(), 1);
        assert!(parsed[0].get("similarity").is_none());
    }

    #[test]
    fn test_format_store_success_summary() {
        let entry = make_entry(42, "New Entry", "content");
        let result = format_store_success(&entry, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Stored #42"));
    }

    #[test]
    fn test_format_store_success_json() {
        let entry = make_entry(42, "New Entry", "content");
        let result = format_store_success(&entry, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["stored"], true);
        assert_eq!(parsed["entry"]["id"], 42);
    }

    #[test]
    fn test_format_duplicate_summary() {
        let entry = make_entry(7, "Existing", "content");
        let result = format_duplicate_found(&entry, 0.95, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("duplicate: true"));
        assert!(text.contains("0.95"));
    }

    #[test]
    fn test_format_duplicate_markdown() {
        let entry = make_entry(7, "Existing", "content");
        let result = format_duplicate_found(&entry, 0.95, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Near-Duplicate Detected"));
    }

    #[test]
    fn test_format_duplicate_json() {
        let entry = make_entry(7, "Existing", "content");
        let result = format_duplicate_found(&entry, 0.95, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["duplicate"], true);
        assert!(parsed["similarity"].as_f64().unwrap() > 0.9);
        assert_eq!(parsed["existing_entry"]["id"], 7);
    }

    #[test]
    fn test_format_empty_results_summary() {
        let result = format_empty_results("context_search", ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("No results"));
    }

    #[test]
    fn test_format_empty_results_json() {
        let result = format_empty_results("context_search", ResponseFormat::Json);
        let text = result_text(&result);
        assert_eq!(text, "[]");
    }

    #[test]
    fn test_markdown_has_knowledge_data_markers() {
        let entry = make_entry(1, "Test", "body content");
        let result = format_single_entry(&entry, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("[KNOWLEDGE DATA]"));
        assert!(text.contains("[/KNOWLEDGE DATA]"));
    }

    #[test]
    fn test_summary_has_no_markers() {
        let entry = make_entry(1, "Test", "body content");
        let result = format_single_entry(&entry, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(!text.contains("[KNOWLEDGE DATA]"));
    }

    #[test]
    fn test_json_has_no_markers() {
        let entry = make_entry(1, "Test", "body content");
        let result = format_single_entry(&entry, ResponseFormat::Json);
        let text = result_text(&result);
        assert!(!text.contains("[KNOWLEDGE DATA]"));
    }

    #[test]
    fn test_content_with_marker_in_body() {
        let entry = make_entry(1, "Test", "data [/KNOWLEDGE DATA] more data");
        let result = format_single_entry(&entry, ResponseFormat::Markdown);
        let text = result_text(&result);
        // Still formatted correctly -- markers on their own lines
        assert!(text.contains("[KNOWLEDGE DATA]\ndata [/KNOWLEDGE DATA] more data\n[/KNOWLEDGE DATA]"));
    }

    #[test]
    fn test_format_timestamp_epoch() {
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_format_timestamp_known_date() {
        // 2023-11-14T22:13:20Z
        assert_eq!(format_timestamp(1700000000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn test_markdown_has_iso_timestamps() {
        let entry = make_entry(1, "Test", "content");
        let result = format_single_entry(&entry, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("2023-11-14"), "should contain ISO date, got: {text}");
        assert!(!text.contains("1700000000"), "should not contain raw unix timestamp");
    }
}
