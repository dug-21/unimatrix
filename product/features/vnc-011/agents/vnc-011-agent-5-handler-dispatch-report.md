# Agent Report: vnc-011-agent-5-handler-dispatch

## Component
handler-dispatch -- format routing in `context_retrospective` handler

## Files Modified
- `crates/unimatrix-server/src/mcp/tools.rs` -- added dispatch logic (cached path + tail section), import for `format_retrospective_markdown`
- `crates/unimatrix-server/src/mcp/response/mod.rs` -- registered `retrospective` module, added `pub use` re-export
- `crates/unimatrix-server/src/mcp/response/retrospective.rs` -- created stub (replaced by formatter agent with full implementation); dispatch tests live in this file's test module

## Changes Summary

### tools.rs dispatch logic
1. **Import**: Added `use crate::mcp::response::format_retrospective_markdown` alongside existing `format_retrospective_report` import. Also imported `ERROR_INVALID_PARAMS` for error handling.
2. **Cached path** (line ~1162): Replaced unconditional `format_retrospective_report(&report)` with format-aware dispatch: `"markdown"` | `"summary"` -> markdown, `"json"` -> JSON, `_` -> error. No evidence_limit truncation on cached path (hotspots is empty).
3. **Tail section** (line ~1451): Replaced the evidence_limit + format block with format-aware dispatch. Markdown path ignores evidence_limit entirely. JSON path preserves existing `unwrap_or(3)` clone-and-truncate behavior unchanged. Invalid format returns `ErrorData` with `ERROR_INVALID_PARAMS`.

### response/mod.rs
Added `mod retrospective` and `pub use retrospective::format_retrospective_markdown` behind `#[cfg(feature = "mcp-briefing")]`.

## Tests
- 94 retrospective-related tests pass (includes 14 dispatch tests + formatter tests from agent-6)
- Full workspace: 993 server tests pass, 0 failures
- 1 flaky test in unimatrix-vector (`test_compact_search_consistency`) -- pre-existing, unrelated

### Dispatch tests (14 total)
| Test | Validates |
|------|-----------|
| test_dispatch_markdown_default | None format -> markdown |
| test_dispatch_markdown_explicit | "markdown" -> markdown |
| test_dispatch_summary_routes_to_markdown | "summary" -> markdown |
| test_dispatch_json_explicit | "json" -> valid JSON |
| test_dispatch_invalid_format_returns_error | "xml" -> ErrorData |
| test_json_evidence_limit_default_3 | JSON unwrap_or(3) |
| test_json_evidence_limit_explicit_5 | Explicit limit=5 |
| test_json_evidence_limit_explicit_0_no_truncation | limit=0 = unlimited |
| test_markdown_ignores_evidence_limit | Markdown ignores evidence_limit |
| test_json_output_matches_direct_call | JSON path = direct call |
| test_json_path_produces_valid_json | Valid pretty JSON |
| test_cached_report_markdown_default | Cached -> markdown |
| test_cached_report_json_explicit | Cached -> JSON |
| test_cached_report_invalid_format_returns_error | Cached invalid -> error |
| test_format_retrospective_markdown_callable | Module re-export works |

## Issues
None. The params-extension agent (vnc-011-agent-4) had already added `format: Option<String>` to `RetrospectiveParams` before this agent ran.
