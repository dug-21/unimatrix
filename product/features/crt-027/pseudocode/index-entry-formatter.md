# Index Entry Formatter — Pseudocode
# File: crates/unimatrix-server/src/mcp/response/briefing.rs

## Purpose

Replace the existing `Briefing` struct and `format_briefing` function with:
1. `IndexEntry` struct — the WA-5 contract type (ADR-005).
2. `SNIPPET_CHARS: usize = 150` — constant for snippet truncation.
3. `format_index_table(entries: &[IndexEntry]) -> String` — canonical flat table formatter.

Retain `format_retrospective_report` unchanged.

This file becomes the WA-5 contract surface. Renaming fields or changing the
`format_index_table` signature is a breaking change for WA-5.

---

## Deleted Items

- `pub struct Briefing { ... }` — deleted
- `pub fn format_briefing(briefing: &Briefing, format: ResponseFormat) -> CallToolResult` — deleted
- All tests in the file that reference `Briefing` or `format_briefing` — deleted

## Retained Items

- `pub fn format_retrospective_report(report: &unimatrix_observe::RetrospectiveReport) -> CallToolResult` — unchanged

---

## New Constant

```rust
/// Number of Unicode characters (not bytes) to include in an index entry snippet.
/// UTF-8 safe: computed via .chars().take(SNIPPET_CHARS).
/// WA-5 contract: this constant is referenced by WA-5 (PreCompact). Do not change
/// without coordinating with WA-5 design.
pub const SNIPPET_CHARS: usize = 150;
```

---

## New Type: `IndexEntry`

```rust
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
    /// UTF-8 char boundary safe (computed via .chars().take(SNIPPET_CHARS)).
    pub snippet: String,
}
```

---

## New Function: `format_index_table`

```rust
/// Format a slice of IndexEntry as a flat indexed table.
///
/// Column order: row#, id, topic, category, confidence (2 decimal places), snippet
/// Separator: single line of Unicode box-drawing characters after header
/// Empty slice: returns empty string (not a header-only string)
///
/// WA-5 contract: this function is the canonical renderer.
/// WA-5 prepends transcript content BEFORE the string returned by this function.
/// WA-5 does not parse the rendered string — it only prepends to it.
/// Column widths are implementation details; only the function signature is the contract.
///
/// See ADR-005 crt-027.
pub fn format_index_table(entries: &[IndexEntry]) -> String {
    // AC-18 / R-05 scenario 4: empty slice returns empty string
    if entries.is_empty() {
        return String::new();
    }

    let mut output = String::new();

    // Header line
    // Columns: "#" (2), "id" (6), "topic" (20), "cat" (14), "conf" (6), "snippet" (remainder)
    // Exact widths are implementation details. The header must contain these column names.
    // Use a format that is readable and stable.
    output.push_str(&format!(
        "{:>2}  {:>6}  {:<20}  {:<14}  {:>6}  {}\n",
        "#", "id", "topic", "cat", "conf", "snippet"
    ));

    // Separator line: dashes matching the header width
    // Use en-dashes or simple ASCII dashes — both are acceptable.
    // The spec shows "─" (U+2500 BOX DRAWINGS LIGHT HORIZONTAL) but ASCII '-' is also fine.
    // For portability, use ASCII dashes:
    let separator = format!(
        "{:->2}  {:->6}  {:->20}  {:->14}  {:->6}  {}\n",
        "", "", "", "", "", "-".repeat(50)
    );
    output.push_str(&separator);

    // One row per entry
    for (i, entry) in entries.iter().enumerate() {
        let row_num = i + 1;

        // Truncate topic to column width for display (EC-05)
        // topic_display is truncated to 20 chars (char boundary safe)
        let topic_display: String = entry.topic.chars().take(20).collect();

        // Truncate category to column width
        let cat_display: String = entry.category.chars().take(14).collect();

        // Confidence formatted as 2 decimal places
        let conf_display = format!("{:.2}", entry.confidence);

        // Snippet: already truncated to SNIPPET_CHARS by IndexBriefingService
        // Additional display truncation: truncate for column if needed
        // Use entry.snippet directly (it's already SNIPPET_CHARS chars max)
        let snippet_display = &entry.snippet;

        output.push_str(&format!(
            "{:>2}  {:>6}  {:<20}  {:<14}  {:>6}  {}\n",
            row_num,
            entry.id,
            topic_display,
            cat_display,
            conf_display,
            snippet_display
        ));
    }

    output
}
```

### Column format specification (FR-12)

The spec FR-12 shows:
```
#    id   topic               cat             conf   snippet
─────────────────────────────────────────────────────────────────────────────────────
 1   2    product-vision      decision        0.60   Unimatrix is a self-learning...
```

The exact column widths are implementation details (ADR-005). The test for R-05 scenario 2
asserts the LITERAL output for a specific entry — so the implementation agent must define
the exact widths and then write the test to match. The pseudocode above uses:
- `#`: right-justified 2 chars
- `id`: right-justified 6 chars
- `topic`: left-justified 20 chars
- `cat`: left-justified 14 chars
- `conf`: right-justified 6 chars (format "{:.2}", 5 chars for "x.xx" + 1 space)
- `snippet`: left-justified, remainder of line

The implementation agent should finalize exact widths and update the test accordingly.
The key constraint: the header must contain the column names `#`, `id`, `topic`, `cat`,
`conf`, `snippet` (R-05 scenario 1 test asserts header line presence).

---

## Error Handling

None. `format_index_table` is a pure formatting function. It cannot fail.
- Empty input → empty string (not None, not panic).
- All entries valid (typed struct ensures no null fields at compile time).

---

## File After Changes

The new `briefing.rs` contains:

```
IMPORTS
  use rmcp::model::{CallToolResult, Content};
  // Remove: use unimatrix_store::EntryRecord;
  // Remove: use super::{ResponseFormat, entry_to_json, entry_to_json_with_similarity};

CONSTANT
  pub const SNIPPET_CHARS: usize = 150;

TYPE
  pub struct IndexEntry { ... }  // WA-5 contract

FUNCTIONS
  pub fn format_index_table(entries: &[IndexEntry]) -> String
  pub fn format_retrospective_report(report: &...) -> CallToolResult  // unchanged

TESTS (new)
```

Total lines: ~100-120 lines. Well within 500-line limit.

---

## Key Test Scenarios

All in `mcp/response/briefing.rs` `#[cfg(test)]` block.

**T-IEF-01** `format_index_table_empty_returns_empty_string` (R-05 scenario 4, AC-18):
- Call: format_index_table(&[])
- Assert: result == "" (empty string, NOT a header-only string)

**T-IEF-02** `format_index_table_single_entry_has_header_and_row` (R-05 scenario 1):
- Input: one IndexEntry
- Assert: output contains the header line with column names "#", "id", "topic", "cat", "conf", "snippet"
- Assert: output contains a separator line after the header
- Assert: output contains exactly one data row

**T-IEF-03** `format_index_table_column_layout_exact_match` (R-05 scenario 2, CRITICAL):
- Input: IndexEntry { id: 2, topic: "product-vision", category: "decision", confidence: 0.60, snippet: "Unimatrix is..." }
- Assert: row contains "0.60" (confidence 2 decimal places)
- Assert: row contains "2" (id)
- Assert: row contains "product-vision" (topic)
- Assert: row contains "decision" (category)
- Note: exact string match depends on implementation. The test should be written to match
  the actual format produced by `format_index_table`. The implementation agent writes this
  test AFTER implementing the function to ensure the assertion matches the actual output.

**T-IEF-04** `format_index_table_row_number_increments` (R-05):
- Input: 3 IndexEntry values
- Assert: first row contains " 1" (row number 1)
- Assert: second row contains " 2"
- Assert: third row contains " 3"

**T-IEF-05** `snippet_chars_constant_is_150` (R-05 scenario 3):
- Assert: SNIPPET_CHARS == 150
- This is a compile-time constant check (just reference the constant in the test)

**T-IEF-06** `format_index_table_topic_truncated_to_column_width` (EC-05):
- Input: IndexEntry with topic = "a".repeat(100) (100-char topic, exceeds column width)
- Assert: output does NOT have the full 100-char topic in the row (it is truncated)
- Assert: output is well-formed (no column overflow that corrupts adjacent columns)

**T-IEF-07** `format_retrospective_report_unchanged` (regression):
- Call format_retrospective_report with a sample report
- Assert: returns CallToolResult::success
- Assert: content is valid JSON

**T-IEF-08** `index_entry_fields_are_public_and_accessible` (compile check):
- Construct: IndexEntry { id: 1, topic: "t".into(), category: "c".into(), confidence: 0.5, snippet: "s".into() }
- Assert: all field accesses compile (fields are `pub`)
