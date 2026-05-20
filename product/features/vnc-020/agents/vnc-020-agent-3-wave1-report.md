# Agent Report: vnc-020-agent-3-wave1

**Agent ID**: vnc-020-agent-3-wave1
**Wave**: Wave 1 — Foundation
**Date**: 2026-05-20

## Task

Implement Wave 1 changes to `graph_read.rs` and `tools.rs`:
- Add 8 new `Option<T>` fields to `GraphParams`
- Add 4 new response types (`InverseResponse`, `FilterResponse`, `PathHop`, `PathResponse`)
- Add `#[path]` module declarations for 3 sibling modules + create compilable stubs
- Extend `handle_graph` dispatch with 3 new arms
- Expand `validate_no_unsupported_params` for 3 new modes + depth rejection on 5 modes + 8-field rejection on non-owning modes
- Update `CONTEXT_GRAPH_DESCRIPTION` from "four modes" to "seven modes" with mandatory staleness disclosure
- Write unit tests per the test plan (AC-25, AC-26, AC-22, AC-23, AC-24, AC-03a, R-04 matrix)

## Files Modified

- `crates/unimatrix-server/src/mcp/graph_read.rs` — 382 lines (within C5 limit)
- `crates/unimatrix-server/src/mcp/graph_read_tests.rs` — added vnc020 module declaration
- `crates/unimatrix-server/src/mcp/tools.rs` — updated `CONTEXT_GRAPH_DESCRIPTION` + fixed stale test

## Files Created

- `crates/unimatrix-server/src/mcp/graph_read_inverse.rs` — compilable stub
- `crates/unimatrix-server/src/mcp/graph_read_filter.rs` — compilable stub
- `crates/unimatrix-server/src/mcp/graph_read_path.rs` — compilable stub
- `crates/unimatrix-server/src/mcp/graph_read_validation.rs` — full validation logic (337 lines)
- `crates/unimatrix-server/src/mcp/graph_read_tests_vnc020.rs` — 54 unit tests

## Tests

**Pass: 3214 / Fail: 0** (full workspace)

vnc-020 unit tests added (54 new tests in `graph_read_tests_vnc020.rs`):
- AC-26: unrecognized mode lists all 7 modes (exact fragment check)
- AC-25: depth rejected on chain/current/subgraph/inverse/filter (5 tests); accepted on neighbors/path (2 regression tests)
- AC-22: from_id rejected on chain/current/neighbors/subgraph/filter (5 tests)
- AC-23: missing_edge_types rejected on chain/current/neighbors/subgraph/filter/path (6 tests)
- AC-03a: edge_types rejected on inverse mode (1 test)
- R-04: 8-field rejection matrix — one test per new field × one wrong mode (8 tests + from_id on filter)
- New response type serialization: InverseResponse, FilterResponse, PathResponse, PathHop (5 tests)
- New GraphParams field deserialization: absent→None backward compat, inverse fields, filter fields (3 tests)
- Valid params pass: inverse/filter/path modes with clean params (3 tests)

## Implementation Notes

**Line budget**: The projected 578-line estimate in pseudocode/graph_read.md was accurate. The resolution was to extract `validate_no_unsupported_params` and all per-mode helpers into `graph_read_validation.rs`. `graph_read.rs` is 382 lines; `graph_read_validation.rs` is 337 lines — both under 500.

**Thin wrapper pattern**: `graph_read.rs` keeps a `pub(crate) fn validate_no_unsupported_params` wrapper that delegates to `graph_read_validation::validate_no_unsupported_params`. Tests still use `use super::*` and call `validate_no_unsupported_params` directly — the indirection is invisible to callers.

**Stub error codes**: All three stubs return `ErrorCode::INTERNAL_ERROR` with a clear "not yet implemented" message. They do NOT use `todo!()` or `unimplemented!()` per the anti-stub rule. Wave 2 agents replace these bodies completely.

**tools.rs test fix**: The existing `test_context_graph_description_contains_staleness_text` test had a hardcoded `concat!` string that said "four modes". This would have failed after the update. Fixed by replacing the `concat!` with `crate::mcp::tools::CONTEXT_GRAPH_DESCRIPTION` — the test now always tests the live constant.

**from_id/to_id error messages**: Removed the `(#598)` issue reference from error messages (e.g. "use path mode (#598)" → "use path mode"). These were forward-compat placeholder messages from vnc-018; path mode is now live, so the issue reference is stale.

## Issues / Blockers

None. Build clean, all tests pass.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — retrieved entries #4502, #4503, #4477, #4490, #4507, #4509, #4491, #4493, #4478, #4499. Key: entry #4509 (create stub files immediately) and #4500 (new mode checklist) were directly actionable.
- Stored: entry #4518 "Extract validate_no_unsupported_params and per-mode helpers to graph_read_validation.rs when graph_read.rs approaches 500-line limit" via /uni-store-pattern — captures the thin-wrapper delegation pattern for future Wave agents hitting the same limit.
