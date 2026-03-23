//! Briefing and retrospective report formatting.
//!
//! Provides the WA-5 contract surface: `IndexEntry`, `format_index_table`,
//! and `SNIPPET_CHARS`. See ADR-005 crt-027.

use rmcp::model::{CallToolResult, Content};

/// Number of Unicode characters (not bytes) to include in an index entry snippet.
///
/// UTF-8 safe: computed via `.chars().take(SNIPPET_CHARS)`.
///
/// WA-5 contract: this constant is referenced by WA-5 (PreCompact). Do not change
/// without coordinating with WA-5 design.
pub const SNIPPET_CHARS: usize = 150;

/// Single entry in a knowledge index briefing.
///
/// WA-5 contract type: do not rename fields without updating WA-5 (PreCompact).
/// This is the stable surface WA-5 depends on for transcript prepend.
///
/// See ADR-005 crt-027.
#[derive(Debug, Clone)]
pub struct IndexEntry {
    /// Entry primary key from ENTRIES table.
    pub id: u64,
    /// Entry topic (direct field from EntryRecord.topic, no join required).
    pub topic: String,
    /// Entry category (e.g., "decision", "pattern", "convention").
    pub category: String,
    /// Fused score: similarity + confidence + WA-2 histogram boost.
    /// Range: [0.0, 1.0] approximately (may slightly exceed 1.0 with boosts).
    pub confidence: f64,
    /// First SNIPPET_CHARS Unicode characters of entry content.
    /// UTF-8 char boundary safe (computed via `.chars().take(SNIPPET_CHARS)`).
    pub snippet: String,
}

/// Format a slice of `IndexEntry` as a flat indexed table.
///
/// Column order: row#, id, topic, category, confidence (2 decimal places), snippet.
/// Separator: single line of ASCII dashes after header.
/// Empty slice: returns empty string (not a header-only string).
///
/// WA-5 contract: this function is the canonical renderer.
/// WA-5 prepends transcript content BEFORE the string returned by this function.
/// WA-5 does not parse the rendered string — it only prepends to it.
/// Column widths are implementation details; only the function signature is the contract.
///
/// See ADR-005 crt-027.
pub fn format_index_table(entries: &[IndexEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut output = String::new();

    // Header line — columns: "#" (2), "id" (6), "topic" (20), "cat" (14), "conf" (6), "snippet"
    output.push_str(&format!(
        "{:>2}  {:>6}  {:<20}  {:<14}  {:>6}  {}\n",
        "#", "id", "topic", "cat", "conf", "snippet"
    ));

    // Separator line using ASCII dashes
    output.push_str(&format!(
        "{:->2}  {:->6}  {:->20}  {:->14}  {:->6}  {}\n",
        "",
        "",
        "",
        "",
        "",
        "-".repeat(50)
    ));

    for (i, entry) in entries.iter().enumerate() {
        let row_num = i + 1;

        // Truncate topic to column width for display (EC-05), char-boundary safe
        let topic_display: String = entry.topic.chars().take(20).collect();

        // Truncate category to column width, char-boundary safe
        let cat_display: String = entry.category.chars().take(14).collect();

        // Confidence formatted to exactly 2 decimal places
        let conf_display = format!("{:.2}", entry.confidence);

        output.push_str(&format!(
            "{:>2}  {:>6}  {:<20}  {:<14}  {:>6}  {}\n",
            row_num, entry.id, topic_display, cat_display, conf_display, entry.snippet
        ));
    }

    output
}

/// Format a `RetrospectiveReport` as a JSON `CallToolResult`.
pub fn format_retrospective_report(
    report: &unimatrix_observe::RetrospectiveReport,
) -> CallToolResult {
    let json = serde_json::to_string_pretty(report).unwrap_or_default();
    CallToolResult::success(vec![Content::text(json)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: u64, topic: &str, category: &str, confidence: f64, snippet: &str) -> IndexEntry {
        IndexEntry {
            id,
            topic: topic.to_string(),
            category: category.to_string(),
            confidence,
            snippet: snippet.to_string(),
        }
    }

    // --- R-05 Non-Negotiable Contract Tests ---

    #[test]
    fn format_index_table_empty_returns_empty_string() {
        let entries: Vec<IndexEntry> = vec![];
        let result = format_index_table(&entries);
        assert!(result.is_empty(), "empty slice must return empty string, got: {:?}", result);
    }

    #[test]
    fn format_index_table_columns_present() {
        let entries = vec![make_entry(
            2,
            "product-vision",
            "decision",
            0.60,
            "Unimatrix is a self-learning knowledge engine...",
        )];
        let result = format_index_table(&entries);

        // Header must contain column names
        assert!(result.contains('#'), "header must contain '#'");
        assert!(result.contains("id"), "header must contain 'id'");
        assert!(result.contains("topic"), "header must contain 'topic'");
        assert!(result.contains("cat"), "header must contain 'cat'");
        assert!(result.contains("conf"), "header must contain 'conf'");
        assert!(result.contains("snippet"), "header must contain 'snippet'");

        // Separator line must be present
        assert!(result.contains("--"), "output must contain a separator line");

        // Data row must contain entry fields
        assert!(result.contains('2'), "row must contain entry id '2'");
        assert!(result.contains("product-vision"), "row must contain topic");
        assert!(result.contains("decision"), "row must contain category");
        assert!(result.contains("0.60"), "confidence must be formatted as '0.60'");
        assert!(!result.contains("0.6\n"), "confidence must be '0.60' not '0.6'");
        assert!(!result.contains("0.600"), "confidence must be '0.60' not '0.600'");
        assert!(
            result.contains("Unimatrix is"),
            "row must contain snippet text"
        );

        // Row 1 must be right-justified to at least 2 chars (" 1")
        assert!(result.contains(" 1"), "row number must be right-justified (' 1')");
    }

    #[test]
    fn format_index_table_multibyte_utf8() {
        // CJK characters are 3 bytes each in UTF-8
        let content: String = "\u{4e16}\u{754c}".repeat(200);
        // Build snippet the same way IndexBriefingService would
        let snippet: String = content.chars().take(SNIPPET_CHARS).collect();

        assert!(
            snippet.chars().count() <= 150,
            "snippet must be at most 150 chars"
        );
        // 150 CJK chars * 3 bytes = 450 bytes max
        assert!(
            snippet.len() <= 450,
            "snippet byte length must be <= 450 for CJK"
        );
        // Must be at a valid UTF-8 char boundary
        assert!(
            snippet.is_char_boundary(snippet.len()),
            "snippet must end on a valid UTF-8 char boundary"
        );

        let entry = make_entry(1, "cjk-topic", "pattern", 0.75, &snippet);
        let result = format_index_table(&[entry]);
        // Table must render without panic and contain the entry id
        assert!(result.contains('1'));
    }

    #[test]
    fn format_index_table_sorted_confidence() {
        // Verify the table renders in INPUT order — sorting is caller responsibility
        let entries = vec![
            make_entry(10, "low-conf", "pattern", 0.30, "low"),
            make_entry(20, "high-conf", "decision", 0.90, "high"),
        ];
        let result = format_index_table(&entries);

        // Row 1 must be the first entry (low-conf), row 2 the second (high-conf)
        let lines: Vec<&str> = result.lines().collect();
        // lines[0] = header, lines[1] = separator, lines[2] = row 1, lines[3] = row 2
        assert!(lines.len() >= 4, "must have header + separator + 2 data rows");
        assert!(
            lines[2].contains("low-conf"),
            "row 1 must be first input entry (low-conf)"
        );
        assert!(
            lines[3].contains("high-conf"),
            "row 2 must be second input entry (high-conf)"
        );
    }

    #[test]
    fn format_index_table_no_section_headers() {
        let entries = vec![make_entry(1, "topic-a", "decision", 0.80, "snippet a")];
        let result = format_index_table(&entries);

        assert!(
            !result.contains("## Decisions"),
            "output must not contain '## Decisions'"
        );
        assert!(
            !result.contains("## Conventions"),
            "output must not contain '## Conventions'"
        );
        assert!(
            !result.contains("## Injections"),
            "output must not contain '## Injections'"
        );
    }

    // --- IndexEntry Struct Tests ---

    #[test]
    fn index_entry_fields_accessible() {
        let entry = IndexEntry {
            id: 1,
            topic: "t".to_string(),
            category: "c".to_string(),
            confidence: 0.5,
            snippet: "s".to_string(),
        };
        // All fields must be accessible (pub)
        assert_eq!(entry.id, 1);
        assert_eq!(entry.topic, "t");
        assert_eq!(entry.category, "c");
        assert_eq!(entry.confidence, 0.5);
        assert_eq!(entry.snippet, "s");
    }

    #[test]
    fn index_entry_debug_clone_derive() {
        let entry = IndexEntry {
            id: 42,
            topic: "test".to_string(),
            category: "pattern".to_string(),
            confidence: 0.77,
            snippet: "hello".to_string(),
        };
        let cloned = entry.clone();
        assert_eq!(cloned.id, entry.id);
        // Debug must format without panic
        let _ = format!("{:?}", entry);
    }

    // --- Snippet Truncation Tests (AC-17) ---

    #[test]
    fn snippet_chars_constant_is_150() {
        assert_eq!(SNIPPET_CHARS, 150);
    }

    #[test]
    fn snippet_truncation_utf8_safe_cjk() {
        let content: String = "\u{4e16}\u{754c}".repeat(200);
        let snippet: String = content.chars().take(SNIPPET_CHARS).collect();

        assert!(snippet.chars().count() <= 150);
        assert!(snippet.len() <= 450);
        assert!(snippet.is_char_boundary(snippet.len()));
    }

    #[test]
    fn snippet_truncation_ascii_exactly_150() {
        let content = "a".repeat(150);
        let snippet: String = content.chars().take(SNIPPET_CHARS).collect();
        assert_eq!(snippet, content, "exactly 150 ASCII chars must not be truncated");
    }

    #[test]
    fn snippet_truncation_longer_than_150() {
        let content = "b".repeat(300);
        let snippet: String = content.chars().take(SNIPPET_CHARS).collect();
        assert_eq!(snippet.chars().count(), 150, "300-char content must be truncated to 150");
    }

    // --- Multiple Entry Tests ---

    #[test]
    fn format_index_table_multiple_entries_numbered_sequentially() {
        let entries = vec![
            make_entry(1, "alpha", "decision", 0.9, "first"),
            make_entry(2, "beta", "pattern", 0.7, "second"),
            make_entry(3, "gamma", "convention", 0.5, "third"),
        ];
        let result = format_index_table(&entries);

        assert!(result.contains(" 1"), "must contain row number 1");
        assert!(result.contains(" 2"), "must contain row number 2");
        assert!(result.contains(" 3"), "must contain row number 3");

        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() >= 5, "must have header + separator + 3 data rows");
        assert!(lines[2].trim_start().starts_with('1'), "first data row starts with 1");
        assert!(lines[3].trim_start().starts_with('2'), "second data row starts with 2");
        assert!(lines[4].trim_start().starts_with('3'), "third data row starts with 3");
    }

    #[test]
    fn format_index_table_confidence_formatted_consistently() {
        let entries = vec![
            make_entry(1, "full-conf", "decision", 1.0, "max"),
            make_entry(2, "zero-conf", "pattern", 0.0, "min"),
        ];
        let result = format_index_table(&entries);

        assert!(result.contains("1.00"), "confidence 1.0 must render as '1.00'");
        assert!(result.contains("0.00"), "confidence 0.0 must render as '0.00'");
    }

    #[test]
    fn format_index_table_topic_truncated_to_column_width() {
        let long_topic = "a".repeat(100);
        let entry = make_entry(5, &long_topic, "decision", 0.5, "short snippet");
        let result = format_index_table(&[entry]);

        // The rendered row must not contain the full 100-char topic
        for line in result.lines().skip(2) {
            // Data rows only — the topic column is 20 chars wide
            assert!(
                !line.contains(&long_topic),
                "full 100-char topic must not appear in row (must be truncated)"
            );
        }
    }

    #[test]
    fn format_index_table_header_and_separator_present() {
        let entries = vec![make_entry(1, "test", "decision", 0.5, "snippet")];
        let result = format_index_table(&entries);
        let lines: Vec<&str> = result.lines().collect();

        assert!(lines.len() >= 3, "must have at least header + separator + data row");
        // Header line (first line) must contain column names
        let header = lines[0];
        assert!(header.contains('#'), "header must contain '#'");
        assert!(header.contains("topic"), "header must contain 'topic'");
        assert!(header.contains("snippet"), "header must contain 'snippet'");

        // Separator (second line) must contain dashes
        let separator = lines[1];
        assert!(separator.contains('-'), "separator must contain dashes");
    }
}
