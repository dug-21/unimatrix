# Agent Report: vnc-018-agent-1-architect

## Status: COMPLETE

## Artifacts Produced

- `/workspaces/unimatrix/product/features/vnc-018/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/vnc-018/architecture/ADR-001-sql-cte-for-supersession-traversal.md`
- `/workspaces/unimatrix/product/features/vnc-018/architecture/ADR-002-truncated-response-envelope.md`
- `/workspaces/unimatrix/product/features/vnc-018/architecture/ADR-003-graphparams-struct-layout.md`
- `/workspaces/unimatrix/product/features/vnc-018/architecture/ADR-004-edgerecord-type-location.md`
- `/workspaces/unimatrix/product/features/vnc-018/architecture/ADR-005-neighbors-execution-split.md`
- `/workspaces/unimatrix/product/features/vnc-018/architecture/ADR-006-advances-motivates-ppr-bfs.md`
- `/workspaces/unimatrix/product/features/vnc-018/architecture/ADR-007-schema-migration-v27.md`

## ADR Unimatrix Entry IDs

| ADR File | Unimatrix ID |
|----------|-------------|
| ADR-001 SQL CTE for supersession traversal | #4475 |
| ADR-002 Truncated response envelope | #4476 |
| ADR-003 GraphParams struct layout | #4477 |
| ADR-004 EdgeRecord type location | #4478 |
| ADR-005 neighbors execution split | #4479 |
| ADR-006 Advances/Motivates PPR/BFS | #4480 |
| ADR-007 Schema migration v26→v27 | #4481 |

## Key Decisions Made

1. **SR-05 resolved** (ADR-002): `truncated` is a struct `{ forward: bool, backward: bool }`, not a flat bool. AC-03b is directly testable.

2. **SR-03 resolved** (ADR-003): `GraphParams` struct layout locked. Forward-compat fields (`seed_ids`, `from_id`, `to_id`, `max_nodes`) error on misuse via centralized `validate_no_unsupported_params` function — not silently dropped.

3. **SR-07 / SR-08 enforced** (ADR-001): `chain` and `current` modes use SQL CTEs exclusively. `find_terminal_active` (graph.rs:523) is not used. vnc-017 merge is a hard gate before delivery begins.

4. **ADR-004**: `EdgeRecord` lives in `mcp/graph_read.rs`, re-exported from `mcp/mod.rs`. `metadata: Option<serde_json::Value>` defined now (always `None` in vnc-018) for W1B-2b forward compatibility.

5. **ADR-005**: depth=1 SQL (live DB), depth>1 in-memory BFS. Exact tool description text specified — mandatory in `#[tool(description = "...")]`. SR-02 staleness test required in infra-001 suite.

6. **ADR-006**: `Advances` and `Motivates` added to PPR and BFS positive type sets. Exact insertion points identified: `graph_ppr.rs` ~lines 131 and 203; `graph_expand.rs` ~line 144.

7. **ADR-007**: Migration v26→v27, 4 indexes only. Full schema cascade checklist (Pattern #4373) documented in ADR. Enforcement gate: `grep -r 'schema_version.*== 26' crates/` must return zero matches.

## Integration Surface Highlights

- New module: `mcp/graph_read.rs` — entry point `handle_graph(store, typed_graph_state, params, ctx)`
- New SQL functions in `unimatrix-store`: `query_supersession_chain`, `query_direct_neighbors`
- `tools.rs` addition: `context_graph` `#[tool]` handler (dispatch only, full module path qualifier required per Pattern #4436)
- `EdgeRecord` re-exported from `mcp/mod.rs` for #597/#598 consumers
- Schema: v26 → v27 (index-only, CURRENT_SCHEMA_VERSION=27)

## Open Questions for Delivery Agent

1. **`node_index` visibility** (informational, not a blocker): `TypedRelationGraph::node_index` is `pub(crate)` within `unimatrix-engine`. The BFS in `mcp/graph_read.rs` (in `unimatrix-server`) needs a way to convert an entry ID to a `NodeIndex`. Options: add a `pub fn node_index_for(id: u64) -> Option<NodeIndex>` accessor to `TypedRelationGraph`, or implement the depth>1 BFS as a function inside `unimatrix-engine` and call it from `graph_read.rs`. Either is valid; determine at implementation time.

2. **Store function location**: `query_supersession_chain` and `query_direct_neighbors` are specified as additions to `unimatrix-store/src/db.rs`. If `db.rs` is already approaching its size limit, the delivery agent may create `unimatrix-store/src/graph_queries.rs` as a submodule instead — the interface contract is unchanged.
