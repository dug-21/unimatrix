# Pseudocode: graph_read_path.rs (Wave 2)
# New file: crates/unimatrix-server/src/mcp/graph_read_path.rs

## Purpose

`handle_path` finds the shortest outgoing-edge path between two entries using BFS over
the in-memory `TypedRelationGraph`. Tick-window staleness applies (same contract as
neighbors depth>1 and subgraph modes). The handler validates parameters, optionally
resolves deprecated endpoints via `follow_to_current` before BFS, acquires a graph
snapshot (read lock → clone → release), and runs a path-carrying BFS.

---

## Imports

```
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use petgraph::Direction as PetgraphDirection;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;
use rmcp::model::ErrorData;
use unimatrix_core::Store;
use unimatrix_engine::graph::RelationType;

use crate::error::ERROR_INVALID_PARAMS;
use crate::services::typed_graph::TypedGraphState;
use super::graph_read_neighbors::{all_non_supersedes_types, follow_to_current};
use super::{GraphParams, PathHop, PathResponse};
```

---

## Constants

```
const DEFAULT_DEPTH: u8 = 5;
const MAX_DEPTH_UPPER: u8 = 10;
```

---

## Entry Point

```
pub(super) async fn handle_path(
    store: &Store,
    typed_graph_state: &Arc<RwLock<TypedGraphState>>,
    params: &GraphParams,
) -> Result<PathResponse, ErrorData>
```

### Step 1: Validate from_id and to_id (both required)

```
let from_id: u64 = match params.from_id {
    Some(id) => id,
    None => return Err(ErrorData::new(
        ERROR_INVALID_PARAMS,
        "path mode requires from_id",
        None,
    )),
};

let to_id: u64 = match params.to_id {
    Some(id) => id,
    None => return Err(ErrorData::new(
        ERROR_INVALID_PARAMS,
        "path mode requires to_id",
        None,
    )),
};
```

### Step 2: Self-path guard (FR-18a, AC-32)

Before any resolution: if from_id == to_id, return not-found immediately. A self-path
is not a meaningful traversal — BFS never fires the destination check on the seed node,
so this is consistent with BFS behavior.

```
if from_id == to_id {
    return Ok(PathResponse {
        found: false,
        from_id,
        to_id,
        hops: vec![],
        length: 0,
    });
}
```

### Step 3: Validate depth (default 5, range [1, 10])

```
let max_depth: u8 = match params.depth {
    None => DEFAULT_DEPTH,
    Some(d) if d >= 1 && d <= MAX_DEPTH_UPPER => d,
    Some(d) => return Err(ErrorData::new(
        ERROR_INVALID_PARAMS,
        format!("depth must be in range 1..=10, got {d}"),
        None,
    )),
};
```

### Step 4: Validate edge_types (optional, default = all non-Supersedes)

```
let edge_types: Vec<RelationType> = match &params.edge_types {
    None => all_non_supersedes_types(),
    Some(v) if v.is_empty() => all_non_supersedes_types(),
    Some(types) => parse_relation_types(types)?,
    // parse_relation_types: same as graph_read_inverse.md — validates via RelationType::from_str
};
```

### Step 5: Resolve supersessions (endpoint resolution before BFS — ADR-006)

```
let resolve_supersessions = params.resolve_supersessions.unwrap_or(false);

let effective_from: u64 = if resolve_supersessions {
    // follow_to_current returns None if the chain is broken (orphaned deprecated terminal).
    // Fallback to original ID (same as subgraph and neighbors modes — ADR-006 §Consequences).
    follow_to_current(store, from_id).await.unwrap_or(from_id)
} else {
    from_id
};

let effective_to: u64 = if resolve_supersessions {
    follow_to_current(store, to_id).await.unwrap_or(to_id)
} else {
    to_id
};
```

Note: `follow_to_current` is `async` and hits the Store. These two calls are sequential
(not parallel) because they are independent and simple. They happen BEFORE the graph
lock is acquired (lock discipline: lock is acquired after all async Store calls).

### Step 6: Acquire graph snapshot (lock → clone → release)

Same pattern as `graph_read_subgraph.rs` (line 147-149) and `graph_read_neighbors.rs`.
The `RwLock` is `std::sync::RwLock` (not tokio). Poison recovery via `unwrap_or_else`.

```
// Acquire read lock, clone the graph, release the lock BEFORE any BFS async work.
let graph = {
    let state = typed_graph_state.read().unwrap_or_else(|e| e.into_inner());
    state.typed_graph.clone()
};
// Lock is now released. BFS operates on the cloned snapshot.
```

### Step 7: Not-in-snapshot guard for effective_from

```
let from_idx: NodeIndex = match graph.node_index_for(effective_from) {
    Some(idx) => idx,
    None => {
        // from_id (or its resolved successor) is not in the current graph snapshot.
        // This is not an error — return found: false (AC-15, FR-13).
        return Ok(PathResponse {
            found: false,
            from_id: effective_from,
            to_id: effective_to,
            hops: vec![],
            length: 0,
        });
    }
};
```

### Step 8: Not-in-snapshot guard for effective_to

```
let to_idx: NodeIndex = match graph.node_index_for(effective_to) {
    Some(idx) => idx,
    None => {
        // to_id (or its resolved successor) is not in the current graph snapshot.
        // Not an error — return found: false (AC-15, FR-13).
        return Ok(PathResponse {
            found: false,
            from_id: effective_from,
            to_id: effective_to,
            hops: vec![],
            length: 0,
        });
    }
};
```

### Step 9: Path-carrying BFS

BFS uses a path-carrying frontier: each frontier entry carries the full path-so-far to
enable path reconstruction on first arrival at the target (no back-pointer table needed
at these graph scales — ARCHITECTURE.md §Memory bound).

Direction: Outgoing only (FR-15). No `direction` parameter on path mode.

CRITICAL (R-03): The visited set is keyed on the RESOLVED node ID (effective neighbor
after `follow_to_current`), not the raw/deprecated ID. This prevents double-enqueue
when multiple deprecated nodes resolve to the same terminal successor.

```
// frontier: each entry is (current_node_idx, path_hops_so_far, current_depth)
// path_hops_so_far: Vec<PathHop> representing the path FROM from_id TO current_node
let mut frontier: VecDeque<(NodeIndex, Vec<PathHop>, u8)> = VecDeque::new();

// visited: keyed on RESOLVED node id (effective_id after follow_to_current) -- R-03.
let mut visited: HashSet<u64> = HashSet::new();

// Seed the frontier with the start node. from_id itself is NOT in hops (ADR-005).
visited.insert(effective_from);
frontier.push_back((from_idx, vec![], 0));

'bfs: while let Some((current_idx, path_so_far, current_depth)) = frontier.pop_front() {
    if current_depth >= max_depth {
        continue;  // depth limit exhausted — do not expand further
    }

    for &rel_type in &edge_types {
        // Collect outgoing edges eagerly to avoid borrow conflicts across async calls
        // (same pattern as graph_read_subgraph.rs, ADR-008).
        let outgoing_pairs: Vec<(NodeIndex, NodeIndex)> = graph
            .edges_of_type(current_idx, rel_type, PetgraphDirection::Outgoing)
            .map(|e| (e.source(), e.target()))
            .collect();

        for (_, tgt_idx) in outgoing_pairs {
            // Resolve raw target ID from NodeIndex.
            let raw_neighbor_id: u64 = match graph.node_id_for_index(tgt_idx) {
                Some(id) => id,
                None => continue,  // stale index — skip
            };

            // Per-hop supersession resolution (ADR-006, follows graph_read_subgraph pattern).
            let effective_neighbor: u64 = if resolve_supersessions {
                follow_to_current(store, raw_neighbor_id)
                    .await
                    .unwrap_or(raw_neighbor_id)
            } else {
                raw_neighbor_id
            };

            // Build the hop that led to effective_neighbor.
            let hop = PathHop {
                entry_id: effective_neighbor,
                relation_type: rel_type.as_str().to_string(),
            };

            // Check if we have reached the destination.
            // Compare effective_neighbor ID against effective_to ID.
            if effective_neighbor == effective_to {
                // Path found! Build and return the complete path.
                let mut full_path = path_so_far.clone();
                full_path.push(hop);
                let length = full_path.len() as u8;
                return Ok(PathResponse {
                    found: true,
                    from_id: effective_from,
                    to_id: effective_to,
                    hops: full_path,
                    length,
                });
            }

            // Not the destination: enqueue if not yet visited.
            // CRITICAL: visited check uses effective_neighbor (resolved ID) — not raw (R-03).
            if !visited.contains(&effective_neighbor) {
                visited.insert(effective_neighbor);  // mark BEFORE enqueue to prevent races

                // Resolve effective_neighbor back to a NodeIndex for BFS continuation.
                // If effective_neighbor is not in the graph snapshot, skip (not an error).
                if let Some(neighbor_idx) = graph.node_index_for(effective_neighbor) {
                    let mut new_path = path_so_far.clone();
                    new_path.push(hop);
                    frontier.push_back((neighbor_idx, new_path, current_depth + 1));
                }
                // If effective_neighbor is not in snapshot: still mark visited (prevents
                // re-enqueue from another path), but do not add to frontier.
            }
        }
    }
}
// Frontier exhausted without finding target.
```

### Step 10: No path found

```
Ok(PathResponse {
    found: false,
    from_id: effective_from,
    to_id: effective_to,
    hops: vec![],
    length: 0,
})
```

---

## BFS Invariants

| Invariant | Enforcement |
|-----------|-------------|
| Outgoing edges only | `PetgraphDirection::Outgoing` only — no Incoming |
| Visited set keyed on resolved ID | `visited.contains(&effective_neighbor)` — R-03 |
| Lock released before async BFS | graph cloned before BFS loop starts |
| depth limit respected | `if current_depth >= max_depth { continue }` |
| from_id not in hops | frontier seeded with `vec![]` — first hop added when neighbor found |
| length == hops.len() | `full_path.len() as u8` used directly |
| Cycles handled | visited set prevents re-enqueue — BFS terminates even on cyclic graphs (SR-C) |

---

## Helper Functions

### parse_relation_types (module-level private)

Same implementation as in `graph_read_inverse.md`. Validates each element via
`RelationType::from_str`, returns `Err(ErrorData)` on failure listing all 16 types.

---

## State Machines / Lifecycle

No persistent state. This is a stateless request-response handler.

Lock discipline: `std::sync::RwLock` acquired exactly once (Step 6), graph cloned,
lock dropped (end of block). All subsequent BFS operations (including async
`follow_to_current` calls) operate on the owned clone — no lock is held during async.

---

## Error Handling

| Error Condition | Error Type | Message |
|-----------------|-----------|---------|
| `from_id` absent | `ERROR_INVALID_PARAMS` | "path mode requires from_id" |
| `to_id` absent | `ERROR_INVALID_PARAMS` | "path mode requires to_id" |
| `from_id == to_id` | — (not an error) | `PathResponse { found: false, hops: [], length: 0 }` |
| `depth` out of range [1, 10] | `ERROR_INVALID_PARAMS` | "depth must be in range 1..=10, got {d}" |
| Unrecognized element in `edge_types` | `ERROR_INVALID_PARAMS` | "unrecognized edge type '{x}' — recognized types: ..." |
| `from_id` not in graph snapshot | — (not an error) | `PathResponse { found: false }` |
| `to_id` not in graph snapshot | — (not an error) | `PathResponse { found: false }` |
| No path within `depth` hops | — (not an error) | `PathResponse { found: false }` |
| `follow_to_current` returns `None` | — (not an error) | falls back to original ID |
| RwLock poisoned | — (not an error) | `unwrap_or_else(|e| e.into_inner())` — no panic |

No error arm returns `ErrorData` for the not-in-snapshot or no-path cases. Both are
`Ok(PathResponse { found: false })` (FR-13, AC-14, AC-15). This signature constraint
(`Result<PathResponse, ErrorData>` — NOT an infallible `PathResponse`) means the
not-found path is covered by a real code path, not a silent panic (R-09, pattern #4497).

---

## Key Test Scenarios

- AC-13 (infra-001): Known typed-edge chain A→B→C; assert `hops=[{B,type},{C,type}]`,
  `from_id=A` not in hops, `length=2`.
- AC-14 (infra-001): Disconnected entries; assert `{ found: false, hops: [], length: 0 }`.
- AC-15: from_id not in graph snapshot (inject graph without from_id) → `found: false`,
  NOT an `ErrorData` return.
- AC-16: from_id absent → exact error "path mode requires from_id".
- AC-17: to_id absent → exact error "path mode requires to_id".
- AC-18: depth default=5 when absent; depth=0 and depth=11 → validation errors.
- AC-20 (infra-001): resolve_supersessions=true — deprecated from_id resolved to
  successor; response.from_id is successor's ID.
- AC-21 (infra-001): resolve_supersessions=false — deprecated from_id used as-is;
  response.from_id is original deprecated ID.
- AC-31 (infra-001): Typed-edge chain; assert hops sequence, no null relation_types.
- AC-32: from_id == to_id → `{ found: false, hops: [], length: 0 }` (not an error).
- R-03: Graph with two deprecated nodes A_dep, B_dep both superseded by C_active;
  from_id→A_dep and from_id→B_dep edges; resolve_supersessions=true;
  assert C_active appears exactly once in hops.
- R-06: to_id resolution — deprecated to_id resolved to active successor; response.to_id
  reflects resolved ID.
- R-09 (distinct fixtures): AC-14 (no path) and AC-15 (not-in-snapshot) must use
  separate test fixtures — not a single test that accidentally covers both.
- R-12: 1-hop path A→B — hops.len()=1, from_id=A not in hops, length=1.
- SR-C: Cyclic graph A→B→C→A with unreachable D; BFS terminates at depth limit
  with `found: false` (no infinite loop).
- IR-02: Graph with 5 consecutive deprecated intermediaries; BFS completes within
  depth budget; Store read count bounded by `2 + N_deprecated_hops`.
