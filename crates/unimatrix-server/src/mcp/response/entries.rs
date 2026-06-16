//! Entry formatting: single entry, search results, lookup results,
//! store success, duplicate detection, correction success.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::EntryRecord;

use super::edges::EdgesView;
use super::edges_render::{
    render_json_edge_totals, render_json_edges, render_markdown_related, render_summary_digest,
};
use super::{
    ResponseFormat, entry_to_json, entry_to_json_with_similarity, format_empty_results,
    format_entry_markdown_section, tags_str,
};

/// Format a single entry (used by context_get and context_lookup with id).
///
/// `edges` is the vnc-037 serializer seam (ADR-003): **only** `context_get` (default-on)
/// passes `Some(view)`; every list-view tool path and an opted-out get pass `None`. `None` is
/// **structural** — the `edges` JSON key is never inserted, the `### Related` markdown section
/// is never appended, and the summary `edges:` digest is never emitted (C-4, byte-identity
/// invariant). `entry_to_json` / `format_entry_markdown_section` stay UNCHANGED; the get-only
/// edge rendering is layered on top of their output here.
pub fn format_single_entry(
    entry: &EntryRecord,
    format: ResponseFormat,
    edges: Option<&EdgesView>,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let mut line = format!(
                "#{} | {} | {} | [{}]",
                entry.id,
                entry.title,
                entry.category,
                tags_str(&entry.tags)
            );
            if let Some(view) = edges {
                line.push_str(&render_summary_digest(view));
            }
            CallToolResult::success(vec![Content::text(line)])
        }
        ResponseFormat::Markdown => {
            let mut text = format_entry_markdown_section(1, entry, None);
            if let Some(view) = edges {
                text.push_str("\n\n");
                text.push_str(&render_markdown_related(view));
            }
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let mut obj = entry_to_json(entry);
            if let Some(view) = edges
                && let Some(map) = obj.as_object_mut()
            {
                map.insert("edges".to_string(), render_json_edges(view));
                map.insert("edge_totals".to_string(), render_json_edge_totals(view));
            }
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

/// Format the vnc-035 carry-forward ack line. COUNT ONLY — no edge identities/content (AC-11).
///
/// The caller guards on `carried > 0` (omit at zero, mirroring `format_redirect_summary`'s
/// `found == 0 -> None` contract); this returns the bare line unconditionally for `carried >= 1`.
/// The integer is the sole awareness channel — no DB provenance marker exists (ADR-003 vnc-035).
pub fn format_edges_carried(carried: usize) -> String {
    format!("Carried {carried} outgoing edges forward")
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
