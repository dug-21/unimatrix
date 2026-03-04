//! Entry formatting: single entry, search results, lookup results,
//! store success, duplicate detection, correction success.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::EntryRecord;

use super::{
    format_empty_results, format_entry_markdown_section,
    entry_to_json, entry_to_json_with_similarity, tags_str,
    ResponseFormat,
};

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
    results: &[(EntryRecord, f64)],
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

/// Format a store success response with an appended note.
pub fn format_store_success_with_note(
    entry: &EntryRecord,
    format: ResponseFormat,
    note: &str,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let text = format!("Stored #{} | {} | {}{}", entry.id, entry.title, entry.category, note);
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
            text.push_str(&format!("*Entry #{} | Version {}*\n\n", entry.id, entry.version));
            text.push_str(&format!("> {}", note.trim_start_matches('\n')));
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = serde_json::json!({
                "stored": true,
                "entry": entry_to_json(entry),
                "note": note.trim_start_matches('\n'),
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
    similarity: f64,
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

/// Format a correction success response showing both deprecated original and new correction.
pub fn format_correct_success(
    original: &EntryRecord,
    correction: &EntryRecord,
    format: ResponseFormat,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let text = format!(
                "Corrected #{} -> #{} | {} | {}",
                original.id, correction.id, correction.title, correction.category
            );
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = String::from("## Correction Applied\n\n");
            text.push_str(&format!(
                "**Original (deprecated):** #{} - {}\n",
                original.id, original.title
            ));
            text.push_str(&format!(
                "**Correction:** #{} - {}\n\n",
                correction.id, correction.title
            ));
            text.push_str("### Corrected Entry\n\n");
            text.push_str(&format_entry_markdown_section(1, correction, None));
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = serde_json::json!({
                "corrected": true,
                "original": entry_to_json(original),
                "correction": entry_to_json(correction),
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}
