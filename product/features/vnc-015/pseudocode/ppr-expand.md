# Component 4: PPR and graph_expand Expansion

## Purpose

Add `RelationType::RelatedTo` to the positive edge type set in `graph_ppr.rs` and
`graph_expand.rs`. This is the only new variant that flows through PPR and BFS expansion
in this feature (ADR-006). `Advances`, `Motivates`, and the 8 other new write-only variants
must NOT be added.

After this change, agents writing `RelatedTo` edges will immediately see PPR mass flow through
those edges, enabling serendipitous associative discovery.

## Files Modified

- `crates/unimatrix-engine/src/graph_ppr.rs`
- `crates/unimatrix-engine/src/graph_expand.rs`

## graph_ppr.rs — Two Insertion Points

### Site A: positive_out_degree_weight function

```
// In: fn positive_out_degree_weight(node: &NodeId, graph: &TypedRelationGraph) -> f64
// Current: hardcoded to 4 positive types (Supports, CoAccess, Prerequisite, Informs)
// Change: add RelatedTo as 5th positive type at equal weight

// BEFORE (illustrative):
let count = graph.edges_of_type(node, RelationType::Supports)
          + graph.edges_of_type(node, RelationType::CoAccess)
          + graph.edges_of_type(node, RelationType::Prerequisite)
          + graph.edges_of_type(node, RelationType::Informs)
          as f64;

// AFTER — add RelatedTo:
let count = graph.edges_of_type(node, RelationType::Supports)
          + graph.edges_of_type(node, RelationType::CoAccess)
          + graph.edges_of_type(node, RelationType::Prerequisite)
          + graph.edges_of_type(node, RelationType::Informs)
          + graph.edges_of_type(node, RelationType::RelatedTo)  // NEW (vnc-015)
          as f64;

// Weight: RelatedTo gets the same weight factor as the existing 4 positive types.
// Equal treatment; no special multiplier. (ADR-006, FR-11)
```

### Site B: personalized_pagerank function — edges_of_type calls

```
// In: fn personalized_pagerank(seed: NodeId, graph: &TypedRelationGraph, ...) -> HashMap<NodeId, f64>
// The PPR inner loop iterates over positive-type outgoing edges from each node.
// Current: iterates Supports, CoAccess, Prerequisite, Informs
// Change: add RelatedTo to the iteration set

// The exact iteration pattern depends on the current implementation structure.
// If using a match or list-of-types:

// BEFORE:
let positive_types = [
    RelationType::Supports,
    RelationType::CoAccess,
    RelationType::Prerequisite,
    RelationType::Informs,
];

// AFTER:
let positive_types = [
    RelationType::Supports,
    RelationType::CoAccess,
    RelationType::Prerequisite,
    RelationType::Informs,
    RelationType::RelatedTo,    // NEW (vnc-015)
];

// If using individual edges_of_type calls, add one more call:
// for edge in graph.edges_of_type(node, RelationType::RelatedTo) { ... }

// DO NOT add Advances or Motivates here — intentionally absent (ADR-006, Phase 2)
// DO NOT add any of the other 8 write-only variants
```

## graph_expand.rs — One Insertion Point

### Site C: positive BFS set

```
// In: graph_expand.rs (the outgoing traversal BFS function)
// Location: positive BFS set, approximately lines 123-136 per spec
// Current: Supports, CoAccess, Prerequisite, Informs
// Change: add RelatedTo as 5th element

// BEFORE:
let positive_set = [
    RelationType::Supports,
    RelationType::CoAccess,
    RelationType::Prerequisite,
    RelationType::Informs,
];

// AFTER:
let positive_set = [
    RelationType::Supports,
    RelationType::CoAccess,
    RelationType::Prerequisite,
    RelationType::Informs,
    RelationType::RelatedTo,    // NEW (vnc-015)
];

// DO NOT add Advances or Motivates — intentionally absent (ADR-006)
```

## Change Scope Summary

Total lines changed: approximately 6 lines across two files (per SCOPE.md Goal 8 estimate).

```
graph_ppr.rs    — 2 changes (positive_out_degree_weight: +1 line; personalized_pagerank: +1 line)
graph_expand.rs — 1 change (positive BFS set: +1 line or +1 element in array)
```

No algorithm changes. No weight formula changes. No new function signatures. Pure additive
changes to existing type-set arrays/patterns.

## Invariants That Must NOT Change

- The 4 existing positive types (Supports, CoAccess, Prerequisite, Informs) remain with
  their current weights — no rebalancing (AC-04, AC-17)
- Advances is NOT added (write-only, Phase 2 — negative check required at Gate-3a)
- Motivates is NOT added (write-only, Phase 2 — negative check required at Gate-3a)
- The remaining 8 write-only variants are NOT added (Cites, Asserts, Mentions, Refutes,
  Tests, DerivedFrom, About)

## Error Handling

No error paths — this component is pure additive changes to static type-set definitions.
A missing RelatedTo means PPR mass does not flow through RelatedTo edges (functional gap,
not a crash). Caught by the PPR integration test (AC-17).

## Key Test Scenarios

1. Write a `RelatedTo` edge between entry A and entry B; run PPR seeded on A; assert
   B appears with positive mass (PPR flows through RelatedTo) (AC-17)
2. Existing 4 positive types: run PPR with Supports/CoAccess/Prerequisite/Informs edges;
   assert same mass flow as pre-feature baseline (AC-04, AC-17)
3. Write an `Advances` edge; run PPR; assert scoring is UNCHANGED from a baseline with
   no edges (Advances is write-only; zero PPR contribution expected) (R-11, AC-17)
4. Write a `Motivates` edge; run PPR; assert scoring is UNCHANGED (R-11, AC-17)
5. BFS expansion: add RelatedTo edge A→B; call graph_expand seeded on A; assert B is
   included in the expanded set
6. BFS expansion: Advances edges do NOT cause BFS expansion (negative check)
7. Gate-3a grep: `RelatedTo` appears in positive_out_degree_weight and personalized_pagerank
8. Gate-3a grep: `Advances` does NOT appear in graph_ppr.rs positive sets (ADR-007 step 7)
9. Gate-3a grep: `Motivates` does NOT appear in graph_ppr.rs positive sets (ADR-007 step 7)
