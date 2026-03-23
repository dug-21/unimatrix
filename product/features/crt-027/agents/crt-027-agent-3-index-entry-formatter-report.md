# Agent Report: crt-027-agent-3-index-entry-formatter

## Component
`IndexEntry` + `format_index_table` in `crates/unimatrix-server/src/mcp/response/briefing.rs`

## Status
COMPLETE

## Files Modified

- `crates/unimatrix-server/src/mcp/response/briefing.rs` — primary deliverable
- `crates/unimatrix-server/src/mcp/response/mod.rs` — re-export updated to expose new API
- `crates/unimatrix-server/src/mcp/tools.rs` — broken import removed; handler body stubbed for compile isolation

## Changes Delivered

### briefing.rs (full rewrite)
- Deleted: `Briefing` struct, `format_briefing` function (AC-13)
- Added: `pub const SNIPPET_CHARS: usize = 150`
- Added: `#[derive(Debug, Clone)] pub struct IndexEntry` with all 5 pub fields (`id: u64`, `topic`, `category`, `confidence: f64`, `snippet`)
- Added: `pub fn format_index_table(entries: &[IndexEntry]) -> String` — flat indexed table, empty slice returns empty string
- Retained: `format_retrospective_report` unchanged
- Column layout: right-justified `#`(2), `id`(6), left-justified `topic`(20), `cat`(14), right-justified `conf`(6), `snippet` (remainder). ASCII dash separator after header.

### mod.rs
- Re-export updated: `briefing::{IndexEntry, SNIPPET_CHARS, format_index_table, format_retrospective_report}`
- Old briefing tests (`make_briefing`, `test_format_briefing_*`) replaced with a single smoke test `test_format_index_table_smoke` using the new API

### tools.rs (compile isolation only)
- Removed `Briefing` / `format_briefing` / `validate_briefing_params` / `validated_max_tokens` imports
- Stubbed the `#[cfg(feature = "mcp-briefing")]` handler block with a temporary error return
- Marked with `crt-027` comment for the context_briefing handler agent to replace

## Tests

15 unit tests in `briefing.rs` `#[cfg(test)]` block:

| Test | Coverage |
|------|----------|
| `format_index_table_empty_returns_empty_string` | R-05 scenario 4, AC-18 |
| `format_index_table_columns_present` | R-05 scenario 1+2, FR-12 |
| `format_index_table_multibyte_utf8` | AC-17, CJK |
| `format_index_table_sorted_confidence` | caller-owns-sort contract |
| `format_index_table_no_section_headers` | no ## Decisions/Conventions/Injections |
| `index_entry_fields_accessible` | pub field compile check |
| `index_entry_debug_clone_derive` | derive correctness |
| `snippet_chars_constant_is_150` | R-05 scenario 3 |
| `snippet_truncation_utf8_safe_cjk` | AC-17 |
| `snippet_truncation_ascii_exactly_150` | boundary |
| `snippet_truncation_longer_than_150` | budget |
| `format_index_table_multiple_entries_numbered_sequentially` | row numbering |
| `format_index_table_confidence_formatted_consistently` | 1.00 / 0.00 |
| `format_index_table_topic_truncated_to_column_width` | EC-05 |
| `format_index_table_header_and_separator_present` | R-05 scenario 1 |

**Result: 15 passed / 0 failed**

Full workspace: all test suites pass, 0 failures, 0 new ignored.

## Build Verification

- `cargo build --workspace` — clean, zero errors
- `cargo clippy -p unimatrix-server --lib` — zero errors
- `cargo fmt -p unimatrix-server -- --check` — ok
- `cargo test --workspace` — all suites pass

## Deviations from Pseudocode

None. Implementation follows pseudocode exactly including column widths, separator style, and empty-slice early return.

## Notes for Other Agents

The `context_briefing` handler in `mcp/tools.rs` has been stubbed. The tools agent must:
1. Remove the `crt-027` stub comment block
2. Implement the new handler using `IndexBriefingService::index()` and `format_index_table`
3. Add back `validate_briefing_params` / `validated_max_tokens` imports if still needed

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` (response formatting patterns) — found entry #949 (Domain-Specific Markdown Formatter Module Pattern) and #298 (Generic Formatter Pattern). Neither covered the specific swarm compile-isolation challenge.
- Queried: `/uni-knowledge-search` (crt-027 ADRs) — found all 5 ADRs (#3242–#3246) confirming design decisions.
- Stored: entry #3256 "Swarm agent compile isolation: stub caller handler when deleting owned pub types across agent boundaries" via `/uni-store-pattern` — novel pattern not previously captured; describes the `let _ = &params; Ok(CallToolResult::error(...))` stub technique for feature-gated handlers when the owning agent deletes pub types that cross agent boundaries.
