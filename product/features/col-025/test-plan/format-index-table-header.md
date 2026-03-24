# Test Plan: format-index-table-header

**Crate**: `unimatrix-server`
**File modified**: `src/mcp/response/briefing.rs` (fn `format_index_table`);
  `src/services/index_briefing.rs` (constant `CONTEXT_GET_INSTRUCTION`)

**Risks covered**: R-11
**ACs covered**: AC-18

---

## Overview

Component 8 adds `CONTEXT_GET_INSTRUCTION` as a named public constant in
`src/services/index_briefing.rs` and prepends it once as a header line in
`format_index_table` before the column header row.

Output structure after change:

```
Use context_get with the entry ID for full content when relevant.

 #      id  topic                 cat             conf  snippet
--  ------  --------------------  --------------  ------  --------------------------------------------------
 1    1234  col-025               decision          0.92  Goal signal improves retrieval precision…
```

The constant appears once per `format_index_table` invocation — never per row.

Because `format_index_table` is called in both the MCP briefing path
(`handle_briefing` → `format_index_table`) and the UDS injection path
(`handle_compact_payload` → `IndexBriefingService::index` → `format_index_table`),
the header appears automatically in both paths with no call-site changes.

---

## Pre-Delivery Audit (R-11)

All current tests asserting `format_index_table` output must be identified and
updated. From the codebase search, the affected test sites are:

| File | Test Function(s) | Issue |
|------|-----------------|-------|
| `src/mcp/response/briefing.rs` | `format_index_table_empty_returns_empty_string` | Must check: does CONTEXT_GET_INSTRUCTION prepend to empty output? (see note below) |
| `src/mcp/response/briefing.rs` | `format_index_table_columns_present` | Asserts header row presence — must now account for instruction line before header |
| `src/mcp/response/briefing.rs` | `format_index_table_multibyte_utf8` | Likely asserts output content — add `strip_briefing_header` |
| `src/mcp/response/briefing.rs` | `format_index_table_sorted_confidence` | Asserts row order — add `strip_briefing_header` |
| `src/mcp/response/briefing.rs` | `format_index_table_no_section_headers` | May assert `!output.starts_with("##")` — update |
| `src/mcp/response/briefing.rs` | `format_index_table_multiple_entries_numbered_sequentially` | Row number assertions — add `strip_briefing_header` |
| `src/mcp/response/briefing.rs` | `format_index_table_confidence_formatted_consistently` | Confidence format assertions — add `strip_briefing_header` |
| `src/mcp/response/briefing.rs` | `format_index_table_topic_truncated_to_column_width` | Topic truncation — add `strip_briefing_header` |
| `src/mcp/response/briefing.rs` | `format_index_table_header_and_separator_present` | Header/separator present — update to expect instruction line first |
| `src/mcp/tools.rs` | `context_briefing_active_only_filter` | Asserts `table_text.contains(...)` — add `strip_briefing_header` |
| `src/mcp/tools.rs` | `context_briefing_default_k_20` | Asserts table line count — add `strip_briefing_header` |
| `src/mcp/tools.rs` | `context_briefing_k_clamped_to_20` | Table assertions — add `strip_briefing_header` |
| `src/mcp/tools.rs` | `context_briefing_format_index_table_renders_correctly` | Table assertions — add `strip_briefing_header` |
| `src/mcp/response/mod.rs` | `test_format_index_table_smoke` | Smoke check — add `strip_briefing_header` |

**Empty slice behavior**: the current `format_index_table` returns `""` for an
empty slice. The `CONTEXT_GET_INSTRUCTION` header should NOT be prepended to an
empty output — otherwise `format_index_table_empty_returns_empty_string` would
break and agents would receive the instruction with no table. Keep the empty-slice
early return before prepending the header.

---

## Required Test Helper: `strip_briefing_header`

Per IMPLEMENTATION-BRIEF.md §Alignment Status and RISK-TEST-STRATEGY.md R-11:

```rust
/// Remove the CONTEXT_GET_INSTRUCTION header (and following blank line) from
/// `format_index_table` output for raw-table assertions.
///
/// Usage in tests:
///   let table = strip_briefing_header(&format_index_table(&entries));
///   assert!(table.starts_with(" #"));
fn strip_briefing_header(s: &str) -> &str {
    // Skip the instruction line and the following blank line.
    // The header is the first non-empty line; the table body starts after
    // the blank separator that follows.
    // Implementation: find first '\n\n' or skip first two lines.
}
```

This helper must be defined once, preferably in `src/mcp/response/briefing.rs`
test module or in a test-only utility module. It must NOT be duplicated per test.

---

## New Tests

### Test: `test_format_index_table_starts_with_instruction_header_exactly_once` (R-11 / AC-18 / Gate 3c scenario 8)

```
#[test] fn test_format_index_table_starts_with_instruction_header_exactly_once()
```

This is the **non-negotiable Gate 3c scenario 8**.

```rust
let entry = IndexEntry { id: 1, topic: "col-025".to_string(),
    category: "decision".to_string(), confidence: 0.9,
    snippet: "Test snippet.".to_string() };
let result = format_index_table(&[entry]);

// Starts with the constant
assert!(
    result.starts_with(CONTEXT_GET_INSTRUCTION),
    "output must start with CONTEXT_GET_INSTRUCTION"
);

// Appears exactly once — not repeated in table rows
let count = result.matches(CONTEXT_GET_INSTRUCTION).count();
assert_eq!(count, 1, "CONTEXT_GET_INSTRUCTION must appear exactly once");
```

### Test: `test_format_index_table_instruction_not_in_table_rows`

```
#[test] fn test_format_index_table_instruction_not_in_table_rows()
```

Build 3 entries. Call `format_index_table`. Strip the first non-empty line
(the instruction). Assert the remaining lines do NOT contain
`CONTEXT_GET_INSTRUCTION` text.

### Test: `test_format_index_table_empty_still_returns_empty_string`

```
#[test] fn test_format_index_table_empty_still_returns_empty_string()
```

Assert `format_index_table(&[])` returns `""` (empty string). The instruction
header must NOT be prepended to empty output.

### Test: `test_context_get_instruction_constant_is_defined`

```
#[test] fn test_context_get_instruction_constant_is_defined()
```

Assert `CONTEXT_GET_INSTRUCTION` is a non-empty string.
Assert it is a `&'static str`.
Assert it does not contain newlines (it is a single-line header).

### Test: `test_format_index_table_mcp_briefing_path_includes_header` (AC-18)

```
#[test] fn test_format_index_table_mcp_briefing_path_includes_header()
```

Verify that the MCP `context_briefing` handler produces output that starts with
`CONTEXT_GET_INSTRUCTION`. This can be tested at the unit level by examining the
output of `handle_briefing` (or the portion that calls `format_index_table`) with
a mocked `IndexBriefingService` returning a known `Vec<IndexEntry>`.

If the MCP handler cannot be called in unit tests without a full server setup,
this assertion is covered by the infra-001 test
`test_briefing_response_starts_with_context_get_instruction`.

### Test: `test_format_index_table_uds_compact_payload_includes_header` (AC-18)

```
#[test] fn test_format_index_table_uds_compact_payload_includes_header()
```

Same pattern for the `handle_compact_payload` UDS path. Since both paths call
`format_index_table` which prepends the header, this is a consequence of the
constant being in `format_index_table` — but should still be verified end-to-end
at least once via infra-001.

---

## Updated Existing Tests Strategy

For every test listed in the pre-delivery audit table above:

1. Replace:
   ```rust
   let result = format_index_table(&entries);
   assert!(result.starts_with(" #"));  // column header is first line
   ```
   With:
   ```rust
   let result = format_index_table(&entries);
   assert!(result.starts_with(CONTEXT_GET_INSTRUCTION));
   let table = strip_briefing_header(&result);
   assert!(table.starts_with(" #"));
   ```

2. For tests asserting `result.lines().count() == N` (N rows + 2 header lines),
   update to `N + 2` lines in the table portion. Or use `strip_briefing_header`
   and count only the table lines.

3. Tests asserting `!result.starts_with("##")` (no section headers) should now
   assert `result.starts_with(CONTEXT_GET_INSTRUCTION)` instead.

---

## infra-001 Integration Tests (AC-18)

| Test | Suite |
|------|-------|
| `test_briefing_response_starts_with_context_get_instruction` | `test_tools.py` |

Sequence: call `context_briefing` via MCP. Assert the `content` field of the
response starts with the `CONTEXT_GET_INSTRUCTION` text.
