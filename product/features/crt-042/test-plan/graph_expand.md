# crt-042: Test Plan — `graph_expand.rs`

## Component Scope

Component 1 of 4. Tests live inline in `crates/unimatrix-engine/src/graph_expand.rs`
(or split to `graph_expand_tests.rs` if the combined file exceeds 500 lines — same
pattern as `graph_ppr_tests.rs`). All tests use hand-constructed `TypedRelationGraph`
fixtures. No DB, no async.

**File under test**: `crates/unimatrix-engine/src/graph_expand.rs`
**Function under test**: `pub fn graph_expand(graph: &TypedRelationGraph, seed_ids: &[u64], depth: usize, max_candidates: usize) -> HashSet<u64>`

---

## Test Fixture Helper Design

Mirror the `graph_ppr_tests.rs` pattern exactly. Two helpers are sufficient:

```rust
/// Returns an empty TypedRelationGraph (no nodes, no edges).
fn make_graph() -> TypedRelationGraph {
    TypedRelationGraph::empty()
}

/// Builds a TypedRelationGraph from a slice of (source_id, target_id, RelationType, weight).
/// All referenced IDs are added as nodes automatically.
fn make_graph_with_edges(edges: &[(u64, u64, RelationType, f32)]) -> TypedRelationGraph {
    // Collect unique node IDs
    let mut ids: Vec<u64> = Vec::new();
    for &(src, tgt, _, _) in edges {
        if !ids.contains(&src) { ids.push(src); }
        if !ids.contains(&tgt) { ids.push(tgt); }
    }
    let entries: Vec<EntryRecord> = ids.iter().map(|&id| make_entry(id)).collect();
    let edge_rows: Vec<GraphEdgeRow> = edges.iter().map(|&(src, tgt, rel, weight)| {
        GraphEdgeRow { source_id: src, target_id: tgt,
                       relation_type: rel.as_str().to_string(), weight,
                       created_at: 0, created_by: "test".to_string(),
                       source: "test".to_string(), bootstrap_only: false }
    }).collect();
    build_typed_relation_graph(&entries, &edge_rows)
        .expect("test graph build must succeed")
}

/// Minimal EntryRecord for graph node insertion (matches graph_ppr_tests.rs make_entry).
fn make_entry(id: u64) -> EntryRecord { /* ... same as graph_ppr_tests.rs ... */ }
```

**Why mirror the PPR helper exactly**: avoids divergent fixture conventions within the
graph module family; `build_typed_relation_graph` is the only safe way to construct
`TypedRelationGraph` (direct field construction is private).

**Determinism note**: `HashSet<u64>` return type means assertion order does not matter.
Use `assert_eq!(result, HashSet::from([a, b]))` syntax.

---

## Unit Tests

### AC-03: Positive Edge Types (four independent tests)

**Risk covered**: R-02 (partial), R-09

One test per positive edge type. Each test is structurally identical with only the
`RelationType` variant changing.

```rust
// test_graph_expand_coaccess_surfaces_neighbor
// graph: 1 → 2 (CoAccess), seeds {1}, depth=2, max=200
// assert: result == {2}
#[test]
fn test_graph_expand_coaccess_surfaces_neighbor() { ... }

// test_graph_expand_supports_surfaces_neighbor
// graph: 1 → 2 (Supports), seeds {1}, depth=2, max=200
// assert: result == {2}
#[test]
fn test_graph_expand_supports_surfaces_neighbor() { ... }

// test_graph_expand_informs_surfaces_neighbor
// graph: 1 → 2 (Informs), seeds {1}, depth=2, max=200
// assert: result == {2}
#[test]
fn test_graph_expand_informs_surfaces_neighbor() { ... }

// test_graph_expand_prerequisite_surfaces_neighbor
// graph: 1 → 2 (Prerequisite), seeds {1}, depth=2, max=200
// assert: result == {2}
#[test]
fn test_graph_expand_prerequisite_surfaces_neighbor() { ... }
```

**Assertion form**: `assert_eq!(result, HashSet::from([2u64]), "...")`. Do not assert
`Direction::Outgoing` — the behavioral outcome (2 appears) is the test (ADR-006, entry #3754).

---

### AC-04: Backward Edge Does NOT Surface

**Risk covered**: R-02 (direction semantics), R-16

```rust
// test_graph_expand_backward_edge_does_not_surface
// graph: 3 → 1 (Supports), seeds {1}, depth=2, max=200
// assert: result is empty
// Rationale: edge goes INTO the seed (3 → seed_1). Outgoing-only traversal
// from seed_1 sees no outgoing positive edges. Entry 3 must not surface.
#[test]
fn test_graph_expand_backward_edge_does_not_surface() {
    let graph = make_graph_with_edges(&[(3, 1, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert!(result.is_empty(),
        "backward edge (3→seed_1) must not surface entry 3 via Outgoing traversal");
}
```

**Note**: This test is the behavioral proof of the direction contract (ADR-006). It MUST
be present even if it seems trivial. The crt-030 post-merge correction (#3754) required
4 spec artifacts because direction was not tested behaviorally.

---

### AC-05: Two-Hop Chain, Depth=2

**Risk covered**: R-02

```rust
// test_graph_expand_two_hop_depth2_surfaces_both
// graph: 1 → 2 (CoAccess), 2 → 3 (Supports), seeds {1}, depth=2, max=200
// assert: result == {2, 3}
#[test]
fn test_graph_expand_two_hop_depth2_surfaces_both() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::CoAccess, 1.0),
        (2, 3, RelationType::Supports, 1.0),
    ]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert_eq!(result, HashSet::from([2u64, 3u64]),
        "depth=2 must surface both A (hop 1) and D (hop 2)");
}
```

---

### AC-06: Two-Hop Chain, Depth=1

**Risk covered**: R-02

```rust
// test_graph_expand_two_hop_depth1_surfaces_only_first
// Same graph as AC-05: 1→2, 2→3
// seeds {1}, depth=1, max=200
// assert: result == {2} (only first hop; 3 is at depth 2, excluded)
#[test]
fn test_graph_expand_two_hop_depth1_surfaces_only_first() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::CoAccess, 1.0),
        (2, 3, RelationType::Supports, 1.0),
    ]);
    let result = graph_expand(&graph, &[1], 1, 200);
    assert_eq!(result, HashSet::from([2u64]),
        "depth=1 must surface only the first hop; second hop must be excluded");
    assert!(!result.contains(&3),
        "entry 3 is at depth 2 and must be absent when depth=1");
}
```

---

### AC-07: Excluded Edge Types (Supersedes, Contradicts)

**Risk covered**: R-09

Two tests, one per excluded type:

```rust
// test_graph_expand_supersedes_not_traversed
// graph: 1 → 2 (Supersedes), seeds {1}, depth=2, max=200
// assert: result is empty
#[test]
fn test_graph_expand_supersedes_not_traversed() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supersedes, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert!(result.is_empty(),
        "Supersedes edges must not be traversed by graph_expand");
}

// test_graph_expand_contradicts_not_traversed
// graph: 1 → 2 (Contradicts), seeds {1}, depth=2, max=200
// assert: result is empty
#[test]
fn test_graph_expand_contradicts_not_traversed() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Contradicts, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert!(result.is_empty(),
        "Contradicts edges must not be traversed by graph_expand");
}
```

---

### AC-08: Seed Exclusion

**Risk covered**: R-12

```rust
// test_graph_expand_seeds_excluded_from_result
// graph: 1 → 2 (Supports), seeds {1, 2}, depth=2, max=200
// assert: result is empty (both 1 and 2 are seeds; 2 is reachable from 1 but is itself a seed)
#[test]
fn test_graph_expand_seeds_excluded_from_result() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[1, 2], 2, 200);
    assert!(result.is_empty(),
        "seed IDs must be excluded from result even if reachable via graph edges");
    assert!(!result.contains(&1), "seed 1 must not appear in result");
    assert!(!result.contains(&2), "seed 2 must not appear in result");
}

// test_graph_expand_self_loop_seed_not_returned
// graph: 1 → 1 (CoAccess self-loop), seeds {1}, depth=2, max=200
// assert: result is empty (self is a seed, self-loop does not add self)
#[test]
fn test_graph_expand_self_loop_seed_not_returned() {
    let graph = make_graph_with_edges(&[(1, 1, RelationType::CoAccess, 1.0)]);
    let result = graph_expand(&graph, &[1], 2, 200);
    assert!(result.is_empty(),
        "self-loop on a seed must not add the seed to the result");
}
```

---

### AC-09: Max Candidates Early Exit

**Risk covered**: R-05

```rust
// test_graph_expand_max_candidates_cap
// graph: seed 1 with outgoing edges to entries 2..=201 (200 positive-edge neighbors)
// seeds {1}, depth=1, max=10
// assert: result.len() == 10 exactly (cap enforced; not all 200 neighbors returned)
#[test]
fn test_graph_expand_max_candidates_cap() {
    let edges: Vec<(u64, u64, RelationType, f32)> = (2u64..=201)
        .map(|i| (1, i, RelationType::Supports, 1.0))
        .collect();
    let graph = make_graph_with_edges(&edges);
    let result = graph_expand(&graph, &[1], 1, 10);
    assert_eq!(result.len(), 10,
        "result must contain exactly max_candidates entries when cap is hit");
}
```

**Additional assertion**: verify no entry outside the expected set is in the result
(cap does not overshoot):
```rust
    assert!(result.iter().all(|id| (2..=201).contains(id)),
        "all returned IDs must be valid neighbors of seed 1");
```

---

### AC-10: Empty Seeds

**Risk covered**: R-02 edge cases, R-12

```rust
// test_graph_expand_empty_seeds_returns_empty
// graph: has nodes and edges, seeds []
// assert: result is empty, no panic
#[test]
fn test_graph_expand_empty_seeds_returns_empty() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[], 2, 200);
    assert!(result.is_empty(),
        "empty seed list must return empty set immediately");
}
```

---

### AC-11: Empty Graph

**Risk covered**: R-02 edge cases

```rust
// test_graph_expand_empty_graph_returns_empty
// graph: TypedRelationGraph::empty() — no nodes
// seeds {1, 2}, depth=2, max=200
// assert: result is empty, no panic
#[test]
fn test_graph_expand_empty_graph_returns_empty() {
    let graph = make_graph();  // TypedRelationGraph::empty()
    let result = graph_expand(&graph, &[1, 2], 2, 200);
    assert!(result.is_empty(),
        "graph with no nodes must return empty set immediately");
}
```

---

### AC-12: Depth Zero

**Risk covered**: R-02 edge cases

```rust
// test_graph_expand_depth_zero_returns_empty
// graph: has nodes and edges, seeds {1}
// depth=0, max=200
// assert: result is empty
#[test]
fn test_graph_expand_depth_zero_returns_empty() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Supports, 1.0)]);
    let result = graph_expand(&graph, &[1], 0, 200);
    assert!(result.is_empty(),
        "depth=0 must return empty set immediately per FR-03");
}
```

---

### R-11: BFS Visited-Set (Cycle Termination)

**Risk covered**: R-11

```rust
// test_graph_expand_bidirectional_terminates
// graph: 1 → 2 (CoAccess), 2 → 1 (CoAccess) — bidirectional pair
// seeds {1}, depth=2, max=200
// assert: result == {2}, terminates without hang
#[test]
fn test_graph_expand_bidirectional_terminates() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::CoAccess, 1.0),
        (2, 1, RelationType::CoAccess, 1.0),
    ]);
    let result = graph_expand(&graph, &[1], 2, 200);
    // Entry 2 is reachable (1→2). Entry 1 is a seed, excluded.
    // Without visited-set: 1→2→1→2... would loop until max_candidates hit with duplicates.
    assert_eq!(result, HashSet::from([2u64]),
        "bidirectional CoAccess must not cause infinite loop; visited-set must prevent revisit");
    // HashSet guarantees no duplicate IDs by construction.
}

// test_graph_expand_triangular_cycle_terminates
// graph: 1→2, 2→3, 3→1 (all Supports) — triangular cycle
// seeds {1}, depth=3, max=200
// assert: result == {2, 3}, terminates
#[test]
fn test_graph_expand_triangular_cycle_terminates() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Supports, 1.0),
        (2, 3, RelationType::Supports, 1.0),
        (3, 1, RelationType::Supports, 1.0),
    ]);
    let result = graph_expand(&graph, &[1], 3, 200);
    // 1 is seed (excluded); 2 and 3 are reachable.
    assert_eq!(result, HashSet::from([2u64, 3u64]),
        "triangular cycle must terminate; only non-seed reachable entries returned");
}
```

---

### R-13: Determinism

**Risk covered**: R-13

```rust
// test_graph_expand_deterministic_across_calls
// graph: fan-out from seed 1 to {2, 3, 4, 5, 6} via mixed edge types
// seeds {1}, depth=2, max=3 (budget-boundary exercised)
// assert: two consecutive calls return identical HashSets
#[test]
fn test_graph_expand_deterministic_across_calls() {
    let graph = make_graph_with_edges(&[
        (1, 5, RelationType::Supports, 1.0),
        (1, 2, RelationType::CoAccess, 1.0),
        (1, 3, RelationType::Informs, 1.0),
        (1, 4, RelationType::Prerequisite, 1.0),
        (1, 6, RelationType::Supports, 1.0),
    ]);
    let result_a = graph_expand(&graph, &[1], 1, 3);
    let result_b = graph_expand(&graph, &[1], 1, 3);
    assert_eq!(result_a, result_b,
        "graph_expand must be deterministic: same inputs must produce identical HashSets");
    assert_eq!(result_a.len(), 3,
        "budget-boundary: exactly max_candidates=3 results expected");
}
```

**Why budget-boundary matters**: the max_candidates early exit processes the BFS frontier
in sorted node-ID order (ADR-004 crt-030). The three lowest IDs (2, 3, 4) should be
returned. This verifies sorted-frontier determinism at the cap boundary.

---

### R-02: S1/S2 Unidirectional Understanding Test (documentation test)

```rust
// test_graph_expand_unidirectional_informs_from_higher_id_seed_misses
// Purpose: documents the S1/S2 single-direction failure mode the back-fill fixes.
// graph: 1 → 2 (Informs, source < target — S1/S2 convention)
// seeds {2} (higher-ID seed, simulating a seed that cannot reach lower-ID via Outgoing)
// assert: result is empty
// This is EXPECTED behavior before back-fill. After back-fill (2→1 also exists), {1} is returned.
#[test]
fn test_graph_expand_unidirectional_informs_from_higher_id_seed_misses() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::Informs, 1.0)]);
    let result = graph_expand(&graph, &[2], 2, 200);
    assert!(result.is_empty(),
        "before back-fill: higher-ID seed (2) cannot reach lower-ID entry (1) via \
         single-direction Informs edge (1→2 only). This is the failure mode AC-00 back-fill fixes.");
}

// test_graph_expand_bidirectional_informs_after_backfill
// graph: 1→2 AND 2→1 (Informs, bidirectional — simulating post-back-fill state)
// seeds {2}, depth=2, max=200
// assert: result == {1}
#[test]
fn test_graph_expand_bidirectional_informs_after_backfill() {
    let graph = make_graph_with_edges(&[
        (1, 2, RelationType::Informs, 1.0),
        (2, 1, RelationType::Informs, 1.0),  // back-fill direction
    ]);
    let result = graph_expand(&graph, &[2], 2, 200);
    assert_eq!(result, HashSet::from([1u64]),
        "after back-fill: higher-ID seed (2) reaches lower-ID entry (1) via reverse Informs edge");
}
```

---

### AC-16: Traversal Boundary — Grep Verification

**Risk covered**: R-09

This is a code inspection test, not a runtime test. Specify as a mandatory Stage 3c
check:

```bash
# Must return zero matches:
grep -n 'edges_directed\|neighbors_directed' \
  crates/unimatrix-engine/src/graph_expand.rs
```

Assert: `echo $?` returns 0 AND output is empty. If any match is found, the AC-16 check
fails and the delivery agent must replace the direct call with `edges_of_type()`.

Additionally assert the module-level doc comment contains the `edges_of_type()` invariant
statement (text search):
```bash
grep -n 'edges_of_type' crates/unimatrix-engine/src/graph_expand.rs | head -5
```
Must find at least one match in the module-level doc comment.

---

### R-17: S8 CoAccess Unidirectional Gap (documentation test)

```rust
// test_graph_expand_s8_coaccess_unidirectional_from_higher_id_misses
// graph: 1 → 2 (CoAccess, a=min=1 → b=max=2, S8 convention)
// seeds {2} (higher-ID seed)
// assert: result is empty (without crt-035 promotion tick reverse direction)
#[test]
fn test_graph_expand_s8_coaccess_unidirectional_from_higher_id_misses() {
    let graph = make_graph_with_edges(&[(1, 2, RelationType::CoAccess, 1.0)]);
    let result = graph_expand(&graph, &[2], 2, 200);
    assert!(result.is_empty(),
        "S8 single-direction CoAccess: higher-ID seed cannot reach lower-ID partner \
         without the crt-035 promotion tick adding the reverse direction");
}
```

---

## Test Count Summary

| Test Name | AC | Risk |
|-----------|-----|------|
| test_graph_expand_coaccess_surfaces_neighbor | AC-03 | R-02, R-09 |
| test_graph_expand_supports_surfaces_neighbor | AC-03 | R-02, R-09 |
| test_graph_expand_informs_surfaces_neighbor | AC-03 | R-02, R-09 |
| test_graph_expand_prerequisite_surfaces_neighbor | AC-03 | R-02, R-09 |
| test_graph_expand_backward_edge_does_not_surface | AC-04 | R-02, R-16 |
| test_graph_expand_two_hop_depth2_surfaces_both | AC-05 | R-02 |
| test_graph_expand_two_hop_depth1_surfaces_only_first | AC-06 | R-02 |
| test_graph_expand_supersedes_not_traversed | AC-07 | R-09 |
| test_graph_expand_contradicts_not_traversed | AC-07 | R-09 |
| test_graph_expand_seeds_excluded_from_result | AC-08 | R-12 |
| test_graph_expand_self_loop_seed_not_returned | AC-08 | R-12 |
| test_graph_expand_max_candidates_cap | AC-09 | R-05 |
| test_graph_expand_empty_seeds_returns_empty | AC-10 | edge case |
| test_graph_expand_empty_graph_returns_empty | AC-11 | edge case |
| test_graph_expand_depth_zero_returns_empty | AC-12 | edge case |
| test_graph_expand_bidirectional_terminates | R-11 | R-11 |
| test_graph_expand_triangular_cycle_terminates | R-11 | R-11 |
| test_graph_expand_deterministic_across_calls | R-13 / NFR-04 | R-13 |
| test_graph_expand_unidirectional_informs_from_higher_id_seed_misses | R-02 | R-02 |
| test_graph_expand_bidirectional_informs_after_backfill | R-02 | R-02 |
| test_graph_expand_s8_coaccess_unidirectional_from_higher_id_misses | R-17 | R-17 |
| AC-16 grep check (shell) | AC-16 | R-09 |

**Total**: 21 unit tests + 1 grep check = 22 coverage points for Component 1.
