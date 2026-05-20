# vnc-019 Pseudocode Overview: context_graph subgraph Mode

## Components Involved

| Component File | Action | Role |
|----------------|--------|------|
| `graph_read.rs` | Modify | Wire types, entry point, parameter validation |
| `graph_read_subgraph.rs` | Create (new) | BFS traversal, metadata hydration, SubgraphResponse construction |
| `graph_read_neighbors.rs` | Modify | Visibility: `follow_to_current` private → `pub(super)` |
| `tools.rs` | Modify | Tool description update (subgraph section + staleness disclosure) |

No new crates, tables, migrations, or MCP tools.

---

## Data Flow

```
MCP caller
  → tools.rs: context_graph()
      require_cap(Read)                             [capability gate, before handle_graph]
      handle_graph(store, typed_graph_state, params, ctx)
        validate_no_unsupported_params(params)      [subgraph arm added; rejects from_id/to_id]
        match params.mode {
          "subgraph" =>
            graph_read_subgraph::handle_subgraph(store, typed_graph_state, params)
              1. Validate params                    [seed_ids, max_depth, max_nodes, edge_types, direction]
              2. RwLock read → clone TypedRelationGraph → release lock
              3. Seed phase                         [visited set initialized; seeds → collected_node_ids]
              4. BFS phase                          [edges_of_type per hop; edge dedup; cap enforcement]
              5. Dangling-edge filter               [retain only edges where both endpoints in collected_node_ids]
              6. store.get_many(collected_node_ids) [batch node hydration; single query]
              7. OR-chain SQL on GRAPH_EDGES        [post-BFS metadata batch; skipped when edges empty]
              8. depth_reached computation          [max depth across collected_edges]
              9. SubgraphResponse { nodes, edges, truncated, seed_ids, depth_reached }
            serialized to JSON
          → CallToolResult::success(json)
        }
```

Cross-module imports within `graph_read.rs` module tree (`pub(super)`):

- `graph_read_subgraph.rs` imports from parent (`super::`):
  - `SubgraphResponse` (defined in `graph_read.rs`)
  - `EdgeRecord` (defined in `graph_read.rs`)
  - `GraphParams` (defined in `graph_read.rs`)
- `graph_read_subgraph.rs` imports from sibling (`super::graph_read_neighbors`):
  - `follow_to_current` (changed to `pub(super)` in `graph_read_neighbors.rs`)
  - `all_non_supersedes_types` (already `pub(super)`)

---

## Shared Types: New and Modified

### Modified: `GraphParams` (in `graph_read.rs`)

One field added. All existing fields unchanged:

```
pub struct GraphParams {
    // ... all existing fields unchanged (ADR-003 vnc-018 lock) ...

    // NEW — subgraph mode only:
    pub max_depth: Option<u8>,
    // "subgraph mode only: BFS max depth 1..=10 (default 3 when absent).
    //  Error if passed to chain, current, or neighbors modes."
}
```

Validation rule: `max_depth` is rejected on `chain`/`current`/`neighbors` by
`validate_no_unsupported_params` with message:
`"max_depth is not supported in {mode} mode — use subgraph mode"`

### New: `SubgraphResponse` (in `graph_read.rs`)

Defined adjacent to `ChainResult`, `CurrentResponse`, `NeighborsResponse`:

```
pub struct SubgraphResponse {
    pub nodes: Vec<EntryRecord>,   // full hydrated entry records
    pub edges: Vec<EdgeRecord>,    // typed edges with metadata populated
    pub truncated: bool,           // true if max_nodes cap was reached
    pub seed_ids: Vec<u64>,        // echo of input seed_ids
    pub depth_reached: u8,         // actual max BFS depth traversed
}
// derives: serde::Serialize
```

### BFS Internal State (local to `handle_subgraph`)

```
visited: HashSet<u64>                         -- keyed on effective_id (post-substitution)
frontier: VecDeque<(NodeIndex, u64, u8)>      -- (graph_idx, entry_id, current_depth)
collected_edges: Vec<(u64, u64, String, u8)>  -- (source_id, target_id, rel_type_str, depth)
collected_node_ids: Vec<u64>                  -- ordered by discovery
edge_set: HashSet<(u64, u64, String)>         -- dedup by canonical (src, tgt, rel_type)
truncated: bool
```

### Metadata Batch Map (local to `handle_subgraph`, post-BFS)

```
HashMap<(u64, u64, String), Option<serde_json::Value>>
// key: (source_id, target_id, relation_type_str)
// value: parsed JSON or None for null/malformed
```

---

## Sequencing Constraints

1. `graph_read_neighbors.rs` visibility change must land first in delivery — compilation
   of `graph_read_subgraph.rs` fails without `pub(super) follow_to_current`.

2. `GraphParams.max_depth` field must be present before `handle_subgraph` can reference
   `params.max_depth`. Both live in the same PR, but graph_read.rs changes compile first.

3. The `"subgraph"` dispatch arm in `handle_graph` must not be added until
   `graph_read_subgraph::handle_subgraph` exists; add both atomically.

4. The `validate_no_unsupported_params` subgraph arm and the unrecognized-mode error
   update must land in the same commit as the dispatch arm (R-05 regression guard).

5. The vnc-018 test `test_validate_unrecognized_mode_fires_before_field_check` that
   passes `mode="subgraph"` and expects `"unrecognized mode"` must be updated as part
   of FR-20 delivery.

---

## Wire Response Format

```json
{
  "nodes": [ { /* full EntryRecord */ } ],
  "edges": [
    {
      "source_id": 42,
      "target_id": 57,
      "relation_type": "Supports",
      "direction": "outgoing",
      "depth": 1,
      "metadata": null
    }
  ],
  "truncated": false,
  "seed_ids": [42],
  "depth_reached": 2
}
```

`direction` is always `"outgoing"` for every `EdgeRecord` — canonical stored direction
(`source_id → target_id`), regardless of traversal direction parameter.
