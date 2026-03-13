//! Briefing and retrospective report formatting.

use rmcp::model::{CallToolResult, Content};
use unimatrix_store::EntryRecord;

use super::{ResponseFormat, entry_to_json, entry_to_json_with_similarity};

/// Assembled briefing for format_briefing.
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

/// Format a RetrospectiveReport as a JSON CallToolResult.
pub fn format_retrospective_report(
    report: &unimatrix_observe::RetrospectiveReport,
) -> CallToolResult {
    let json = serde_json::to_string_pretty(report).unwrap_or_default();
    CallToolResult::success(vec![Content::text(json)])
}

/// Format a briefing response with conventions and relevant context.
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
