# Architecture: crt-014 — Topology-Aware Supersession

## System Overview

crt-014 introduces `petgraph` into `unimatrix-engine` and builds a directed supersession DAG at query time. It replaces two hardcoded scalar penalty constants (`DEPRECATED_PENALTY`, `SUPERSEDED_PENALTY`) with a topology-derived penalty function, and upgrades single-hop successor injection in `search.rs` to full multi-hop traversal following chains to their terminal active node.

This feature is Phase 1 of the three-phase Graph Enablement milestone. It lays the graph infrastructure that crt-017 (Contradiction Cluster Detection) will extend.

```
unimatrix-store (SQLite — supersedes/superseded_by fields)
       ↓ load_all_entries_for_graph()
unimatrix-engine/src/graph.rs  (petgraph DAG + graph_penalty + find_terminal_active)
       ↓ graph_penalty(id, &graph) → f64
       ↓ find_terminal_active(id, &graph) → Option<u64>
unimatrix-server/src/services/search.rs  (penalty_map + successor injection)
```

---

## Component Breakdown

### Component 1: `unimatrix-engine/src/graph.rs` (NEW)

Responsibility: Build the supersession DAG, compute topology-derived penalties, and find terminal active successors.

Public API:
```rust
// Error type for cycle detection
#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("supersession cycle detected")]
    CycleDetected,
}

// Opaque wrapper over StableGraph — field is pub(crate) for test access
pub struct SupersessionGraph {
    pub(crate) inner: StableGraph<u64, ()>,
    pub(crate) node_index: HashMap<u64, NodeIndex>,
}

// Penalty constants (named, fixed for v1)
pub const ORPHAN_PENALTY: f64 = 0.75;
pub const CLEAN_REPLACEMENT_PENALTY: f64 = 0.40;
pub const HOP_DECAY_FACTOR: f64 = 0.60;
pub const PARTIAL_SUPERSESSION_PENALTY: f64 = 0.60;
pub const DEAD_END_PENALTY: f64 = 0.65;
pub const FALLBACK_PENALTY: f64 = 0.70;  // used on CycleError in search.rs
pub const MAX_TRAVERSAL_DEPTH: usize = 10;

/// Build directed supersession DAG from a slice of all entries.
/// Edges: if entry.supersedes == Some(pred_id), add edge pred_id → entry.id
/// Returns Err(GraphError::CycleDetected) if is_cyclic_directed().
pub fn build_supersession_graph(entries: &[EntryRecord]) -> Result<SupersessionGraph, GraphError>;

/// Topology-derived penalty for a node.
/// Returns 1.0 (no penalty) for nodes not in the graph.
/// Returns a value in (0.0, 1.0) for nodes that are deprecated or superseded.
pub fn graph_penalty(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> f64;

/// Follow directed edges from node_id to find the first Active, non-superseded node.
/// DFS, depth-capped at MAX_TRAVERSAL_DEPTH.
/// Returns None if no active terminal reachable.
pub fn find_terminal_active(
    node_id: u64,
    graph: &SupersessionGraph,
    entries: &[EntryRecord],
) -> Option<u64>;
```

Internal design of `build_supersession_graph`:
1. Insert one `StableGraph` node per entry; store `NodeIndex` in `node_index: HashMap<u64, NodeIndex>`
2. For each entry with `supersedes: Some(pred_id)`: add directed edge `node_index[pred_id] → node_index[entry.id]`
3. Dangling references (pred_id not in entries) are skipped with a `tracing::warn!`
4. Call `petgraph::algo::is_cyclic_directed(&inner)` — if true, return `Err(GraphError::CycleDetected)`

Internal design of `graph_penalty`:
1. Look up `node_id` in `node_index` — if absent, return `1.0`
2. Look up the entry in the `entries` slice by `id`
3. Compute signals:
   - `is_orphan`: status == Deprecated AND no outgoing edges
   - `successor_count`: outgoing edge count from this node
   - `active_reachable`: DFS following outgoing edges, depth-capped, returns bool (found Active + non-superseded)
   - `chain_depth`: BFS distance to nearest Active terminal (0 = is_terminal, None = no path)
4. Penalty derivation (in order of match priority):
   - `is_orphan` → `ORPHAN_PENALTY` (0.75)
   - `!active_reachable` → `DEAD_END_PENALTY` (0.65)
   - `successor_count > 1` (partial supersession) → `PARTIAL_SUPERSESSION_PENALTY` (0.60)
   - `chain_depth == Some(1)` → `CLEAN_REPLACEMENT_PENALTY` (0.40)
   - `chain_depth == Some(d) where d >= 2` → `CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR^(d-1)` clamped to `[0.10, CLEAN_REPLACEMENT_PENALTY]`
   - None of the above → `DEAD_END_PENALTY` (defensive fallback)

Internal design of `find_terminal_active`:
- Iterative DFS from `node_id`, following outgoing edges
- At each visited node: check if entry is Active and `superseded_by.is_none()` — if so, return `Some(id)`
- Depth limit: `MAX_TRAVERSAL_DEPTH` (10), return `None` if exceeded
- Return `None` if graph is exhausted without finding terminal

### Component 2: `unimatrix-engine/src/lib.rs` (MODIFIED)

Add `pub mod graph;` export.

### Component 3: `unimatrix-engine/Cargo.toml` (MODIFIED)

Add:
```toml
petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }
```

Also add `thiserror` if not already present (for `GraphError`).

### Component 4: `unimatrix-server/src/services/search.rs` (MODIFIED)

Replace penalty constant usage with graph-derived calls. Changes:

1. **Remove import**: `use crate::confidence::{..., DEPRECATED_PENALTY, SUPERSEDED_PENALTY, ...}`
2. **Add import**: `use unimatrix_engine::graph::{build_supersession_graph, graph_penalty, find_terminal_active, GraphError, FALLBACK_PENALTY};`
3. **Before Step 6a (penalty marking)**: Load all entries for graph construction
   - Call `Store::query(QueryFilter::default())` via `spawn_blocking` to get all entries
   - Call `build_supersession_graph(&all_entries)`:
     - `Ok(graph)` → use graph for penalty and traversal
     - `Err(GraphError::CycleDetected)` → `tracing::error!("supersession cycle detected — falling back to constant penalties")`, set `use_fallback = true`
4. **Step 6a (penalty marking in Flexible mode)**: Replace constant assignment:
   ```rust
   // Old:
   penalty_map.insert(entry.id, SUPERSEDED_PENALTY);
   penalty_map.insert(entry.id, DEPRECATED_PENALTY);
   // New:
   let penalty = if use_fallback {
       FALLBACK_PENALTY
   } else {
       graph_penalty(entry.id, &graph, &all_entries)
   };
   penalty_map.insert(entry.id, penalty);
   ```
   Condition for penalty: `entry.superseded_by.is_some() || entry.status == Status::Deprecated` (same guard as before, now unified)
5. **Step 6b (successor injection)**: Replace single-hop with multi-hop:
   ```rust
   // Old (single-hop, ADR-003):
   let successor_ids: Vec<u64> = results.iter()
       .filter_map(|(e, _)| e.superseded_by)
       .collect();
   // ... inject if status==Active && superseded_by.is_none()

   // New (multi-hop via graph):
   for (entry, _) in &candidate_superseded {
       let terminal = if use_fallback {
           entry.superseded_by  // fall back to single-hop
       } else {
           find_terminal_active(entry.id, &graph, &all_entries)
       };
       // inject terminal if Some and not already in results
   }
   ```

### Component 5: `unimatrix-engine/src/confidence.rs` (MODIFIED)

Remove:
- `pub const DEPRECATED_PENALTY: f64 = 0.7;`
- `pub const SUPERSEDED_PENALTY: f64 = 0.5;`
- Tests `deprecated_penalty_value`, `superseded_penalty_value`, `superseded_penalty_harsher_than_deprecated`, `penalties_independent_of_confidence_formula`

These are replaced by behavioral assertions in `graph.rs` unit tests.

---

## Component Interactions

```
search.rs::search()
  │
  ├─ Store::query(QueryFilter::default())         → Vec<EntryRecord> (all entries)
  │     [spawn_blocking, before Step 6a]
  │
  ├─ graph::build_supersession_graph(&all_entries) → Result<SupersessionGraph, GraphError>
  │     [sync, in spawn_blocking or inline]
  │
  ├─ [Step 6a] graph::graph_penalty(id, &graph, &all_entries) → f64
  │     [for each deprecated/superseded entry in results]
  │
  └─ [Step 6b] graph::find_terminal_active(id, &graph, &all_entries) → Option<u64>
        [for each superseded entry needing successor injection]
```

The full-store read for graph construction must be synchronous (SQLite is `Mutex<Connection>`). It fits inside the existing `spawn_blocking` pattern used throughout the search pipeline. The graph construction call should be added to an existing or new `spawn_blocking` block before Step 6 — not as a separate task.

---

## Technology Decisions

See ADR files:
- `ADR-001-petgraph-stable-graph-only.md` — petgraph feature restriction
- `ADR-002-per-query-graph-rebuild.md` — Option A (per-query, no cache)
- `ADR-003-supersede-prior-adr-003.md` — supersedes system ADR-003 (single-hop limit)
- `ADR-004-supersede-prior-adr-005.md` — supersedes system ADR-005 (hardcoded penalties)
- `ADR-005-cycle-fallback-strategy.md` — cycle detection fallback behavior
- `ADR-006-graph-penalty-constants.md` — named constants, fixed for v1

---

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `build_supersession_graph` | `fn(&[EntryRecord]) -> Result<SupersessionGraph, GraphError>` | `unimatrix-engine/src/graph.rs` |
| `graph_penalty` | `fn(u64, &SupersessionGraph, &[EntryRecord]) -> f64` | `unimatrix-engine/src/graph.rs` |
| `find_terminal_active` | `fn(u64, &SupersessionGraph, &[EntryRecord]) -> Option<u64>` | `unimatrix-engine/src/graph.rs` |
| `GraphError::CycleDetected` | `enum GraphError` | `unimatrix-engine/src/graph.rs` |
| `FALLBACK_PENALTY` | `pub const f64 = 0.70` | `unimatrix-engine/src/graph.rs` |
| `ORPHAN_PENALTY` | `pub const f64 = 0.75` | `unimatrix-engine/src/graph.rs` |
| `CLEAN_REPLACEMENT_PENALTY` | `pub const f64 = 0.40` | `unimatrix-engine/src/graph.rs` |
| `HOP_DECAY_FACTOR` | `pub const f64 = 0.60` | `unimatrix-engine/src/graph.rs` |
| `PARTIAL_SUPERSESSION_PENALTY` | `pub const f64 = 0.60` | `unimatrix-engine/src/graph.rs` |
| `DEAD_END_PENALTY` | `pub const f64 = 0.65` | `unimatrix-engine/src/graph.rs` |
| `MAX_TRAVERSAL_DEPTH` | `pub const usize = 10` | `unimatrix-engine/src/graph.rs` |
| `Store::query(QueryFilter::default())` | `fn(QueryFilter) -> Result<Vec<EntryRecord>>` | `unimatrix-store/src/read.rs:282` |
| `EntryRecord.supersedes` | `Option<u64>` | `unimatrix-store/src/schema.rs:67` |
| `EntryRecord.superseded_by` | `Option<u64>` | `unimatrix-store/src/schema.rs:69` |
| `DEPRECATED_PENALTY` (REMOVED) | was `pub const f64 = 0.7` | `unimatrix-engine/src/confidence.rs` |
| `SUPERSEDED_PENALTY` (REMOVED) | was `pub const f64 = 0.5` | `unimatrix-engine/src/confidence.rs` |

---

## crt-017 Forward Compatibility

`SupersessionGraph` is an opaque struct wrapping `StableGraph<u64, ()>`. crt-017 will add contradiction edges as a second edge type (likely `StableGraph<u64, EdgeKind>` where `EdgeKind` is an enum). The `graph.rs` public API is designed to allow this:
- `SupersessionGraph` is a named type (not a type alias) — can be extended
- Edge type `()` is the unit type; crt-017 will change this to an enum
- `build_supersession_graph` returns `SupersessionGraph` by value — crt-017 can add a parallel `build_contradiction_graph` or extend this function with additional edge types

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `build_supersession_graph` returns `CycleDetected` | Log `tracing::error!`, set `use_fallback = true`, apply `FALLBACK_PENALTY` to all penalized entries, use single-hop fallback for injection |
| Dangling `supersedes` reference (pred_id not in entries) | `tracing::warn!`, skip edge, continue graph construction |
| Entry not in graph (node_id not in node_index) | `graph_penalty` returns `1.0` (no penalty) |
| `find_terminal_active` hits MAX_TRAVERSAL_DEPTH | Return `None`, log warn |
| `Store::query` fails | Propagate existing `ServiceError`; search fails as before |

---

## Affected Files

| File | Change |
|------|--------|
| `crates/unimatrix-engine/Cargo.toml` | Add petgraph dep |
| `crates/unimatrix-engine/src/graph.rs` | NEW — full graph module |
| `crates/unimatrix-engine/src/lib.rs` | Add `pub mod graph;` |
| `crates/unimatrix-engine/src/confidence.rs` | Remove penalty constants + 4 tests |
| `crates/unimatrix-server/src/services/search.rs` | Replace constants + single-hop with graph calls |
| `product/features/crt-014/architecture/ADR-*.md` | 6 new ADR files |
