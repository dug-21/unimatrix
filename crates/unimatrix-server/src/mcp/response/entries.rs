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

/// What resolution did to a requested `context_get` id (vnc-042). The handler builds it; the
/// formatter renders it. Clean passthrough carries NO `ResolutionNote` — the handler routes a
/// clean get to `format_single_entry` instead, preserving the byte-identity invariant (ADR-003).
#[derive(Debug, Clone)]
pub enum ResolutionNote {
    /// AC-02 hop: requested `from` (deprecated) resolved to the active terminal `to`.
    Followed { from: u64, to: u64 },
    /// AC-04 fail-loud: the chain dead-ends on a non-active entry; the requested id is returned.
    DeadEnd { requested: u64 },
    /// AC-03 / AC-08 escape hatch: a deprecated entry returned as-stored. `superseded_by` is
    /// `Some(z)` when a successor pointer exists, `None` for an orphaned/quarantined terminal.
    AsStoredDeprecated {
        requested: u64,
        superseded_by: Option<u64>,
    },
}

/// Render the text (summary/markdown) note as a `(prefix, suffix)` pair (ADR-003, R-08).
///
/// `Followed`/`DeadEnd` prepend a line (`prefix`); `AsStoredDeprecated` appends a footer
/// (`suffix`). `superseded_by` is matched, never unwrapped — the `None` arm emits the
/// well-formed pointerless footer with no `#{}`, no `#null`, no panic (AC-08, C-4).
fn render_note_text(note: &ResolutionNote) -> (Option<String>, Option<String>) {
    match note {
        ResolutionNote::Followed { from, to } => (
            Some(format!(
                "\u{21bb} Requested #{from} (deprecated) \u{2192} returning current version #{to}."
            )),
            None,
        ),
        ResolutionNote::DeadEnd { requested } => (
            Some(format!(
                "\u{26a0} Requested #{requested}: no active successor found (chain dead-ends on a non-active entry)."
            )),
            None,
        ),
        ResolutionNote::AsStoredDeprecated {
            superseded_by: Some(z),
            ..
        } => (
            None,
            Some(format!(
                "deprecated; superseded by #{z} (omit follow_supersessions to follow)."
            )),
        ),
        ResolutionNote::AsStoredDeprecated {
            superseded_by: None,
            ..
        } => (None, Some("deprecated; no recorded successor.".to_string())),
    }
}

/// Build the structured JSON `resolution` object for a note (ADR-003, OQ-3). A typed
/// discriminant callers branch on — not a parsed string. `superseded_by` serializes to `null`
/// in the no-successor case (`Option<u64>`), never a malformed value.
fn render_note_json(note: &ResolutionNote) -> serde_json::Value {
    match note {
        ResolutionNote::Followed { from, to } => serde_json::json!({
            "status": "followed",
            "requested_id": from,
            "returned_id": to,
        }),
        ResolutionNote::DeadEnd { requested } => serde_json::json!({
            "status": "no_active_successor",
            "requested_id": requested,
        }),
        ResolutionNote::AsStoredDeprecated {
            requested,
            superseded_by,
        } => serde_json::json!({
            "status": "as_stored_deprecated",
            "requested_id": requested,
            "superseded_by": superseded_by,
        }),
    }
}

/// Format a single entry that carries a resolution note (vnc-042, ADR-003).
///
/// The entry body + edges are rendered by the EXACT same expressions as
/// [`format_single_entry`] (R-06, AC-01 body-equivalence); the note is layered on top —
/// a prepended `↻`/`⚠` line or an appended deprecated footer for text, or a single
/// `resolution` key for json. This function is only ever called on NON-clean paths, so the
/// `resolution` key appears only when a real note exists (R-07); clean passthrough routes to
/// [`format_single_entry`], keeping the common active-entry json byte-identical.
///
/// `format_single_entry` itself is NEVER modified — this protects the byte-identity canary and
/// the shape tests (SR-04, C-7).
pub fn format_single_entry_with_note(
    entry: &EntryRecord,
    format: ResponseFormat,
    edges: Option<&EdgesView>,
    note: &ResolutionNote,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let (prefix, suffix) = render_note_text(note);
            let mut body = format!(
                "#{} | {} | {} | [{}]",
                entry.id,
                entry.title,
                entry.category,
                tags_str(&entry.tags)
            );
            if let Some(view) = edges {
                body.push_str(&render_summary_digest(view));
            }
            let line = [prefix, Some(body), suffix]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n");
            CallToolResult::success(vec![Content::text(line)])
        }
        ResponseFormat::Markdown => {
            let (prefix, suffix) = render_note_text(note);
            let mut body = format_entry_markdown_section(1, entry, None);
            if let Some(view) = edges {
                body.push_str("\n\n");
                body.push_str(&render_markdown_related(view));
            }
            let mut text = String::new();
            if let Some(prefix) = prefix {
                text.push_str(&prefix);
                text.push_str("\n\n");
            }
            text.push_str(&body);
            if let Some(suffix) = suffix {
                text.push_str("\n\n> ");
                text.push_str(&suffix);
            }
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let mut obj = entry_to_json(entry);
            if let Some(map) = obj.as_object_mut() {
                if let Some(view) = edges {
                    map.insert("edges".to_string(), render_json_edges(view));
                    map.insert("edge_totals".to_string(), render_json_edge_totals(view));
                }
                map.insert("resolution".to_string(), render_note_json(note));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::response::edges::{EdgeTotals, EdgesView, GetEdge};
    use unimatrix_store::Status;

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
            helpful_count: 0,
            unhelpful_count: 0,
            pre_quarantine_status: None,
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

    fn sample_edges() -> EdgesView {
        EdgesView {
            edges: vec![GetEdge::new(
                "Supports".to_string(),
                "outbound",
                100,
                Some("Target".to_string()),
                "agent",
            )],
            totals: EdgeTotals {
                inbound: 0,
                outbound: 1,
                both: 0,
            },
            authored_total: 1,
        }
    }

    // --- Additivity / no-drift (R-01 sc.4, R-06) ---

    /// The `_with_note` body, minus the note region, equals the base formatter output — the note
    /// is purely additive / outside the base body. Run across summary/markdown/json.
    #[test]
    fn test_with_note_stripped_equals_base_formatter() {
        let entry = make_entry(5, "Base", "body");
        let note = ResolutionNote::Followed { from: 5, to: 9 };

        // Summary: prepended line + "\n" + body → body preserved verbatim as the tail.
        let base_s = result_text(&format_single_entry(&entry, ResponseFormat::Summary, None));
        let note_s = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Summary,
            None,
            &note,
        ));
        assert_eq!(
            note_s.strip_suffix(&base_s).map(|p| p.ends_with('\n')),
            Some(true)
        );

        // Markdown: prepended line + "\n\n" + body → body preserved verbatim as the tail.
        let base_m = result_text(&format_single_entry(&entry, ResponseFormat::Markdown, None));
        let note_m = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Markdown,
            None,
            &note,
        ));
        assert!(
            note_m.ends_with(&base_m),
            "markdown body must be preserved verbatim"
        );

        // Json: removing the `resolution` key yields the base object byte-for-byte.
        let base_j = result_text(&format_single_entry(&entry, ResponseFormat::Json, None));
        let note_j = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let mut parsed: serde_json::Value = serde_json::from_str(&note_j).unwrap();
        parsed.as_object_mut().unwrap().remove("resolution");
        assert_eq!(serde_json::to_string_pretty(&parsed).unwrap(), base_j);
    }

    /// The entry body rendered by `_with_note` matches the base-formatter body (id, fields,
    /// content), differing only by the note. Guards drift between the two entry points.
    #[test]
    fn test_with_note_body_matches_base_across_formats() {
        let entry = make_entry(11, "Title", "content-body");
        let note = ResolutionNote::AsStoredDeprecated {
            requested: 11,
            superseded_by: Some(12),
        };

        // Json body-equivalence: the base object equals `_with_note` minus `resolution`.
        let base = result_text(&format_single_entry(&entry, ResponseFormat::Json, None));
        let with_note = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let base_v: serde_json::Value = serde_json::from_str(&base).unwrap();
        let mut with_v: serde_json::Value = serde_json::from_str(&with_note).unwrap();
        with_v.as_object_mut().unwrap().remove("resolution");
        assert_eq!(with_v, base_v);

        // Summary/markdown: the base body appears verbatim within the noted output.
        let base_s = result_text(&format_single_entry(&entry, ResponseFormat::Summary, None));
        let note_s = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Summary,
            None,
            &note,
        ));
        assert!(note_s.contains(&base_s));
    }

    // --- Followed { from, to } (AC-02 hop) ---

    #[test]
    fn test_note_followed_summary_prepends_hop_line() {
        let entry = make_entry(9, "Current", "body");
        let note = ResolutionNote::Followed { from: 5, to: 9 };
        let expected = "\u{21bb} Requested #5 (deprecated) \u{2192} returning current version #9.";

        let summary = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Summary,
            None,
            &note,
        ));
        assert!(
            summary.starts_with(expected),
            "summary prepends the hop line"
        );

        let markdown = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Markdown,
            None,
            &note,
        ));
        assert!(
            markdown.starts_with(expected),
            "markdown prepends the hop line"
        );
    }

    #[test]
    fn test_note_followed_json_resolution_object() {
        let entry = make_entry(9, "Current", "body");
        let note = ResolutionNote::Followed { from: 5, to: 9 };
        let json = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resolution"]["status"], "followed");
        assert_eq!(parsed["resolution"]["requested_id"], 5);
        assert_eq!(parsed["resolution"]["returned_id"], 9);
    }

    // --- DeadEnd { requested } (AC-04 fail-loud) ---

    #[test]
    fn test_note_deadend_summary_prepends_loud_line() {
        let entry = make_entry(5, "Requested", "body");
        let note = ResolutionNote::DeadEnd { requested: 5 };
        let expected = "\u{26a0} Requested #5: no active successor found (chain dead-ends on a non-active entry).";

        let summary = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Summary,
            None,
            &note,
        ));
        assert!(
            summary.starts_with(expected),
            "summary prepends the loud line"
        );

        let markdown = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Markdown,
            None,
            &note,
        ));
        assert!(
            markdown.starts_with(expected),
            "markdown prepends the loud line"
        );
    }

    #[test]
    fn test_note_deadend_json_resolution_object() {
        let entry = make_entry(5, "Requested", "body");
        let note = ResolutionNote::DeadEnd { requested: 5 };
        let json = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        assert!(!json.is_empty());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resolution"]["status"], "no_active_successor");
        assert_eq!(parsed["resolution"]["requested_id"], 5);
    }

    // --- AsStoredDeprecated { requested, superseded_by } (AC-03 / AC-08, R-08) ---

    #[test]
    fn test_note_asstored_with_successor_appends_footer() {
        let entry = make_entry(5, "AsStored", "body");
        let note = ResolutionNote::AsStoredDeprecated {
            requested: 5,
            superseded_by: Some(9),
        };
        let footer = "deprecated; superseded by #9 (omit follow_supersessions to follow).";

        let summary = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Summary,
            None,
            &note,
        ));
        assert!(summary.ends_with(footer), "summary appends the footer");

        let markdown = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Markdown,
            None,
            &note,
        ));
        assert!(
            markdown.ends_with(&format!("> {footer}")),
            "markdown appends a blockquote footer"
        );

        let json = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resolution"]["status"], "as_stored_deprecated");
        assert_eq!(parsed["resolution"]["requested_id"], 5);
        assert_eq!(parsed["resolution"]["superseded_by"], 9);
    }

    /// AC-08 / R-08: `superseded_by = None` yields the well-formed pointerless footer — no panic,
    /// no `#{}`, no `#null` — and `superseded_by: null` in json.
    #[test]
    fn test_note_asstored_null_successor_wellformed_footer() {
        let entry = make_entry(5, "Orphaned", "body");
        let note = ResolutionNote::AsStoredDeprecated {
            requested: 5,
            superseded_by: None,
        };

        let summary = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Summary,
            None,
            &note,
        ));
        assert!(summary.ends_with("deprecated; no recorded successor."));
        assert!(!summary.contains("#null"));
        // No malformed pointer: the footer never names a successor id.
        assert!(!summary.contains("superseded by #"));

        let markdown = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Markdown,
            None,
            &note,
        ));
        assert!(markdown.ends_with("> deprecated; no recorded successor."));

        let json = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resolution"]["status"], "as_stored_deprecated");
        assert!(parsed["resolution"]["superseded_by"].is_null());
    }

    // --- JSON `resolution`-key presence / absence (R-07, all four ADR-003 cases) ---

    /// Clean passthrough is routed through `format_single_entry` (NOT this function), so it MUST
    /// NOT carry a `resolution` key. Ties directly to the TS-01 byte-identity canary.
    #[test]
    fn test_json_clean_passthrough_has_no_resolution_key() {
        let entry = make_entry(7, "Clean", "body");
        let json = result_text(&format_single_entry(&entry, ResponseFormat::Json, None));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(!parsed.as_object().unwrap().contains_key("resolution"));
    }

    #[test]
    fn test_json_followed_has_resolution_key() {
        let entry = make_entry(9, "Current", "body");
        let note = ResolutionNote::Followed { from: 5, to: 9 };
        let json = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resolution"]["status"], "followed");
    }

    #[test]
    fn test_json_deadend_has_resolution_key() {
        let entry = make_entry(5, "Requested", "body");
        let note = ResolutionNote::DeadEnd { requested: 5 };
        let json = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resolution"]["status"], "no_active_successor");
    }

    #[test]
    fn test_json_asstored_has_resolution_key() {
        let entry = make_entry(5, "AsStored", "body");
        let note = ResolutionNote::AsStoredDeprecated {
            requested: 5,
            superseded_by: Some(9),
        };
        let json = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["resolution"]["status"], "as_stored_deprecated");
    }

    // --- Edges on the note path (R-03 boundary — formatter side) ---

    /// `_with_note` renders whatever `Option<&EdgesView>` it is handed; `None` ⇒ no `edges` key.
    /// The formatter neither re-keys nor resolves edge targets (NG-1 asymmetry accepted).
    #[test]
    fn test_with_note_renders_provided_edges() {
        let entry = make_entry(9, "Current", "body");
        let note = ResolutionNote::Followed { from: 5, to: 9 };
        let view = sample_edges();

        let with_edges = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            Some(&view),
            &note,
        ));
        let parsed: serde_json::Value = serde_json::from_str(&with_edges).unwrap();
        assert!(parsed.get("edges").is_some(), "provided edges must render");
        assert!(parsed.get("edge_totals").is_some());
        assert!(parsed.get("resolution").is_some());

        let without_edges = result_text(&format_single_entry_with_note(
            &entry,
            ResponseFormat::Json,
            None,
            &note,
        ));
        let parsed_none: serde_json::Value = serde_json::from_str(&without_edges).unwrap();
        assert!(!parsed_none.as_object().unwrap().contains_key("edges"));
    }
}
