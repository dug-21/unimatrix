# Test Plan: index-entry-formatter (mcp/response/briefing.rs)

## Component

`crates/unimatrix-server/src/mcp/response/briefing.rs`

Changes:
- Delete `Briefing` struct and `format_briefing()` function
- Add `IndexEntry` struct (WA-5 contract type)
- Add `format_index_table(entries: &[IndexEntry]) -> String`
- Add `SNIPPET_CHARS: usize = 150` constant
- Retain `format_retrospective_report` (unchanged)

## Risks Covered

R-05 (WA-5 format contract drift), R-03 (format function behavior)

## ACs Covered

AC-08 (format layer), AC-13 (Briefing struct deleted), AC-17 (snippet UTF-8)

---

## Unit Test Expectations

Tests in `crates/unimatrix-server/src/mcp/response/briefing.rs` `#[cfg(test)]` block.

### Non-Negotiable Contract Tests (R-05)

#### `format_index_table_header_and_separator` (R-05 scenario 1)
**Arrange**:
```rust
let entries = vec![IndexEntry {
    id: 2,
    topic: "product-vision".to_string(),
    category: "decision".to_string(),
    confidence: 0.60,
    snippet: "Unimatrix is a self-learning knowledge engine...".to_string(),
}];
```
**Act**: `let result = format_index_table(&entries)`
**Assert**:
- `result` contains a header line with column names (e.g., `"#"`, `"id"`, `"topic"`, `"cat"`, `"conf"`, `"snippet"`)
- `result` contains a separator line (dashes `─` or `-` after the header)
- `result` contains a data row

#### `format_index_table_exact_column_layout` (R-05 scenario 2 — format contract test)
**Arrange**: Same single-entry as above
**Act**: `let result = format_index_table(&entries)`
**Assert**:
- `result` contains `"2"` (the entry ID)
- `result` contains `"product-vision"` (the topic)
- `result` contains `"decision"` (the category)
- `result` contains `"0.60"` (confidence formatted to 2 decimal places, NOT `"0.6"` or `"0.600"`)
- `result` contains the snippet text
- Row 1 is right-justified (2-char minimum width for row number: `" 1"` not `"1"`)

This test asserts the **exact column layout** specified in FR-12. Any column shift breaks
this test and flags a WA-5 integration risk. The literal output structure is the contract.

#### `format_index_table_empty_slice_returns_empty_string` (R-05 scenario 4)
**Arrange**: `let entries: Vec<IndexEntry> = vec![]`
**Act**: `let result = format_index_table(&entries)`
**Assert**: `result.is_empty()` — empty slice produces empty string, not a header-only string

#### `snippet_chars_constant_is_150` (R-05 scenario 3)
**Arrange**: Reference `SNIPPET_CHARS` directly
**Assert**: `SNIPPET_CHARS == 150`
This is a compile-time constant assertion — even a simple `assert_eq!(SNIPPET_CHARS, 150)` test.

### IndexEntry Struct Tests

#### `index_entry_fields_accessible` (compile-time, AC-25 adjacent)
**Arrange**: Construct `IndexEntry { id: 1, topic: "t".to_string(), category: "c".to_string(), confidence: 0.5, snippet: "s".to_string() }`
**Assert**: All five fields are accessible (`entry.id`, `entry.topic`, `entry.category`,
`entry.confidence`, `entry.snippet`). Confirms the struct is `pub` and fields are `pub`.

#### `index_entry_debug_clone_derive` (required for WA-5)
**Assert**: `IndexEntry` can be cloned and debug-printed (i.e., `#[derive(Debug, Clone)]`
compiles correctly).

### Snippet Truncation Tests (AC-17)

#### `snippet_truncation_utf8_safe_cjk` (AC-17)
**Arrange**: `content = "\u{4e16}\u{754c}".repeat(200)` (CJK characters, 3 bytes each)
**Act**: Build `IndexEntry.snippet` using `content.chars().take(SNIPPET_CHARS).collect::<String>()`
**Assert**:
- `snippet.chars().count() <= 150`
- `snippet.len() <= 450` (150 chars * 3 bytes)
- `snippet.is_char_boundary(snippet.len())` — no split

#### `snippet_truncation_ascii_exactly_150` (boundary)
**Arrange**: `content = "a".repeat(150)` (exactly 150 ASCII chars)
**Act**: `snippet = content.chars().take(SNIPPET_CHARS).collect::<String>()`
**Assert**: `snippet == content` (no truncation, exactly at boundary)

#### `snippet_truncation_longer_than_150` (budget)
**Arrange**: `content = "b".repeat(300)` (300 chars, all ASCII)
**Act**: `snippet = content.chars().take(SNIPPET_CHARS).collect::<String>()`
**Assert**: `snippet.chars().count() == 150`

### Multiple Entry Tests

#### `format_index_table_multiple_entries_numbered_sequentially`
**Arrange**: 3 `IndexEntry` values
**Act**: `format_index_table(&entries)`
**Assert**: Output contains rows numbered `" 1"`, `" 2"`, `" 3"` in order

#### `format_index_table_confidence_formatted_consistently`
**Arrange**: `IndexEntry { confidence: 1.0 }` and `IndexEntry { confidence: 0.0 }`
**Act**: `format_index_table(&entries)`
**Assert**: Output contains `"1.00"` and `"0.00"` (always 2 decimal places)

---

## Deleted Content Verification (AC-13)

After implementation, `mcp/response/briefing.rs` must NOT contain:
- `Briefing` struct definition
- `format_briefing` function
- `InjectionEntry` type
- `InjectionSections` type

Must STILL contain:
- `format_retrospective_report` function (retained, unrelated to briefing)
- `IndexEntry` struct
- `format_index_table` function
- `SNIPPET_CHARS` constant

**Gate check**: `grep "pub struct Briefing\|fn format_briefing" crates/unimatrix-server/src/mcp/response/briefing.rs`
returns no results.

---

## WA-5 Contract Stability Checklist

The following are the WA-5 contract surfaces. Changes to any of these require updating
WA-5 (PreCompact transcript feature) as well:

| Surface | Type | Stable? |
|---------|------|---------|
| `IndexEntry.id` | `u64` | MUST NOT change |
| `IndexEntry.topic` | `String` | MUST NOT change |
| `IndexEntry.category` | `String` | MUST NOT change |
| `IndexEntry.confidence` | `f64` | MUST NOT change |
| `IndexEntry.snippet` | `String` | MUST NOT change |
| `format_index_table` signature | `(&[IndexEntry]) -> String` | MUST NOT change |
| `SNIPPET_CHARS` | `usize = 150` | SHOULD NOT change without WA-5 review |

Column widths and padding in the rendered table are implementation details and are NOT
part of the WA-5 contract. WA-5 prepends content before calling `format_index_table`,
not after parsing its output.

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|----------|
| Empty `IndexEntry` slice | `format_index_table_empty_slice_returns_empty_string` | Empty string |
| Very long `topic` (> column width) | Test: long topic truncated in table | No column corruption |
| CJK snippet at 150-char boundary | `snippet_truncation_utf8_safe_cjk` | Valid UTF-8 |
| `confidence = 0.0` | `format_index_table_confidence_formatted_consistently` | `"0.00"` |
| `confidence = 1.0` | same | `"1.00"` |
