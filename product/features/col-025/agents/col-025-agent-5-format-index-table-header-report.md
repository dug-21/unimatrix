# Agent Report: col-025-agent-5-format-index-table-header

**Component**: format-index-table-header
**Feature**: col-025
**Wave**: 1

---

## Summary

Implemented the `CONTEXT_GET_INSTRUCTION` constant and header injection per ADR-006.

---

## Files Modified

1. `/workspaces/unimatrix/crates/unimatrix-server/src/services/index_briefing.rs`
   - Added `pub const CONTEXT_GET_INSTRUCTION: &str = "Use context_get with the entry ID for full content when relevant.";` after `use` declarations, under a `// Constants` section header.

2. `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/response/briefing.rs`
   - Added `use crate::services::index_briefing::CONTEXT_GET_INSTRUCTION;` import.
   - Modified `format_index_table` to prepend `CONTEXT_GET_INSTRUCTION + "\n\n"` before the table header for all non-empty input. Empty slice still returns `""`.
   - Added `strip_briefing_header` test helper (in `#[cfg(test)] mod tests`).
   - Added 5 new tests: `test_format_index_table_starts_with_instruction_header_exactly_once`, `test_format_index_table_instruction_not_in_table_rows`, `test_format_index_table_empty_still_returns_empty_string`, `test_context_get_instruction_constant_is_defined`, `test_strip_briefing_header_removes_instruction_prefix`.
   - Updated 6 existing tests: `format_index_table_columns_present`, `format_index_table_sorted_confidence`, `format_index_table_multiple_entries_numbered_sequentially`, `format_index_table_topic_truncated_to_column_width`, `format_index_table_header_and_separator_present` (all updated to use `strip_briefing_header` or assert `starts_with(CONTEXT_GET_INSTRUCTION)`).

3. `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/tools.rs`
   - Updated `context_briefing_default_k_20`: changed `.skip(2)` to `.skip(4)`.
   - Updated `context_briefing_k_override`: changed `.skip(2)` to `.skip(4)`.
   - Updated `context_briefing_flat_table_format`: changed `lines.len() >= 4` to `lines.len() >= 6`.

---

## Tests

**All briefing module tests: 20 passed, 0 failed.**
**All index_briefing module tests: 12 passed, 0 failed.**

Full suite (`cargo test -p unimatrix-server`): **1932 passed, 1 failed** (pre-existing failure).

**Pre-existing failure**: `server::tests::test_migration_v7_to_v8_backfill` in `server.rs` asserts `schema_version == 15` but gets `16`. This was caused by the schema-migration Wave 1 agent bumping `CURRENT_SCHEMA_VERSION` to 16. Not caused by or related to my changes. `server.rs` is not in my component scope.

---

## Existing format_index_table Tests Updated

| Test | File | Change |
|------|------|--------|
| `format_index_table_columns_present` | briefing.rs | Added `starts_with(CONTEXT_GET_INSTRUCTION)` assertion |
| `format_index_table_sorted_confidence` | briefing.rs | `strip_briefing_header` before line indexing |
| `format_index_table_multiple_entries_numbered_sequentially` | briefing.rs | `strip_briefing_header` before line indexing |
| `format_index_table_topic_truncated_to_column_width` | briefing.rs | `strip_briefing_header` before `skip(2)` loop |
| `format_index_table_header_and_separator_present` | briefing.rs | `starts_with(CONTEXT_GET_INSTRUCTION)` + `strip_briefing_header` for line checks |
| `context_briefing_default_k_20` | tools.rs | `.skip(2)` → `.skip(4)` |
| `context_briefing_k_override` | tools.rs | `.skip(2)` → `.skip(4)` |
| `context_briefing_flat_table_format` | tools.rs | `lines.len() >= 4` → `lines.len() >= 6` |

Tests not updated (still correct without changes):
- `format_index_table_empty_returns_empty_string` — empty input, no header prepended
- `format_index_table_no_section_headers` — contains checks, not line indexing
- `format_index_table_multibyte_utf8` — contains check only
- `format_index_table_confidence_formatted_consistently` — contains check only
- `test_format_index_table_smoke` (mod.rs) — contains checks only
- All `format_compaction_payload` tests — use `.contains()` and `.find()` position comparisons, unaffected by new header line order within a larger string

---

## Issues / Blockers

None for my component. The pre-existing `test_migration_v7_to_v8_backfill` failure in `server.rs` needs to be fixed by whoever owns the server migration cascade audit (AC-16).

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `format_index_table index briefing header constant pattern` -- found entry #3406 (strip_briefing_header test helper pattern) and #3231 (BriefingService caller map). Both relevant and applied.
- Stored: entry #3407 "Cross-module constant import in briefing.rs: CONTEXT_GET_INSTRUCTION from index_briefing.rs" via `/uni-store-pattern` — the non-obvious gotcha is that `use super::*` inside `#[cfg(test)] mod tests` does NOT re-export `use` imports from the parent module; the constant must be imported explicitly inside the test module.
