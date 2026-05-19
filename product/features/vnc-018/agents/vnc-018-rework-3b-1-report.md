# Agent Report: vnc-018-rework-3b-1

Gate 3b rework — three targeted fixes for code quality and test alignment failures.

## Task

Fix the three gate-3b failures:
- C: `graph_read_neighbors.rs` = 506 lines (6 over limit)
- D: `graph_queries.rs` = 1045 lines (double the limit, test module inline)
- A: `test_protocol.py` P-03 asserts 13 tools, must be 14 with `context_graph`

## Files Modified

- `crates/unimatrix-server/src/mcp/graph_read_neighbors.rs` — replaced 153-line test module with `#[path = "graph_read_neighbors_tests.rs"] mod tests;`
- `crates/unimatrix-server/src/mcp/graph_read_neighbors_tests.rs` — new: extracted test module (5 tests)
- `crates/unimatrix-store/src/graph_queries.rs` — extracted test module + neighbor SQL helpers; now 450 lines
- `crates/unimatrix-store/src/graph_queries_tests.rs` — new: extracted test module (11 tests)
- `crates/unimatrix-store/src/graph_queries_neighbors.rs` — new: `run_outgoing_query`, `run_incoming_query`, `map_edge_row` as `pub(super)` functions
- `product/test/infra-001/suites/test_protocol.py` — renamed `test_list_tools_returns_thirteen` → `test_list_tools_returns_fourteen`, added `"context_graph"` to expected list

## Line Counts After Fix

- `graph_read_neighbors.rs` = 356 lines (was 506)
- `graph_queries.rs` = 450 lines (was 1045)

## Extraction Pattern Used

`#[cfg(test)] #[path = "{file}_tests.rs"] mod tests;` — same pattern as `query_log.rs` / `query_log_tests.rs` in unimatrix-store. Neighbor SQL helpers extracted with `#[path = "graph_queries_neighbors.rs"] mod neighbors;` using `pub(super)` visibility, `use neighbors::{run_incoming_query, run_outgoing_query};` at call sites.

## Test Results

- `cargo test -p unimatrix-server --lib`: 3044 passed, 0 failed
- `cargo test -p unimatrix-store --lib`: 333 passed, 0 failed
- `cargo build --workspace`: clean (zero errors)

## Issues

None. All three fixes applied cleanly.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- not called (task was mechanical file extraction with no design decisions; pattern was already in the codebase as query_log_tests.rs)
- Stored: nothing novel to store — the `#[path]` test extraction pattern and `pub(super)` submodule helper extraction are both already in use in this codebase; no new traps discovered.
