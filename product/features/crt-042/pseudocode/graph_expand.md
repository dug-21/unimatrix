# crt-042: graph_expand — Pseudocode

## Purpose

Pure BFS traversal of `TypedRelationGraph` that returns the set of entry IDs reachable from
a seed set within `depth` hops via positive edge types, excluding seeds themselves, capped at
`max_candidates`. Used by Phase 0 in search.rs to widen the PPR candidate pool.

---

## Module Declaration (graph.rs modifications)

Two lines are added to `crates/unimatrix-engine/src/graph.rs`, following the existing pattern
for `graph_suppression.rs` and `graph_ppr.rs` (lines 26–31 of graph.rs):

```
#[path = "graph_suppress.rs"]
mod graph_suppression;
pub use graph_suppression::suppress_contradicts;

#[path = "graph_ppr.rs"]
mod graph_ppr;
pub use graph_ppr::personalized_pagerank;

// NEW — add after the above:
#[path = "graph_expand.rs"]
mod graph_expand;
pub use graph_expand::graph_expand;
```

No other change to graph.rs. The function does NOT appear in lib.rs (same rule as graph_ppr.rs
per ADR-001 crt-030, entry #3731).

---

## File Header (graph_expand.rs)

```
//! Graph expansion via BFS — widen PPR candidate pool (crt-042).
//!
//! Declared as a submodule of `graph.rs` via `#[path = "graph_expand.rs"]`.
//! Re-exported from `graph.rs` as `pub use graph_expand::graph_expand`.
//! Does NOT appear in `lib.rs` (ADR-001, entry #3731).
//!
//! Structural mirrors: `graph_suppression.rs`, `graph_ppr.rs`.
//! - Pure function, no I/O, no async, no mutable global state.
//! - All traversal via `edges_of_type()` exclusively — no `.edges_directed()` calls (AC-16, SR-01).
//! - Tests live in `graph_expand_tests.rs` if inline tests push file over 500 lines (NFR-09).
//!
//! ## Caller Quarantine Obligation (FR-06)
//!
//! `graph_expand` is a pure function and performs NO quarantine checks. Any caller
//! that adds returned IDs to a result set (e.g., Phase 0 in search.rs) MUST
//! independently apply `SecurityGateway::is_quarantined()` before use. Future callers
//! outside search.rs must observe the same obligation — the function contract does not
//! include security enforcement.
//!
//! ## Traversal Contract (behavioral, entry #3754)
//!
//! Given seed B and edge B→A (Outgoing from B), entry A is reachable.
//! Given seed B and edge C→B (Incoming to B), entry C is NOT reachable.
//! Traversal is expressed behaviorally. No Direction:: constant is authoritative
//! in this specification (ADR-006).
```

---

## Imports

```
use std::collections::{HashSet, VecDeque};

use petgraph::Direction;
use petgraph::stable_graph::NodeIndex;
use petgraph::visit::EdgeRef;

use crate::graph::{RelationType, TypedRelationGraph};
```

---

## Function: `graph_expand`

### Signature

```rust
pub fn graph_expand(
    graph: &TypedRelationGraph,
    seed_ids: &[u64],
    depth: usize,
    max_candidates: usize,
) -> HashSet<u64>
```

### Pseudocode

```
FUNCTION graph_expand(graph, seed_ids, depth, max_candidates) -> HashSet<u64>:

  // Degenerate case guards (FR-03 / AC-10, AC-11, AC-12).
  IF seed_ids.is_empty()     → return empty HashSet
  IF graph.node_index.is_empty() → return empty HashSet
  IF depth == 0              → return empty HashSet

  // Initialize BFS state.
  // visited: tracks all entry IDs already considered — prevents revisiting (R-11).
  // Seeds are pre-inserted so they can never appear in the result (AC-08).
  // BFS queue carries (entry_id, current_hop_depth).
  LET visited: HashSet<u64> = HashSet from seed_ids
  LET result: HashSet<u64> = empty
  LET queue: VecDeque<(u64, usize)> = empty

  // Enqueue all seeds at hop 0.
  // Seeds whose entry_id is NOT in graph.node_index are silently skipped (no panic).
  FOR EACH seed_id IN seed_ids:
      IF graph.node_index.contains_key(seed_id):
          queue.push_back((seed_id, 0))

  // BFS loop.
  WHILE queue is not empty AND result.len() < max_candidates:

      // Pop from front (BFS order).
      LET (current_id, current_depth) = queue.pop_front()

      // Resolve NodeIndex for current entry ID.
      LET node_idx = match graph.node_index.get(current_id):
          None → continue  // defensive; should not happen if visited set is consistent
          Some(idx) → idx

      // Stop expanding from this node if we have already reached max depth.
      // (We still process the node itself, but don't enqueue its neighbors.)
      LET can_expand_further = current_depth < depth

      // Traverse all positive edge types from this node (Direction::Outgoing).
      // Four separate edges_of_type calls (SR-01: no .edges_directed() allowed, AC-16).
      // Process neighbors in SORTED NODE-ID ORDER for determinism (NFR-04, ADR-004 crt-030).
      //
      // Positive types: CoAccess, Supports, Informs, Prerequisite.
      // Excluded types: Supersedes (structural chain), Contradicts (negative signal).

      LET mut neighbors: Vec<u64> = []

      FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::CoAccess, Direction::Outgoing):
          neighbors.push(graph.inner[edge_ref.target()])

      FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::Supports, Direction::Outgoing):
          neighbors.push(graph.inner[edge_ref.target()])

      FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing):
          neighbors.push(graph.inner[edge_ref.target()])

      FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Outgoing):
          neighbors.push(graph.inner[edge_ref.target()])

      // Sort for deterministic frontier processing (NFR-04, C-09).
      neighbors.sort_unstable()
      neighbors.dedup()

      FOR EACH neighbor_id IN neighbors:
          IF result.len() >= max_candidates:
              // Early exit: budget reached. Stop processing this frontier and return.
              BREAK (outer WHILE loop — return result immediately)

          IF visited.contains(neighbor_id):
              continue  // Already queued or added (prevents cycles, R-11).

          visited.insert(neighbor_id)
          result.insert(neighbor_id)

          IF can_expand_further:
              // Enqueue for further expansion in subsequent BFS wave.
              queue.push_back((neighbor_id, current_depth + 1))

  RETURN result
```

### Implementation Notes

1. **Visited set initialised with all seeds** — seeds can never appear in `result` (AC-08),
   even when a graph path leads back to a seed from another seed.

2. **Sorted neighbor expansion** — `neighbors.sort_unstable()` is called on the local `Vec`
   before inserting into visited/result/queue. This creates deterministic frontier ordering at
   the intra-node level. Combined with BFS level-by-level ordering, the sorted-order constraint
   from NFR-04 is met without sorting the entire queue.

3. **Early exit correctness** — when `result.len() >= max_candidates`, we break immediately
   after adding the entry that hit the cap. Entries already in `queue` are discarded (no partial
   result beyond the cap). This matches AC-09.

4. **can_expand_further** — a node at `current_depth == depth` is added to `result` (if not
   visited) but does NOT enqueue its neighbors. This enforces the depth limit correctly: a seed
   at depth 0, its neighbors at depth 1, their neighbors at depth 2. With `depth=2`, depth-2
   nodes are collected but not expanded. With `depth=1`, only direct seed neighbors are
   collected (AC-05, AC-06).

5. **dedup after sort** — `neighbors.dedup()` removes duplicate target IDs that could arise
   from multiple parallel edges between the same pair of nodes (different edge types both
   pointing to the same target). Without dedup, the same neighbor would be inserted twice into
   the visited set and counted twice toward max_candidates.

6. **No direct `.edges_directed()` calls** — all four edge-type traversals use
   `graph.edges_of_type()` exclusively (SR-01 / AC-16 / entry #3627). This is the sole
   traversal boundary in unimatrix-engine; future RelationType additions must update this
   positive-type list or they will be silently excluded (entry #3950).

---

## State Machine

`graph_expand` has no persistent state. It is a pure function; each call is independent.

Internal BFS state (not exposed):
- `visited: HashSet<u64>` — set of entry IDs seen (prevents cycles and duplicates)
- `result: HashSet<u64>` — accumulated reachable entries (excludes seeds)
- `queue: VecDeque<(u64, usize)>` — BFS frontier with hop depth

---

## Initialization Sequence

None — pure function. No constructor, no Arc, no lazy initialization.

The single input invariant: `graph` must be a valid `TypedRelationGraph` reference (produced
by `build_typed_relation_graph` and held under the pre-cloned lock snapshot). The caller
(Phase 0 in search.rs) is responsible for providing a non-null, lock-free reference (C-04).

---

## Error Handling

`graph_expand` is infallible. It returns `HashSet<u64>` — no `Result`, no `Option`.

- Missing `node_index` entry for a seed: silently skipped (not panicked). This covers the
  case where a seed ID was returned by HNSW but is absent from the typed graph snapshot
  (e.g., graph not yet rebuilt after a new entry was stored).
- Missing `node_index` entry for a neighbor (defensive arm): silently skipped.
- All edge traversals via `edges_of_type()` return iterators; empty iterators produce zero
  neighbors with no error.

The Phase 0 caller handles all errors that arise from async operations (entry_store.get,
vector_store.get_embedding) — those are not graph_expand's concern.

---

## Tests in graph_expand_tests.rs

Split to `graph_expand_tests.rs` if inline tests push `graph_expand.rs` over 500 lines,
following the `graph_ppr_tests.rs` split pattern.

Test file declaration pattern (inside graph_expand.rs, bottom of file):

```rust
#[cfg(test)]
#[path = "graph_expand_tests.rs"]
mod tests;
```

### Key Test Scenarios

**AC-03: Each positive edge type surfaces reachable entry.**
For each of {CoAccess, Supports, Informs, Prerequisite}: construct a graph with seed B and
edge B→A of that type. Assert graph_expand({B}, depth=1) returns {A}.

**AC-04: Backward-only edge does not surface entry.**
Construct graph: edge C→B (only, no reverse). seed={B}.
Assert graph_expand returns empty — C is unreachable from B via Outgoing.

**AC-05: Two-hop traversal (depth=2).**
Graph: B→A→D (positive edges). seeds={B}, depth=2.
Assert result contains both A and D.

**AC-06: One-hop cap (depth=1).**
Same graph as AC-05. seeds={B}, depth=1.
Assert result contains A but NOT D.

**AC-07: Excluded edge types are not traversed.**
Graph: edge B→X of type Supersedes. seeds={B}.
Assert X is absent from result.
Repeat with Contradicts type.

**AC-08: Seeds excluded from result.**
Graph: edges A→B, B→A (bidirectional). seeds={A, B}.
Assert neither A nor B appears in result.

**Self-loop exclusion (sub-case of AC-08).**
Graph: edge A→A (self-loop). seeds={A}.
Assert result is empty.

**AC-09: max_candidates cap.**
Graph: seed B with 5 outgoing positive edges (B→1, B→2, B→3, B→4, B→5).
max_candidates=3. Assert result.len() == 3.

**AC-10: Empty seeds.**
graph_expand(graph, &[], depth=2, max_candidates=200).
Assert result is empty.

**AC-11: Empty graph.**
graph_expand(TypedRelationGraph::empty(), &[1, 2], depth=2, max_candidates=200).
Assert result is empty.

**AC-12: depth=0.**
graph_expand(graph, &[seed_id], depth=0, max_candidates=200).
Assert result is empty (even if seed has outgoing edges).

**R-11 cycle safety.**
Graph: bidirectional edges A↔B (both directions). seed={A}, depth=2.
Assert result == {B} (A excluded as seed; B found once; no infinite loop).

**R-11 triangle cycle.**
Graph: A→B, B→C, C→A. seed={A}, depth=3.
Assert result == {B, C}. Terminates without panic.

**R-13 determinism.**
Call graph_expand twice with identical inputs.
Assert both calls return identical HashSets (same IDs, regardless of HashSet iteration order).

**S1/S2 single-direction failure mode documentation test (R-02).**
Graph: edge A→B (Informs, single direction). seed={B} (higher ID).
Assert result is empty — documents the behavior the back-fill fixes.
Repeat with seed={A}. Assert result == {B}.

**S8 CoAccess directionality gap test (R-17).**
Graph: edge A→B (CoAccess, A < B, single direction). seed={B}.
Assert result is empty.
seed={A}. Assert result == {B}.
