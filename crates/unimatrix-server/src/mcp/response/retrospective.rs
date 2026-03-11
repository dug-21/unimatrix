//! Markdown formatter for retrospective reports (vnc-011).
//!
//! Transforms a `RetrospectiveReport` into compact, scannable markdown
//! optimized for LLM consumption. All collapse, filtering, grouping, and
//! deduplication logic lives here.

use rmcp::model::{CallToolResult, Content};

/// Format a retrospective report as compact markdown.
///
/// The formatter controls its own evidence selection (k=3 earliest by timestamp)
/// and ignores the `evidence_limit` parameter — that parameter only applies to
/// the JSON path.
pub fn format_retrospective_markdown(
    report: &unimatrix_observe::RetrospectiveReport,
) -> CallToolResult {
    let mut md = String::new();

    // Header
    md.push_str(&format!("# Retrospective: {}\n", report.feature_cycle));
    md.push_str(&format!(
        "{} sessions | {} tool calls | {}\n",
        report.session_count,
        report.total_records,
        format_duration(report.metrics.universal.total_duration_secs),
    ));

    if report.is_cached {
        md.push_str("\n> Cached result — no new observation data.\n");
    }

    CallToolResult::success(vec![Content::text(md)])
}

/// Format a duration in seconds as a human-readable string.
fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}
