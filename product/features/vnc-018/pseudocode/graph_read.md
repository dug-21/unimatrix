# Pseudocode: mcp/graph_read.rs + unimatrix-engine/src/graph.rs (node_index_for)

## Purpose

New module `crates/unimatrix-server/src/mcp/graph_read.rs`. Owns all
`context_graph` dispatch and mode logic. Exposes one public entry point called from
`tools.rs`. Must not exceed 500 lines; split into `graph_read_supersession.rs` and
`graph_read_neighbors.rs` if approaching the limit.

Also covers the `node_index_for` accessor added to `TypedRelationGraph` in
`crates/unimatrix-engine/src/graph.rs` (ADR-008).

---

## New/Modified Functions

### `graph.rs` — TypedRelationGraph accessor (ADR-008)

**Location**: `crates/unimatrix-engine/src/graph.rs`, inside `impl TypedRelationGraph`

```
pub fn node_index_for(&self, id: u64) -> Option<NodeIndex> {
    // self.node_index is the existing HashMap<u64, NodeIndex> field (pub(crate))
    self.node_index.get(&id).copied()
    // Returns None when the entry ID is not in the current tick's graph
    // (cold-start, unknown ID, or entry not loaded into graph)
}
```

This is ~3 lines. It exposes only the lookup primitive; no interior mutability,
no HashMap exposure.

---

### `graph_read.rs` — Types

```
// Derives: Debug, Serialize, Deserialize, JsonSchema
struct GraphParams {
    mode: String,
    agent_id: Option<String>,
    format: Option<String>,
    id: Option<u64>,
    direction: Option<String>,
    edge_types: Option<Vec<String>>,
    depth: Option<u8>,
    resolve_supersessions: Option<bool>,
    // Forward-compat fields (validated to error on misuse):
    seed_ids: Option<Vec<u64>>,
    max_nodes: Option<u32>,
    from_id: Option<u64>,
    to_id: Option<u64>,
}

// Derives: Debug, Serialize, Deserialize, JsonSchema, Clone
// ADR-004: defined here, re-exported from mcp/mod.rs
pub struct EdgeRecord {
    pub source_id: u64,
    pub target_id: u64,
    pub relation_type: String,
    pub direction: String,        // "incoming" | "outgoing"
    pub depth: u8,
    pub metadata: Option<serde_json::Value>,   // always None; NO skip_serializing_if (ADR-004, R-15)
}

// Derives: Debug, Serialize, Deserialize, JsonSchema
pub struct Truncated {
    pub forward: bool,
    pub backward: bool,
}

// Derives: Debug, Serialize, Deserialize, JsonSchema
pub struct ChainResult {
    pub entries: Vec<EntryRecord>,
    pub truncated: Truncated,
}

// Derives: Debug, Serialize, Deserialize, JsonSchema
pub struct CurrentResponse {
    pub entry: EntryRecord,
}

// Derives: Debug, Serialize, Deserialize, JsonSchema
pub struct NeighborsResponse {
    pub edges: Vec<EdgeRecord>,
}
```

---

### `handle_graph` — Public Entry Point

```
pub(crate) async fn handle_graph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: GraphParams,
    ctx: &ToolContext,
) -> Result<CallToolResult, rmcp::ErrorData>
```

**Body**:

```
// Step 1: Centralized parameter validation (before mode dispatch)
// Note: capability check already ran in tools.rs before this function is called
if let Err(msg) = validate_no_unsupported_params(&params) {
    return Err(rmcp::ErrorData {
        code: rmcp::ErrorCode::INVALID_PARAMS,
        message: msg,
        data: None,
    })
}

// Step 2: Require anchor ID for all three modes
let id = match params.id {
    Some(id) => id,
    None => return Err(rmcp::ErrorData {
        code: rmcp::ErrorCode::INVALID_PARAMS,
        message: "id is required for chain, current, and neighbors modes".to_string(),
        data: None,
    }),
}

// Step 3: Mode dispatch
match params.mode.as_str() {
    "chain" => {
        let result = handle_chain(store, &params, id).await
        // Serialize ChainResult to JSON text; format via ctx.format if present
        // Return as CallToolResult text content
        let json = serde_json::to_string(&result)?
        Ok(CallToolResult::text(json))
    }
    "current" => {
        match handle_current(store, &params, id).await {
            Ok(resp) => {
                let json = serde_json::to_string(&resp)?
                Ok(CallToolResult::text(json))
            }
            Err(msg) => Err(rmcp::ErrorData {
                code: rmcp::ErrorCode::INVALID_PARAMS,
                message: msg,
                data: None,
            })
        }
    }
    "neighbors" => {
        let result = handle_neighbors(store, typed_graph_state, &params, id).await?
        let json = serde_json::to_string(&result)?
        Ok(CallToolResult::text(json))
    }
    // validate_no_unsupported_params already caught unrecognized modes above;
    // this arm is unreachable under normal flow but required for exhaustiveness
    _ => unreachable!("validate_no_unsupported_params must catch unrecognized modes first")
}
```

---

### `validate_no_unsupported_params` — Centralized Validation (ADR-003)

```
fn validate_no_unsupported_params(params: &GraphParams) -> Result<(), String>
```

**Body**:

```
// Unrecognized mode: fires FIRST, before any field checks (R-04)
// This match is the single point of truth for supported modes.
match params.mode.as_str() {
    "chain" => {
        // chain rejects forward-compat fields AND resolve_supersessions=Some(true)
        if params.seed_ids.is_some() {
            return Err("seed_ids is not supported in chain mode — use subgraph mode (#597)".to_string())
        }
        if params.max_nodes.is_some() {
            return Err("max_nodes is not supported in chain mode — use subgraph mode (#597)".to_string())
        }
        if params.from_id.is_some() {
            return Err("from_id is not supported in chain mode — use path mode (#598)".to_string())
        }
        if params.to_id.is_some() {
            return Err("to_id is not supported in chain mode — use path mode (#598)".to_string())
        }
        // resolve_supersessions on chain is semantically circular (FR-08, AC-15c, R-08)
        if params.resolve_supersessions == Some(true) {
            return Err(
                "resolve_supersessions is not applicable to chain mode — chain IS the supersession audit".to_string()
            )
        }
        Ok(())
    }
    "current" => {
        if params.seed_ids.is_some() {
            return Err("seed_ids is not supported in current mode — use subgraph mode (#597)".to_string())
        }
        if params.max_nodes.is_some() {
            return Err("max_nodes is not supported in current mode — use subgraph mode (#597)".to_string())
        }
        if params.from_id.is_some() {
            return Err("from_id is not supported in current mode — use path mode (#598)".to_string())
        }
        if params.to_id.is_some() {
            return Err("to_id is not supported in current mode — use path mode (#598)".to_string())
        }
        Ok(())
    }
    "neighbors" => {
        if params.seed_ids.is_some() {
            return Err("seed_ids is not supported in neighbors mode — use subgraph mode (#597)".to_string())
        }
        if params.max_nodes.is_some() {
            return Err("max_nodes is not supported in neighbors mode — use subgraph mode (#597)".to_string())
        }
        if params.from_id.is_some() {
            return Err("from_id is not supported in neighbors mode — use path mode (#598)".to_string())
        }
        if params.to_id.is_some() {
            return Err("to_id is not supported in neighbors mode — use path mode (#598)".to_string())
        }
        Ok(())
    }
    // _ arm fires BEFORE field checks — unrecognized mode error is the first thing callers see (R-04)
    // When #597 ships, add "subgraph" arm here that permits seed_ids and max_nodes
    _ => Err(format!(
        "unrecognized mode '{}' — supported modes: chain, current, neighbors",
        params.mode
    ))
}
```

**Critical**: The `_` fallthrough arm is the final arm. It fires ONLY for unrecognized
modes. All known modes (`"chain"`, `"current"`, `"neighbors"`) have explicit arms that
run field checks. This ordering guarantees that `mode="subgraph", seed_ids=[1]` returns
"unrecognized mode" rather than "seed_ids not supported in subgraph mode".

---

### `handle_chain` — Supersession Chain Walk (FR-04)

```
async fn handle_chain(
    store: &Store,
    params: &GraphParams,
    id: u64,
) -> ChainResult
```

**Body**:

```
// Assumes validate_no_unsupported_params has already run.
// resolve_supersessions on chain is guaranteed absent by validate_no_unsupported_params.

let direction = match params.direction.as_deref().unwrap_or("both") {
    "forward"  => ChainDirection::Forward,
    "backward" => ChainDirection::Backward,
    "both"     => ChainDirection::Both,
    other => {
        // chain uses forward/backward/both; "incoming"/"outgoing" are neighbors-mode values
        // Return an empty truncated result with an error — or surface as handle_graph error
        // Preferred: surface as rmcp error from handle_graph by returning early
        // Implementation note: validate direction before calling handle_chain, or handle_chain
        // returns a Result<ChainResult, String>
        // For simplicity, invalid direction returns empty chain (direction-specific error is R-17-adjacent)
        // IMPLEMENTATION DECISION: validate direction string in handle_graph before calling handle_chain
        //   valid for chain: "forward", "backward", "both" (default "both")
        //   invalid for chain: "incoming", "outgoing" → error before calling handle_chain
        return ChainResult {
            entries: vec![],
            truncated: Truncated { forward: false, backward: false },
        }
    }
}

// Call the SQL CTE query function (ADR-001 — NO in-memory path)
let result = unimatrix_store::db::query_supersession_chain(
    store.read_pool(),
    id,
    direction,
    50,  // depth_cap — enforced at CTE level
).await

match result {
    Ok(chain_result) => ChainResult {
        entries: chain_result.entries,
        truncated: Truncated {
            forward: chain_result.forward_capped,
            backward: chain_result.backward_capped,
        },
    },
    Err(e) => {
        // Store error — log and return empty result
        tracing::error!("query_supersession_chain failed: {e}");
        ChainResult {
            entries: vec![],
            truncated: Truncated { forward: false, backward: false },
        }
    }
}
```

**Key constraints**:
- Non-existent ID → CTE returns zero rows → `entries: vec![]`, no error (AC-04).
- `truncated.forward` and `truncated.backward` are independently set per direction.
- `direction` for `chain` mode: "forward" | "backward" | "both" (default "both"). The words "incoming" and "outgoing" are neighbors-mode vocabulary and must produce a direction validation error if passed to chain mode. Validate this in `handle_graph` before calling `handle_chain`.

---

### `handle_current` — Terminal-Active Lookup (FR-05)

```
async fn handle_current(
    store: &Store,
    params: &GraphParams,
    id: u64,
) -> Result<CurrentResponse, String>
```

**Body**:

```
// Uses SQL recursive CTE following superseded_by to terminal with status='Active'.
// CTE MUST include AND e.status = 'Active' — without it, orphaned deprecated entries
// (superseded_by IS NULL, status = 'Deprecated') are silently returned (R-20, Critical).

// query_supersession_chain called in Forward direction (follows superseded_by chain)
// For current mode: we need a specialized CTE that follows superseded_by links
// and requires AND status = 'Active' at the terminal step.
// This is query_supersession_chain with a special mode, OR a separate query function.

// IMPLEMENTATION NOTE: query_supersession_chain as designed in store_queries.md
// returns the full chain. For current mode, we need only the terminal active entry.
// Two options:
//   A) Call query_supersession_chain(id, Forward, 50) and filter the results in-process
//      for the terminal with superseded_by IS NULL AND status = 'Active'.
//   B) Add a separate query_current_terminal(pool, id) function to db.rs.
//
// Option B is cleaner (matches the CTE specified in ARCHITECTURE.md exactly).
// The CTE for current mode is different from chain: it follows superseded_by
// (not supersedes) and uses LIMIT 1 with AND e.status = 'Active'.
// See store_queries.md for query_current_terminal pseudocode.
// handle_current calls query_current_terminal.

let terminal_entry = unimatrix_store::db::query_current_terminal(
    store.read_pool(),
    id,
).await

match terminal_entry {
    Ok(Some(entry)) => Ok(CurrentResponse { entry }),
    Ok(None) => {
        // Covers all three failure cases (no distinction at SQL level):
        //   - Non-existent ID (CTE anchor SELECT returns empty)
        //   - Orphaned deprecated terminal (status='Active' filter drops it)
        //   - Chain exceeds 50 hops (depth cap fires, no terminal reachable)
        // All produce zero rows → same error (intentional per spec FR-05)
        Err(format!("No active terminal found for entry {id}"))
    }
    Err(e) => {
        tracing::error!("query_current_terminal failed for id={id}: {e}");
        Err(format!("No active terminal found for entry {id}"))
    }
}
```

**Critical constraints**:
- Non-existent ID → error, NOT empty (AC-05a). This is intentionally asymmetric with chain mode (AC-04 returns empty). A comment in the code MUST state this asymmetry is intentional (R-21).
- Orphaned deprecated entry (superseded_by IS NULL, status = 'Deprecated') → error (R-20).
- The `AND e.status = 'Active'` filter in the CTE is the ONLY guard against the orphaned-deprecated defect.
- Error message: "No active terminal found for entry {id}" (or equivalent with id).

---

### `handle_neighbors` — Typed-Edge Neighbor Retrieval (FR-06)

```
async fn handle_neighbors(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
    id: u64,
) -> Result<NeighborsResponse, rmcp::ErrorData>
```

**Body**:

```
// Step 1: Validate depth parameter (R-11)
let depth = params.depth.unwrap_or(1)
if depth == 0 || depth > 10 {
    return Err(rmcp::ErrorData {
        code: rmcp::ErrorCode::INVALID_PARAMS,
        message: format!("depth must be in range 1..=10, got {depth}"),
        data: None,
    })
}

// Step 2: Validate direction for neighbors mode (R-17)
// neighbors uses: "incoming" | "outgoing" | "both" (NOT "forward"/"backward")
let direction = match params.direction.as_deref().unwrap_or("both") {
    "incoming" => NeighborDirection::Incoming,
    "outgoing" => NeighborDirection::Outgoing,
    "both"     => NeighborDirection::Both,
    other => return Err(rmcp::ErrorData {
        code: rmcp::ErrorCode::INVALID_PARAMS,
        message: format!(
            "invalid direction '{other}' for neighbors mode — valid values: incoming, outgoing, both"
        ),
        data: None,
    }),
}

// Step 3: Validate and resolve edge_types (FR-07, R-06)
// Empty or absent edge_types → use all non-Supersedes types
let requested_types: Vec<RelationType> = match &params.edge_types {
    None | Some(empty) if empty.is_empty() => {
        // All types except Supersedes (silent exclusion, AC-10, AC-10a — no warning in response)
        RelationType::all_non_supersedes()   // returns all 15 non-Supersedes variants
    }
    Some(type_strings) => {
        let mut resolved = Vec::new()
        for s in type_strings {
            // Reject Supersedes explicitly (FR-07, AC-15a)
            if s.eq_ignore_ascii_case("Supersedes") {
                return Err(rmcp::ErrorData {
                    code: rmcp::ErrorCode::INVALID_PARAMS,
                    message: "Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation".to_string(),
                    data: None,
                })
            }
            // Validate each type via from_str (FR-07, AC-15)
            match RelationType::from_str(s) {
                Some(rel_type) => resolved.push(rel_type),
                None => return Err(rmcp::ErrorData {
                    code: rmcp::ErrorCode::INVALID_PARAMS,
                    message: format!("unknown edge type '{s}' — valid types: Advances, Cites, ..."),
                    data: None,
                }),
            }
        }
        resolved
    }
}

// Step 4: Dispatch to SQL path (depth=1) or BFS path (depth>1) (ADR-005)
let edges = if depth == 1 {
    handle_neighbors_sql(store, id, &requested_types, direction).await?
} else {
    handle_neighbors_bfs(store, typed_graph_state, id, &requested_types, direction, depth,
                         params.resolve_supersessions.unwrap_or(false)).await?
}

Ok(NeighborsResponse { edges })
```

---

### `handle_neighbors_sql` — depth=1 Live SQL Path (ADR-005)

```
async fn handle_neighbors_sql(
    store: &Store,
    id: u64,
    types: &[RelationType],
    direction: NeighborDirection,
) -> Result<Vec<EdgeRecord>, rmcp::ErrorData>
```

**Body**:

```
// Convert RelationType slice to &[&str] for db.rs query function
let type_strs: Vec<&str> = types.iter().map(|t| t.as_str()).collect()

let raw_rows = unimatrix_store::db::query_direct_neighbors(
    store.read_pool(),
    id,
    &type_strs,
    direction,
).await.map_err(|e| {
    tracing::error!("query_direct_neighbors failed for id={id}: {e}");
    rmcp::ErrorData {
        code: rmcp::ErrorCode::INTERNAL_ERROR,
        message: format!("graph query failed: {e}"),
        data: None,
    }
})?

// Convert RawEdgeRow → EdgeRecord
// direction field: "outgoing" if source_id == id; "incoming" if target_id == id
let edges = raw_rows.into_iter().map(|row| {
    let dir_str = if row.source_id == id { "outgoing" } else { "incoming" }
    EdgeRecord {
        source_id: row.source_id,
        target_id: row.target_id,
        relation_type: row.relation_type,
        direction: dir_str.to_string(),
        depth: 1,
        metadata: None,   // always None in vnc-018 (ADR-004, R-15)
    }
}).collect()

Ok(edges)
```

**Non-existent anchor**: `query_direct_neighbors` returns an empty `Vec<RawEdgeRow>` →
`edges` is empty → `NeighborsResponse { edges: [] }` with no error (OQ-01 resolution:
consistent with chain mode empty-for-unknown-id behavior).

---

### `handle_neighbors_bfs` — depth>1 In-Memory BFS Path (ADR-005)

```
async fn handle_neighbors_bfs(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    id: u64,
    types: &[RelationType],
    direction: NeighborDirection,
    depth: u8,
    resolve_supersessions: bool,
) -> Result<Vec<EdgeRecord>, rmcp::ErrorData>
```

**Body**:

```
// Acquire read lock once — hold for full BFS duration (IR-02 accepted risk)
let graph_guard = typed_graph_state.read().await
let graph = &graph_guard.graph     // TypedRelationGraph reference

// Find the anchor node in the in-memory graph (ADR-008)
let start_node = match graph.node_index_for(id) {
    Some(idx) => idx,
    None => {
        // Anchor ID not in current tick's graph (cold-start or genuinely absent)
        // Return empty result — no error (consistent with depth=1 behavior)
        return Ok(vec![])
    }
}

// BFS state
let mut result_edges: Vec<EdgeRecord> = Vec::new()
let mut visited: HashSet<u64> = HashSet::new()           // keyed by node_id ONLY (AC-11a, R-18)
let mut frontier: VecDeque<(NodeIndex, u64, u8)> = VecDeque::new()  // (node_idx, node_id, depth)

visited.insert(id)
frontier.push_back((start_node, id, 0))

while let Some((current_idx, current_id, current_depth)) = frontier.pop_front() {
    if current_depth >= depth {
        // At max depth: process edges for recording but do NOT enqueue neighbors
        // (same pattern as graph_expand.rs)
    }

    // For each requested type, query edges_of_type in the appropriate direction(s)
    for rel_type in types {
        let edge_directions_to_query = match direction {
            NeighborDirection::Outgoing => vec![Direction::Outgoing],
            NeighborDirection::Incoming => vec![Direction::Incoming],
            NeighborDirection::Both     => vec![Direction::Outgoing, Direction::Incoming],
        }

        for petgraph_dir in edge_directions_to_query {
            for edge_ref in graph.edges_of_type(current_idx, *rel_type, petgraph_dir) {
                // Determine actual source/target node IDs from the edge
                let (src_node_id, tgt_node_id, edge_dir_str) = match petgraph_dir {
                    Direction::Outgoing => {
                        let tgt_id = graph.inner[edge_ref.target()]
                        (current_id, tgt_id, "outgoing")
                    }
                    Direction::Incoming => {
                        let src_id = graph.inner[edge_ref.source()]
                        (src_id, current_id, "incoming")
                    }
                }
                let neighbor_id = if petgraph_dir == Direction::Outgoing { tgt_node_id } else { src_node_id }

                // Determine the effective neighbor ID (with optional supersession resolution)
                let effective_neighbor_id = if resolve_supersessions {
                    // follow_to_current uses store.read_pool() (ADR-001, not in-memory graph)
                    // Returns None on 50-hop cap or orphaned deprecated — use original ID (ADR-005, R-10)
                    follow_to_current(store, neighbor_id).await.unwrap_or(neighbor_id)
                } else {
                    neighbor_id
                }

                // BFS visited set keyed by node_id only (AC-11a, R-18)
                // First encounter at shallowest depth wins; later encounters via longer paths are skipped
                if !visited.contains(&effective_neighbor_id) {
                    visited.insert(effective_neighbor_id)
                    let hop_depth = current_depth + 1

                    // Record the edge using the effective_neighbor_id if resolved
                    let (record_src, record_tgt) = if petgraph_dir == Direction::Outgoing {
                        (current_id, effective_neighbor_id)
                    } else {
                        (effective_neighbor_id, current_id)
                    }

                    result_edges.push(EdgeRecord {
                        source_id: record_src,
                        target_id: record_tgt,
                        relation_type: rel_type.as_str().to_string(),
                        direction: edge_dir_str.to_string(),
                        depth: hop_depth,
                        metadata: None,
                    })

                    // Enqueue for further expansion only if not at max depth
                    if hop_depth < depth {
                        if let Some(neighbor_node_idx) = graph.node_index_for(effective_neighbor_id) {
                            frontier.push_back((neighbor_node_idx, effective_neighbor_id, hop_depth))
                        }
                        // If node_index_for returns None: effective_neighbor_id resolved to an entry
                        // not in the current tick's graph. BFS stops there — no error, no warning (tracing::warn optional)
                    }
                }
                // If already visited: skip (shallowest depth wins — node_id keying invariant)
            }
        }
    }
}

// Drop graph_guard here — lock released when guard drops at end of function
Ok(result_edges)
```

**BFS visited set invariant**: `HashSet<u64>` keyed by `node_id` only. Each node
appears at most once in `result_edges`, at its minimum hop depth. If the same node
is reachable at depth 1 via type A AND at depth 2 via type B through an intermediate,
it appears at depth 1 (first encounter). Do NOT key on `(node_id, depth)` — that
produces duplicates (AC-11a, R-18).

---

### `follow_to_current` — Supersession Resolution Helper (ADR-005)

```
async fn follow_to_current(store: &Store, id: u64) -> Option<u64>
```

**Body**:

```
// 50-hop safety cap enforced by loop bound (not CTE — this is a store-layer helper)
// Uses store.read_pool() via store.get() — NOT the in-memory graph (ADR-001 consistency)
let mut current = id
for _ in 0..50 {
    let entry = match store.get(current).await {
        Ok(e) => e,
        Err(_) => return None,   // Store error — treat as unresolvable; caller uses original id
    }
    match entry.superseded_by {
        None => {
            // Check status: if Active, this is a valid terminal
            if entry.status == Status::Active {
                return Some(current)
            } else {
                // Orphaned deprecated terminal (superseded_by IS NULL, status != Active)
                // No valid substitution — return None (R-10 edge case)
                return None
            }
        }
        Some(next_id) => current = next_id,
    }
}
// Loop exhausted: chain exceeds 50 hops — return None (caller uses original id, no error)
None
```

**Caller behavior when None returned**: use the original `neighbor_id` without
substitution (ADR-005). This means deprecated endpoints may appear in the result
when resolve_supersessions=true fails to resolve. No error is surfaced to the MCP
caller (R-10 acceptance).

---

## State Machines

### handle_neighbors dispatch state machine

```
params.depth
  == 1  →  handle_neighbors_sql  (live DB, always fresh)
  >  1  →  handle_neighbors_bfs  (in-memory graph, tick-window staleness)
  == 0  →  validation error before dispatch
  > 10  →  validation error before dispatch
```

### BFS node lifecycle

```
Node enters frontier at depth D
  → edges_of_type called per (rel_type, petgraph_dir) combination
  → for each edge's neighbor:
      if neighbor not in visited:
        insert into visited
        append EdgeRecord at depth D+1
        if D+1 < max_depth: enqueue neighbor at D+1
      else: skip (shallowest-depth invariant)
  → node complete; never re-enqueued
```

---

## Initialization Sequence

No constructor or initialization needed. `graph_read.rs` is a pure function module.
The `TypedRelationGraph` passed via `typed_graph_state` is initialized by the background
tick service. `graph_read.rs` only reads it (shared read lock, never writes).

---

## Error Handling

| Error condition | Handling | Function |
|----------------|---------|---------|
| Unknown mode | `validate_no_unsupported_params` returns Err → rmcp INVALID_PARAMS | handle_graph |
| Missing `id` field | Early Err before mode dispatch | handle_graph |
| Forward-compat field on wrong mode | `validate_no_unsupported_params` → Err | handle_graph |
| resolve_supersessions=true on chain | `validate_no_unsupported_params` → Err | handle_graph |
| Invalid direction for chain mode | Validate in handle_graph before calling handle_chain | handle_graph |
| Invalid direction for neighbors mode | rmcp INVALID_PARAMS in handle_neighbors | handle_neighbors |
| depth out of 1..=10 range | rmcp INVALID_PARAMS | handle_neighbors |
| Supersedes in edge_types | rmcp INVALID_PARAMS with exact error string | handle_neighbors |
| Unknown edge type string | rmcp INVALID_PARAMS before any traversal | handle_neighbors |
| Non-existent id (chain mode) | Empty ChainResult, no error (AC-04) | handle_chain |
| Non-existent id (current mode) | Err("No active terminal found for entry {id}") (AC-05a) | handle_current |
| Orphaned deprecated terminal | Same error as non-existent id (R-20) | handle_current |
| Chain > 50 hops (current) | Err("No active terminal found for entry {id}") | handle_current |
| Non-existent id (neighbors) | Empty NeighborsResponse, no error (OQ-01 resolved) | handle_neighbors |
| follow_to_current returns None | Use original id, no error propagated (ADR-005, R-10) | handle_neighbors_bfs |
| Store error in query_supersession_chain | Log + empty ChainResult for chain; Err for current | handle_chain/current |
| Store error in query_direct_neighbors | rmcp INTERNAL_ERROR | handle_neighbors_sql |

---

## Key Test Scenarios

### graph_read unit tests

1. `validate_no_unsupported_params` with mode="chain", seed_ids=Some([1]) → Err containing "seed_ids" and "chain"
2. `validate_no_unsupported_params` with mode="chain", resolve_supersessions=Some(true) → Err containing "resolve_supersessions" (AC-15c, R-08)
3. `validate_no_unsupported_params` with mode="neighbors", from_id=Some(1) → Err containing "from_id" (AC-15b)
4. `validate_no_unsupported_params` with mode="walk" (unrecognized) → Err containing "unrecognized mode" (R-04)
5. `validate_no_unsupported_params` with mode="subgraph", seed_ids=Some([1]) → Err containing "unrecognized mode", NOT "seed_ids" (R-04 — unrecognized mode fires before field checks)
6. handle_graph with mode="chain", non-existent id → ChainResult with empty entries, no error (AC-04)
7. handle_graph with mode="current", non-existent id → error response (AC-05a)
8. handle_graph with mode="current", id of orphaned deprecated entry → error response (R-20)
9. handle_neighbors with depth=0 → INVALID_PARAMS error (R-11)
10. handle_neighbors with depth=11 → INVALID_PARAMS error (R-11)
11. handle_neighbors with edge_types=["Supersedes"] → INVALID_PARAMS with exact error string (AC-15a, R-06)
12. handle_neighbors with edge_types=["BogusEdge"] → INVALID_PARAMS (AC-15)
13. handle_neighbors with direction="forward" → INVALID_PARAMS (neighbors uses incoming/outgoing/both) (R-17)
14. BFS with diamond graph (X→Z direct, X→Y→Z two-hop): Z appears once at depth=1 (AC-11a, R-18)
15. follow_to_current with orphaned deprecated entry → None (R-10)
16. follow_to_current with 51-hop chain → None (50-hop cap)
17. node_index_for with known id → Some(NodeIndex); unknown id → None

### Integration tests (infra-001 Python suite)

18. Full chain traversal: 5-entry chain, seed=C → all 5 entries returned ordered oldest to newest (AC-01)
19. Directional filtering: direction="forward" from C → C, D, E (AC-02)
20. Truncation: 60-hop chain → truncated.forward=true (AC-03)
21. Per-direction truncation: 55 forward hops, 3 backward hops → truncated={forward:true, backward:false} (AC-03b)
22. current on active entry → same entry returned (AC-05)
23. current on deprecated entry with valid chain → active terminal returned (AC-06)
24. current on orphaned deprecated → error response (AC-06b / R-20 scenario 1)
25. neighbors outgoing depth=1 → correct edges (AC-08)
26. neighbors incoming depth=1 → correct edges (AC-09)
27. neighbors edge_types=[] → all non-Supersedes returned, no extra fields in response JSON (AC-10, AC-10a)
28. neighbors depth=2 returns depth-1 and depth-2 edges with correct depth field (AC-11)
29. resolve_supersessions=true substitutes deprecated endpoints (AC-12)
30. resolve_supersessions=false returns raw edges (AC-13)
31. Write edge then depth=1 query → edge appears; write then immediate depth=2 query → edge ABSENT (R-03 staleness test — comment must state this is expected behavior)
32. AC-04/AC-05a asymmetry pair: chain on id=999999 → empty; current on id=999999 → error (R-21)
