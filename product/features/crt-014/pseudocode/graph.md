# Pseudocode: graph.rs (NEW)

**File**: `crates/unimatrix-engine/src/graph.rs`
**Crate**: `unimatrix-engine`
**Change type**: NEW

---

## Purpose

Builds a directed supersession DAG from entry metadata at query time. Provides:
- `build_supersession_graph` — constructs the DAG and detects cycles
- `graph_penalty` — derives a topology-informed penalty multiplier per entry
- `find_terminal_active` — traverses directed edges to find the terminal active successor
- Seven named penalty constants replacing the former two scalar constants in `confidence.rs`

This module has no async functions. All computation is synchronous and pure (graph_penalty, find_terminal_active). Callers (search.rs) wrap in `spawn_blocking`.

---

## Imports Required

```
use std::collections::HashMap;
use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::Direction;
use petgraph::algo::is_cyclic_directed;
use unimatrix_core::{EntryRecord, Status};
```

---

## Constants

```
// All pub const — declared at module top level.

/// Deprecated entry with no successors — softest penalty (orphan, not replaceable).
pub const ORPHAN_PENALTY: f64 = 0.75;

/// Superseded entry with exactly one active terminal at depth 1 — cleanly replaced.
pub const CLEAN_REPLACEMENT_PENALTY: f64 = 0.40;

/// Multiplier applied per additional hop beyond depth 1.
pub const HOP_DECAY_FACTOR: f64 = 0.60;

/// Superseded entry with more than one direct successor — ambiguous replacement.
pub const PARTIAL_SUPERSESSION_PENALTY: f64 = 0.60;

/// Entry with successors but no active terminal reachable — chain leads nowhere.
pub const DEAD_END_PENALTY: f64 = 0.65;

/// Flat fallback used by search.rs when CycleDetected prevents graph construction.
pub const FALLBACK_PENALTY: f64 = 0.70;

/// Maximum DFS depth for find_terminal_active. Chains beyond this return None.
pub const MAX_TRAVERSAL_DEPTH: usize = 10;
```

---

## Error Type

```
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("supersession cycle detected")]
    CycleDetected,
}
```

---

## SupersessionGraph Struct

```
pub struct SupersessionGraph {
    /// Directed petgraph StableGraph. Node weight = entry id (u64). Edge weight = () (unit).
    /// Direction: edge A → B means "B supersedes A" (B is the successor of A).
    pub(crate) inner: StableGraph<u64, ()>,

    /// Maps entry id → NodeIndex for O(1) lookup.
    pub(crate) node_index: HashMap<u64, NodeIndex>,
}
```

The `pub(crate)` visibility allows unit tests within the crate to inspect `inner` directly (R-04 edge direction test scenario).

---

## Function: build_supersession_graph

```
pub fn build_supersession_graph(
    entries: &[EntryRecord],
) -> Result<SupersessionGraph, GraphError>
```

### Algorithm

```
FUNCTION build_supersession_graph(entries):
    graph = SupersessionGraph {
        inner: StableGraph::new(),    // directed by default
        node_index: HashMap::new(),
    }

    // Pass 1: Add one node per entry
    FOR EACH entry IN entries:
        idx = graph.inner.add_node(entry.id)
        graph.node_index.insert(entry.id, idx)

    // Pass 2: Add directed edges for supersession relationships
    FOR EACH entry IN entries:
        IF entry.supersedes == Some(pred_id):
            IF pred_id NOT IN graph.node_index:
                // Dangling reference — predecessor not in entries slice
                tracing::warn!(
                    entry_id = entry.id,
                    missing_pred_id = pred_id,
                    "build_supersession_graph: dangling supersedes reference, skipping edge"
                )
                CONTINUE  // skip this edge, do not error

            pred_idx = graph.node_index[pred_id]
            succ_idx = graph.node_index[entry.id]
            // Edge direction: predecessor → successor (outgoing = toward newer knowledge)
            graph.inner.add_edge(pred_idx, succ_idx, ())

    // Pass 3: Cycle detection
    IF is_cyclic_directed(&graph.inner):
        RETURN Err(GraphError::CycleDetected)

    RETURN Ok(graph)
```

### Notes

- Two-pass approach: all nodes added before any edges, so edge-add never panics on missing NodeIndex (dangling refs are explicitly checked against the populated map).
- `StableGraph` is used (not `Graph`) for crt-017 forward compatibility — node indices remain stable even if nodes are removed in future use.
- `petgraph::algo::is_cyclic_directed` operates on the directed graph as-is. No additional setup required.
- Empty entry slice: returns `Ok` with zero nodes, zero edges — valid empty DAG.
- Self-referential entry (entry.supersedes == Some(entry.id)): passes node_index lookup (self is present), adds a self-loop, which `is_cyclic_directed` detects as a cycle → `Err(CycleDetected)`.

---

## Function: graph_penalty

```
pub fn graph_penalty(
    node_id: u64,
    graph: &SupersessionGraph,
    entries: &[EntryRecord],
) -> f64
```

### Algorithm

```
FUNCTION graph_penalty(node_id, graph, entries):
    // Guard: node not in graph → no penalty (Active entries not in graph return 1.0)
    IF node_id NOT IN graph.node_index:
        RETURN 1.0

    node_idx = graph.node_index[node_id]

    // Lookup entry record for this node
    entry = find_entry_by_id(node_id, entries)
    IF entry IS None:
        // node_id is in graph but not in entries slice — defensive, should not happen
        RETURN 1.0

    // -- Compute topology signals --

    // Signal 1: is_orphan
    // Definition: status == Deprecated AND zero outgoing edges
    outgoing_count = graph.inner.edges_directed(node_idx, Direction::Outgoing).count()
    is_orphan = (entry.status == Status::Deprecated) AND (outgoing_count == 0)

    // Signal 2: successor_count (direct outgoing edges)
    successor_count = outgoing_count

    // Signal 3: active_reachable
    // DFS following outgoing edges; returns true if any reachable node is
    // Active AND superseded_by.is_none()
    active_reachable = dfs_active_reachable(node_idx, graph, entries)

    // Signal 4: chain_depth
    // BFS distance to nearest Active terminal (0 = current node is terminal; None = unreachable)
    chain_depth = bfs_chain_depth(node_idx, graph, entries)

    // -- Priority-ordered penalty derivation --

    // Priority 1: Orphan deprecated — no successors, no path forward
    IF is_orphan:
        RETURN ORPHAN_PENALTY  // 0.75

    // Priority 2: No active terminal reachable — dead end despite having successors
    IF NOT active_reachable:
        RETURN DEAD_END_PENALTY  // 0.65

    // Priority 3: Partial supersession — multiple direct successors (ambiguous)
    IF successor_count > 1:
        RETURN PARTIAL_SUPERSESSION_PENALTY  // 0.60

    // Priority 4: Clean replacement, depth 1 — one active successor, one hop away
    IF chain_depth == Some(1):
        RETURN CLEAN_REPLACEMENT_PENALTY  // 0.40

    // Priority 5: Clean replacement, depth >= 2 — decay per additional hop
    IF chain_depth == Some(d) AND d >= 2:
        raw = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR.pow(d - 1)
        RETURN clamp(raw, 0.10, CLEAN_REPLACEMENT_PENALTY)

    // Priority 6: Defensive fallback — should not be reached in valid data
    RETURN DEAD_END_PENALTY
```

### Helper: dfs_active_reachable

```
FUNCTION dfs_active_reachable(start_idx, graph, entries) -> bool:
    // Iterative DFS following outgoing edges from start_idx
    // Does NOT start from start_idx itself — checks successors only
    stack = [start_idx]
    visited = HashSet::new()

    WHILE stack NOT EMPTY:
        current_idx = stack.pop()
        IF current_idx IN visited:
            CONTINUE
        visited.insert(current_idx)

        FOR EACH neighbor_idx IN graph.inner.neighbors_directed(current_idx, Direction::Outgoing):
            neighbor_id = graph.inner[neighbor_idx]
            entry = find_entry_by_id(neighbor_id, entries)
            IF entry IS Some(e):
                IF e.status == Status::Active AND e.superseded_by.is_none():
                    RETURN true
            stack.push(neighbor_idx)

    RETURN false
```

### Helper: bfs_chain_depth

```
FUNCTION bfs_chain_depth(start_idx, graph, entries) -> Option<usize>:
    // BFS to find shortest hop distance to nearest Active, non-superseded node.
    // Depth 0 would mean start_idx itself is the terminal — but graph_penalty
    // is only called on entries that need penalizing, so depth 0 is not expected.
    // Returns None if no active terminal reachable or depth exceeds MAX_TRAVERSAL_DEPTH.
    queue = VecDeque::new()
    visited = HashSet::new()
    queue.push_back((start_idx, 0usize))
    visited.insert(start_idx)

    WHILE queue NOT EMPTY:
        (current_idx, depth) = queue.pop_front()

        IF depth > MAX_TRAVERSAL_DEPTH:
            CONTINUE  // skip — do not add neighbors beyond depth cap

        FOR EACH neighbor_idx IN graph.inner.neighbors_directed(current_idx, Direction::Outgoing):
            IF neighbor_idx IN visited:
                CONTINUE
            visited.insert(neighbor_idx)
            next_depth = depth + 1

            neighbor_id = graph.inner[neighbor_idx]
            entry = find_entry_by_id(neighbor_id, entries)
            IF entry IS Some(e):
                IF e.status == Status::Active AND e.superseded_by.is_none():
                    RETURN Some(next_depth)
            queue.push_back((neighbor_idx, next_depth))

    RETURN None
```

### Notes

- `graph_penalty` is a pure function: no I/O, no side effects, deterministic (NFR-02).
- The `dfs_active_reachable` and `bfs_chain_depth` helpers are private to this module.
- `find_entry_by_id` is a shared private helper (see below).
- Clamp formula: `CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR.pow(d - 1)` uses f64 `powi` or `powf`. Since `d` is `usize`, cast to `i32` for `powi`: `HOP_DECAY_FACTOR.powi((d - 1) as i32)`. This avoids f64 log domain issues.
- Depth-5 example: `0.40 * 0.60^4 = 0.40 * 0.1296 = 0.05184` → clamped to `0.10`.
- Depth-10 example: `0.40 * 0.60^9 ≈ 0.40 * 0.010078 ≈ 0.004031` → clamped to `0.10`.

---

## Function: find_terminal_active

```
pub fn find_terminal_active(
    node_id: u64,
    graph: &SupersessionGraph,
    entries: &[EntryRecord],
) -> Option<u64>
```

### Algorithm

```
FUNCTION find_terminal_active(node_id, graph, entries):
    // Guard: node not in graph
    IF node_id NOT IN graph.node_index:
        RETURN None

    start_idx = graph.node_index[node_id]

    // Iterative DFS following outgoing edges
    // Uses explicit depth tracking to enforce MAX_TRAVERSAL_DEPTH
    stack = [(start_idx, 0usize)]  // (node_index, depth_from_start)
    visited = HashSet::new()
    visited.insert(start_idx)

    WHILE stack NOT EMPTY:
        (current_idx, depth) = stack.pop()

        // Check current node (including starting node itself, depth 0)
        current_id = graph.inner[current_idx]
        entry = find_entry_by_id(current_id, entries)
        IF entry IS Some(e):
            IF e.status == Status::Active AND e.superseded_by.is_none():
                RETURN Some(current_id)

        // Do not traverse beyond MAX_TRAVERSAL_DEPTH
        IF depth >= MAX_TRAVERSAL_DEPTH:
            CONTINUE

        FOR EACH neighbor_idx IN graph.inner.neighbors_directed(current_idx, Direction::Outgoing):
            IF neighbor_idx NOT IN visited:
                visited.insert(neighbor_idx)
                stack.push((neighbor_idx, depth + 1))

    RETURN None
```

### Notes

- Iterative DFS — no recursion, no stack overflow risk on pathological chains (R-07).
- The starting node itself is checked at depth 0. This handles the edge case where `node_id` is already an Active, non-superseded entry — though the search pipeline only calls this for superseded entries.
- Depth is counted from `start_idx`. Neighbors of `start_idx` are at depth 1. Neighbors of those are at depth 2. The depth cap `>= MAX_TRAVERSAL_DEPTH` prevents traversal at depth 10, so the last traversable depth is 9, meaning maximum reachable node is at depth 10 from start.

  Wait — re-read the spec: "Depth-capped at MAX_TRAVERSAL_DEPTH (10)". AC-11 says "chain of 11 entries → None". This means a chain A(0)→B(1)→…→K(10) should return None, and a chain of 10 entries where J is at depth 9 should succeed.

  Correction: depth cap check should be `IF depth >= MAX_TRAVERSAL_DEPTH: CONTINUE (do not push neighbors)`. This means nodes at depth MAX_TRAVERSAL_DEPTH are visited and checked, but their neighbors are not pushed. A node at depth 10 (MAX_TRAVERSAL_DEPTH) is checked. A chain of 11 where K is at depth 10 would be returned — but AC-11 requires None for 11 entries. Therefore the cap must be `depth > MAX_TRAVERSAL_DEPTH - 1` i.e., do not visit nodes at depth >= MAX_TRAVERSAL_DEPTH.

  Revised depth logic: push neighbors only if `depth + 1 < MAX_TRAVERSAL_DEPTH`. Implementation must verify the exact boundary by testing AC-11 (11-hop → None) and R-07 (exactly 10 hops → Some).

  Pseudocode correction:
  ```
  IF depth + 1 > MAX_TRAVERSAL_DEPTH:
      CONTINUE  // would exceed cap, do not push neighbors
  stack.push((neighbor_idx, depth + 1))
  ```

- `visited` set prevents revisiting nodes in case of (unexpected) cycles that somehow bypass the cycle check in `build_supersession_graph`. This is a defensive measure.
- Returns `None` if graph is exhausted without finding a terminal. No tracing::warn needed here — the caller decides how to handle `None` (skip injection).

---

## Private Helper: find_entry_by_id

```
FUNCTION find_entry_by_id(id: u64, entries: &[EntryRecord]) -> Option<&EntryRecord>:
    // Linear scan. Performance acceptable for expected slice sizes (≤1000 entries, NFR-01).
    RETURN entries.iter().find(|e| e.id == id)
```

This is a module-private function. Its O(n) cost is acceptable since `graph_penalty` and `bfs_chain_depth` already traverse graph edges (O(E)), and graph construction is already O(V+E). The full pipeline over all penalized candidates is O(candidates * avg_chain_depth * n_entries) — bounded by NFR-01 at 1,000 entries.

If profiling shows this is a bottleneck post-crt-014, the fix is an `id → &EntryRecord` HashMap built once by the caller before calling `graph_penalty` — but that optimization is out of scope for v1.

---

## State Machine: None

`graph.rs` has no lifecycle states. `SupersessionGraph` is built, used within a query, then dropped. There is no caching, no background state, and no persistent graph handle.

---

## Initialization Sequence: None

`SupersessionGraph` is not constructed via a constructor — it is returned by `build_supersession_graph`. No `new()` method. No config loading.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `entry.supersedes` references pred_id not in entries | `tracing::warn!` with entry_id and pred_id context, skip edge, continue |
| `is_cyclic_directed` returns true | Return `Err(GraphError::CycleDetected)` — caller applies fallback |
| `node_id` not in `node_index` (graph_penalty) | Return `1.0` (no penalty) |
| `node_id` not in `node_index` (find_terminal_active) | Return `None` |
| Entry in graph but not in entries slice | `find_entry_by_id` returns None; treated as non-terminal |
| DFS exceeds MAX_TRAVERSAL_DEPTH | Stop traversal in that branch; return `None` if no terminal found |
| Empty entries slice | Returns `Ok(graph)` with zero nodes — valid |

---

## Key Test Scenarios

These scenarios map directly to the Risk-Test Strategy. Implementation must cover all of them.

### Unit Tests (no store required — use EntryRecord values directly)

**Build + Cycle Detection (R-02, AC-03, AC-04)**
- `build_empty_graph`: entries = [], assert Ok, 0 nodes
- `build_single_entry_no_supersedes`: 1 entry, no supersedes, assert Ok, 1 node, 0 edges
- `build_valid_chain_depth1`: A supersedes B → B→A edge, assert Ok
- `build_valid_chain_depth3`: A←B←C chain, assert Ok
- `detect_two_node_cycle`: A.supersedes=B, B.supersedes=A → assert Err(CycleDetected)
- `detect_three_node_cycle`: A←B←C←A → assert Err(CycleDetected)
- `detect_self_loop`: A.supersedes=A → assert Err(CycleDetected)
- `dangling_supersedes_skipped`: B.supersedes=Some(9999), 9999 not in entries → assert Ok (R-09, AC-17)

**Edge Direction (R-04)**
- `edge_direction_correct`: B.supersedes=Some(A.id) → inspect graph.inner.edges_directed(A_idx, Outgoing) contains B_idx; assert find_terminal_active(A.id) returns Some(B.id) when B is Active

**graph_penalty Priority Branches (R-01, AC-05 through AC-08)**
- `penalty_orphan`: entry status=Deprecated, outgoing_count=0 → assert == ORPHAN_PENALTY
- `penalty_dead_end`: entry has outgoing edge to Deprecated terminal → assert == DEAD_END_PENALTY
- `penalty_partial_supersession`: entry has 2 outgoing edges both Active → assert == PARTIAL_SUPERSESSION_PENALTY
- `penalty_clean_replacement_depth1`: entry has 1 Active successor at depth 1 → assert == CLEAN_REPLACEMENT_PENALTY
- `penalty_clean_replacement_depth2`: depth-2 chain → assert value == 0.40*0.60 == 0.24
- `penalty_clamp_deep_chain`: depth-5 chain → assert == 0.10 (clamped)
- `penalty_absent_node`: node_id not in graph → assert == 1.0

**Behavioral Ordering (R-05 — replaces removed confidence.rs tests)**
- `orphan_softer_than_clean_replacement`: ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY (0.75 > 0.40)
- `two_hop_harsher_than_one_hop`: graph_penalty(A with depth-2 terminal) < graph_penalty(A with depth-1 terminal)
- `partial_supersession_softer_than_clean`: PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY (0.60 > 0.40)
- `dead_end_softer_than_orphan`: DEAD_END_PENALTY < ORPHAN_PENALTY (0.65 < 0.75) — note this ordering assertion
- `fallback_softer_than_clean`: FALLBACK_PENALTY > CLEAN_REPLACEMENT_PENALTY (0.70 > 0.40)

**find_terminal_active (R-03, AC-09 through AC-11)**
- `terminal_depth1`: A→B (B Active, superseded_by=None) → assert Some(B.id)
- `terminal_depth2`: A→B→C (B superseded, C Active) → assert Some(C.id)
- `terminal_skips_superseded_active`: A→B→C where B is Active but superseded_by=Some(C.id), C is Active superseded_by=None → assert Some(C.id) NOT Some(B.id) (R-03 scenario 4)
- `terminal_none_dead_end`: A→B where B is Deprecated → assert None
- `terminal_absent_node`: node_id not in graph → assert None
- `terminal_depth_cap_11`: chain of 11 entries → assert None (AC-11)
- `terminal_depth_cap_10`: chain of 10 entries where K at depth 10 is Active → assert Some(K.id)

**Hop Decay Formula (R-12)**
- `hop_decay_depth1`: assert == 0.40 exactly
- `hop_decay_depth2`: assert == 0.24 (0.40 * 0.60)
- `hop_decay_depth5`: assert == 0.10 (clamped from ~0.052)
- `hop_decay_depth10`: assert == 0.10 (clamped)
- `hop_decay_never_above_clean_replacement`: for d in 1..=10, assert result <= CLEAN_REPLACEMENT_PENALTY

**Edge Cases**
- `all_entries_active_no_penalty`: entries all Active, no supersedes → graph has nodes, no edges, penalty_map stays empty (tested at search.rs integration level)
- `node_id_zero`: graph_penalty(0, ...) where 0 not in graph → assert 1.0, no panic
