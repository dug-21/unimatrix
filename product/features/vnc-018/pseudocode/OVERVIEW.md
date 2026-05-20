# vnc-018 Pseudocode Overview: context_graph (14th MCP Tool)

## Components Involved

| Component | File | Role |
|-----------|------|------|
| `mcp/graph_read.rs` | NEW | All mode logic: GraphParams, EdgeRecord, Truncated, ChainResult, handle_graph, validate_no_unsupported_params, handle_chain, handle_current, handle_neighbors, follow_to_current, node_index_for coverage |
| `mcp/tools.rs` | MODIFY | Add context_graph #[tool] handler — dispatch only, no mode logic |
| `unimatrix-store/src/db.rs` | MODIFY | Add query_supersession_chain, query_direct_neighbors, 4 index DDL, schema_version bump |
| `unimatrix-engine/src/graph_ppr.rs` | MODIFY | Add Advances + Motivates to positive type sets (2 insertion points) |
| `unimatrix-engine/src/graph_expand.rs` | MODIFY | Add Advances + Motivates to BFS positive type set (1 insertion point) |
| `unimatrix-engine/src/graph.rs` | MODIFY | Add node_index_for accessor to TypedRelationGraph |
| `unimatrix-store/src/migration.rs` | MODIFY | Add v26→v27 block + CURRENT_SCHEMA_VERSION = 27 |

---

## Data Flow Between Components

```
MCP call → tools.rs (context_graph #[tool] handler)
  │  require_cap(Read)      ← via self.services registry
  │  build_context_with_external_identity
  │  pass: &self.store, self.services.typed_graph_handle(), params
  ▼
mcp/graph_read.rs (handle_graph)
  │  validate_no_unsupported_params(params) → early error on misuse
  │
  ├─ mode="chain" ─────────────────────────────────────────────────►
  │    handle_chain(store, params)
  │      query_supersession_chain(store.read_pool(), id, direction, 50)  [db.rs]
  │      returns ChainResult { entries: Vec<EntryRecord>, truncated: Truncated }
  │
  ├─ mode="current" ───────────────────────────────────────────────►
  │    handle_current(store, params)
  │      query_supersession_chain(store.read_pool(), id, Forward, 50)  [db.rs]
  │      applies AND status = 'Active' filter at CTE level
  │      returns Ok(CurrentResponse { entry }) or Err("no active terminal found")
  │
  └─ mode="neighbors" ─────────────────────────────────────────────►
       handle_neighbors(store, typed_graph_state, params)
         depth=1: query_direct_neighbors(store.read_pool(), id, types, dir)  [db.rs]
         depth>1: BFS over TypedRelationGraph
                    typed_graph_state.read()
                    node_index_for(id) → NodeIndex  [graph.rs accessor]
                    edges_of_type per type per hop
                    visited: HashSet<u64> (node_id only)
                    resolve_supersessions=true: follow_to_current(store, id) per hop
         returns NeighborsResponse { edges: Vec<EdgeRecord> }
```

---

## Shared Types — Definitions

All types defined in `mcp/graph_read.rs` (ADR-004). Re-exported from `mcp/mod.rs`.

### Wire Types (graph_read.rs)

```
struct GraphParams {
    mode: String,                        // required: "chain"|"current"|"neighbors"
    agent_id: Option<String>,
    format: Option<String>,
    id: Option<u64>,                     // required for all three modes
    direction: Option<String>,           // chain: "forward"|"backward"|"both"
                                         // neighbors: "incoming"|"outgoing"|"both"
    edge_types: Option<Vec<String>>,     // neighbors only
    depth: Option<u8>,                   // neighbors only; 1..=10, default 1
    resolve_supersessions: Option<bool>, // neighbors only; default false
    seed_ids: Option<Vec<u64>>,          // forward-compat: subgraph (#597)
    max_nodes: Option<u32>,              // forward-compat: subgraph (#597)
    from_id: Option<u64>,               // forward-compat: path (#598)
    to_id: Option<u64>,                 // forward-compat: path (#598)
}

struct EdgeRecord {                      // re-exported via mcp::EdgeRecord
    source_id: u64,
    target_id: u64,
    relation_type: String,
    direction: String,                   // "incoming"|"outgoing" relative to anchor
    depth: u8,
    metadata: Option<serde_json::Value>, // always None in vnc-018; NEVER skip_serializing_if
}

struct Truncated {
    forward: bool,
    backward: bool,
}

struct ChainResult {
    entries: Vec<EntryRecord>,
    truncated: Truncated,
}

struct CurrentResponse {
    entry: EntryRecord,
}

struct NeighborsResponse {
    edges: Vec<EdgeRecord>,
}
```

### Store-Layer Types (db.rs)

```
enum ChainDirection { Forward, Backward, Both }
enum NeighborDirection { Incoming, Outgoing, Both }

struct ChainQueryResult {
    entries: Vec<EntryRecord>,
    forward_capped: bool,
    backward_capped: bool,
}

struct RawEdgeRow {
    source_id: u64,
    target_id: u64,
    relation_type: String,
}
```

---

## Module Dependencies and Build Order

1. **unimatrix-engine/src/graph.rs** — add `node_index_for` accessor (no new dependencies)
2. **unimatrix-engine/src/graph_ppr.rs** — add Advances + Motivates (no new dependencies)
3. **unimatrix-engine/src/graph_expand.rs** — add Advances + Motivates (no new dependencies)
4. **unimatrix-store/src/migration.rs** — v26→v27 block (no new dependencies)
5. **unimatrix-store/src/db.rs** — new query functions + 4 index DDL + schema bump
6. **mcp/graph_read.rs** — all mode logic; depends on db.rs functions and graph.rs accessor
7. **mcp/mod.rs** — add `pub(crate) mod graph_read;` and `pub use graph_read::EdgeRecord;`
8. **mcp/tools.rs** — context_graph handler; depends on graph_read::handle_graph

Items 1–5 compile independently. Items 6–8 require the earlier items to be in place.

---

## Sequencing Constraints

- Migration runs before connection pools are constructed and before MCP server accepts connections — the four indexes are always present when any handler executes.
- `validate_no_unsupported_params` runs BEFORE mode dispatch inside `handle_graph`, but AFTER `require_cap` in `tools.rs`. Order is: capability check → parameter validation → mode dispatch.
- depth=1 path uses `query_direct_neighbors` (db.rs). depth>1 path uses BFS over `TypedRelationGraph` — both paths produce `Vec<EdgeRecord>` consumed by `handle_neighbors`.
- `follow_to_current` is a store-level async helper (uses `store.read_pool()`, not the in-memory graph).
