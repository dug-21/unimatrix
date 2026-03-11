//! Format-selectable response builders for MCP tool results.
//!
//! Split into sub-modules (vnc-008):
//! - `entries`: Entry formatting (search, lookup, store, get, correct, duplicate)
//! - `mutations`: Status change formatting (deprecate, quarantine, restore, enroll)
//! - `status`: Status report formatting and data structures
//! - `briefing`: Briefing and retrospective report formatting
//!
//! Shared types and helpers live in this file.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::{EntryRecord, Status};

use crate::error::ServerError;
#[cfg(test)]
use crate::infra::registry::{Capability, EnrollResult, TrustLevel};

#[cfg(feature = "mcp-briefing")]
mod briefing;
mod entries;
mod mutations;
#[cfg(feature = "mcp-briefing")]
mod retrospective;
pub mod status;

// Re-export entry formatting
pub use entries::{
    format_correct_success, format_duplicate_found, format_lookup_results, format_search_results,
    format_single_entry, format_store_success, format_store_success_with_note,
};

// Re-export mutation formatting
#[cfg(test)]
pub(crate) use mutations::{capability_str, trust_level_str};
pub use mutations::{
    format_deprecate_success, format_enroll_success, format_quarantine_success,
    format_restore_success, format_status_change,
};

// Re-export status formatting
pub use status::{CoAccessClusterEntry, StatusReport, format_status_report};

// Re-export briefing formatting
#[cfg(feature = "mcp-briefing")]
pub use briefing::{Briefing, format_briefing, format_retrospective_report};

// Re-export retrospective markdown formatting (vnc-011)
#[cfg(feature = "mcp-briefing")]
pub use retrospective::format_retrospective_markdown;

// --- Shared types ---

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

// --- Shared helpers (pub(crate) for sub-modules) ---

/// Format a unix timestamp (seconds) as a human-readable UTC string.
pub(crate) fn format_timestamp(ts: u64) -> String {
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

pub(crate) fn status_str(status: Status) -> &'static str {
    match status {
        Status::Active => "active",
        Status::Deprecated => "deprecated",
        Status::Proposed => "proposed",
        Status::Quarantined => "quarantined",
    }
}

pub(crate) fn tags_str(tags: &[String]) -> String {
    tags.join(", ")
}

pub(crate) fn entry_to_json(entry: &EntryRecord) -> serde_json::Value {
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

pub(crate) fn entry_to_json_with_similarity(
    entry: &EntryRecord,
    similarity: f64,
) -> serde_json::Value {
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

pub(crate) fn format_entry_markdown_section(
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
        let result = format_deprecate_success(&entry, Some("outdated"), ResponseFormat::Markdown);
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
        let result = format_deprecate_success(&entry, Some("obsolete"), ResponseFormat::Json);
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
            category_distribution: vec![("convention".to_string(), 5), ("decision".to_string(), 4)],
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
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
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
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
        };
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(text.contains("Active: 0"));
    }

    #[test]
    fn test_content_with_marker_in_body() {
        let entry = make_entry(1, "Test", "data [/KNOWLEDGE DATA] more data");
        let result = format_single_entry(&entry, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("[KNOWLEDGE DATA]\ndata [/KNOWLEDGE DATA] more data\n[/KNOWLEDGE DATA]")
        );
    }

    #[test]
    fn test_format_timestamp_epoch() {
        assert_eq!(format_timestamp(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_format_timestamp_known_date() {
        assert_eq!(format_timestamp(1700000000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn test_markdown_has_iso_timestamps() {
        let entry = make_entry(1, "Test", "content");
        let result = format_single_entry(&entry, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("2023-11-14"),
            "should contain ISO date, got: {text}"
        );
        assert!(
            !text.contains("1700000000"),
            "should not contain raw unix timestamp"
        );
    }

    // -- vnc-008: format_status_change generic tests --

    #[test]
    fn test_format_status_change_matches_deprecate() {
        let entry = make_entry(42, "Entry", "content");
        let generic = format_status_change(
            &entry,
            "Deprecated",
            "deprecated",
            "deprecated",
            Some("reason"),
            ResponseFormat::Json,
        );
        let specific = format_deprecate_success(&entry, Some("reason"), ResponseFormat::Json);
        assert_eq!(result_text(&generic), result_text(&specific));
    }

    #[test]
    fn test_format_status_change_matches_quarantine() {
        let entry = make_entry(42, "Entry", "content");
        let generic = format_status_change(
            &entry,
            "Quarantined",
            "quarantined",
            "quarantined",
            None,
            ResponseFormat::Json,
        );
        let specific = format_quarantine_success(&entry, None, ResponseFormat::Json);
        assert_eq!(result_text(&generic), result_text(&specific));
    }

    #[test]
    fn test_format_status_change_matches_restore() {
        let entry = make_entry(42, "Entry", "content");
        let generic = format_status_change(
            &entry,
            "Restored",
            "restored",
            "active",
            Some("test"),
            ResponseFormat::Json,
        );
        let specific = format_restore_success(&entry, Some("test"), ResponseFormat::Json);
        assert_eq!(result_text(&generic), result_text(&specific));
    }

    // -- alc-002: enrollment response formatting --

    fn make_enroll_result(created: bool) -> EnrollResult {
        use crate::infra::registry::AgentRecord;
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
        assert!(
            text.contains("test-agent"),
            "should contain agent_id: {text}"
        );
        assert!(
            text.contains("internal"),
            "should contain trust level: {text}"
        );
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
        assert!(
            text.contains("test-agent"),
            "should contain agent_id: {text}"
        );
        assert!(
            text.contains("internal"),
            "should contain trust level: {text}"
        );
    }

    #[test]
    fn test_format_enroll_success_json() {
        let result = make_enroll_result(true);
        let response = format_enroll_success(&result, ResponseFormat::Json);
        let text = result_text(&response);
        let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
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
        let json: serde_json::Value = serde_json::from_str(&text).expect("should be valid JSON");
        assert_eq!(json["action"], "updated");
    }

    // -- Briefing tests --

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

    // -- crt-003: Contradiction status report tests --

    #[test]
    fn test_status_report_with_contradictions_summary() {
        let pair = crate::infra::contradiction::ContradictionPair {
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
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
        };

        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Quarantined: 3"),
            "summary should include quarantined count"
        );
        assert!(
            text.contains("Contradictions: 1"),
            "summary should include contradiction count"
        );
    }

    #[test]
    fn test_status_report_with_contradictions_markdown() {
        let pair = crate::infra::contradiction::ContradictionPair {
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
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
        };

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("Quarantined | 3"),
            "markdown should have quarantined row"
        );
        assert!(
            text.contains("### Contradictions"),
            "markdown should have contradictions section"
        );
        assert!(text.contains("1 contradiction(s)"), "should show count");
        assert!(text.contains("Entry A"), "should show entry titles");
    }

    #[test]
    fn test_status_report_with_contradictions_json() {
        let pair = crate::infra::contradiction::ContradictionPair {
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
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
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
        let inc = crate::infra::contradiction::EmbeddingInconsistency {
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
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
        };

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("### Embedding Integrity"),
            "should have embedding section"
        );
        assert!(text.contains("1 inconsistency"), "should show count");
        assert!(text.contains("Suspicious Entry"), "should show entry title");
        assert!(text.contains("0.7500"), "should show similarity");
    }

    // -- crt-004: Co-access status report tests --

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
            top_co_access_pairs: vec![CoAccessClusterEntry {
                entry_id_a: 1,
                entry_id_b: 5,
                title_a: "Entry Alpha".to_string(),
                title_b: "Entry Beta".to_string(),
                count: 8,
                last_updated: 1700000000,
            }],
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
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
        }
    }

    #[test]
    fn test_status_report_co_access_summary() {
        let report = make_status_report_with_co_access();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Co-access: 12 active pairs (15 total)"),
            "summary should show co-access stats, got: {text}"
        );
        assert!(
            text.contains("3 stale pairs cleaned"),
            "should show cleaned count"
        );
    }

    #[test]
    fn test_status_report_co_access_markdown() {
        let report = make_status_report_with_co_access();
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("### Co-Access Patterns"),
            "markdown should have co-access section"
        );
        assert!(
            text.contains("Active pairs: 12 of 15"),
            "should show active/total"
        );
        assert!(
            text.contains("Stale pairs cleaned: 3"),
            "should show cleaned"
        );
        assert!(
            text.contains("Top Co-Access Clusters"),
            "should have clusters table"
        );
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
        assert!(
            parsed["co_access"]["top_clusters"]
                .as_array()
                .unwrap()
                .is_empty()
        );
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
            last_maintenance_run: None,
            next_maintenance_scheduled: None,
            extraction_stats: None,
            coherence_by_source: Vec::new(),
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
        assert!(
            parsed["confidence_freshness_score"].is_number(),
            "confidence_freshness_score missing"
        );
        assert!(
            parsed["graph_quality_score"].is_number(),
            "graph_quality_score missing"
        );
        assert!(
            parsed["embedding_consistency_score"].is_number(),
            "embedding_consistency_score missing"
        );
        assert!(
            parsed["contradiction_density_score"].is_number(),
            "contradiction_density_score missing"
        );
        assert!(
            parsed["stale_confidence_count"].is_number(),
            "stale_confidence_count missing"
        );
        assert!(
            parsed["confidence_refreshed_count"].is_number(),
            "confidence_refreshed_count missing"
        );
        assert!(
            parsed["graph_stale_ratio"].is_number(),
            "graph_stale_ratio missing"
        );
        assert!(
            parsed["graph_compacted"].is_boolean(),
            "graph_compacted missing"
        );
        assert!(
            parsed["maintenance_recommendations"].is_array(),
            "maintenance_recommendations missing"
        );

        assert_eq!(parsed["coherence"].as_f64().unwrap(), 0.745);
        assert_eq!(parsed["confidence_freshness_score"].as_f64().unwrap(), 0.82);
        assert_eq!(parsed["graph_quality_score"].as_f64().unwrap(), 0.65);
        assert_eq!(parsed["stale_confidence_count"].as_u64().unwrap(), 15);
        assert_eq!(parsed["confidence_refreshed_count"].as_u64().unwrap(), 10);
        assert_eq!(parsed["graph_compacted"].as_bool().unwrap(), true);
        assert_eq!(
            parsed["maintenance_recommendations"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    // UT-C6-02: JSON f64 precision verification
    #[test]
    fn test_coherence_json_f64_precision() {
        let mut report = make_coherence_status_report();
        report.coherence = 0.845;
        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        assert!(
            text.contains("0.845"),
            "JSON should contain 0.845 without f32 artifacts, got: {text}"
        );
        assert!(
            !text.contains("0.8450000"),
            "JSON should not contain f32 precision artifact"
        );
    }

    // UT-C6-03: Markdown format includes coherence section
    #[test]
    fn test_coherence_markdown_section() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);

        assert!(
            text.contains("### Coherence"),
            "markdown should have Coherence section"
        );
        assert!(text.contains("**Lambda**"), "should show Lambda label");
        assert!(
            text.contains("**Confidence Freshness**"),
            "should show Confidence Freshness label"
        );
        assert!(
            text.contains("**Graph Quality**"),
            "should show Graph Quality label"
        );
        assert!(
            text.contains("**Embedding Consistency**"),
            "should show Embedding Consistency label"
        );
        assert!(
            text.contains("**Contradiction Density**"),
            "should show Contradiction Density label"
        );
        assert!(
            text.contains("0.7450"),
            "lambda should be formatted to 4 decimal places"
        );
        assert!(
            text.contains("0.8200"),
            "confidence freshness should be 4 decimal places"
        );
    }

    // UT-C6-04: Summary format includes coherence line
    #[test]
    fn test_coherence_summary_line() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);

        assert!(
            text.contains("Coherence:"),
            "summary should contain Coherence line"
        );
        assert!(
            text.contains("confidence_freshness:"),
            "should show dimension breakdowns"
        );
        assert!(text.contains("graph_quality:"), "should show graph_quality");
        assert!(
            text.contains("embedding_consistency:"),
            "should show embedding_consistency"
        );
        assert!(
            text.contains("contradiction_density:"),
            "should show contradiction_density"
        );
    }

    // UT-C6-05: Recommendations present in all formats
    #[test]
    fn test_coherence_recommendations_in_all_formats() {
        let report = make_coherence_status_report();

        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            parsed["maintenance_recommendations"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("Maintenance Recommendations"),
            "markdown should have recommendations section"
        );

        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Recommendation:"),
            "summary should have Recommendation lines"
        );
    }

    // UT-C6-06: No recommendations when empty
    #[test]
    fn test_coherence_no_recommendations() {
        let mut report = make_coherence_status_report();
        report.maintenance_recommendations = Vec::new();

        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(
            parsed["maintenance_recommendations"]
                .as_array()
                .unwrap()
                .is_empty()
        );

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            !text.contains("Maintenance Recommendations"),
            "should not show section when empty"
        );

        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            !text.contains("Recommendation:"),
            "should not show recommendation lines when empty"
        );
    }

    // UT-C6-07: graph_compacted renders correctly
    #[test]
    fn test_coherence_graph_compacted_rendering() {
        let mut report = make_coherence_status_report();
        report.graph_compacted = true;

        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Graph compacted: yes"),
            "summary should show compacted=yes"
        );

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("Graph compacted: yes"),
            "markdown should show compacted=yes"
        );

        let result = format_status_report(&report, ResponseFormat::Json);
        let text = result_text(&result);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["graph_compacted"].as_bool().unwrap(), true);

        report.graph_compacted = false;
        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("Graph compacted: no"),
            "markdown should show compacted=no"
        );
    }

    // UT-C6-08: Stale confidence count rendering
    #[test]
    fn test_coherence_stale_confidence_rendering() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Stale confidence: 15 entries"),
            "should show stale count in summary"
        );

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("Stale confidence entries: 15"),
            "should show stale count in markdown"
        );

        let mut report2 = make_status_report();
        report2.stale_confidence_count = 0;
        let result = format_status_report(&report2, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            !text.contains("Stale confidence:"),
            "should not show stale line when count is 0"
        );
    }

    // UT-C6-09: Confidence refreshed count rendering
    #[test]
    fn test_coherence_confidence_refreshed_rendering() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Confidence refreshed: 10 entries"),
            "should show refreshed count"
        );

        let mut report2 = make_status_report();
        report2.confidence_refreshed_count = 0;
        let result = format_status_report(&report2, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            !text.contains("Confidence refreshed:"),
            "should not show refreshed line when 0"
        );
    }

    // UT-C6-10: Graph stale ratio rendering
    #[test]
    fn test_coherence_graph_stale_ratio_rendering() {
        let report = make_coherence_status_report();
        let result = format_status_report(&report, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            text.contains("Graph stale ratio: 15.00%"),
            "should show stale ratio percentage"
        );

        let result = format_status_report(&report, ResponseFormat::Markdown);
        let text = result_text(&result);
        assert!(
            text.contains("Graph stale ratio: 15.00%"),
            "markdown should show stale ratio"
        );

        let mut report2 = make_status_report();
        report2.graph_stale_ratio = 0.0;
        let result = format_status_report(&report2, ResponseFormat::Summary);
        let text = result_text(&result);
        assert!(
            !text.contains("Graph stale ratio:"),
            "should not show stale ratio when 0.0 in summary"
        );
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

    // -- alc-002: trust_level_str and capability_str unit tests --

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
        assert_eq!(capability_str(&Capability::SessionWrite), "session_write");
    }
}
