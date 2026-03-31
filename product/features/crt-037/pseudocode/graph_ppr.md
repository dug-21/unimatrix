# Component: graph_ppr.rs (unimatrix-engine)

## Purpose

Extend `personalized_pagerank` and `positive_out_degree_weight` to include `Informs` edges
in PPR traversal. Each function gains one additional `edges_of_type` call for
`RelationType::Informs` with `Direction::Outgoing` — matching the exact pattern already
used for `Supports`, `CoAccess`, and `Prerequisite`.

Wave 2. Depends on `RelationType::Informs` being present in `graph.rs` (Wave 1).
Pure function — no I/O, no async, no mutable global state.

## Files Modified

`crates/unimatrix-engine/src/graph_ppr.rs`

## New/Modified Functions

### personalized_pagerank — add fourth edges_of_type call

The inner loop body (lines 96–115 in current codebase) has three `edges_of_type` calls.
Add a fourth, immediately after the `Prerequisite` block:

```
// Existing structure (three calls — unchanged):
if out_degree > 0.0:
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Supports, Direction::Outgoing):
        neighbor_contribution += outgoing_contribution(...)
    for edge_ref in graph.edges_of_type(node_idx, RelationType::CoAccess, Direction::Outgoing):
        neighbor_contribution += outgoing_contribution(...)
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Outgoing):
        neighbor_contribution += outgoing_contribution(...)
    // NEW — fourth call (crt-037):
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing):
        neighbor_contribution += outgoing_contribution(...)
```

Direction: `Direction::Outgoing`. This is the reverse random walk (transpose PPR).
For an Informs edge A→B (lesson A, decision B): when B is in the seed set and has a
non-zero score, node A (which has an outgoing edge to B) accumulates mass from B's score.
Result: seeding on a decision node surfaces the lesson that informed it. See AC-05.

The `outgoing_contribution` private helper is unchanged — it takes any `EdgeReference` and
reads `edge_ref.weight().weight` as f64. The `RelationEdge.weight` field for Informs edges
holds `candidate.cosine * config.nli_informs_ppr_weight` (written by Phase 8b).

### positive_out_degree_weight — add fourth edges_of_type call

The function body (lines 162–175 in current codebase) has three loops. Add a fourth:

```
fn positive_out_degree_weight(graph: &TypedRelationGraph, node_idx: NodeIndex) -> f64:
    total: f64 = 0.0

    // Existing three calls (unchanged):
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Supports, Direction::Outgoing):
        total += edge_ref.weight().weight as f64
    for edge_ref in graph.edges_of_type(node_idx, RelationType::CoAccess, Direction::Outgoing):
        total += edge_ref.weight().weight as f64
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Outgoing):
        total += edge_ref.weight().weight as f64
    // NEW — fourth call (crt-037):
    for edge_ref in graph.edges_of_type(node_idx, RelationType::Informs, Direction::Outgoing):
        total += edge_ref.weight().weight as f64

    return total
```

This ensures out-degree normalization in PPR accounts for Informs edge weights, so a node
with only Informs edges contributes mass correctly to the reverse walk (AC-06).

### Module doc comment update

The module-level doc comment at the top of `graph_ppr.rs` states:

```
// Before:
/// Compute Personalized PageRank over positive edges (Supports, CoAccess, Prerequisite).

// After:
/// Compute Personalized PageRank over positive edges (Supports, CoAccess, Prerequisite, Informs).
```

Update all occurrences in the module doc and the function-level doc for
`personalized_pagerank` (line 19 and line 40 vicinity). The phrase "Three separate
edges_of_type calls" in the inline comment at line 89 becomes "Four separate
edges_of_type calls":

```
// Before (line 89 vicinity):
// Three separate edges_of_type calls (AC-02 — no .edges_directed() allowed).

// After:
// Four separate edges_of_type calls (AC-02 — no .edges_directed() allowed).
// Fourth call: RelationType::Informs (crt-037).
```

## PPR Direction Semantics

`Direction::Outgoing` is the reverse random walk contract established for all positive edge
types. For an edge A→B (Informs: empirical A, normative B):
- `edges_of_type(node_idx_for_A, Informs, Outgoing)` yields the edge A→B.
- `outgoing_contribution` reads `current_scores[B] * edge_weight / out_degree`.
- When B is seeded (non-zero score), A accumulates mass.

`Direction::Incoming` would implement forward PPR (B pulls from A) — the opposite of the
desired behavior. This distinction is critical (R-02, C-14, AC-05). Test AC-05 must assert
`scores[A] > 0.0` (specifically the lesson node), not `scores.values().any(|&v| v > 0.0)`.

Historical note (RISK-TEST-STRATEGY.md §R-02): entry #3754 documents a crt-030 direction
error that survived two gate checks. The assertion specificity is deliberate.

## State Machines

None. Pure functions. No mutable state.

## Initialization Sequence

No initialization. These are pure functions called at query time.

## Data Flow

```
TypedRelationGraph (contains Informs edges after build_typed_relation_graph recognizes them)
  --personalized_pagerank(graph, seed_scores, alpha, iterations)-->
  each node in iteration:
    out_degree = positive_out_degree_weight(graph, node_idx)
      // now includes Informs edge weights
    neighbor_contribution += sum over Informs edges: outgoing_contribution(...)
      // targets' scores flow back to source via Outgoing traversal
  -->
  HashMap<u64, f64> final PPR scores
```

## Error Handling

Both functions are pure and infallible — no `Result` return. `edges_of_type` returns an
iterator; if no `Informs` edges exist for a node, the iterator is empty and `total`/
`neighbor_contribution` is unaffected. Zero-edge nodes handled by the `out_degree > 0.0`
guard already present.

## Key Test Scenarios

AC-05: Two-node graph. Node A (lesson-learned, id=1), Node B (decision, id=2). Add one
`Informs` edge A→B with weight 0.5. Build via `TypedRelationGraph` directly (or use
`build_typed_relation_graph` with a `GraphEdgeRow`). Call `personalized_pagerank` with
`seed_scores = {B: 1.0}`, alpha=0.85, iterations=20. Assert `scores[A] > 0.0` —
specifically node A, not just any non-zero score.

AC-06: Single node A with one Informs edge A→B. Call `positive_out_degree_weight` on A.
Assert result equals the edge weight (0.5 in the example). With only an Informs edge and
no Supports/CoAccess/Prerequisite edges, the return value must be the Informs weight, not
zero.

No mass propagation without Informs edges: same two-node graph but without the Informs
edge — assert `scores[A]` is zero (or near zero from teleportation only, depending on
whether A is in the seed set).

Direction regression guard: CI grep check — `grep -n 'Direction::Incoming' graph_ppr.rs`
returns empty after the change. Document in the test file as a comment referencing entry
#3754.

Existing tests: all existing `graph_ppr_tests.rs` tests must pass unchanged. The fourth
call adds to the iteration; it does not alter existing behavior when `Informs` edges are
absent.

## Constraints

- C-07: All PPR traversal via `edges_of_type()` exclusively. No `.edges_directed()` calls.
  This is the AC-02 boundary.
- C-14: `Direction::Outgoing` for the fourth call. Not `Direction::Incoming`.
- The fourth call is added in BOTH `personalized_pagerank` AND `positive_out_degree_weight`.
  Omitting it from `positive_out_degree_weight` would cause incorrect normalization (AC-06).
- `outgoing_contribution` helper is not modified — it is type-agnostic on edge references.
- SR-01: `graph_penalty` and `find_terminal_active` are in `graph.rs`, not `graph_ppr.rs`.
  No changes to those functions. This file only touches PPR traversal.
