//! Entry formatting: single entry, search results, lookup results,
//! store success, duplicate detection, correction success.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::EntryRecord;

use super::{
    ResponseFormat, entry_to_json, entry_to_json_with_similarity, format_empty_results,
    format_entry_markdown_section, tags_str,
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
            let text = format!(
                "Stored #{} | {} | {}",
                entry.id, entry.title, entry.category
            );
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
            text.push_str(&format!(
                "*Entry #{} | Version {}*",
                entry.id, entry.version
            ));
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
            let text = format!(
                "Stored #{} | {} | {}{}",
                entry.id, entry.title, entry.category, note
            );
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
            text.push_str(&format!(
                "*Entry #{} | Version {}*\n\n",
                entry.id, entry.version
            ));
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
            text.push_str(&format_entry_markdown_section(
                1,
                existing,
                Some(similarity),
            ));
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

/// Build the redirect summary string for appending to a `context_correct` response.
///
/// Returns `None` when no non-Supersedes incoming edges were found (`found == 0`), producing
/// no change to the response text (FR-10, ADR-004 vnc-017 SR-05 zero-edge omit rule).
///
/// Returns `Some(text)` for the three non-empty variants from the FR-10 authoritative table:
/// - **Normal** (`truncated == false`, `skipped == 0`):
///   `"Redirected {redirected} incoming edges ({failed} failed, see logs)"`
/// - **Skipped** (`truncated == false`, `skipped > 0`):
///   `"Redirected {redirected} incoming edges ({skipped} skipped — invalid source, {failed} failed, see logs)"`
/// - **Truncated** (`truncated == true`):
///   `"Redirected {redirected} incoming edges (truncated from {total_raw}, see logs)"`
///
/// # Parameters
/// - `found`: total non-Supersedes incoming edges queried (after ceiling cap); gates the append
/// - `skipped`: edges whose source was Quarantined or Deprecated (skip-with-warn, not failure)
/// - `redirected`: edges where `redirect_graph_edge` returned `Ok(())`
/// - `failed`: edges where `redirect_graph_edge` returned `Err(_)`
/// - `truncated`: `true` when the raw row count exceeded `REDIRECT_CEILING` before capping
/// - `total_raw`: raw incoming count before ceiling truncation (used in truncation variant only)
pub fn format_redirect_summary(
    found: usize,
    skipped: usize,
    redirected: usize,
    failed: usize,
    truncated: bool,
    total_raw: usize,
) -> Option<String> {
    if found == 0 {
        return None;
    }

    let text = if truncated {
        // Truncation variant: total_raw is the raw count before ceiling cap (ADR-004 vnc-017).
        format!(
            "Redirected {} incoming edges (truncated from {}, see logs)",
            redirected, total_raw
        )
    } else if skipped > 0 {
        // Skipped-count variant: some sources were Quarantined or Deprecated (FR-06, ADR-003 vnc-017).
        format!(
            "Redirected {} incoming edges ({} skipped \u{2014} invalid source, {} failed, see logs)",
            redirected, skipped, failed
        )
    } else {
        // Normal variant: no truncation, no skipped sources.
        format!(
            "Redirected {} incoming edges ({} failed, see logs)",
            redirected, failed
        )
    };

    Some(text)
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

#[cfg(test)]
mod tests {
    use super::format_redirect_summary;

    // AC-11: found == 0 → no append (None)
    #[test]
    fn test_response_format_no_append_when_found_zero() {
        let result = format_redirect_summary(0, 0, 0, 0, false, 0);
        assert!(
            result.is_none(),
            "Expected None when found == 0, got {:?}",
            result
        );
    }

    // AC-12: found > 0, skipped == 0, truncated == false — all succeed (normal variant)
    #[test]
    fn test_response_format_all_success_variant() {
        let result = format_redirect_summary(2, 0, 2, 0, false, 2);
        let text = result.expect("Expected Some for found > 0");
        assert!(
            text.contains("Redirected 2 incoming edges (0 failed, see logs)"),
            "Unexpected text: {:?}",
            text
        );
        assert!(
            !text.contains("skipped"),
            "Should not contain 'skipped': {:?}",
            text
        );
        assert!(
            !text.contains("truncated"),
            "Should not contain 'truncated': {:?}",
            text
        );
    }

    // AC-13: found > 0, some failed, skipped == 0, truncated == false (partial failure variant)
    #[test]
    fn test_response_format_partial_failure_variant() {
        let result = format_redirect_summary(3, 0, 1, 2, false, 3);
        let text = result.expect("Expected Some for found > 0");
        assert!(
            text.contains("Redirected 1 incoming edges (2 failed, see logs)"),
            "Unexpected text: {:?}",
            text
        );
        assert!(
            !text.contains("skipped"),
            "Should not contain 'skipped': {:?}",
            text
        );
    }

    // AC-17: all-skipped case — all sources Quarantined/Deprecated
    #[test]
    fn test_response_format_all_skipped_variant() {
        let result = format_redirect_summary(3, 3, 0, 0, false, 3);
        let text = result.expect("Expected Some for found > 0");
        // em-dash U+2014 in the skipped variant
        assert!(
            text.contains("Redirected 0 incoming edges")
                && text.contains("3 skipped")
                && text.contains("invalid source")
                && text.contains("0 failed"),
            "Unexpected text: {:?}",
            text
        );
        assert!(
            !text.contains("truncated"),
            "Should not contain 'truncated': {:?}",
            text
        );
    }

    // Mixed skipped and failed (Variant 3, skipped > 0)
    #[test]
    fn test_response_format_mixed_skipped_and_failed_variant() {
        let result = format_redirect_summary(4, 1, 2, 1, false, 4);
        let text = result.expect("Expected Some for found > 0");
        assert!(
            text.contains("Redirected 2 incoming edges")
                && text.contains("1 skipped")
                && text.contains("invalid source")
                && text.contains("1 failed"),
            "Unexpected text: {:?}",
            text
        );
    }

    // R-05: truncation variant
    #[test]
    fn test_response_format_truncated_variant() {
        let result = format_redirect_summary(50, 0, 50, 0, true, 55);
        let text = result.expect("Expected Some for found > 0");
        assert!(
            text.contains("Redirected 50 incoming edges (truncated from 55, see logs)"),
            "Unexpected text: {:?}",
            text
        );
        assert!(
            !text.contains("failed"),
            "Truncation variant should not contain 'failed': {:?}",
            text
        );
        assert!(
            !text.contains("skipped"),
            "Truncation variant should not contain 'skipped': {:?}",
            text
        );
    }

    // All failed, redirected == 0 (Variant 2 with zero success)
    #[test]
    fn test_response_format_all_failed_variant() {
        let result = format_redirect_summary(2, 0, 0, 2, false, 2);
        let text = result.expect("Expected Some for found > 0");
        assert!(
            text.contains("Redirected 0 incoming edges (2 failed, see logs)"),
            "Unexpected text: {:?}",
            text
        );
    }

    // found > 0 with a single edge (plural form per FR-10, no special singular handling)
    #[test]
    fn test_response_format_singular_edge_uses_plural_form() {
        let result = format_redirect_summary(1, 0, 1, 0, false, 1);
        let text = result.expect("Expected Some for found == 1");
        assert!(
            text.contains("Redirected 1 incoming edges"),
            "FR-10 specifies no singular form; expected 'edges': {:?}",
            text
        );
    }
}
