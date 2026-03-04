//! Format-selectable response builders for MCP tool results.
//!
//! Produces summary (default), markdown, or json format responses.
//! Output framing markers are applied in markdown format only.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::{EntryRecord, Status};

use crate::error::ServerError;
use crate::registry::{Capability, EnrollResult, TrustLevel};

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
        Status::Quarantined => "quarantined",
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
        "supersedes": entry.supersedes,
        "superseded_by": entry.superseded_by,
        "correction_count": entry.correction_count,
    })
}

fn entry_to_json_with_similarity(entry: &EntryRecord, similarity: f64) -> serde_json::Value {
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
    similarity: Option<f64>,
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

/// Aggregated health metrics for format_status_report.
pub struct StatusReport {
    /// Count of active entries.
    pub total_active: u64,
    /// Count of deprecated entries.
    pub total_deprecated: u64,
    /// Count of proposed entries.
    pub total_proposed: u64,
    /// Count of quarantined entries.
    pub total_quarantined: u64,
    /// Category name to entry count.
    pub category_distribution: Vec<(String, u64)>,
    /// Topic name to entry count.
    pub topic_distribution: Vec<(String, u64)>,
    /// Entries that supersede another entry.
    pub entries_with_supersedes: u64,
    /// Entries that are superseded by another entry.
    pub entries_with_superseded_by: u64,
    /// Sum of correction_count across all entries.
    pub total_correction_count: u64,
    /// Trust source to entry count.
    pub trust_source_distribution: Vec<(String, u64)>,
    /// Entries with empty created_by field.
    pub entries_without_attribution: u64,
    /// Detected contradictions between entries.
    pub contradictions: Vec<crate::contradiction::ContradictionPair>,
    /// Number of contradictions detected.
    pub contradiction_count: usize,
    /// Entries with inconsistent embeddings.
    pub embedding_inconsistencies: Vec<crate::contradiction::EmbeddingInconsistency>,
    /// Whether contradiction scanning was performed.
    pub contradiction_scan_performed: bool,
    /// Whether embedding consistency check was performed.
    pub embedding_check_performed: bool,
    /// Total co-access pairs in CO_ACCESS table.
    pub total_co_access_pairs: u64,
    /// Active (non-stale) co-access pairs.
    pub active_co_access_pairs: u64,
    /// Top co-access pairs by count.
    pub top_co_access_pairs: Vec<CoAccessClusterEntry>,
    /// Number of stale pairs cleaned during this status call.
    pub stale_pairs_cleaned: u64,
    /// Composite lambda coherence score [0.0, 1.0].
    pub coherence: f64,
    /// Confidence freshness dimension score.
    pub confidence_freshness_score: f64,
    /// Graph quality dimension score.
    pub graph_quality_score: f64,
    /// Embedding consistency dimension score (1.0 if not checked).
    pub embedding_consistency_score: f64,
    /// Contradiction density dimension score.
    pub contradiction_density_score: f64,
    /// Number of entries with stale confidence.
    pub stale_confidence_count: u64,
    /// Number of entries whose confidence was refreshed this call.
    pub confidence_refreshed_count: u64,
    /// Stale node ratio in HNSW graph.
    pub graph_stale_ratio: f64,
    /// Whether graph compaction was performed this call.
    pub graph_compacted: bool,
    /// Actionable maintenance recommendations.
    pub maintenance_recommendations: Vec<String>,
    /// Total outcome entries.
    pub total_outcomes: u64,
    /// Outcome count by workflow type (from type: tag).
    pub outcomes_by_type: Vec<(String, u64)>,
    /// Outcome count by result (from result: tag).
    pub outcomes_by_result: Vec<(String, u64)>,
    /// Top feature cycles by outcome count.
    pub outcomes_by_feature_cycle: Vec<(String, u64)>,
    /// Number of observation JSONL files.
    pub observation_file_count: u64,
    /// Total size of observation files in bytes.
    pub observation_total_size_bytes: u64,
    /// Age of oldest observation file in days.
    pub observation_oldest_file_days: u64,
    /// Session IDs approaching 60-day cleanup.
    pub observation_approaching_cleanup: Vec<String>,
    /// Number of feature cycles with stored metrics.
    pub retrospected_feature_count: u64,
}

/// A co-access cluster entry for status reporting.
pub struct CoAccessClusterEntry {
    /// First entry ID in the pair.
    pub entry_id_a: u64,
    /// Second entry ID in the pair.
    pub entry_id_b: u64,
    /// Title of the first entry.
    pub title_a: String,
    /// Title of the second entry.
    pub title_b: String,
    /// Number of times the pair was co-retrieved.
    pub count: u32,
    /// Unix timestamp of most recent co-retrieval.
    pub last_updated: u64,
}

/// Assembled briefing for format_briefing.
#[cfg(feature = "mcp-briefing")]
pub struct Briefing {
    /// Role the briefing is for.
    pub role: String,
    /// Task description.
    pub task: String,
    /// Role conventions.
    pub conventions: Vec<EntryRecord>,
    /// Semantically relevant context with similarity scores.
    pub relevant_context: Vec<(EntryRecord, f64)>,
    /// Whether semantic search was available.
    pub search_available: bool,
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

/// Format a deprecation success response.
pub fn format_deprecate_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let text = format!("Deprecated #{} | {}", entry.id, entry.title);
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = String::from("## Entry Deprecated\n\n");
            text.push_str(&format!(
                "**Entry:** #{} - {}\n**Status:** deprecated\n",
                entry.id, entry.title
            ));
            if let Some(r) = reason {
                text.push_str(&format!("**Reason:** {r}\n"));
            }
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = serde_json::json!({
                "deprecated": true,
                "entry": entry_to_json(entry),
                "reason": reason,
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}

/// Format a quarantine success response.
pub fn format_quarantine_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let text = format!("Quarantined #{} | {}", entry.id, entry.title);
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = String::from("## Entry Quarantined\n\n");
            text.push_str(&format!(
                "**Entry:** #{} - {}\n**Status:** quarantined\n",
                entry.id, entry.title
            ));
            if let Some(r) = reason {
                text.push_str(&format!("**Reason:** {r}\n"));
            }
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = serde_json::json!({
                "quarantined": true,
                "entry": entry_to_json(entry),
                "reason": reason,
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}

/// Format a restore success response.
pub fn format_restore_success(
    entry: &EntryRecord,
    reason: Option<&str>,
    format: ResponseFormat,
) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let text = format!("Restored #{} | {}", entry.id, entry.title);
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = String::from("## Entry Restored\n\n");
            text.push_str(&format!(
                "**Entry:** #{} - {}\n**Status:** active\n",
                entry.id, entry.title
            ));
            if let Some(r) = reason {
                text.push_str(&format!("**Reason:** {r}\n"));
            }
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let obj = serde_json::json!({
                "restored": true,
                "entry": entry_to_json(entry),
                "reason": reason,
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}

/// Format a status report response with health metrics.
pub fn format_status_report(report: &StatusReport, format: ResponseFormat) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let mut text = format!(
                "Active: {} | Deprecated: {} | Proposed: {} | Quarantined: {} | Corrections: {}",
                report.total_active,
                report.total_deprecated,
                report.total_proposed,
                report.total_quarantined,
                report.total_correction_count
            );
            if report.contradiction_scan_performed {
                text.push_str(&format!(" | Contradictions: {}", report.contradiction_count));
            }
            text.push_str(&format!(
                "\nCoherence: {:.4} (confidence_freshness: {:.4}, graph_quality: {:.4}, embedding_consistency: {:.4}, contradiction_density: {:.4})",
                report.coherence,
                report.confidence_freshness_score,
                report.graph_quality_score,
                report.embedding_consistency_score,
                report.contradiction_density_score,
            ));
            if report.stale_confidence_count > 0 {
                text.push_str(&format!(
                    "\nStale confidence: {} entries",
                    report.stale_confidence_count
                ));
            }
            if report.confidence_refreshed_count > 0 {
                text.push_str(&format!(
                    "\nConfidence refreshed: {} entries",
                    report.confidence_refreshed_count
                ));
            }
            if report.graph_stale_ratio > 0.0 {
                text.push_str(&format!(
                    "\nGraph stale ratio: {:.2}%",
                    report.graph_stale_ratio * 100.0
                ));
            }
            if report.graph_compacted {
                text.push_str("\nGraph compacted: yes");
            }
            for rec in &report.maintenance_recommendations {
                text.push_str(&format!("\nRecommendation: {rec}"));
            }
            text.push_str(&format!(
                "\nCo-access: {} active pairs ({} total), {} stale pairs cleaned",
                report.active_co_access_pairs,
                report.total_co_access_pairs,
                report.stale_pairs_cleaned,
            ));
            if report.total_outcomes > 0 {
                text.push_str(&format!("\nOutcomes: {} total", report.total_outcomes));
            }
            text.push_str(&format!(
                "\nObservation: {} files ({} bytes), oldest {} days, {} retrospected",
                report.observation_file_count,
                report.observation_total_size_bytes,
                report.observation_oldest_file_days,
                report.retrospected_feature_count,
            ));
            if !report.observation_approaching_cleanup.is_empty() {
                text.push_str(&format!(
                    "\nApproaching cleanup (>45 days): {}",
                    report.observation_approaching_cleanup.join(", ")
                ));
            }
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let mut text = String::from("## Knowledge Base Status\n\n");
            text.push_str("### Entry Counts\n");
            text.push_str("| Status | Count |\n|--------|-------|\n");
            text.push_str(&format!("| Active | {} |\n", report.total_active));
            text.push_str(&format!("| Deprecated | {} |\n", report.total_deprecated));
            text.push_str(&format!("| Proposed | {} |\n", report.total_proposed));
            text.push_str(&format!("| Quarantined | {} |\n\n", report.total_quarantined));

            text.push_str("### Category Distribution\n");
            text.push_str("| Category | Count |\n|----------|-------|\n");
            for (cat, count) in &report.category_distribution {
                text.push_str(&format!("| {cat} | {count} |\n"));
            }

            text.push_str("\n### Topic Distribution\n");
            text.push_str("| Topic | Count |\n|-------|-------|\n");
            for (topic, count) in &report.topic_distribution {
                text.push_str(&format!("| {topic} | {count} |\n"));
            }

            text.push_str("\n### Correction Chains\n");
            text.push_str(&format!(
                "- Entries with supersedes: {}\n",
                report.entries_with_supersedes
            ));
            text.push_str(&format!(
                "- Entries with superseded_by: {}\n",
                report.entries_with_superseded_by
            ));
            text.push_str(&format!(
                "- Total correction count: {}\n",
                report.total_correction_count
            ));

            text.push_str("\n### Security Metrics\n");
            text.push_str("| Trust Source | Count |\n|-------------|-------|\n");
            for (source, count) in &report.trust_source_distribution {
                text.push_str(&format!("| {source} | {count} |\n"));
            }
            text.push_str(&format!(
                "\n- Entries without attribution: {}\n",
                report.entries_without_attribution
            ));

            if report.contradiction_scan_performed {
                text.push_str("\n### Contradictions\n\n");
                if report.contradictions.is_empty() {
                    text.push_str("No contradictions detected.\n");
                } else {
                    text.push_str(&format!(
                        "{} contradiction(s) found:\n\n",
                        report.contradiction_count
                    ));
                    text.push_str("| Entry A | Entry B | Similarity | Conflict Score | Explanation |\n");
                    text.push_str("|---------|---------|-----------|---------------|-------------|\n");
                    for pair in &report.contradictions {
                        text.push_str(&format!(
                            "| #{} {} | #{} {} | {:.2} | {:.2} | {} |\n",
                            pair.entry_id_a, pair.title_a,
                            pair.entry_id_b, pair.title_b,
                            pair.similarity, pair.conflict_score,
                            pair.explanation,
                        ));
                    }
                }
            }

            if report.embedding_check_performed {
                text.push_str("\n### Embedding Integrity\n\n");
                if report.embedding_inconsistencies.is_empty() {
                    text.push_str("All embeddings consistent.\n");
                } else {
                    text.push_str(&format!(
                        "{} inconsistency(ies) found:\n\n",
                        report.embedding_inconsistencies.len()
                    ));
                    text.push_str("| Entry | Title | Self-Match Similarity |\n");
                    text.push_str("|-------|-------|----------------------|\n");
                    for inc in &report.embedding_inconsistencies {
                        text.push_str(&format!(
                            "| #{} | {} | {:.4} |\n",
                            inc.entry_id, inc.title, inc.expected_similarity,
                        ));
                    }
                }
            }

            text.push_str("\n### Coherence\n\n");
            text.push_str(&format!("- **Lambda**: {:.4}\n", report.coherence));
            text.push_str(&format!(
                "- **Confidence Freshness**: {:.4}\n",
                report.confidence_freshness_score
            ));
            text.push_str(&format!(
                "- **Graph Quality**: {:.4}\n",
                report.graph_quality_score
            ));
            text.push_str(&format!(
                "- **Embedding Consistency**: {:.4}\n",
                report.embedding_consistency_score
            ));
            text.push_str(&format!(
                "- **Contradiction Density**: {:.4}\n\n",
                report.contradiction_density_score
            ));
            text.push_str(&format!(
                "Stale confidence entries: {}\n",
                report.stale_confidence_count
            ));
            text.push_str(&format!(
                "Confidence refreshed: {}\n",
                report.confidence_refreshed_count
            ));
            text.push_str(&format!(
                "Graph stale ratio: {:.2}%\n",
                report.graph_stale_ratio * 100.0
            ));
            text.push_str(&format!(
                "Graph compacted: {}\n",
                if report.graph_compacted { "yes" } else { "no" }
            ));
            if !report.maintenance_recommendations.is_empty() {
                text.push_str("\n#### Maintenance Recommendations\n\n");
                for rec in &report.maintenance_recommendations {
                    text.push_str(&format!("- {rec}\n"));
                }
            }

            text.push_str("\n### Co-Access Patterns\n\n");
            text.push_str(&format!(
                "- Active pairs: {} of {} total\n",
                report.active_co_access_pairs, report.total_co_access_pairs
            ));
            text.push_str(&format!(
                "- Stale pairs cleaned: {}\n",
                report.stale_pairs_cleaned
            ));
            if !report.top_co_access_pairs.is_empty() {
                text.push_str("\n#### Top Co-Access Clusters\n");
                text.push_str("| Entry A | Entry B | Count | Last Updated |\n");
                text.push_str("|---------|---------|-------|-------------|\n");
                for cluster in &report.top_co_access_pairs {
                    text.push_str(&format!(
                        "| {} (#{}) | {} (#{}) | {} | {} |\n",
                        cluster.title_a, cluster.entry_id_a,
                        cluster.title_b, cluster.entry_id_b,
                        cluster.count,
                        format_timestamp(cluster.last_updated),
                    ));
                }
            }

            if report.total_outcomes > 0 || !report.outcomes_by_type.is_empty() {
                text.push_str("\n### Outcome Statistics\n\n");
                text.push_str(&format!("- Total outcomes: {}\n", report.total_outcomes));

                if !report.outcomes_by_type.is_empty() {
                    text.push_str("\n#### By Workflow Type\n");
                    text.push_str("| Type | Count |\n|------|-------|\n");
                    for (type_name, count) in &report.outcomes_by_type {
                        text.push_str(&format!("| {} | {} |\n", type_name, count));
                    }
                }

                if !report.outcomes_by_result.is_empty() {
                    text.push_str("\n#### By Result\n");
                    text.push_str("| Result | Count |\n|--------|-------|\n");
                    for (result_name, count) in &report.outcomes_by_result {
                        text.push_str(&format!("| {} | {} |\n", result_name, count));
                    }
                }

                if !report.outcomes_by_feature_cycle.is_empty() {
                    text.push_str("\n#### Top Feature Cycles\n");
                    text.push_str("| Feature Cycle | Outcomes |\n|--------------|----------|\n");
                    for (fc, count) in &report.outcomes_by_feature_cycle {
                        text.push_str(&format!("| {} | {} |\n", fc, count));
                    }
                }
            }

            text.push_str("\n### Observation Pipeline\n\n");
            text.push_str(&format!("- Files: {}\n", report.observation_file_count));
            text.push_str(&format!(
                "- Total size: {} bytes\n",
                report.observation_total_size_bytes
            ));
            text.push_str(&format!(
                "- Oldest file: {} days\n",
                report.observation_oldest_file_days
            ));
            text.push_str(&format!(
                "- Retrospected features: {}\n",
                report.retrospected_feature_count
            ));
            if !report.observation_approaching_cleanup.is_empty() {
                text.push_str(&format!(
                    "- **Approaching cleanup**: {}\n",
                    report.observation_approaching_cleanup.join(", ")
                ));
            }

            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let cat_dist: serde_json::Value = report
                .category_distribution
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into();
            let topic_dist: serde_json::Value = report
                .topic_distribution
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into();
            let trust_dist: serde_json::Value = report
                .trust_source_distribution
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect::<serde_json::Map<String, serde_json::Value>>()
                .into();

            let mut obj = serde_json::json!({
                "total_active": report.total_active,
                "total_deprecated": report.total_deprecated,
                "total_proposed": report.total_proposed,
                "total_quarantined": report.total_quarantined,
                "category_distribution": cat_dist,
                "topic_distribution": topic_dist,
                "correction_chains": {
                    "entries_with_supersedes": report.entries_with_supersedes,
                    "entries_with_superseded_by": report.entries_with_superseded_by,
                    "total_correction_count": report.total_correction_count,
                },
                "security": {
                    "trust_source_distribution": trust_dist,
                    "entries_without_attribution": report.entries_without_attribution,
                },
            });

            obj["coherence"] = serde_json::json!(report.coherence);
            obj["confidence_freshness_score"] = serde_json::json!(report.confidence_freshness_score);
            obj["graph_quality_score"] = serde_json::json!(report.graph_quality_score);
            obj["embedding_consistency_score"] = serde_json::json!(report.embedding_consistency_score);
            obj["contradiction_density_score"] = serde_json::json!(report.contradiction_density_score);
            obj["stale_confidence_count"] = serde_json::json!(report.stale_confidence_count);
            obj["confidence_refreshed_count"] = serde_json::json!(report.confidence_refreshed_count);
            obj["graph_stale_ratio"] = serde_json::json!(report.graph_stale_ratio);
            obj["graph_compacted"] = serde_json::json!(report.graph_compacted);
            obj["maintenance_recommendations"] = serde_json::json!(report.maintenance_recommendations);

            if report.contradiction_scan_performed {
                let contradictions: Vec<serde_json::Value> = report.contradictions.iter().map(|p| {
                    serde_json::json!({
                        "entry_id_a": p.entry_id_a,
                        "entry_id_b": p.entry_id_b,
                        "title_a": p.title_a,
                        "title_b": p.title_b,
                        "similarity": p.similarity,
                        "conflict_score": p.conflict_score,
                        "explanation": p.explanation,
                    })
                }).collect();
                obj["contradictions"] = serde_json::json!(contradictions);
                obj["contradiction_count"] = serde_json::json!(report.contradiction_count);
            }

            if report.embedding_check_performed {
                let inconsistencies: Vec<serde_json::Value> = report.embedding_inconsistencies.iter().map(|i| {
                    serde_json::json!({
                        "entry_id": i.entry_id,
                        "title": i.title,
                        "self_match_similarity": i.expected_similarity,
                    })
                }).collect();
                obj["embedding_inconsistencies"] = serde_json::json!(inconsistencies);
            }

            let top_clusters: Vec<serde_json::Value> = report.top_co_access_pairs.iter().map(|c| {
                serde_json::json!({
                    "entry_a": { "id": c.entry_id_a, "title": c.title_a },
                    "entry_b": { "id": c.entry_id_b, "title": c.title_b },
                    "count": c.count,
                    "last_updated": c.last_updated,
                })
            }).collect();
            obj["co_access"] = serde_json::json!({
                "total_pairs": report.total_co_access_pairs,
                "active_pairs": report.active_co_access_pairs,
                "stale_pairs_cleaned": report.stale_pairs_cleaned,
                "top_clusters": top_clusters,
            });

            if report.total_outcomes > 0 || !report.outcomes_by_type.is_empty() {
                let type_dist: serde_json::Value = report
                    .outcomes_by_type
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                    .collect::<serde_json::Map<String, serde_json::Value>>()
                    .into();
                let result_dist: serde_json::Value = report
                    .outcomes_by_result
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                    .collect::<serde_json::Map<String, serde_json::Value>>()
                    .into();
                let fc_list: Vec<serde_json::Value> = report
                    .outcomes_by_feature_cycle
                    .iter()
                    .map(|(fc, count)| {
                        serde_json::json!({"feature_cycle": fc, "count": count})
                    })
                    .collect();

                obj["outcomes"] = serde_json::json!({
                    "total": report.total_outcomes,
                    "by_type": type_dist,
                    "by_result": result_dist,
                    "top_feature_cycles": fc_list,
                });
            }

            obj["observation"] = serde_json::json!({
                "file_count": report.observation_file_count,
                "total_size_bytes": report.observation_total_size_bytes,
                "oldest_file_days": report.observation_oldest_file_days,
                "approaching_cleanup": report.observation_approaching_cleanup,
                "retrospected_feature_count": report.retrospected_feature_count,
            });

            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&obj).unwrap_or_default(),
            )])
        }
    }
}

/// Format a RetrospectiveReport as a JSON CallToolResult.
pub fn format_retrospective_report(
    report: &unimatrix_observe::RetrospectiveReport,
) -> CallToolResult {
    let json = serde_json::to_string_pretty(report).unwrap_or_default();
    CallToolResult::success(vec![Content::text(json)])
}

/// Format a briefing response with conventions and relevant context.
#[cfg(feature = "mcp-briefing")]
pub fn format_briefing(briefing: &Briefing, format: ResponseFormat) -> CallToolResult {
    match format {
        ResponseFormat::Summary => {
            let mut lines = vec![
                format!("Briefing for {}: {}", briefing.role, briefing.task),
                format!(
                    "Conventions: {} | Context: {}",
                    briefing.conventions.len(),
                    briefing.relevant_context.len()
                ),
            ];
            if !briefing.search_available {
                lines.push("[search unavailable - lookup only]".to_string());
            }
            CallToolResult::success(vec![Content::text(lines.join("\n"))])
        }
        ResponseFormat::Markdown => {
            let mut text = format!("## Briefing: {}\n\n", briefing.role);
            text.push_str(&format!("**Task:** {}\n\n", briefing.task));

            if !briefing.search_available {
                text.push_str(
                    "> Note: Semantic search unavailable. Showing lookup results only.\n\n",
                );
            }

            text.push_str("### Conventions\n\n");
            if briefing.conventions.is_empty() {
                text.push_str("No conventions found for this role.\n\n");
            } else {
                for entry in &briefing.conventions {
                    text.push_str(&format!("- **{}**: {}\n", entry.title, entry.content));
                }
                text.push('\n');
            }

            text.push_str("### Relevant Context\n\n");
            if briefing.relevant_context.is_empty() {
                text.push_str("No relevant context found.\n");
            } else {
                for (entry, score) in &briefing.relevant_context {
                    text.push_str(&format!(
                        "- **{}** ({:.2}): {}\n",
                        entry.title, score, entry.content
                    ));
                }
            }

            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let conventions: Vec<serde_json::Value> =
                briefing.conventions.iter().map(entry_to_json).collect();
            let context: Vec<serde_json::Value> = briefing
                .relevant_context
                .iter()
                .map(|(e, s)| entry_to_json_with_similarity(e, *s))
                .collect();

            let obj = serde_json::json!({
                "role": briefing.role,
                "task": briefing.task,
                "search_available": briefing.search_available,
                "conventions": conventions,
                "relevant_context": context,
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

// -- alc-002: enrollment response formatting --

fn trust_level_str(tl: TrustLevel) -> &'static str {
    match tl {
        TrustLevel::System => "system",
        TrustLevel::Privileged => "privileged",
        TrustLevel::Internal => "internal",
        TrustLevel::Restricted => "restricted",
    }
}

fn capability_str(cap: &Capability) -> &'static str {
    match cap {
        Capability::Read => "read",
        Capability::Write => "write",
        Capability::Search => "search",
        Capability::Admin => "admin",
    }
}

fn capabilities_str(caps: &[Capability]) -> String {
    caps.iter()
        .map(|c| capability_str(c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format a successful enrollment result for the given response format.
pub fn format_enroll_success(result: &EnrollResult, format: ResponseFormat) -> CallToolResult {
    let action = if result.created {
        "Enrolled"
    } else {
        "Updated"
    };
    let agent = &result.agent;
    let caps = capabilities_str(&agent.capabilities);
    let trust = trust_level_str(agent.trust_level);

    match format {
        ResponseFormat::Summary => {
            let text = format!(
                "{action} agent '{}' as {trust} with capabilities: {caps}",
                agent.agent_id
            );
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Markdown => {
            let text = format!(
                "---BEGIN UNIMATRIX RESPONSE---\n\
                 ## Agent {action}\n\n\
                 | Field | Value |\n\
                 |-------|-------|\n\
                 | Agent ID | {} |\n\
                 | Action | {action} |\n\
                 | Trust Level | {trust} |\n\
                 | Capabilities | {caps} |\n\
                 ---END UNIMATRIX RESPONSE---",
                agent.agent_id
            );
            CallToolResult::success(vec![Content::text(text)])
        }
        ResponseFormat::Json => {
            let json = serde_json::json!({
                "action": action.to_lowercase(),
                "agent_id": agent.agent_id,
                "trust_level": trust,
                "capabilities": agent.capabilities
                    .iter()
                    .map(|c| capability_str(c))
                    .collect::<Vec<_>>(),
            });
            CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&json).unwrap_or_default(),
            )])
        }
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
            helpful_count: 0,
            unhelpful_count: 0,
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
            (make_entry(1, "Entry 1", "c1"), 0.95_f64),
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
            (make_entry(1, "Entry 1", "content 1"), 0.95_f64),
            (make_entry(2, "Entry 2", "content 2"), 0.88_f64),
        ];
        let result = format_search_results(&results, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("[KNOWLEDGE DATA]"));
        assert!(text.contains("similarity: 0.95"));
    }

    #[test]
    fn test_format_search_results_json() {
        let results = vec![
            (make_entry(1, "E1", "c1"), 0.95_f64),
            (make_entry(2, "E2", "c2"), 0.88_f64),
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

    // -- vnc-003: entry_to_json correction fields --

    #[test]
    fn test_entry_to_json_includes_correction_fields() {
        let mut entry = make_entry(1, "Test", "content");
        entry.supersedes = Some(10);
        entry.superseded_by = Some(20);
        entry.correction_count = 3;
        let json = entry_to_json(&entry);
        assert_eq!(json["supersedes"], 10);
        assert_eq!(json["superseded_by"], 20);
        assert_eq!(json["correction_count"], 3);
    }

    // -- vnc-003: format_correct_success --

    #[test]
    fn test_format_correct_success_summary() {
        let mut original = make_entry(42, "Original Title", "old content");
        original.status = Status::Deprecated;
        let correction = make_entry(43, "Corrected Title", "new content");
        let result = format_correct_success(&original, &correction, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Corrected #42 -> #43"));
        assert!(text.contains("Corrected Title"));
    }

    #[test]
    fn test_format_correct_success_markdown() {
        let mut original = make_entry(42, "Old", "old");
        original.status = Status::Deprecated;
        let correction = make_entry(43, "New", "new");
        let result = format_correct_success(&original, &correction, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Correction Applied"));
        assert!(text.contains("Original (deprecated)"));
        assert!(text.contains("#42"));
        assert!(text.contains("#43"));
    }

    #[test]
    fn test_format_correct_success_json() {
        let mut original = make_entry(42, "Old", "old");
        original.status = Status::Deprecated;
        let correction = make_entry(43, "New", "new");
        let result = format_correct_success(&original, &correction, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["corrected"], true);
        assert_eq!(parsed["original"]["id"], 42);
        assert_eq!(parsed["correction"]["id"], 43);
    }

    #[test]
    fn test_format_correct_success_original_shows_deprecated() {
        let mut original = make_entry(42, "Old", "old");
        original.status = Status::Deprecated;
        let correction = make_entry(43, "New", "new");
        let result = format_correct_success(&original, &correction, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["original"]["status"], "deprecated");
    }

    // -- vnc-003: format_deprecate_success --

    #[test]
    fn test_format_deprecate_success_summary() {
        let entry = make_entry(42, "Deprecated Entry", "content");
        let result = format_deprecate_success(&entry, None, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Deprecated #42"));
        assert!(text.contains("Deprecated Entry"));
    }

    #[test]
    fn test_format_deprecate_success_markdown_with_reason() {
        let entry = make_entry(42, "Entry", "content");
        let result =
            format_deprecate_success(&entry, Some("outdated"), ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Entry Deprecated"));
        assert!(text.contains("Reason:"));
        assert!(text.contains("outdated"));
    }

    #[test]
    fn test_format_deprecate_success_markdown_no_reason() {
        let entry = make_entry(42, "Entry", "content");
        let result = format_deprecate_success(&entry, None, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Entry Deprecated"));
        assert!(!text.contains("Reason:"));
    }

    #[test]
    fn test_format_deprecate_success_json() {
        let entry = make_entry(42, "Entry", "content");
        let result = format_deprecate_success(&entry, None, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["deprecated"], true);
        assert_eq!(parsed["entry"]["id"], 42);
    }

    #[test]
    fn test_format_deprecate_success_json_with_reason() {
        let entry = make_entry(42, "Entry", "content");
        let result =
            format_deprecate_success(&entry, Some("obsolete"), ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["reason"], "obsolete");
    }

    #[test]
    fn test_format_deprecate_success_json_no_reason() {
        let entry = make_entry(42, "Entry", "content");
        let result = format_deprecate_success(&entry, None, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(parsed["reason"].is_null());
    }

    // -- vnc-003: format_status_report --

    fn make_status_report() -> StatusReport {
        StatusReport {
            total_active: 10,
            total_deprecated: 3,
            total_proposed: 1,
            total_quarantined: 0,
            category_distribution: vec![
                ("convention".to_string(), 5),
                ("decision".to_string(), 4),
            ],
            topic_distribution: vec![("auth".to_string(), 8)],
            entries_with_supersedes: 2,
            entries_with_superseded_by: 2,
            total_correction_count: 3,
            trust_source_distribution: vec![("agent".to_string(), 12)],
            entries_without_attribution: 1,
            contradictions: Vec::new(),
            contradiction_count: 0,
            embedding_inconsistencies: Vec::new(),
            contradiction_scan_performed: false,
            embedding_check_performed: false,
            total_co_access_pairs: 0,
            active_co_access_pairs: 0,
            top_co_access_pairs: Vec::new(),
            stale_pairs_cleaned: 0,
            coherence: 1.0,
            confidence_freshness_score: 1.0,
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,
            confidence_refreshed_count: 0,
            graph_stale_ratio: 0.0,
            graph_compacted: false,
            maintenance_recommendations: Vec::new(),
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
        }
    }

    #[test]
    fn test_format_status_report_summary() {
        let report = make_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Active: 10"));
        assert!(text.contains("Deprecated: 3"));
        assert!(text.contains("Proposed: 1"));
        assert!(text.contains("Corrections: 3"));
    }

    #[test]
    fn test_format_status_report_markdown() {
        let report = make_status_report();
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Entry Counts"));
        assert!(text.contains("Category Distribution"));
        assert!(text.contains("Correction Chains"));
        assert!(text.contains("Security Metrics"));
    }

    #[test]
    fn test_format_status_report_json() {
        let report = make_status_report();
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["total_active"], 10);
        assert_eq!(parsed["total_deprecated"], 3);
        assert!(parsed["category_distribution"]["convention"].is_number());
        assert!(parsed["correction_chains"]["total_correction_count"].is_number());
        assert!(parsed["security"]["entries_without_attribution"].is_number());
    }

    #[test]
    fn test_format_status_report_empty() {
        let report = StatusReport {
            total_active: 0,
            total_deprecated: 0,
            total_proposed: 0,
            total_quarantined: 0,
            category_distribution: vec![],
            topic_distribution: vec![],
            entries_with_supersedes: 0,
            entries_with_superseded_by: 0,
            total_correction_count: 0,
            trust_source_distribution: vec![],
            entries_without_attribution: 0,
            contradictions: Vec::new(),
            contradiction_count: 0,
            embedding_inconsistencies: Vec::new(),
            contradiction_scan_performed: false,
            embedding_check_performed: false,
            total_co_access_pairs: 0,
            active_co_access_pairs: 0,
            top_co_access_pairs: Vec::new(),
            stale_pairs_cleaned: 0,
            coherence: 1.0,
            confidence_freshness_score: 1.0,
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,
            confidence_refreshed_count: 0,
            graph_stale_ratio: 0.0,
            graph_compacted: false,
            maintenance_recommendations: Vec::new(),
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
        };
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Active: 0"));
    }

    // -- crt-003: StatusReport with contradictions --

    #[test]
    fn test_status_report_with_contradictions_summary() {
        let pair = crate::contradiction::ContradictionPair {
            entry_id_a: 1,
            entry_id_b: 5,
            title_a: "Entry A".to_string(),
            title_b: "Entry B".to_string(),
            similarity: 0.92,
            conflict_score: 0.6,
            explanation: "negation opposition (1.00)".to_string(),
        };
        let report = StatusReport {
            total_active: 10,
            total_deprecated: 2,
            total_proposed: 1,
            total_quarantined: 3,
            category_distribution: vec![],
            topic_distribution: vec![],
            entries_with_supersedes: 0,
            entries_with_superseded_by: 0,
            total_correction_count: 0,
            trust_source_distribution: vec![],
            entries_without_attribution: 0,
            contradictions: vec![pair],
            contradiction_count: 1,
            embedding_inconsistencies: Vec::new(),
            contradiction_scan_performed: true,
            embedding_check_performed: false,
            total_co_access_pairs: 0,
            active_co_access_pairs: 0,
            top_co_access_pairs: Vec::new(),
            stale_pairs_cleaned: 0,
            coherence: 1.0,
            confidence_freshness_score: 1.0,
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,
            confidence_refreshed_count: 0,
            graph_stale_ratio: 0.0,
            graph_compacted: false,
            maintenance_recommendations: Vec::new(),
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
        };

        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Quarantined: 3"), "summary should include quarantined count");
        assert!(text.contains("Contradictions: 1"), "summary should include contradiction count");
    }

    #[test]
    fn test_status_report_with_contradictions_markdown() {
        let pair = crate::contradiction::ContradictionPair {
            entry_id_a: 1,
            entry_id_b: 5,
            title_a: "Entry A".to_string(),
            title_b: "Entry B".to_string(),
            similarity: 0.92,
            conflict_score: 0.6,
            explanation: "negation opposition".to_string(),
        };
        let report = StatusReport {
            total_active: 10,
            total_deprecated: 2,
            total_proposed: 1,
            total_quarantined: 3,
            category_distribution: vec![],
            topic_distribution: vec![],
            entries_with_supersedes: 0,
            entries_with_superseded_by: 0,
            total_correction_count: 0,
            trust_source_distribution: vec![],
            entries_without_attribution: 0,
            contradictions: vec![pair],
            contradiction_count: 1,
            embedding_inconsistencies: Vec::new(),
            contradiction_scan_performed: true,
            embedding_check_performed: false,
            total_co_access_pairs: 0,
            active_co_access_pairs: 0,
            top_co_access_pairs: Vec::new(),
            stale_pairs_cleaned: 0,
            coherence: 1.0,
            confidence_freshness_score: 1.0,
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,
            confidence_refreshed_count: 0,
            graph_stale_ratio: 0.0,
            graph_compacted: false,
            maintenance_recommendations: Vec::new(),
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
        };

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Quarantined | 3"), "markdown should have quarantined row");
        assert!(text.contains("### Contradictions"), "markdown should have contradictions section");
        assert!(text.contains("1 contradiction(s)"), "should show count");
        assert!(text.contains("Entry A"), "should show entry titles");
    }

    #[test]
    fn test_status_report_with_contradictions_json() {
        let pair = crate::contradiction::ContradictionPair {
            entry_id_a: 1,
            entry_id_b: 5,
            title_a: "Entry A".to_string(),
            title_b: "Entry B".to_string(),
            similarity: 0.92,
            conflict_score: 0.6,
            explanation: "negation opposition".to_string(),
        };
        let report = StatusReport {
            total_active: 10,
            total_deprecated: 2,
            total_proposed: 1,
            total_quarantined: 3,
            category_distribution: vec![],
            topic_distribution: vec![],
            entries_with_supersedes: 0,
            entries_with_superseded_by: 0,
            total_correction_count: 0,
            trust_source_distribution: vec![],
            entries_without_attribution: 0,
            contradictions: vec![pair],
            contradiction_count: 1,
            embedding_inconsistencies: Vec::new(),
            contradiction_scan_performed: true,
            embedding_check_performed: false,
            total_co_access_pairs: 0,
            active_co_access_pairs: 0,
            top_co_access_pairs: Vec::new(),
            stale_pairs_cleaned: 0,
            coherence: 1.0,
            confidence_freshness_score: 1.0,
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,
            confidence_refreshed_count: 0,
            graph_stale_ratio: 0.0,
            graph_compacted: false,
            maintenance_recommendations: Vec::new(),
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
        };

        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["total_quarantined"], 3);
        assert_eq!(parsed["contradiction_count"], 1);
        assert!(parsed["contradictions"].is_array());
        assert_eq!(parsed["contradictions"][0]["entry_id_a"], 1);
        assert_eq!(parsed["contradictions"][0]["entry_id_b"], 5);
    }

    #[test]
    fn test_status_report_embedding_integrity_markdown() {
        let inc = crate::contradiction::EmbeddingInconsistency {
            entry_id: 42,
            title: "Suspicious Entry".to_string(),
            expected_similarity: 0.75,
        };
        let report = StatusReport {
            total_active: 5,
            total_deprecated: 0,
            total_proposed: 0,
            total_quarantined: 0,
            category_distribution: vec![],
            topic_distribution: vec![],
            entries_with_supersedes: 0,
            entries_with_superseded_by: 0,
            total_correction_count: 0,
            trust_source_distribution: vec![],
            entries_without_attribution: 0,
            contradictions: Vec::new(),
            contradiction_count: 0,
            embedding_inconsistencies: vec![inc],
            contradiction_scan_performed: false,
            embedding_check_performed: true,
            total_co_access_pairs: 0,
            active_co_access_pairs: 0,
            top_co_access_pairs: Vec::new(),
            stale_pairs_cleaned: 0,
            coherence: 1.0,
            confidence_freshness_score: 1.0,
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,
            confidence_refreshed_count: 0,
            graph_stale_ratio: 0.0,
            graph_compacted: false,
            maintenance_recommendations: Vec::new(),
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
        };

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("### Embedding Integrity"), "should have embedding section");
        assert!(text.contains("1 inconsistency"), "should show count");
        assert!(text.contains("Suspicious Entry"), "should show entry title");
        assert!(text.contains("0.7500"), "should show similarity");
    }

    // -- vnc-003/vnc-007: format_briefing (duties removed in vnc-007) --

    #[cfg(feature = "mcp-briefing")]
    fn make_briefing(search_available: bool) -> Briefing {
        Briefing {
            role: "architect".to_string(),
            task: "design auth module".to_string(),
            conventions: vec![make_entry(1, "Convention 1", "Always use trait objects")],
            relevant_context: vec![(make_entry(3, "Context 1", "Auth patterns"), 0.85)],
            search_available,
        }
    }

    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn test_format_briefing_summary() {
        let briefing = make_briefing(true);
        let result = format_briefing(&briefing, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Briefing for architect"));
        assert!(text.contains("Conventions: 1"));
        assert!(text.contains("Context: 1"));
        assert!(!text.contains("Duties"));
    }

    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn test_format_briefing_markdown_all_sections() {
        let briefing = make_briefing(true);
        let result = format_briefing(&briefing, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("### Conventions"));
        assert!(text.contains("### Relevant Context"));
        assert!(text.contains("Convention 1"));
        assert!(text.contains("Context 1"));
        assert!(!text.contains("Duties"));
        assert!(!text.contains("duties"));
    }

    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn test_format_briefing_markdown_search_unavailable() {
        let briefing = make_briefing(false);
        let result = format_briefing(&briefing, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("search unavailable"));
    }

    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn test_format_briefing_json() {
        let briefing = make_briefing(true);
        let result = format_briefing(&briefing, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["role"], "architect");
        assert_eq!(parsed["task"], "design auth module");
        assert_eq!(parsed["search_available"], true);
        assert!(parsed["conventions"].is_array());
        assert!(parsed["relevant_context"].is_array());
        assert!(parsed.get("duties").is_none());
    }

    #[cfg(feature = "mcp-briefing")]
    #[test]
    fn test_format_briefing_empty_sections() {
        let briefing = Briefing {
            role: "dev".to_string(),
            task: "code".to_string(),
            conventions: vec![],
            relevant_context: vec![],
            search_available: true,
        };
        let result = format_briefing(&briefing, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("No conventions found"));
        assert!(text.contains("No relevant context found"));
        assert!(!text.contains("duties"));
        assert!(!text.contains("Duties"));
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

    // -- crt-004: Co-access fields in StatusReport --

    fn make_status_report_with_co_access() -> StatusReport {
        StatusReport {
            total_active: 10,
            total_deprecated: 0,
            total_proposed: 0,
            total_quarantined: 0,
            category_distribution: vec![],
            topic_distribution: vec![],
            entries_with_supersedes: 0,
            entries_with_superseded_by: 0,
            total_correction_count: 0,
            trust_source_distribution: vec![],
            entries_without_attribution: 0,
            contradictions: Vec::new(),
            contradiction_count: 0,
            embedding_inconsistencies: Vec::new(),
            contradiction_scan_performed: false,
            embedding_check_performed: false,
            total_co_access_pairs: 15,
            active_co_access_pairs: 12,
            top_co_access_pairs: vec![
                CoAccessClusterEntry {
                    entry_id_a: 1,
                    entry_id_b: 5,
                    title_a: "Entry Alpha".to_string(),
                    title_b: "Entry Beta".to_string(),
                    count: 8,
                    last_updated: 1700000000,
                },
            ],
            stale_pairs_cleaned: 3,
            coherence: 1.0,
            confidence_freshness_score: 1.0,
            graph_quality_score: 1.0,
            embedding_consistency_score: 1.0,
            contradiction_density_score: 1.0,
            stale_confidence_count: 0,
            confidence_refreshed_count: 0,
            graph_stale_ratio: 0.0,
            graph_compacted: false,
            maintenance_recommendations: Vec::new(),
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
        }
    }

    #[test]
    fn test_status_report_co_access_summary() {
        let report = make_status_report_with_co_access();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Co-access: 12 active pairs (15 total)"), "summary should show co-access stats, got: {text}");
        assert!(text.contains("3 stale pairs cleaned"), "should show cleaned count");
    }

    #[test]
    fn test_status_report_co_access_markdown() {
        let report = make_status_report_with_co_access();
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("### Co-Access Patterns"), "markdown should have co-access section");
        assert!(text.contains("Active pairs: 12 of 15"), "should show active/total");
        assert!(text.contains("Stale pairs cleaned: 3"), "should show cleaned");
        assert!(text.contains("Top Co-Access Clusters"), "should have clusters table");
        assert!(text.contains("Entry Alpha"), "should show title_a");
        assert!(text.contains("Entry Beta"), "should show title_b");
    }

    #[test]
    fn test_status_report_co_access_json() {
        let report = make_status_report_with_co_access();
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["co_access"]["total_pairs"], 15);
        assert_eq!(parsed["co_access"]["active_pairs"], 12);
        assert_eq!(parsed["co_access"]["stale_pairs_cleaned"], 3);
        assert!(parsed["co_access"]["top_clusters"].is_array());
        assert_eq!(parsed["co_access"]["top_clusters"][0]["entry_a"]["id"], 1);
        assert_eq!(parsed["co_access"]["top_clusters"][0]["entry_b"]["id"], 5);
        assert_eq!(parsed["co_access"]["top_clusters"][0]["count"], 8);
    }

    #[test]
    fn test_status_report_co_access_empty() {
        let report = make_status_report();
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["co_access"]["total_pairs"], 0);
        assert_eq!(parsed["co_access"]["active_pairs"], 0);
        assert_eq!(parsed["co_access"]["stale_pairs_cleaned"], 0);
        assert!(parsed["co_access"]["top_clusters"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_status_report_defaults_have_co_access_zero() {
        let report = make_status_report();
        assert_eq!(report.total_co_access_pairs, 0);
        assert_eq!(report.active_co_access_pairs, 0);
        assert!(report.top_co_access_pairs.is_empty());
        assert_eq!(report.stale_pairs_cleaned, 0);
    }

    // -- crt-005: StatusReport Coherence Format Tests --

    fn make_coherence_status_report() -> StatusReport {
        StatusReport {
            total_active: 50,
            total_deprecated: 5,
            total_proposed: 2,
            total_quarantined: 3,
            category_distribution: vec![("decision".to_string(), 30)],
            topic_distribution: vec![("architecture".to_string(), 20)],
            entries_with_supersedes: 4,
            entries_with_superseded_by: 4,
            total_correction_count: 8,
            trust_source_distribution: vec![("agent".to_string(), 50)],
            entries_without_attribution: 2,
            contradictions: Vec::new(),
            contradiction_count: 0,
            embedding_inconsistencies: Vec::new(),
            contradiction_scan_performed: false,
            embedding_check_performed: false,
            total_co_access_pairs: 10,
            active_co_access_pairs: 8,
            top_co_access_pairs: Vec::new(),
            stale_pairs_cleaned: 0,
            coherence: 0.7450,
            confidence_freshness_score: 0.8200,
            graph_quality_score: 0.6500,
            embedding_consistency_score: 0.9000,
            contradiction_density_score: 0.7000,
            stale_confidence_count: 15,
            confidence_refreshed_count: 10,
            graph_stale_ratio: 0.15,
            graph_compacted: true,
            maintenance_recommendations: vec![
                "15 entries have stale confidence (oldest: 2 days) -- run with maintain: true to refresh".to_string(),
                "HNSW graph has 15% stale nodes -- run with maintain: true to compact".to_string(),
            ],
            total_outcomes: 0,
            outcomes_by_type: Vec::new(),
            outcomes_by_result: Vec::new(),
            outcomes_by_feature_cycle: Vec::new(),
            observation_file_count: 0,
            observation_total_size_bytes: 0,
            observation_oldest_file_days: 0,
            observation_approaching_cleanup: Vec::new(),
            retrospected_feature_count: 0,
        }
    }

    // UT-C6-01: JSON format includes all 10 coherence fields
    #[test]
    fn test_coherence_json_all_fields() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert!(parsed["coherence"].is_number(), "coherence field missing");
        assert!(parsed["confidence_freshness_score"].is_number(), "confidence_freshness_score missing");
        assert!(parsed["graph_quality_score"].is_number(), "graph_quality_score missing");
        assert!(parsed["embedding_consistency_score"].is_number(), "embedding_consistency_score missing");
        assert!(parsed["contradiction_density_score"].is_number(), "contradiction_density_score missing");
        assert!(parsed["stale_confidence_count"].is_number(), "stale_confidence_count missing");
        assert!(parsed["confidence_refreshed_count"].is_number(), "confidence_refreshed_count missing");
        assert!(parsed["graph_stale_ratio"].is_number(), "graph_stale_ratio missing");
        assert!(parsed["graph_compacted"].is_boolean(), "graph_compacted missing");
        assert!(parsed["maintenance_recommendations"].is_array(), "maintenance_recommendations missing");

        // Verify f64 values
        assert_eq!(parsed["coherence"].as_f64().unwrap(), 0.745);
        assert_eq!(parsed["confidence_freshness_score"].as_f64().unwrap(), 0.82);
        assert_eq!(parsed["graph_quality_score"].as_f64().unwrap(), 0.65);
        assert_eq!(parsed["stale_confidence_count"].as_u64().unwrap(), 15);
        assert_eq!(parsed["confidence_refreshed_count"].as_u64().unwrap(), 10);
        assert_eq!(parsed["graph_compacted"].as_bool().unwrap(), true);
        assert_eq!(parsed["maintenance_recommendations"].as_array().unwrap().len(), 2);
    }

    // UT-C6-02: JSON f64 precision verification
    #[test]
    fn test_coherence_json_f64_precision() {
        let mut report = make_coherence_status_report();
        report.coherence = 0.845;
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        // Should contain 0.845 not f32 artifact like 0.8450000286102295
        assert!(text.contains("0.845"), "JSON should contain 0.845 without f32 artifacts, got: {text}");
        assert!(!text.contains("0.8450000"), "JSON should not contain f32 precision artifact");
    }

    // UT-C6-03: Markdown format includes coherence section
    #[test]
    fn test_coherence_markdown_section() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);

        assert!(text.contains("### Coherence"), "markdown should have Coherence section");
        assert!(text.contains("**Lambda**"), "should show Lambda label");
        assert!(text.contains("**Confidence Freshness**"), "should show Confidence Freshness label");
        assert!(text.contains("**Graph Quality**"), "should show Graph Quality label");
        assert!(text.contains("**Embedding Consistency**"), "should show Embedding Consistency label");
        assert!(text.contains("**Contradiction Density**"), "should show Contradiction Density label");
        // Check 4 decimal places
        assert!(text.contains("0.7450"), "lambda should be formatted to 4 decimal places");
        assert!(text.contains("0.8200"), "confidence freshness should be 4 decimal places");
    }

    // UT-C6-04: Summary format includes coherence line
    #[test]
    fn test_coherence_summary_line() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);

        assert!(text.contains("Coherence:"), "summary should contain Coherence line");
        assert!(text.contains("confidence_freshness:"), "should show dimension breakdowns");
        assert!(text.contains("graph_quality:"), "should show graph_quality");
        assert!(text.contains("embedding_consistency:"), "should show embedding_consistency");
        assert!(text.contains("contradiction_density:"), "should show contradiction_density");
    }

    // UT-C6-05: Recommendations present in all formats
    #[test]
    fn test_coherence_recommendations_in_all_formats() {
        let report = make_coherence_status_report();

        // JSON
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["maintenance_recommendations"].as_array().unwrap().len(), 2);

        // Markdown
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Maintenance Recommendations"), "markdown should have recommendations section");

        // Summary
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Recommendation:"), "summary should have Recommendation lines");
    }

    // UT-C6-06: No recommendations when empty
    #[test]
    fn test_coherence_no_recommendations() {
        let mut report = make_coherence_status_report();
        report.maintenance_recommendations = Vec::new();

        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(parsed["maintenance_recommendations"].as_array().unwrap().is_empty());

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(!text.contains("Maintenance Recommendations"), "should not show section when empty");

        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(!text.contains("Recommendation:"), "should not show recommendation lines when empty");
    }

    // UT-C6-07: graph_compacted renders correctly
    #[test]
    fn test_coherence_graph_compacted_rendering() {
        let mut report = make_coherence_status_report();
        report.graph_compacted = true;

        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Graph compacted: yes"), "summary should show compacted=yes");

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Graph compacted: yes"), "markdown should show compacted=yes");

        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["graph_compacted"].as_bool().unwrap(), true);

        // Test false
        report.graph_compacted = false;
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Graph compacted: no"), "markdown should show compacted=no");
    }

    // UT-C6-08: Stale confidence count rendering
    #[test]
    fn test_coherence_stale_confidence_rendering() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Stale confidence: 15 entries"), "should show stale count in summary");

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Stale confidence entries: 15"), "should show stale count in markdown");

        // Zero stale count: should not show in summary
        let mut report2 = make_status_report();
        report2.stale_confidence_count = 0;
        let result = format_status_report(&report2, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(!text.contains("Stale confidence:"), "should not show stale line when count is 0");
    }

    // UT-C6-09: Confidence refreshed count rendering
    #[test]
    fn test_coherence_confidence_refreshed_rendering() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Confidence refreshed: 10 entries"), "should show refreshed count");

        // Zero refreshed
        let mut report2 = make_status_report();
        report2.confidence_refreshed_count = 0;
        let result = format_status_report(&report2, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(!text.contains("Confidence refreshed:"), "should not show refreshed line when 0");
    }

    // UT-C6-10: Graph stale ratio rendering
    #[test]
    fn test_coherence_graph_stale_ratio_rendering() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Graph stale ratio: 15.00%"), "should show stale ratio percentage");

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(text.contains("Graph stale ratio: 15.00%"), "markdown should show stale ratio");

        // Zero ratio
        let mut report2 = make_status_report();
        report2.graph_stale_ratio = 0.0;
        let result = format_status_report(&report2, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(!text.contains("Graph stale ratio:"), "should not show stale ratio when 0.0 in summary");
    }

    // UT-C6-11: Default coherence values
    #[test]
    fn test_coherence_default_values() {
        let report = make_status_report();
        assert_eq!(report.coherence, 1.0);
        assert_eq!(report.confidence_freshness_score, 1.0);
        assert_eq!(report.graph_quality_score, 1.0);
        assert_eq!(report.embedding_consistency_score, 1.0);
        assert_eq!(report.contradiction_density_score, 1.0);
        assert_eq!(report.stale_confidence_count, 0);
        assert_eq!(report.confidence_refreshed_count, 0);
        assert_eq!(report.graph_stale_ratio, 0.0);
        assert_eq!(report.graph_compacted, false);
        assert!(report.maintenance_recommendations.is_empty());
    }

    // -- alc-002: enrollment response formatting --

    fn make_enroll_result(created: bool) -> EnrollResult {
        use crate::registry::AgentRecord;
        EnrollResult {
            created,
            agent: AgentRecord {
                agent_id: "test-agent".to_string(),
                trust_level: TrustLevel::Internal,
                capabilities: vec![Capability::Read, Capability::Write, Capability::Search],
                allowed_topics: None,
                allowed_categories: None,
                enrolled_at: 1000,
                last_seen_at: 2000,
                active: true,
            },
        }
    }

    #[test]
    fn test_format_enroll_success_summary_created() {
        let result = make_enroll_result(true);
        let response = format_enroll_success(&result, ResponseFormat::Summary);
        let text = result_text(&response);
        assert!(text.contains("Enrolled"), "should say Enrolled: {text}");
        assert!(text.contains("test-agent"), "should contain agent_id: {text}");
        assert!(text.contains("internal"), "should contain trust level: {text}");
        assert!(text.contains("read"), "should contain capabilities: {text}");
    }

    #[test]
    fn test_format_enroll_success_summary_updated() {
        let result = make_enroll_result(false);
        let response = format_enroll_success(&result, ResponseFormat::Summary);
        let text = result_text(&response);
        assert!(text.contains("Updated"), "should say Updated: {text}");
    }

    #[test]
    fn test_format_enroll_success_markdown() {
        let result = make_enroll_result(true);
        let response = format_enroll_success(&result, ResponseFormat::Markdown);
        let text = result_text(&response);
        assert!(
            text.contains("---BEGIN UNIMATRIX RESPONSE---"),
            "should have begin marker: {text}"
        );
        assert!(
            text.contains("---END UNIMATRIX RESPONSE---"),
            "should have end marker: {text}"
        );
        assert!(text.contains("test-agent"), "should contain agent_id: {text}");
        assert!(text.contains("internal"), "should contain trust level: {text}");
    }

    #[test]
    fn test_format_enroll_success_json() {
        let result = make_enroll_result(true);
        let response = format_enroll_success(&result, ResponseFormat::Json);
        let text = result_text(&response);
        let json: serde_json::Value =
            serde_json::from_str(&text).expect("should be valid JSON");
        assert_eq!(json["action"], "enrolled");
        assert_eq!(json["agent_id"], "test-agent");
        assert_eq!(json["trust_level"], "internal");
        assert!(json["capabilities"].is_array());
    }

    #[test]
    fn test_format_enroll_success_json_updated() {
        let result = make_enroll_result(false);
        let response = format_enroll_success(&result, ResponseFormat::Json);
        let text = result_text(&response);
        let json: serde_json::Value =
            serde_json::from_str(&text).expect("should be valid JSON");
        assert_eq!(json["action"], "updated");
    }

    #[test]
    fn test_trust_level_str_all() {
        assert_eq!(trust_level_str(TrustLevel::System), "system");
        assert_eq!(trust_level_str(TrustLevel::Privileged), "privileged");
        assert_eq!(trust_level_str(TrustLevel::Internal), "internal");
        assert_eq!(trust_level_str(TrustLevel::Restricted), "restricted");
    }

    #[test]
    fn test_capability_str_all() {
        assert_eq!(capability_str(&Capability::Read), "read");
        assert_eq!(capability_str(&Capability::Write), "write");
        assert_eq!(capability_str(&Capability::Search), "search");
        assert_eq!(capability_str(&Capability::Admin), "admin");
    }
}
