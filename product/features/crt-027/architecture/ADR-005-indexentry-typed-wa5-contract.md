## ADR-005: IndexEntry as Typed WA-5 Contract Surface

### Context

The flat indexed table format for `context_briefing` and CompactPayload must be a stable
contract that WA-5 (PreCompact transcript restoration) can depend on. WA-5 needs to prepend
a transcript block before the Unimatrix briefing content in the CompactPayload response.

SR-06 from the risk assessment identifies this risk: if the flat table format is specified
only by prose (column widths, separator characters, row number field width) and not by a
typed struct, any implementation detail that shifts during crt-027 delivery will force WA-5
to adapt.

Two approaches for the WA-5 contract surface:

**Option A: Inline string formatting.** `format_index_table` formats entries as a string
directly. WA-5 depends on the output string format by convention — column order, separator
width, field widths are documented in comments.

Rejected: String formats are fragile contracts. Minor formatting adjustments (right-padding,
separator length) break WA-5's prepend logic if WA-5 does any parsing beyond "append after
header." Column width changes are undetectable at compile time.

**Option B: Typed `IndexEntry` struct + dedicated formatter function.** Define `IndexEntry`
as a public-within-crate struct in `mcp/response/briefing.rs`. Define `format_index_table`
as the single canonical formatter. WA-5 can call `format_index_table` on the same slice
it receives from `IndexBriefingService`, prepend its transcript block as a string, and then
emit the combined result — without parsing the table.

Selected. WA-5 interacts with `Vec<IndexEntry>` and `format_index_table` by name, not by
parsing the rendered string. Both surfaces are typed and compile-time stable.

### Decision

Define in `mcp/response/briefing.rs` (replacing current `Briefing` struct and
`format_briefing` function):

```rust
/// Single entry in a knowledge index briefing.
/// WA-5 contract type: do not rename fields without updating WA-5 (PreCompact).
#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub id: u64,
    pub topic: String,
    pub category: String,
    pub confidence: f64,
    pub snippet: String,  // first 150 chars of entry.content, UTF-8 safe
}

/// Format a slice of IndexEntry as a flat indexed table.
///
/// Column order: row#, id, topic, category, confidence (2 decimal places), snippet
/// Separator: single line of dashes (─) after header
/// Empty slice: returns empty string (callers should return None/empty for empty)
///
/// WA-5 contract: this function is the canonical renderer. WA-5 prepends transcript
/// content BEFORE calling this function (or before the outer wrapper that calls it) —
/// WA-5 does not parse the rendered string.
pub fn format_index_table(entries: &[IndexEntry]) -> String
```

`IndexEntry::snippet` is constructed as: `entry.content.chars().take(150).collect::<String>()`,
which is UTF-8 safe (operates on char boundaries). The 150-char limit is a named constant
`SNIPPET_CHARS: usize = 150` in `briefing.rs` so WA-5 can reference it if needed.

Both callers — `context_briefing` MCP handler and `format_compaction_payload` — construct
`Vec<IndexEntry>` from `IndexBriefingService::index()` results and pass to
`format_index_table`. The compaction formatter wraps the table with the session context
header and histogram block.

The existing `Briefing` struct, `format_briefing`, and `format_retrospective_report` in
`briefing.rs` are evaluated for removal: `Briefing` and `format_briefing` are removed
(replaced by `IndexEntry` and `format_index_table`). `format_retrospective_report` is
unrelated to briefing and is retained in the same file.

### Consequences

- WA-5 has a named, typed, compile-time-stable contract: `IndexEntry` struct +
  `format_index_table` function + `SNIPPET_CHARS` constant.
- Any crt-027 implementation change to `IndexEntry` field names or `format_index_table`
  signature is immediately visible to WA-5 as a compile error.
- The `Briefing` struct and `format_briefing` function are removed, along with their tests
  in `mcp/response/briefing.rs`. The `format_retrospective_report` function remains.
- The `mcp-briefing` feature flag governs the MCP tool but does NOT gate `IndexEntry` or
  `format_index_table` — both are always compiled because the UDS CompactPayload path
  uses them regardless of the feature flag.
- The rendered table format (column widths, padding) is an implementation detail of
  `format_index_table`. Only the function signature and `IndexEntry` type are the contract.
  WA-5 must not depend on specific column widths.
