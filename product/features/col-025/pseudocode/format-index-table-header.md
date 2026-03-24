# Component: format-index-table-header

**Crate**: `unimatrix-server`
**Files**: `src/services/index_briefing.rs` (constant), `src/mcp/response/briefing.rs` (function)

---

## Purpose

Define the `CONTEXT_GET_INSTRUCTION` constant and prepend it as a single
header line to every `format_index_table` output. This ensures agents
receiving a briefing table — whether via MCP tool call or UDS injection —
immediately know how to act on it.

---

## New Constant

Add to `src/services/index_briefing.rs` (per ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md,
and ADR-006), after the existing constants in that file. Import it in
`src/mcp/response/briefing.rs` with `use crate::services::index_briefing::CONTEXT_GET_INSTRUCTION;`:

```
/// Agent instruction prepended once before every `format_index_table` output.
///
/// Tells agents that `context_get` with an entry ID retrieves full content.
/// Applied to both MCP context_briefing responses and UDS CompactPayload
/// injection content. Appears once as a header line — never per row.
///
/// col-025, ADR-006. Do not inline this value at call sites.
/// Update this constant to change the instruction globally.
pub const CONTEXT_GET_INSTRUCTION: &str =
    "Use context_get with the entry ID for full content when relevant.";
```

---

## Modified Function: `format_index_table`

Current function returns empty string for empty input, or a table with header
+ separator + data rows for non-empty input.

Updated function: prepend `CONTEXT_GET_INSTRUCTION` as the first line (with
a blank line between the instruction and the table), but ONLY when entries
is non-empty. Empty input still returns empty string.

```
pub fn format_index_table(entries: &[IndexEntry]) -> String {
    if entries.is_empty() {
        return String::new();
        // No change: empty slice returns empty string.
        // CONTEXT_GET_INSTRUCTION is NOT prepended to empty output.
        // This preserves the existing empty-check contract used by callers
        // that test for empty string to detect "no results".
    }

    let mut output = String::new();

    // col-025 ADR-006: Prepend instruction header before the table.
    // Appears once, not per row. A blank line separates it from the table.
    output.push_str(CONTEXT_GET_INSTRUCTION);
    output.push('\n');
    output.push('\n');   // blank line between instruction and table header

    // Existing header line (unchanged)
    output.push_str(&format!(
        "{:>2}  {:>6}  {:<20}  {:<14}  {:>6}  {}\n",
        "#", "id", "topic", "cat", "conf", "snippet"
    ));

    // Existing separator line (unchanged)
    output.push_str(&format!(
        "{:->2}  {:->6}  {:->20}  {:->14}  {:->6}  {}\n",
        "", "", "", "", "", "-".repeat(50)
    ));

    // Existing data rows (unchanged)
    for (i, entry) in entries.iter().enumerate() {
        let row_num = i + 1;
        let topic_display: String = entry.topic.chars().take(20).collect();
        let cat_display: String = entry.category.chars().take(14).collect();
        let conf_display = format!("{:.2}", entry.confidence);
        output.push_str(&format!(
            "{:>2}  {:>6}  {:<20}  {:<14}  {:>6}  {}\n",
            row_num, entry.id, topic_display, cat_display, conf_display, entry.snippet
        ));
    }

    output
}
```

The instruction is followed by `\n\n` (instruction line + blank separator
line). The blank line visually separates the instruction from the table
header. This makes the table parseable by agents even when the instruction
is present.

---

## Test Helper: `strip_briefing_header`

All existing `format_index_table` tests that assert content starting at the
first table column line (header row) will fail because the instruction is
now the first line. Introduce a test helper in the test module:

```
#[cfg(test)]
fn strip_briefing_header(s: &str) -> &str {
    // Skip lines until we find the table header line (starts with "#" or row numbers).
    // The table header contains "  #" or similar.
    // More robust: skip lines until we find a line containing "  #  " (the column header).
    for (i, line) in s.lines().enumerate() {
        // The header row contains "#", "id", "topic", "cat", "conf", "snippet"
        if line.contains("  id ") || line.trim_start().starts_with('#') {
            // Return the substring starting at this line.
            // Find the byte offset of this line.
            let offset = s.lines()
                .take(i)
                .map(|l| l.len() + 1)  // +1 for \n
                .sum::<usize>();
            return &s[offset..];
        }
    }
    s   // fallback: return as-is if header not found
}
```

Simpler alternative (recommended): since the format is deterministic, the
instruction is always the first line, followed by a blank line, followed by
the table. Just skip the first two lines:

```
#[cfg(test)]
fn strip_briefing_header(s: &str) -> &str {
    // Skip CONTEXT_GET_INSTRUCTION line + blank line.
    // Returns the substring starting at the table header row.
    // If the instruction is not present (e.g., empty input), returns s unchanged.
    if s.starts_with(CONTEXT_GET_INSTRUCTION) {
        // Skip: instruction line + '\n' + blank line + '\n' = CONTEXT_GET_INSTRUCTION.len() + 2
        let skip = CONTEXT_GET_INSTRUCTION.len() + 2;
        if s.len() > skip {
            return &s[skip..];
        }
    }
    s
}
```

Use this helper in all existing tests that assert column headers, separators,
or data row content.

---

## Existing Test Updates (R-11)

All tests in `src/mcp/response/briefing.rs` that call `format_index_table`
with non-empty entries must be updated. Each currently asserts:
- `result.contains('#')` — still valid; the table header line still has '#'
- `result.contains("id")` — still valid
- lines[0] is the header line — NOW INCORRECT; lines[0] is CONTEXT_GET_INSTRUCTION
- lines[1] is separator — NOW INCORRECT; lines[1] is blank, lines[2] is header, lines[3] is separator

Update pattern for tests that use `result.lines().collect()`:

Old:
```
let lines: Vec<&str> = result.lines().collect();
// lines[0] = header, lines[1] = separator, lines[2] = row 1
```

New:
```
let table = strip_briefing_header(&result);
let lines: Vec<&str> = table.lines().collect();
// lines[0] = header, lines[1] = separator, lines[2] = row 1
```

Tests to audit in `briefing.rs`:
1. `format_index_table_sorted_confidence` — uses `lines[2]`, `lines[3]`
2. `format_index_table_multiple_entries_numbered_sequentially` — uses line indexing
3. `format_index_table_header_and_separator_present` — checks `lines[0]` and `lines[1]`
4. `format_index_table_columns_present` — content assertions; most still valid
5. `format_index_table_topic_truncated_to_column_width` — uses `result.lines().skip(2)`; update to `skip(4)` or use `strip_briefing_header`

Tests that remain valid without changes:
- `format_index_table_empty_returns_empty_string` — empty input, no change
- `format_index_table_no_section_headers` — checks for absence of "## Decisions" etc.
- `format_index_table_confidence_formatted_consistently` — content check

Tests in `listener.rs` that call `format_index_table` indirectly via
`format_compaction_payload`:
- These tests check `result.contains("--- Unimatrix Compaction Context ---")`,
  not `format_index_table` output directly.
- `format_compaction_payload` calls `format_index_table` and embeds its
  output in a larger payload. The instruction line will appear within the
  `format_index_table` section of the compaction output. Tests that assert
  row positions within the compaction payload output must be audited.

---

## Data Flow

Input: `entries: &[IndexEntry]`
Output: `String`

When `entries` is non-empty:
```
"Use context_get with the entry ID for full content when relevant.\n"
"\n"
" #      id  topic                 cat             conf  snippet\n"
"--  ------  --------------------  --------------  ------  ...\n"
" 1   12345  my-topic              decision          0.85  first 150 chars...\n"
...
```

When `entries` is empty: `""` (unchanged)

---

## Error Handling

No failure modes. `format_index_table` is pure — no I/O, no allocations that
can fail beyond OOM. The `CONTEXT_GET_INSTRUCTION` constant is a static string;
prepending it cannot fail.

---

## Key Test Scenarios

### T-FIT-01: Output starts with CONTEXT_GET_INSTRUCTION (AC-18)
```
act:   format_index_table with one or more entries
assert: result.starts_with(CONTEXT_GET_INSTRUCTION)
assert: result[CONTEXT_GET_INSTRUCTION.len()..].starts_with('\n')
```

### T-FIT-02: CONTEXT_GET_INSTRUCTION appears exactly once (AC-18)
```
act:   format_index_table with 3 entries
assert: result.matches(CONTEXT_GET_INSTRUCTION).count() == 1
        // Not once per row; exactly once as header
```

### T-FIT-03: Empty slice returns empty string — no header (existing, unchanged)
```
act:   format_index_table(&[])
assert: result.is_empty()
assert: result does NOT contain CONTEXT_GET_INSTRUCTION
```

### T-FIT-04: strip_briefing_header test helper works correctly (R-11 tooling)
```
let full = format_index_table(&[make_entry(...)]);
let table = strip_briefing_header(&full);
assert table starts with the column header line (contains "#", "id", "topic")
assert table does NOT start with CONTEXT_GET_INSTRUCTION
```

### T-FIT-05: MCP context_briefing response contains instruction (AC-18 via MCP)
```
// Integration test via handle_briefing in tools.rs.
// The handler calls format_index_table; result must start with instruction.
```

### T-FIT-06: UDS CompactPayload injection contains instruction (AC-18 via UDS)
```
// Integration test via handle_compact_payload in listener.rs.
// The payload includes format_index_table output; must contain instruction.
```

### T-FIT-07: Existing format_index_table tests pass after header addition (R-11)
```
// All tests in briefing.rs must pass after instruction is added.
// Tests using line-index assertions must use strip_briefing_header.
// cargo test must pass with no failures.
```
