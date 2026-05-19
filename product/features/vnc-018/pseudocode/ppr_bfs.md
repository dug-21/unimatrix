# Pseudocode: graph_ppr.rs + graph_expand.rs — Advances and Motivates Additions

## Purpose

Targeted surgical additions to two existing files in `crates/unimatrix-engine/src/`.
Completes the deferred `Advances`/`Motivates` PPR/BFS addition from W1B-1 (vnc-015
ADR-006). Approximately 16 lines across both files, plus module-level doc comment
updates in each file.

No structural changes. No new functions. No new modules. Insertion-only.

---

## Modified Files

- `crates/unimatrix-engine/src/graph_ppr.rs` — 2 insertion points
- `crates/unimatrix-engine/src/graph_expand.rs` — 1 insertion point

---

## graph_ppr.rs Modifications

### Insertion Point 1: `personalized_pagerank` — neighbor_contribution loop

**Location**: After the `RelationType::RelatedTo` `edges_of_type` block (~line 131).

The existing code has:
```rust
// Fifth call: RelationType::RelatedTo (vnc-015, ADR-006).
for edge_ref in graph.edges_of_type(node_idx, RelationType::RelatedTo, Direction::Outgoing) {
    neighbor_contribution +=
        outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
}
// NOTE: Advances and Motivates are intentionally absent (write-only, Phase 2 — ADR-006).
```

**Add after the RelatedTo block** (replacing the NOTE comment):

```rust
// vnc-018 (ADR-006): Advances and Motivates added to positive type set.
// Removed "write-only until Phase 2" restriction.
for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
    neighbor_contribution +=
        outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
}
for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
    neighbor_contribution +=
        outgoing_contribution(&current_scores, &edge_ref, out_degree, graph);
}
```

After this change, `personalized_pagerank` makes 7 `edges_of_type` calls per node:
Supports, CoAccess, Prerequisite, Informs, RelatedTo, Advances, Motivates.

### Insertion Point 2: `positive_out_degree_weight` — weight summation loop

**Location**: After the `RelationType::RelatedTo` `edges_of_type` block (~line 203).

The existing code has:
```rust
for edge_ref in graph.edges_of_type(node_idx, RelationType::RelatedTo, Direction::Outgoing) {
    total += edge_ref.weight().weight;
}
// (implicit: Advances and Motivates absent — write-only)
```

**Add after the RelatedTo block**:

```rust
// vnc-018 (ADR-006): Advances and Motivates added to positive out-degree weight.
for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
    total += edge_ref.weight().weight;
}
for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
    total += edge_ref.weight().weight;
}
```

After this change, `positive_out_degree_weight` sums weights from 7 edge types.

### Module-Level Doc Comment Update (graph_ppr.rs)

**Current** (line 1 area):
```
//! Personalized PageRank over positive edges (Supports, CoAccess, Prerequisite, Informs, RelatedTo).
//! ...
//! vnc-015: `RelatedTo` added to positive type set (ADR-006). `Advances` and `Motivates` are
//! intentionally NOT in the positive type set — write-only until Phase 2.
```

**Replace with**:
```
//! Personalized PageRank over positive edges
//! (Supports, CoAccess, Prerequisite, Informs, RelatedTo, Advances, Motivates).
//! ...
//! vnc-015: `RelatedTo` added to positive type set (ADR-006).
//! vnc-018: `Advances` and `Motivates` added to positive type set (ADR-006 vnc-018).
```

Also update the function-level doc comments in `personalized_pagerank` and
`positive_out_degree_weight` to remove references to the absent note and list all 7
positive types.

---

## graph_expand.rs Modifications

### Insertion Point: BFS neighbor collection loop

**Location**: After the `RelationType::RelatedTo` `edges_of_type` block (~line 144).

The existing code has:
```rust
for edge_ref in graph.edges_of_type(node_idx, RelationType::RelatedTo, Direction::Outgoing) {
    // vnc-015 (ADR-006): RelatedTo added to positive BFS set at equal weight.
    neighbors.push(graph.inner[edge_ref.target()]);
}
// NOTE: Advances and Motivates are intentionally absent — write-only until Phase 2 (ADR-006).
```

**Add after the RelatedTo block** (replacing the NOTE comment):

```rust
// vnc-018 (ADR-006): Advances and Motivates added to positive BFS type set.
for edge_ref in graph.edges_of_type(node_idx, RelationType::Advances, Direction::Outgoing) {
    neighbors.push(graph.inner[edge_ref.target()]);
}
for edge_ref in graph.edges_of_type(node_idx, RelationType::Motivates, Direction::Outgoing) {
    neighbors.push(graph.inner[edge_ref.target()]);
}
```

After this change, `graph_expand` collects neighbors from 7 edge types: CoAccess,
Supports, Informs, Prerequisite, RelatedTo, Advances, Motivates.

The existing `neighbors.sort_unstable(); neighbors.dedup();` calls after the
collection loop remain unchanged — they are already correct for the expanded type set.

### Module-Level Doc Comment Update (graph_expand.rs)

**Current** (line 1 area):
```
//! BFS graph expansion over positive edges (CoAccess, Supports, Informs, Prerequisite, RelatedTo).
//! ...
//! vnc-015: `RelatedTo` added to positive BFS set (ADR-006). `Advances` and `Motivates` are
//! intentionally absent — write-only until Phase 2.
```

**Replace with**:
```
//! BFS graph expansion over positive edges
//! (CoAccess, Supports, Informs, Prerequisite, RelatedTo, Advances, Motivates).
//! ...
//! vnc-015: `RelatedTo` added to positive BFS set (ADR-006).
//! vnc-018: `Advances` and `Motivates` added to positive BFS type set (ADR-006 vnc-018).
```

---

## Data Flow

```
graph_ppr.rs::personalized_pagerank(graph, seed_ids, alpha, iterations)
  for each node in each iteration:
    7 edges_of_type calls (Supports, CoAccess, Prerequisite, Informs, RelatedTo,
                           Advances [new], Motivates [new])
    positive_out_degree_weight(graph, node_idx)
      → 7 outgoing type weight sums
    neighbor_contribution accumulated

graph_expand.rs::graph_expand(graph, seed_ids, max_depth, ...)
  for each BFS node:
    7 edges_of_type calls collect neighbor node IDs
    neighbors.sort_unstable(); neighbors.dedup()
    qualified neighbors enqueued for next BFS depth
```

---

## Error Handling

No new error conditions introduced. Both changes are additive additions to existing
loops. `edges_of_type` returns an iterator — if no edges of that type exist, the
loop body executes zero times. No panics possible.

**Regression risk** (R-09): Nodes with `Advances` or `Motivates` edges will have a
higher `positive_out_degree_weight` denominator after this change. This reduces the
per-edge weight contribution for nodes that previously had many positive edges of
other types. This is correct normalization behavior, not a bug. However, existing PPR
unit tests with hardcoded score assertions may fail if their test graphs include
entries with Advances or Motivates edges. Delivery agent must audit existing PPR tests
before merging.

---

## Key Test Scenarios

1. **AC-17**: Unit test constructs `TypedRelationGraph` with entries connected via
   `Advances` and `Motivates` edges. Calls `positive_out_degree_weight` for a node
   that has only Advances and Motivates outgoing edges. Asserts the returned weight
   is > 0.0 (proves both types are included in the weight computation).

2. **AC-17b**: Unit test calls `personalized_pagerank` with a graph containing
   `Advances` edges. Asserts that mass flows from the source entry to the target
   entry (PPR score of target entry is higher than without the Advances edge).

3. **AC-18**: Unit test constructs `TypedRelationGraph` with `Advances` and `Motivates`
   edges. Calls `graph_expand`. Asserts entries connected by those types are returned
   in the expansion output (BFS traversal follows them).

4. **Regression check**: Review existing PPR unit tests in `graph_ppr_tests.rs` for
   `assert_approx_eq!` or `assert!(score > X)` assertions. If any test graph includes
   entries with Advances or Motivates edges, the expected scores must be recalculated.
   If no test graph includes those types, no existing tests should fail.

5. **Boundary**: `positive_out_degree_weight_pub_for_test` is the test-visible export
   of `positive_out_degree_weight`. The AC-17 unit test should call this function
   directly to assert Advances and Motivates are included, consistent with how existing
   PPR tests access the function.
