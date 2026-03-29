# Test Plan: graph_ppr.rs

## Component

`crates/unimatrix-engine/src/graph_ppr.rs`

Pure function `personalized_pagerank(graph, seed_scores, alpha, iterations) -> HashMap<u64, f64>`.
No async, no I/O, no mutable global state.

Tests live in the `#[cfg(test)]` module of `graph_ppr.rs` (or `graph_ppr_tests.rs` if >500 lines).
All tests use `#[test]` (sync — the function is synchronous by design).

---

## Test Helpers

### `make_graph() -> TypedRelationGraph`
Returns an empty `TypedRelationGraph` (no nodes, no edges).

### `make_graph_with_edges(edges: &[(u64, u64, RelationType, f32)]) -> TypedRelationGraph`
Builds a `TypedRelationGraph` by inserting nodes for all referenced IDs and adding directed
edges per the input slice. Used by every test that needs a specific topology.

### `uniform_seeds(ids: &[u64]) -> HashMap<u64, f64>`
Returns a normalized seed map where each ID has weight `1.0 / ids.len()`.

---

## AC-01: Function Signature and Re-Export

### `test_ppr_function_exists_in_graph_ppr_rs`
- Verify: `grep "pub fn personalized_pagerank" crates/unimatrix-engine/src/graph_ppr.rs` returns a result.
- Verify: `grep "pub use graph_ppr::personalized_pagerank" crates/unimatrix-engine/src/graph.rs` returns a result.
- Method: `cargo check` compiles without errors after adding `use unimatrix_engine::graph::personalized_pagerank;`.
- This is a build-time verification; a unit test that calls the function also implicitly validates it.

---

## AC-02 / R-09: No Direct `.edges_directed()` Calls

### `test_edges_directed_not_in_graph_ppr_rs` (static gate)
- Assertion: `grep "edges_directed" crates/unimatrix-engine/src/graph_ppr.rs` returns no results.
- This is a grep-based CI check, not a runtime test. Include in the Stage 3c checklist.
- Behavioral counterpart: `test_supersedes_edge_excluded_from_ppr` (see AC-03 below).

---

## AC-03 / R-09: Supersedes and Contradicts Edges Excluded

### `test_supersedes_edge_excluded_from_ppr`
Arrange: Graph with nodes {S=1, N=2}. Edge: S→N via `RelationType::Supersedes`.
Seed: `{S: 1.0}` (normalized).
Act: `personalized_pagerank(&graph, &seeds, 0.85, 20)`.
Assert: `result.get(&2).copied().unwrap_or(0.0) == 0.0` — N receives zero mass from Supersedes edge.

### `test_contradicts_edge_excluded_from_ppr`
Arrange: Graph with nodes {S=1, N=2}. Edge: S→N via `RelationType::Contradicts`.
Seed: `{S: 1.0}`.
Act: run PPR with alpha=0.85, iterations=20.
Assert: `result.get(&2).copied().unwrap_or(0.0) == 0.0`.

---

## AC-05 / R-04: Determinism and Sort Placement

### `test_ppr_deterministic_same_inputs`
Arrange: A graph with 5 nodes and a mix of Supports and CoAccess edges. Seeds: two entries with different weights.
Act: Call `personalized_pagerank` twice with identical inputs.
Assert: `result_1 == result_2` (exact HashMap equality — every key and value identical).

### `test_ppr_deterministic_large_graph`
Arrange: Programmatically generate a graph with 100 nodes and 300 edges (mixed types, random-ish IDs).
Act: Call `personalized_pagerank` twice.
Assert: exact HashMap equality.
Rationale: Validates that HashMap iteration non-determinism in the score accumulation phase does
NOT affect output — confirming the node-ID sort is actually in effect.

### `test_ppr_sort_covers_all_nodes` (R-04 sort-length check)
Arrange: Graph with 50 nodes (varied IDs, not sequential).
Act: Call `personalized_pagerank` and capture result.
Assert: `result.len() <= 50` — the score map cannot have more keys than the graph has nodes.
(This test will catch cases where the sort list is built from a subset of nodes.)

---

## AC-07 / R-07: Zero Positive Out-Degree Node Does Not Propagate

### `test_zero_positive_out_degree_no_forward_propagation`
Arrange: Graph nodes {A=1, B=2, C=3}. Edges: A→B via `Supersedes` (only). Seed: `{A: 1.0}`.
Act: run PPR, alpha=0.85, iterations=20.
Assert: `result.get(&2).copied().unwrap_or(0.0) == 0.0` — B receives zero from A (A has zero positive out-degree).
Assert: `result.get(&1).unwrap() > 0.0` — A itself has teleportation mass (it is a seed).

### `test_node_with_mixed_edges_only_propagates_via_positive`
Arrange: Graph nodes {A=1, B=2, C=3}. Edges: A→B via `Supersedes`; A→C via `Supports`.
Seed: `{A: 1.0}`.
Act: run PPR.
Assert: `result.get(&3).copied().unwrap_or(0.0) > 0.0` — C receives mass (via Supports, Incoming on C finds A).
Assert: `result.get(&2).copied().unwrap_or(0.0) == 0.0` — B receives zero (Supersedes excluded).

---

## AC-08 / R-12: Edge Direction Semantics — Supports, CoAccess, Prerequisite

### `test_supports_incoming_direction` (T-PPR-03)
Arrange: Graph nodes {A=1, B=2}. Edge: A→B via `Supports`, weight=1.0.
Seed: `{B: 1.0}` (B is the seed — a decision that A supports).
Act: run PPR, alpha=0.85, iterations=20.
Assert: `result.get(&1).copied().unwrap_or(0.0) > 0.0` — A is surfaced as supporter of B.
Rationale: Incoming direction on B finds A (the edge A→B, when traversed incoming from B, yields A).

### `test_supports_seed_does_not_propagate_to_target` (direction sanity)
Arrange: Same graph {A=1, B=2}, edge A→B via `Supports`.
Seed: `{A: 1.0}` (A is the seed, not B).
Act: run PPR, alpha=0.85, iterations=20.
Assert: `result.get(&2).copied().unwrap_or(0.0) < result.get(&1).unwrap()` — B should NOT receive
direct propagation from A via the Supports edge (Incoming direction means mass flows B←A,
so when A is the seed, B does not receive direct propagation through this edge).
Note: B may receive teleportation mass if it is in the graph, but should not receive
edge-propagated mass from A via Supports when traversing Incoming.

### `test_coaccess_incoming_direction` (T-PPR-06 / AC-18)
Arrange: Graph nodes {S=1, N=2}. Edge: N→S via `CoAccess`, weight=0.8.
Seed: `{S: 1.0}`.
Act: run PPR, alpha=0.85, iterations=20.
Assert: `result.get(&2).copied().unwrap_or(0.0) > 0.0` — N surfaces because CoAccess N→S,
traversed Incoming from S, finds N.

### `test_prerequisite_incoming_direction` (R-12 critical)
Arrange: Graph nodes {A=1, B=2}. Edge: A→B via `Prerequisite`, weight=1.0.
Semantics: A is a prerequisite OF B (B requires A).
Seed: `{B: 1.0}` (B is the seed, requiring A).
Act: run PPR, alpha=0.85, iterations=20.
Assert: `result.get(&1).copied().unwrap_or(0.0) > 0.0` — A surfaces as the prerequisite of seed B.

### `test_prerequisite_wrong_direction_does_not_propagate` (R-12 regression guard)
Arrange: Same graph {A=1, B=2}, edge A→B via `Prerequisite`.
Seed: `{A: 1.0}` (A is the seed).
Act: run PPR.
Assert: `result.get(&2).copied().unwrap_or(0.0) < result.get(&1).unwrap()`
Rationale: With Incoming direction, when A is the seed, edge A→B does not propagate TO B
(that would require Outgoing). B should receive at most teleportation mass but no edge-based mass.
This test will catch a Direction::Outgoing regression.

---

## R-07: NaN / Infinity Guards

### `test_ppr_scores_all_finite`
Arrange: Realistic 10-node graph with mixed edge types and weights in [0.1, 1.0].
Seeds: 3 entries with non-uniform weights (normalized).
Act: run PPR with default config (alpha=0.85, iterations=20).
Assert: every value in the result map is finite (`f64::is_finite()`) and in `[0.0, 1.0]`.

### `test_ppr_single_min_positive_seed_no_nan`
Arrange: Two-node graph, one edge. Seed: `{1: f64::MIN_POSITIVE}` (normalized → 1.0 after divide-by-self).
Act: run PPR.
Assert: all result values finite, no NaN.

---

## Edge Cases

### `test_ppr_empty_seed_map_returns_empty` (E-01 / FR-01)
Arrange: Non-empty graph (5 nodes, 3 edges).
Act: `personalized_pagerank(&graph, &HashMap::new(), 0.85, 20)`.
Assert: return value is `HashMap::is_empty()`.

### `test_ppr_empty_graph_returns_empty` (E-01)
Arrange: `make_graph()` (no nodes, no edges).
Act: `personalized_pagerank(&graph, &HashMap::new(), 0.85, 20)`.
Assert: return value is empty.

### `test_ppr_single_node_no_edges` (E-03)
Arrange: Graph with one node (id=1), no edges. Seed: `{1: 1.0}`.
Act: run PPR, iterations=20.
Assert: `result.get(&1).unwrap() > 0.0` — seed node gets teleportation mass.
Assert: `result.len() == 1` — no other nodes in the map.
Assert: no divide-by-zero panic.

### `test_ppr_no_positive_edges_only_teleportation` (E-02)
Arrange: Graph with 3 nodes, all edges via `Supersedes` (no positive edges).
Seed: `{1: 1.0}`.
Act: run PPR.
Assert: `result.get(&1).unwrap() > 0.0` — seed has teleportation mass.
Assert: nodes 2 and 3 have zero score OR are absent from the result (no positive-edge propagation path).

### `test_ppr_disconnected_subgraph_zero_expansion` (E-07)
Arrange: Nodes {A, B, C} with Supports A→B and B→C. Nodes {D, E} isolated (no positive edges to
{A, B, C}). Seeds: `{A: 0.5, B: 0.5}` (normalized).
Act: run PPR.
Assert: D and E have zero score (or absent from result).
Assert: A, B, C all have positive scores.

---

## R-04: Timing Test (Regression Gate)

### `test_ppr_dense_50_node_coaccess_completes_under_1ms` (R-13 also)
Arrange: Build a dense graph: 50 nodes each connected to every other node via `CoAccess` (2450 edges).
Uniform seeds across all 50 nodes.
Act: `let t = std::time::Instant::now(); personalized_pagerank(...); let elapsed = t.elapsed();`
Assert: `elapsed.as_millis() < 5` — 5ms ceiling (5× NFR budget as test gate; actual must be < 1 ms).
Note: Run in `#[cfg(not(debug_assertions))]` or document as release-build-only for reliability.

### `test_ppr_10k_node_completes_within_budget` (R-04 scale gate)
Arrange: Programmatically generate 10K nodes with approximately 50K edges (100 CoAccess edges per
popular node). Uniform seeds on 10 nodes.
Act: measure wall time.
Assert: `elapsed.as_millis() < 10` — 10ms (10× NFR budget; accounts for test overhead).
Note: This is a stress/timing test. Mark with `#[ignore]` to exclude from normal `cargo test` runs
and run explicitly with `cargo test -- --ignored` in CI timing gates.

---

## Knowledge Stewardship
- Queried: `context_briefing` — surfaced ADRs #3731-#3740 (crt-030 decisions), pattern #3740 (graph traversal submodule), pattern #264 (test gateway new_permissive)
- Queried: `context_search` for graph traversal testing patterns — surfaced #1607 (SupersessionGraph two-pass DAG testing), #3627 (edges_of_type boundary pattern)
