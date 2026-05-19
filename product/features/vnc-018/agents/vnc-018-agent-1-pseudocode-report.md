# Agent Report: vnc-018-agent-1-pseudocode

## Task

Produce pseudocode files for the context_graph MCP tool (vnc-018): chain/current/neighbors
mode dispatch in graph_read.rs, tools.rs handler wiring, SQL CTE query functions in db.rs,
PPR/BFS Advances+Motivates additions, v26→v27 schema migration, and node_index_for accessor.

## Files Written

- `product/features/vnc-018/pseudocode/OVERVIEW.md`
- `product/features/vnc-018/pseudocode/graph_read.md`
- `product/features/vnc-018/pseudocode/tools_dispatch.md`
- `product/features/vnc-018/pseudocode/store_queries.md`
- `product/features/vnc-018/pseudocode/ppr_bfs.md`
- `product/features/vnc-018/pseudocode/migration.md`

## Components Covered

1. `mcp/graph_read.rs` (new) — handle_graph, validate_no_unsupported_params, handle_chain,
   handle_current, handle_neighbors, handle_neighbors_sql, handle_neighbors_bfs,
   follow_to_current; all response types; EdgeRecord; Truncated; ChainResult; GraphParams
2. `mcp/tools.rs` + `mcp/mod.rs` — context_graph #[tool] handler + re-export
3. `unimatrix-store/src/db.rs` — query_supersession_chain, query_current_terminal,
   query_direct_neighbors; 4 index DDL; schema_version bump
4. `unimatrix-engine/src/graph_ppr.rs` + `graph_expand.rs` — Advances + Motivates additions
5. `unimatrix-store/src/migration.rs` + 6 other cascade touch points — v26→v27 migration
6. `unimatrix-engine/src/graph.rs` — node_index_for accessor (covered in graph_read.md)

## Open Questions / Flags for Delivery Agent

### OQ-1: query_supersession_chain vs. separate query_current_terminal

The architecture specifies `query_supersession_chain(id, direction, depth_cap)` as the
single store-layer function. However, `current` mode requires a fundamentally different
CTE: it follows `superseded_by` (not `supersedes`), applies `AND e.status = 'Active'` at
the terminal step, and returns only 1 row. These semantics cannot be expressed cleanly via
a `ChainDirection` parameter on `query_supersession_chain`.

**Resolution in pseudocode**: Added a separate `query_current_terminal(pool, id)` function
in `store_queries.md`. This function has the exact CTE from ARCHITECTURE.md. The delivery
agent should use this two-function design unless the architecture agent specifies otherwise.
The `query_supersession_chain` function remains for `chain` mode (full chain walk).

If the delivery agent wants to unify them, the unification requires `query_supersession_chain`
to accept a `mode: ChainMode` parameter (Full | TerminalOnly) which controls the CTE shape.
That is a minor design choice and does not affect correctness.

### OQ-2: Cap detection for query_supersession_chain

The CTE uses `WHERE c.depth < 50`, so entries at depth 50 are NOT returned. Detecting
whether the cap fired requires a follow-up query per row at depth 49. The pseudocode
describes the recommended approach (check if any row at max depth has successors). The
delivery agent must implement this efficiently — one additional `COUNT` query per direction
is acceptable.

### OQ-3: direction validation for chain mode

ARCHITECTURE.md does not explicitly define the error message for invalid direction values
on chain mode (e.g., "incoming" passed to chain). The pseudocode routes this as a
validation error in `handle_graph` before calling `handle_chain`. The delivery agent should
choose an appropriate error message (`"invalid direction 'incoming' for chain mode — valid
values: forward, backward, both"`).

### OQ-4: handle_graph's unreachable! arm

After `validate_no_unsupported_params` returns `Ok(())`, the `match params.mode.as_str()`
in `handle_graph` cannot receive an unrecognized mode (it was already rejected). The `_`
arm is semantically unreachable but syntactically required. Use `unreachable!()` or
`panic!()` in the final arm. This is safe — `validate_no_unsupported_params` guarantees
the mode is one of the three supported values on the `Ok(())` path.

### OQ-5: graph_read.rs file size

The pseudocode covers ~8 functions. The 500-line limit is tight. Estimated implementation
sizes:
- Types + struct definitions: ~60 lines
- validate_no_unsupported_params: ~50 lines
- handle_graph: ~50 lines
- handle_chain: ~30 lines
- handle_current: ~25 lines
- handle_neighbors: ~60 lines
- handle_neighbors_sql: ~30 lines
- handle_neighbors_bfs: ~80 lines
- follow_to_current: ~25 lines
- Tests: not counted (separate test module or file)

Total: ~410 lines before tests. Within the 500-line limit but close. If the implementation
runs over, split into `graph_read_supersession.rs` (chain/current/follow_to_current) and
`graph_read_neighbors.rs` (neighbors/sql/bfs) per the ARCHITECTURE.md guidance.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned ADR entries #4475, #4478, #4481,
  #4482, #4477 (all vnc-018 ADRs, directly applicable); pattern #4468 (SQL CTE for
  supersession, confirms ADR-001 mandatory path); ADR-005 vnc-015 #4422 (edge_write.rs
  extraction pattern, confirms tools.rs dispatch-only rule).
- Queried: `context_search` for MCP tool handler patterns — found pattern #317
  (ToolContext pre-validated context), #1265 (dual-path validation), #4301 (tool
  description audit). All confirmed existing conventions followed in pseudocode.
- Queried: `context_search` for vnc-018 decisions — found #4477 (ADR-003), #4479 (ADR-005).
- Deviations from established patterns:
  - Added `query_current_terminal` as a separate function (not in original architecture
    spec, which only lists `query_supersession_chain`). This is a clarification, not a
    contradiction — the `current` mode CTE cannot reuse `query_supersession_chain` cleanly.
  - `handle_neighbors_bfs` holds `Arc<RwLock<TypedGraphState>>::read()` for the full BFS
    duration. This matches IR-02 (known tick-blocking risk, accepted tradeoff).
