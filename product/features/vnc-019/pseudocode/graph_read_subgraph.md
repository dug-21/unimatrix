# Pseudocode: `graph_read_subgraph.rs` (new file)

## Purpose

New file: `crates/unimatrix-server/src/mcp/graph_read_subgraph.rs`

Contains the full `subgraph` mode implementation: parameter validation, BFS traversal
using the in-memory `TypedRelationGraph`, edge deduplication, `max_nodes` cap enforcement,
dangling-edge filtering, batch node hydration, post-BFS metadata batch query, and
`SubgraphResponse` construction.

Declared as a `#[path]`-submodule of `graph_read.rs` (ADR-002):

```rust
// In graph_read.rs:
#[path = "graph_read_subgraph.rs"]
mod graph_read_subgraph;
```

Tests live in `graph_read_subgraph_tests.rs`, declared inside this file:

```rust
// At bottom of graph_read_subgraph.rs:
#[cfg(test)]
#[path = "graph_read_subgraph_tests.rs"]
mod tests;
```

---

## Imports

```
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use petgraph::stable_graph::NodeIndex;
use petgraph::Direction;
use rmcp::model::ErrorData;
use sqlx::Row;
use unimatrix_core::Store;
use unimatrix_engine::graph::{RelationType, TypedRelationGraph};

use crate::error::{ERROR_INTERNAL, ERROR_INVALID_PARAMS};
use crate::services::typed_graph::TypedGraphState;

use super::{EdgeRecord, GraphParams, SubgraphResponse};
use super::graph_read_neighbors::{all_non_supersedes_types, follow_to_current};
```

Note: `follow_to_current` must be `pub(super)` in `graph_read_neighbors.rs` for this
import to compile. This is the first delivery action (ARCHITECTURE.md §3).

---

## Function: `handle_subgraph`

```
pub(super) async fn handle_subgraph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> Result<SubgraphResponse, ErrorData>
```

### Step 1: Parameter Validation

Validate in this order (match the spec's error message strings exactly):

```
// 1a. seed_ids — must be present and non-empty (FR-03)
let seed_ids: Vec<u64> = match &params.seed_ids {
    None | Some([]) => {
        return Err(ErrorData::new(
            ERROR_INVALID_PARAMS,
            "subgraph mode requires at least one entry ID in seed_ids",
            None,
        ));
    }
    Some(ids) if ids.is_empty() => { /* same error — handled above by pattern */ }
    Some(ids) => ids.clone(),
};

// 1b. max_depth — default 3, range [1, 10] (FR-06, ADR-001)
let max_depth: u8 = params.max_depth.unwrap_or(3);
if max_depth == 0 || max_depth > 10 {
    return Err(ErrorData::new(
        ERROR_INVALID_PARAMS,
        format!("max_depth must be in range 1..=10, got {max_depth}"),
        None,
    ));
}

// 1c. max_nodes — default 200, hard cap 200, must be in [1, 200] (FR-07)
// Values above 200 are rejected (not clamped) — consistent with max_depth pattern.
let max_nodes: usize = match params.max_nodes {
    None => 200,
    Some(n) if n == 0 || n > 200 => {
        return Err(ErrorData::new(
            ERROR_INVALID_PARAMS,
            format!("max_nodes must be in range 1..=200, got {n}"),
            None,
        ));
    }
    Some(n) => n as usize,
};

// 1d. edge_types — validate each via RelationType::from_str; default to all 15 non-Supersedes (FR-04)
// Note: unlike neighbors mode, subgraph DOES allow "Supersedes" if explicitly requested.
// (Specification FR-04: "Callers who want supersession-chain edges must request Supersedes explicitly.")
let requested_types: Vec<RelationType> = match &params.edge_types {
    None => all_non_supersedes_types(),
    Some(type_strings) if type_strings.is_empty() => all_non_supersedes_types(),
    Some(type_strings) => {
        let mut resolved = Vec::new();
        for s in type_strings {
            match RelationType::from_str(s) {
                Some(rt) => resolved.push(rt),
                None => {
                    return Err(ErrorData::new(
                        ERROR_INVALID_PARAMS,
                        format!(
                            "unrecognized edge_type '{s}' — recognized types: \
                             Advances, Asserts, About, Cites, CoAccess, Contradicts, \
                             DerivedFrom, Informs, Mentions, Motivates, Prerequisite, \
                             Refutes, RelatedTo, Supersedes, Supports, Tests"
                        ),
                        None,
                    ));
                }
            }
        }
        resolved
    }
};

// 1e. direction — default "both" (FR-05)
// Subgraph uses petgraph Direction; neighbors mode used NeighborDirection.
// Parse to a petgraph dirs slice here for clarity in BFS.
enum SubgraphDir { Outgoing, Incoming, Both }
let direction = match params.direction.as_deref().unwrap_or("both") {
    "outgoing" => SubgraphDir::Outgoing,
    "incoming" => SubgraphDir::Incoming,
    "both"     => SubgraphDir::Both,
    other => {
        return Err(ErrorData::new(
            ERROR_INVALID_PARAMS,
            "direction must be one of: incoming, outgoing, both",
            None,
        ));
    }
};

let resolve_supersessions = params.resolve_supersessions.unwrap_or(false);
```

### Step 2: Acquire Graph (Lock Discipline)

```
// Acquire std::sync::RwLock (not tokio) — same pattern as neighbors_bfs.
// Clone the graph out from under the lock; release lock before any async work.
// Poison recovery: unwrap_or_else(|e| e.into_inner()) (established pattern).
let graph: TypedRelationGraph = {
    let guard = typed_graph_state.read().unwrap_or_else(|e| e.into_inner());
    guard.typed_graph.clone()
};
// Lock is released here. All subsequent work is lock-free.
```

### Step 3: Initialize BFS State

```
let mut visited: HashSet<u64> = HashSet::new();
let mut frontier: VecDeque<(NodeIndex, u64, u8)> = VecDeque::new();
// collected_edges: (source_id, target_id, relation_type_str, depth)
let mut collected_edges: Vec<(u64, u64, String, u8)> = Vec::new();
let mut collected_node_ids: Vec<u64> = Vec::new();
// edge_set: dedup by canonical triple (source_id, target_id, relation_type_str)
let mut edge_set: HashSet<(u64, u64, String)> = HashSet::new();
let mut truncated = false;
```

### Step 4: Seed Phase

```
// Process seeds before BFS begins. Seeds count toward max_nodes cap.
// R-01: supersession substitution BEFORE visited check.
// R-03: if seeds alone fill the cap, BFS is skipped with truncated=true.
for seed_id in &seed_ids {
    let seed_id = *seed_id;

    // Resolve supersession for seed if requested (R-01 applies to seeds too)
    let effective_seed = if resolve_supersessions {
        follow_to_current(store, seed_id).await.unwrap_or(seed_id)
    } else {
        seed_id
    };

    // Skip if already visited (handles duplicate seed_ids)
    if visited.contains(&effective_seed) {
        continue;
    }

    // Cap check before inserting
    if collected_node_ids.len() >= max_nodes {
        truncated = true;
        break;
    }

    visited.insert(effective_seed);
    collected_node_ids.push(effective_seed);

    // Only push to frontier if this seed is present in the in-memory graph.
    // Seeds absent from graph contribute to nodes (via batch hydration) but
    // have no BFS expansion — empty result is not an error (FR-15, AC-17).
    if let Some(seed_idx) = graph.node_index_for(effective_seed) {
        frontier.push_back((seed_idx, effective_seed, 0u8));
    }
    // If seed not in graph: node appears in collected_node_ids (hydrated later),
    // but frontier gets nothing — that seed contributes 0 edges.
}

// R-03: if seeds alone saturated the cap, skip BFS entirely.
// truncated is already true from the loop above.
// depth_reached will compute to 0 (no edges collected yet).
```

### Step 5: BFS Phase

```
// BFS over in-memory TypedRelationGraph.
// Edge dedup keyed on canonical (source_id, target_id, rel_type_str) — R-02.
// visited keyed on effective_id (post-substitution) — R-01.

'bfs: while let Some((current_idx, current_id, current_depth)) = frontier.pop_front() {
    // Depth guard: do not expand nodes at or beyond max_depth
    if current_depth >= max_depth {
        continue;
    }

    for rel_type in &requested_types {
        // Determine which petgraph directions to iterate
        let petgraph_dirs: &[Direction] = match direction {
            SubgraphDir::Outgoing => &[Direction::Outgoing],
            SubgraphDir::Incoming => &[Direction::Incoming],
            SubgraphDir::Both    => &[Direction::Outgoing, Direction::Incoming],
        };

        for &petgraph_dir in petgraph_dirs {
            // Collect edge references before async work (same pattern as neighbors_bfs)
            let neighbors: Vec<(u64, u64, u64)> = graph
                .edges_of_type(current_idx, *rel_type, petgraph_dir)
                .filter_map(|e| {
                    let neighbor_idx = match petgraph_dir {
                        Direction::Outgoing => e.target(),
                        Direction::Incoming => e.source(),
                    };
                    let neighbor_id = graph.node_id_for_index(neighbor_idx)?;

                    // Canonical edge direction: always (source → target) as stored.
                    // R-02: build edge_key from stored direction, NOT from petgraph_dir.
                    let (edge_src, edge_tgt) = if petgraph_dir == Direction::Outgoing {
                        (current_id, neighbor_id)
                    } else {
                        // petgraph_dir == Incoming: we traversed B←A from B's perspective.
                        // The canonical stored edge is A→B, so source=neighbor, target=current.
                        (neighbor_id, current_id)
                    };

                    Some((neighbor_id, edge_src, edge_tgt))
                })
                .collect();

            for (neighbor_id, edge_src, edge_tgt) in neighbors {
                let rel_type_str = rel_type.as_str().to_string();
                let edge_key = (edge_src, edge_tgt, rel_type_str.clone());

                // Edge dedup: each canonical triple appears at most once (R-02, FR-12).
                // First discovery wins (BFS processes frontier FIFO → shallowest depth first).
                if !edge_set.contains(&edge_key) {
                    edge_set.insert(edge_key);
                    let edge_depth = current_depth + 1;
                    collected_edges.push((edge_src, edge_tgt, rel_type_str.clone(), edge_depth));
                }

                // Supersession substitution BEFORE visited check (R-01).
                let effective_id = if resolve_supersessions {
                    // follow_to_current is async; must NOT hold the RwLock during this call.
                    // Lock was released in Step 2 — safe.
                    // unwrap_or(neighbor_id): if chain broken or 50-hop exceeded, use original (R-13).
                    follow_to_current(store, neighbor_id).await.unwrap_or(neighbor_id)
                } else {
                    neighbor_id
                };

                // Node dedup and cap check
                if !visited.contains(&effective_id) {
                    if collected_node_ids.len() >= max_nodes {
                        truncated = true;
                        break 'bfs;  // labeled break exits all nested loops (see note below)
                    }
                    visited.insert(effective_id);
                    collected_node_ids.push(effective_id);

                    let hop_depth = current_depth + 1;
                    // Only enqueue for further expansion if not at max depth
                    if hop_depth < max_depth {
                        if let Some(neighbor_node_idx) = graph.node_index_for(effective_id) {
                            frontier.push_back((neighbor_node_idx, effective_id, hop_depth));
                        }
                        // node_index_for returns None: node not in current graph — no expansion,
                        // node still appears in collected_node_ids for hydration (FR-15).
                    }
                }
                // Already visited: skip. Shallowest-depth discovery wins.
            }
        }
    }
}

// Implementation note on labeled break:
// The 'bfs label on the while-let loop enables breaking out of all nested loops
// (for rel_type / for petgraph_dir / for neighbor) with a single `break 'bfs`.
// Alternative: boolean `truncated` flag checked at top of while-let loop body.
// Choose ONE approach. Mixing both creates dead code. Labeled break is preferred
// for clarity — it is the explicit exit point documented in ARCHITECTURE.md §4.
```

### Step 5b: Dangling-Edge Filter (POST_BFS — required correctness step)

```
// When the cap fires mid-hop, edges pointing to the truncated neighbor are already
// in collected_edges, but that neighbor is NOT in collected_node_ids.
// Without this filter, the response contains EdgeRecords whose target_id references
// a node absent from nodes — agents cannot reconstruct the graph.
// This is a correctness invariant, not an optimization (ARCHITECTURE.md §5b, FR-14).

let node_id_set: HashSet<u64> = collected_node_ids.iter().copied().collect();

collected_edges.retain(|(src, tgt, _, _)| {
    node_id_set.contains(src) && node_id_set.contains(tgt)
});

// Complexity: O(edges). Bounded by max_nodes=200 cap → ~600 edges max at depth 3.
```

### Step 6: Batch Node Hydration

```
// Single batch query for all collected node IDs (FR-13, NFR-02).
// No per-node individual queries — not an N+1 pattern.
// store.get_many returns Vec<EntryRecord> for matched IDs; absent IDs are silently
// omitted (R-09: graceful partial hydration, no panic).
let nodes: Vec<EntryRecord> = store.get_many(&collected_node_ids).await.map_err(|e| {
    tracing::error!(error = %e, "batch node hydration failed in handle_subgraph");
    ErrorData::new(ERROR_INTERNAL, format!("node hydration failed: {e}"), None)
})?;
```

Note on `store.get_many` signature: verify the actual Store API name and signature
against the vnc-018 implementation. The architecture references `Store::get_many` or
equivalent batch query function. The implementor must use whatever batch function
exists in `unimatrix-store` at delivery time, not invent a new one.

### Step 7: Post-BFS Metadata Batch Query (ADR-003)

```
// R-04: MUST skip entirely when collected_edges is empty.
// An empty WHERE clause in the dynamically built SQL is a syntax error or full-table scan.

let mut metadata_map: HashMap<(u64, u64, String), Option<serde_json::Value>> = HashMap::new();

if !collected_edges.is_empty() {
    // Build OR-chain SQL dynamically (ADR-003, ARCHITECTURE.md §Post-BFS Metadata SQL)
    // SQLite does not support tuple-IN syntax, so each triple is a separate OR clause.
    // Bind parameter numbering: ?1/?2/?3 for first clause, ?4/?5/?6 for second, etc.
    //
    // Template (expand per collected edge):
    // SELECT source_id, target_id, relation_type, metadata
    // FROM graph_edges
    // WHERE (source_id = ?1 AND target_id = ?2 AND relation_type = ?3)
    //    OR (source_id = ?4 AND target_id = ?5 AND relation_type = ?6)
    //    ...

    let mut where_clauses: Vec<String> = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();  // alternating src, tgt, rel_type strings

    for (i, (src, tgt, rel_type_str, _depth)) in collected_edges.iter().enumerate() {
        let base = i * 3 + 1;  // 1-indexed bind params
        where_clauses.push(format!(
            "(source_id = ?{} AND target_id = ?{} AND relation_type = ?{})",
            base, base + 1, base + 2
        ));
        // Bind values tracked separately for sqlx binding loop below
        // (actual binding uses sqlx query builder or bind() chaining)
        let _ = (src, tgt, rel_type_str);  // placeholder; see binding note
    }

    let sql = format!(
        "SELECT source_id, target_id, relation_type, metadata FROM graph_edges WHERE {}",
        where_clauses.join(" OR ")
    );

    // Binding: sqlx does not support runtime-variable bind counts natively.
    // Approach: use sqlx::query (not query!) and chain .bind() calls in a loop.
    // The implementor must use the same bind-in-a-loop pattern established in the codebase.
    // Example skeleton:
    //
    //   let mut q = sqlx::query(&sql);
    //   for (src, tgt, rel_type_str, _) in &collected_edges {
    //       q = q.bind(*src as i64).bind(*tgt as i64).bind(rel_type_str);
    //   }
    //   let rows = q.fetch_all(store.read_pool_server()).await.map_err(...)?;
    //
    // Use store.read_pool_server() — same pool used by query_direct_neighbors.

    // row processing:
    for row in rows {
        let src: i64 = row.get("source_id");
        let tgt: i64 = row.get("target_id");
        let rel: String = row.get("relation_type");
        let meta_text: Option<String> = row.get("metadata");

        // SEC-05: serde_json::from_str(...).ok() → None on malformed JSON (not panic).
        let meta_value: Option<serde_json::Value> = meta_text
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok());

        metadata_map.insert((src as u64, tgt as u64, rel), meta_value);
    }
}
```

Binding implementation detail: The exact sqlx bind-chaining pattern must follow
whatever approach is established in the codebase (see `query_direct_neighbors` in
`unimatrix-store` for reference). The pseudocode above shows the structural intent;
the implementor resolves the exact sqlx API call.

### Step 8: Assemble EdgeRecords

```
// Build Vec<EdgeRecord> from collected_edges, populating metadata from metadata_map.
// direction is always "outgoing" for all EdgeRecords in subgraph mode (FR-12, ADR-004 vnc-018).
let edges: Vec<EdgeRecord> = collected_edges
    .into_iter()
    .map(|(src, tgt, rel_type_str, depth)| {
        let metadata = metadata_map
            .get(&(src, tgt, rel_type_str.clone()))
            .cloned()
            .flatten();  // Option<Option<V>> → Option<V>
        EdgeRecord {
            source_id: src,
            target_id: tgt,
            relation_type: rel_type_str,
            direction: "outgoing".to_string(),  // ALWAYS "outgoing" — canonical direction
            depth,
            metadata,
        }
    })
    .collect();
```

### Step 9: Compute `depth_reached`

```
// depth_reached = max depth across all collected edges (FR-16).
// When no edges collected (isolated seeds, cold-start, or truncated at seed phase):
// depth_reached = 0.
// R-08: reflects actual traversal depth, not the requested max_depth.
let depth_reached: u8 = edges.iter().map(|e| e.depth).max().unwrap_or(0);
```

### Step 10: Return

```
Ok(SubgraphResponse {
    nodes,
    edges,
    truncated,
    seed_ids,   // echo of original input (not effective_ids after substitution)
    depth_reached,
})
```

---

## Full Function Structure (Summary)

```
pub(super) async fn handle_subgraph(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> Result<SubgraphResponse, ErrorData> {
    // 1. Validate params (seed_ids, max_depth, max_nodes, edge_types, direction)
    // 2. Clone graph from RwLock; release lock
    // 3. Init BFS state (visited, frontier, collected_edges, collected_node_ids, edge_set)
    // 4. Seed phase (supersession resolution, cap check, visited insert, frontier push)
    // 5. BFS loop 'bfs (per-hop edge enumeration, edge dedup, cap enforcement, node enqueue)
    // 5b. Dangling-edge filter (retain only edges with both endpoints in collected_node_ids)
    // 6. Batch node hydration via store.get_many
    // 7. Post-BFS metadata batch query (OR-chain SQL, skipped if collected_edges empty)
    // 8. Assemble EdgeRecords with metadata
    // 9. Compute depth_reached
    // 10. Return SubgraphResponse
}
```

---

## Error Handling

| Condition | Error Returned |
|-----------|---------------|
| `seed_ids` absent or empty | `ERROR_INVALID_PARAMS`, exact message from brief |
| `max_depth` out of range | `ERROR_INVALID_PARAMS`, `"max_depth must be in range 1..=10, got {depth}"` |
| `max_nodes` > 200 or == 0 | `ERROR_INVALID_PARAMS`, `"max_nodes must be in range 1..=200, got {value}"` |
| Unknown `edge_type` string | `ERROR_INVALID_PARAMS`, names bad value + lists all 16 |
| Invalid `direction` | `ERROR_INVALID_PARAMS`, `"direction must be one of: incoming, outgoing, both"` |
| Batch node hydration fails | `ERROR_INTERNAL`, propagated from store |
| OR-chain SQL fails | `ERROR_INTERNAL`, propagated from sqlx |
| Malformed `metadata` JSON | `None` (silent, not an error — SEC-05) |
| TypedRelationGraph cold (empty) | Empty `SubgraphResponse`, not an error |
| Seed ID absent from graph | Seed included in nodes (hydrated), no BFS expansion — not an error |

---

## Key Test Scenarios (from RISK-TEST-STRATEGY.md)

**R-01 (Critical): Supersession substitution ordering**
- Graph: A → B(deprecated, superseded_by C) → C. Call with seed=[A], resolve_supersessions=true.
  Assert: nodes contains A and C; B absent; C exactly once.
- Graph: A → C; D → C (C reachable via two paths). Call with seed=[A, D], resolve_supersessions=true.
  Assert: C appears exactly once (visited set keyed on terminal ID prevents double-enqueue).
- Same graph with resolve_supersessions=false. Assert: B present in nodes.

**R-02 (Critical): direction="both" edge dedup and canonical direction**
- Graph: A Supports B (single stored edge). Call with seed=[A, B], direction="both".
  Assert: edges has exactly one record, source_id=A, target_id=B, direction="outgoing".
- Call again. Assert: len(edges)==1 (no duplicate from bidirectional traversal).
- All returned EdgeRecords have direction="outgoing" regardless of traversal perspective.

**R-03 (Critical): Seed count at max_nodes boundary**
- 201 seed IDs (all in graph), default max_nodes=200.
  Assert: nodes exactly 200; truncated=true; depth_reached=0.
- Exactly 200 seeds. Assert: truncated=true; depth_reached=0; BFS skipped.
- 1 seed + dense graph + max_nodes=5. Assert: nodes len <= 5; truncated=true.

**R-04 (High): Empty-edges OR-chain guard**
- Seed with no edges of requested type. Assert: edges=[]; no SQL error; metadata query skipped.
- All seeds absent from graph (cold-start). Assert: empty SubgraphResponse; no error.
- Seed + edge with non-null metadata. Assert: EdgeRecord.metadata is populated JSON value.

**R-06 (High): Circular supersession chain**
- A.superseded_by=B.id, B.superseded_by=A.id. Call with resolve_supersessions=true, seed=[A].
  Assert: returns within timeout; no panic; result uses fallback (original ID).
- Chain of exactly 50 hops. Assert: follow_to_current terminates; BFS completes.

**R-07 (High): max_nodes > 200 rejected**
- max_nodes=201 → validation error `"max_nodes must be in range 1..=200, got 201"`.
- max_nodes=0 → validation error.
- max_nodes=200 → accepted; response nodes.len() <= 200.

**R-08 (High): depth_reached accuracy**
- Linear chain A→B→C→D, max_depth=10. Assert: depth_reached=3.
- Same chain, max_nodes=2. Assert: truncated=true; depth_reached=1.
- Isolated seed, no edges. Assert: depth_reached=0.

**R-15 (Med): Malformed metadata JSON**
- Edge with metadata='invalid json{'. Assert: EdgeRecord.metadata=None; call succeeds.
- Edge with metadata=NULL. Assert: EdgeRecord.metadata=JSON null.
- Edge with metadata='{"key":"value"}'. Assert: EdgeRecord.metadata is parsed JSON object.

**AC-19: No metadata query on empty edge set**
- Isolated seed (exists in graph, has no edges). Assert: metadata SQL not issued;
  response has nodes=[seed], edges=[], depth_reached=0.
