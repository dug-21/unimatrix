# vnc-019 Architecture: context_graph subgraph Mode

## System Overview

vnc-019 extends the existing `context_graph` MCP tool (vnc-018, GH #596) with a fourth
mode: `subgraph`. Where the three existing modes answer point-lookup questions (chain,
current) or single-anchor neighbor enumeration (neighbors), `subgraph` answers bounded
multi-hop, multi-seed, multi-edge-type traversal: "give me the full typed evidence web
around these entries."

The feature adds no new MCP tool, no new crate, no new table, and no new migration.
It lands entirely in `unimatrix-server` as a sibling module to the existing
`graph_read_neighbors.rs`, building on the infrastructure delivered by vnc-018:
`TypedRelationGraph`, `node_index_for`, `edges_of_type`, `EdgeRecord`, `GraphParams`,
`validate_no_unsupported_params`, and the four schema v27 indexes.

MCP tool count remains 14 after delivery.

## Component Breakdown

### 1. `graph_read.rs` — Wire types, entry point, parameter validation

Owns `GraphParams` (the tool's wire contract, ADR-003 vnc-018) and `EdgeRecord`
(ADR-004 vnc-018). Both types are extended minimally:

- `GraphParams` gains one field: `max_depth: Option<u8>` (ADR-001 vnc-019).
- `EdgeRecord` is unchanged structurally; `metadata` is populated for the first time.

`validate_no_unsupported_params` gains a `"subgraph"` arm that permits `seed_ids`,
`max_nodes`, and `max_depth`; all three remain rejected on the other three modes.

`handle_graph` gains a `"subgraph"` dispatch arm delegating to
`graph_read_subgraph::handle_subgraph`.

A new response envelope `SubgraphResponse` is defined here (with the other envelopes):

```rust
pub struct SubgraphResponse {
    pub nodes: Vec<EntryRecord>,
    pub edges: Vec<EdgeRecord>,
    pub truncated: bool,
    pub seed_ids: Vec<u64>,
    pub depth_reached: u8,
}
```

### 2. `graph_read_subgraph.rs` — BFS traversal and metadata hydration (new file)

All subgraph-mode logic lives here as a `#[path]`-declared sibling module. This is
**not** inline in `graph_read.rs` — see ADR-002 vnc-019 for the split rationale.

Responsible for:

- Parameter validation (seed_ids, max_depth, edge_types, direction).
- Acquiring the `TypedRelationGraph` read lock once, cloning, releasing before any
  async work (same pattern as `graph_read_neighbors.rs`).
- BFS frontier loop: hop-by-hop node expansion using `edges_of_type`.
- Per-hop `resolve_supersessions` substitution via `follow_to_current` (already in
  `graph_read_neighbors.rs`; re-used via `pub(super)` — see §3 below).
- Hard-cap enforcement: `max_nodes` checked pre-enqueue, BFS terminated with
  `truncated = true` when reached.
- Post-BFS node hydration: one batch `Store::get_batch()` or equivalent for all
  collected node IDs.
- Post-BFS metadata fetch: one `GRAPH_EDGES` batch query for all collected
  `(source_id, target_id, relation_type)` triples (ADR-003 vnc-019).
- Edge deduplication by `(source_id, target_id, relation_type)` when
  `direction = "both"`.

### 3. `graph_read_neighbors.rs` — Re-used `follow_to_current`

**Confirmed current state on `feature/vnc-018`** (verified against branch before design
session ended):
- `follow_to_current`: `async fn follow_to_current(store: &Store, id: u64) -> Option<u64>` — **private** (no `pub`). Must be changed to `pub(super)` in vnc-019 delivery.
- `all_non_supersedes_types`: already `pub(super) fn all_non_supersedes_types() -> Vec<RelationType>` — no change needed.
- `handle_neighbors`: already `pub(super)` — no change needed.

The subgraph module re-uses both via `pub(super)`. Both modules are `#[path]`-declared
submodules of `graph_read.rs`, so `pub(super)` makes them visible to sibling modules
without exposing them further. A private copy of `follow_to_current` is **not** acceptable
— if the 50-hop guard or `Store::get` signature changes, a copy drifts silently. The
delivery agent must add `pub(super)` to `follow_to_current` in `graph_read_neighbors.rs`
as the **first action** in vnc-019 delivery (compilation will fail without it).

### 4. `tools.rs` — Tool description update only

The `context_graph` tool description is updated to include:
- `subgraph` mode description.
- Staleness disclosure (in-memory BFS, tick-window lag — ADR-005 vnc-018 mandate).
- `direction` field in `EdgeRecord` is always `"outgoing"` for subgraph mode.
- Truncation semantics (`truncated: true`, `depth_reached`).
- Unknown seed ID behavior (empty result, not an error).

No logic change in `tools.rs`. The dispatch remains:
`capability_check → handle_graph → validate_no_unsupported_params → mode dispatch`.

### 5. `unimatrix-store` — No changes

No new SQL functions, no new migration, no new schema. The post-BFS metadata query
is issued via `sqlx::query` directly from `graph_read_subgraph.rs` using
`store.read_pool_server()` — the same pattern used by `query_direct_neighbors`.

### 6. `unimatrix-engine` — No changes

`TypedRelationGraph`, `node_index_for`, `node_id_for_index`, `edges_of_type`, and
`RelationType` are all sufficient as-is. `RelationEdge` is NOT modified (Non-Goal:
no engine struct changes for metadata).

## Component Interactions

```
tools.rs
  context_graph()
    require_cap(Read)                        ← checked first (FR-02)
    handle_graph(store, typed_graph_state, params, ctx)
      validate_no_unsupported_params(params)  ← centralized (ADR-003 vnc-018)
      match params.mode {
        "subgraph" =>
          graph_read_subgraph::handle_subgraph(store, typed_graph_state, params)
            1. Validate params (seed_ids, max_depth, edge_types, direction)
            2. typed_graph_state.read() → clone TypedRelationGraph → release lock
            3. BFS over TypedRelationGraph (edges_of_type per hop)
               - resolve_supersessions: follow_to_current(store, id) per deprecated node
               - max_nodes pre-enqueue cap → truncated flag
            4. Batch node hydration: Store → Vec<EntryRecord>
            5. Post-BFS metadata fetch: GRAPH_EDGES batch query
            6. Build SubgraphResponse
      }
```

### Lock Discipline

`TypedGraphStateHandle` is `Arc<RwLock<TypedGraphState>>` using `std::sync::RwLock`
(not tokio). Lock acquired once with `.read().unwrap_or_else(|e| e.into_inner())`,
graph cloned, lock released. No async work occurs while the lock is held. This is
identical to `neighbors_bfs` in `graph_read_neighbors.rs`.

### Ordering: resolve_supersessions BEFORE enqueue

When `resolve_supersessions = true`:
1. BFS discovers neighbor node ID from `edges_of_type`.
2. Call `follow_to_current(store, neighbor_id)` → returns terminal active ID.
3. Check `visited.contains(terminal_id)`.
4. If not visited: insert terminal_id into visited, enqueue terminal_id for expansion.

The deprecated intermediate node is never enqueued; only the terminal active node
appears in `nodes`. This ordering prevents expanding deprecated nodes and prevents
the resolved successor from being double-enqueued if it appears via multiple paths.

### Missing Seed ID Behavior

If a seed ID is not present in `TypedRelationGraph.node_index`:
- The seed is absent from the in-memory graph (cold-start, or genuinely missing).
- Return: `SubgraphResponse { nodes: [], edges: [], truncated: false,
  seed_ids: [N], depth_reached: 0 }`.
- Not an error. Consistent with `neighbors_bfs` cold-start behavior.

### Seeds in nodes

Seed entries are always included in `nodes`, regardless of BFS traversal results.
Seeds are hydrated from the store (same batch query as BFS-discovered nodes) and
count toward `max_nodes`. If seeds alone reach `max_nodes`, BFS terminates
immediately with `truncated: true, depth_reached: 0`.

## Technology Decisions

| Decision | Choice | ADR | Unimatrix ID |
|----------|--------|-----|--------------|
| `max_depth` field location | Added to `GraphParams` as `Option<u8>` | ADR-001 | #4490 |
| File placement for `handle_subgraph` | New `graph_read_subgraph.rs`, not inline | ADR-002 | #4491 |
| Post-BFS metadata strategy | Batch GRAPH_EDGES query after BFS, not per-hop | ADR-003 | #4492 |
| Staleness disclosure | Tool description text only; no `graph_rebuilt_at` field | ADR-004 | #4493 |
| BFS traversal engine | In-memory `TypedRelationGraph` only (no SQL fallback) | Inherited ADR-005 vnc-018 | #4479 |
| `EdgeRecord` type | Unchanged; `metadata` populated for first time | Inherited ADR-004 vnc-018 | #4478 |
| `GraphParams` struct lock | `Option<T>` additions permitted; removal/retyping prohibited | Inherited ADR-003 vnc-018 | #4477 |

## Integration Points

### Depends On (vnc-018 deliverables — SR-06)

- `graph_read.rs`: `GraphParams`, `EdgeRecord`, `validate_no_unsupported_params`,
  `handle_graph` dispatch. These are stub-only until vnc-018 PR #596 merges.
- `graph_read_neighbors.rs`: `follow_to_current`, `all_non_supersedes_types` — both
  needed by subgraph BFS.
- `graph_read_supersession.rs`: not directly used but `handle_graph` dispatch relies on it.
- `unimatrix-engine/graph.rs`: `TypedRelationGraph::node_index_for`, `node_id_for_index`,
  `edges_of_type`, `RelationType::from_str` — all delivered in vnc-018.
- Schema v27 indexes (`idx_graph_edges_source_type`, `idx_graph_edges_target_type`) —
  required by the post-BFS metadata batch query.

### Produces

- `SubgraphResponse` — new wire type, defined in `graph_read.rs`.
- Populated `EdgeRecord.metadata` — first actual population of this field (was always
  `None` in vnc-018).

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|----------------|--------|
| `GraphParams.max_depth` | `Option<u8>` | `graph_read.rs` (ADR-001) |
| `SubgraphResponse` | `{ nodes: Vec<EntryRecord>, edges: Vec<EdgeRecord>, truncated: bool, seed_ids: Vec<u64>, depth_reached: u8 }` | `graph_read.rs` |
| `handle_subgraph` | `async fn(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: &GraphParams) -> Result<SubgraphResponse, ErrorData>` | `graph_read_subgraph.rs` |
| `follow_to_current` | `async fn follow_to_current(store: &Store, id: u64) -> Option<u64>` | `graph_read_neighbors.rs` — currently private; **add `pub(super)`** as first delivery action |
| `all_non_supersedes_types` | `fn() -> Vec<RelationType>` | `graph_read_neighbors.rs` (re-used) |
| `TypedRelationGraph::node_index_for` | `fn(&self, id: u64) -> Option<NodeIndex>` | `unimatrix-engine/graph.rs` |
| `TypedRelationGraph::node_id_for_index` | `fn(&self, idx: NodeIndex) -> Option<u64>` | `unimatrix-engine/graph.rs` |
| `TypedRelationGraph::edges_of_type` | `fn(&self, NodeIndex, RelationType, Direction) -> impl Iterator<Item = EdgeReference>` | `unimatrix-engine/graph.rs` |
| `EdgeRecord.direction` | Always `"outgoing"` in subgraph mode | `graph_read.rs` (ADR-004 vnc-018) |
| Metadata batch SQL | `SELECT source_id, target_id, relation_type, metadata FROM graph_edges WHERE (source_id, target_id, relation_type) IN (...)` — see ADR-003 | `graph_read_subgraph.rs` |
| `validate_no_unsupported_params` | Extended: `"subgraph"` arm permits `seed_ids`, `max_nodes`, `max_depth`; all three rejected on other modes | `graph_read.rs` |

## BFS Algorithm Contract

The BFS in `handle_subgraph` follows this exact ordering:

```
Input: seed_ids, edge_types, direction, max_depth (default 3), max_nodes (default 200),
       resolve_supersessions (default false)

1. Validate:
   - seed_ids non-empty (else: validation error)
   - max_depth in [1, 10] (else: validation error with range message)
   - max_nodes in [1, 200] (else: validation error: "max_nodes must be in range 1..=200, got {value}")
   - each edge_type parses via RelationType::from_str (else: validation error naming
     the unrecognized value and listing valid types)
   - direction in ["incoming", "outgoing", "both"] (else: validation error)
   - if edge_types absent or empty: expand to all_non_supersedes_types() (15 types,
     excludes Supersedes — consistent with neighbors mode default)

2. Acquire TypedRelationGraph (std::sync::RwLock, unwrap_or_else poison recovery)
   Clone → release lock

3. Initialize:
   visited: HashSet<u64>  (keyed by node_id only — same invariant as neighbors BFS)
   frontier: VecDeque<(NodeIndex, u64, u8)>  — (idx, id, depth)
   collected_edges: Vec<(u64, u64, String, u8)>  — (source_id, target_id, rel_type, depth)
   collected_node_ids: Vec<u64>

4. Seed phase:
   For each seed_id:
     - If resolve_supersessions: follow_to_current → use terminal (fallback: original)
     - If not in visited AND collected_node_ids.len() < max_nodes:
         add to visited, collected_node_ids
         if node present in graph: push to frontier with depth=0
   - If collected_node_ids.len() >= max_nodes after seeds: truncated=true, skip BFS

5. BFS phase (while frontier non-empty):
   pop (current_idx, current_id, current_depth)
   if current_depth >= max_depth: continue (do not expand)
   for each rel_type in requested_types:
     for each petgraph_dir in [Outgoing] | [Incoming] | [Outgoing, Incoming]:
       for each edge_ref from edges_of_type(current_idx, rel_type, petgraph_dir):
         neighbor_idx = edge_ref.target() or .source() per direction
         neighbor_id = node_id_for_index(neighbor_idx) — skip if None

         // Canonical edge always stored as (source→target):
         (edge_src, edge_tgt) = if petgraph_dir == Outgoing:
                                    (current_id, neighbor_id)
                                  else:
                                    (neighbor_id, current_id)
         edge_key = (edge_src, edge_tgt, rel_type.as_str())

         // Dedup edges by canonical triple (AC-12):
         if edge_key not in edge_set:
           edge_set.insert(edge_key)
           collect edge with depth = current_depth + 1

         effective_id = if resolve_supersessions:
                          follow_to_current(store, neighbor_id).unwrap_or(neighbor_id)
                        else: neighbor_id

         if effective_id not in visited:
           if collected_node_ids.len() >= max_nodes:
             truncated = true
             // Break ALL nested loops immediately.
             // In Rust: use a labeled break on the outer 'bfs loop.
             //   'bfs: while let Some(...) = frontier.pop_front() {
             //     'hop: for rel_type in ... {
             //       for edge_ref in ... {
             //         ...
             //         if cap_reached { truncated = true; break 'bfs; }
             //       }
             //     }
             //   }
             // A boolean `truncated` flag checked at the top of the while loop
             // is an acceptable alternative if the labeled-break form is harder
             // to read, but the implementor must choose ONE approach before
             // writing the function — mixing both creates dead code.
             break 'bfs
           visited.insert(effective_id)
           collected_node_ids.push(effective_id)
           hop_depth = current_depth + 1
           if hop_depth < max_depth:
             if let Some(idx) = graph.node_index_for(effective_id):
               frontier.push_back((idx, effective_id, hop_depth))

POST_BFS:
5b. Dangling-edge filter (REQUIRED — correctness invariant):
   When the max_nodes cap fires mid-hop, edges to the truncated neighbor are already
   in collected_edges but the neighbor is NOT in collected_node_ids. Without this step
   the response would contain EdgeRecords whose target_id references a node absent from
   nodes — agents cannot reconstruct the graph from a dangling reference.

   node_id_set = HashSet::from(collected_node_ids)
   collected_edges.retain(|(src, tgt, _, _)| {
       node_id_set.contains(src) && node_id_set.contains(tgt)
   })

   This is a single O(edges) pass, bounded by the 200-node cap (~600 edges max).

6. Batch node hydration:
   nodes = store.get_many(collected_node_ids) — single query

7. Post-BFS metadata fetch (ADR-003):
   triples = collected_edges as (source_id, target_id, relation_type) set
   metadata_map = batch_query_graph_edges_metadata(store, triples)
   for each edge in collected_edges:
     edge.metadata = metadata_map.get(&(src, tgt, rel)).cloned()

8. Compute depth_reached = collected_edges.iter().map(|e| e.depth).max().unwrap_or(0)

9. Return SubgraphResponse { nodes, edges, truncated, seed_ids, depth_reached }
```

## Post-BFS Metadata SQL

The metadata batch query uses SQLite's `IN` clause with a dynamically built condition.
Because SQLite does not support tuple IN syntax `(a, b, c) IN (...)`, the query uses
a series of `OR (source_id = ? AND target_id = ? AND relation_type = ?)` conditions
or a VALUES subquery approach. For the bounded case (max ~600 edges at the 200-node cap),
the OR-chain approach is acceptable. The composite indexes `idx_graph_edges_source_type`
and `idx_graph_edges_target_type` from schema v27 cover each lookup.

Exact SQL pattern (to be expanded per collected edge count):

```sql
SELECT source_id, target_id, relation_type, metadata
FROM graph_edges
WHERE (source_id = ?1 AND target_id = ?2 AND relation_type = ?3)
   OR (source_id = ?4 AND target_id = ?5 AND relation_type = ?6)
   -- ... one clause per collected edge
```

This is O(1) round-trips regardless of edge count, bounded by the 200-node cap which
produces at most ~600 edges at depth 3.

## SR Disposition

| Risk ID | Resolution |
|---------|------------|
| SR-01 (staleness) | Tool description includes mandatory staleness text (ADR-004). No `graph_rebuilt_at` timestamp in response — `depth_reached` and `truncated` are sufficient per ADR-004 rationale. |
| SR-02 (truncated signal ambiguity) | `truncated` bool is sufficient for this feature. Structured truncation reason (seed saturation vs. BFS expansion) deferred to W1B-2c or a future AC amendment. |
| SR-03 (edge batch size) | Bounded by `max_nodes=200` cap which caps edges at ~600. OR-chain SQL is acceptable at this scale. ADR-003 records this bound explicitly. |
| SR-04 (file split) | Decided upfront: `graph_read_subgraph.rs` (ADR-002). Not a delivery-time call. |
| SR-05 (supersession I/O) | Inline per-hop `follow_to_current` (50-hop guard already present). Batch pre-resolve deferred — the 50-hop guard + ~200 node cap bounds worst-case to 200 Store::get calls, which is acceptable for a read-only path. |
| SR-06 (vnc-018 dependency) | Accepted sequencing dependency. Delivery must not begin until vnc-018 PR #596 merges. |
| SR-07 (validate regression) | Covered by AC-11 (seed_ids) plus new AC: `max_depth` on non-subgraph modes returns validation error. |

## Open Questions

None. All SCOPE.md OQs (OQ-01 through OQ-06) are resolved. SR-02 structured truncation
reason is deferred to W1B-2c scope.
