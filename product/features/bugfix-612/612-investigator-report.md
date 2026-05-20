# Investigator Report: 612-investigator

## Root Cause
`context_edge add` writes to `GRAPH_EDGES` in SQLite but does not update the in-memory `TypedRelationGraph`. `handle_path` reads exclusively from the in-memory snapshot (via `typed_graph_state.read()`). On cold-start or cycle-detected state, the snapshot is empty, returning `found: false` immediately.

## Affected Files
- `crates/unimatrix-server/src/mcp/graph_read_path.rs` — `handle_path`
- `crates/unimatrix-server/src/mcp/tools.rs:3013` — `context_edge` (reference only)
- `crates/unimatrix-server/src/services/typed_graph.rs` — `TypedGraphState::rebuild` (reference only)
- `product/test/infra-001/suites/test_tools.py:4791` — remove xfail

## Proposed Fix
Option (c): DB-backed BFS fallback in `handle_path` using `query_direct_neighbors` when `use_fallback == true`.

## Knowledge Stewardship
- Queried: TypedRelationGraph ADRs, context_graph conventions, tick mechanism patterns (entries #4517, #4479, #4500, #4493)
- Stored: Entry #4526 — "context_edge add does not trigger TypedRelationGraph rebuild; fix is DB-backed fallback on use_fallback=true"
