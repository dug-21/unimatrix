# Rust Dev Report: 612-agent-1-fix

## Files Modified
- `crates/unimatrix-server/src/mcp/graph_read_path.rs` — path_via_db() + use_fallback extraction
- `crates/unimatrix-server/src/mcp/graph_read_path_tests.rs` — delegated cold-start test to new db_fallback module
- `crates/unimatrix-server/src/mcp/graph_read_path_db_tests.rs` — cold-start regression test (new file, split from graph_read_path_tests.rs to stay under 500-line limit)
- `product/test/infra-001/suites/test_tools.py` — removed xfail from test_context_graph_path_found; updated stale comment at line 4369

## New Tests
- `test_handle_path_db_fallback_cold_start_finds_path` (in `graph_read_path_db_tests.rs`)

## Implementation Notes
- `path_via_db()` uses `query_direct_neighbors(store.read_pool_server(), ..., NeighborDirection::Outgoing)`
- Fan-out capped at `MAX_DB_NEIGHBORS_PER_NODE = 1000` via `.truncate()` post-fetch
- `use_fallback` extracted in same lock guard as `typed_graph` clone

## Knowledge Stewardship
- Queried: query_direct_neighbors usage, graph_read_path patterns, TypedGraphState use_fallback conventions (entries #4526, #4517, #4527)
- Stored: Entry #4527 — "Extract use_fallback in same lock guard as typed_graph and branch to DB BFS before using the snapshot"
