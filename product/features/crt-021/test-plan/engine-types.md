# Test Plan: engine-types (unimatrix-engine/src/graph.rs)

Covers: `RelationType`, `RelationEdge`, `TypedRelationGraph`, `edges_of_type`,
`build_typed_relation_graph`, `graph_penalty`, `find_terminal_active`

Risks addressed: R-01, R-02, R-03, R-10, R-12

---

## Preserve All 25+ Existing Unit Tests (R-01, AC-10)

**Requirement**: All existing `graph.rs` unit tests must pass without modification to test
expectations when `build_supersession_graph` is renamed/replaced by `build_typed_relation_graph`
and `SupersessionGraph` becomes `TypedRelationGraph`.

Each existing test calls `build_supersession_graph`. After crt-021, these calls become
`build_typed_relation_graph(entries, &[])` — passing an empty edges slice so Supersedes edges
are derived from `entries.supersedes` only (authoritative source, Architecture §1 pass 2).

The full list of existing tests that must pass without expectation changes:

| Test Name | Validates |
|-----------|-----------|
| `cycle_two_node_detected` | Two-node cycle → `CycleDetected` |
| `cycle_three_node_detected` | Triangle cycle → `CycleDetected` |
| `cycle_self_referential_detected` | Self-loop → `CycleDetected` |
| `valid_dag_depth_1` | Depth-1 chain is valid DAG |
| `valid_dag_depth_2` | Depth-2 chain is valid DAG |
| `valid_dag_depth_3` | Depth-3 chain is valid DAG |
| `empty_entry_slice_is_valid_dag` | Empty slice → zero-node valid graph |
| `single_entry_no_supersedes` | Single node, no edges |
| `edge_direction_pred_to_successor` | Edge A→B when B.supersedes=Some(A.id) |
| `penalty_range_all_scenarios` | All 6 priority cases: orphan, dead-end, partial, clean depth-1, depth-2, depth-5 |
| `penalty_absent_node_returns_one` | Node not in graph → 1.0 |
| `orphan_softer_than_clean_replacement` | `ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY` |
| `two_hop_harsher_than_one_hop` | depth-2 penalty < depth-1 penalty |
| `partial_supersession_softer_than_clean` | `PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY` |
| `terminal_active_three_hop_chain` | DFS returns terminal at depth 3 |
| `terminal_active_depth_one_chain` | DFS returns terminal at depth 1 |
| `terminal_active_superseded_intermediate_skipped` | Skips superseded intermediates |
| `terminal_active_no_reachable` | All successors deprecated → None |
| `terminal_active_absent_node` | Node not in graph → None |
| `terminal_active_depth_cap` | Chain of 12 entries (terminal at depth 11) → None |
| `terminal_active_depth_boundary` | Chain of 11 entries (terminal at depth 10) → Some |
| `dangling_supersedes_ref_is_skipped` | Dangling ref skipped, no panic |
| `dead_end_softer_than_orphan` | `DEAD_END_PENALTY < ORPHAN_PENALTY` |
| `fallback_softer_than_clean` | `FALLBACK_PENALTY > CLEAN_REPLACEMENT_PENALTY` |
| `decay_formula_depth_1` | penalty == CLEAN_REPLACEMENT_PENALTY |
| `decay_formula_depth_2` | penalty == 0.24 |
| `decay_formula_depth_5_clamped` | penalty clamped to 0.10 |
| `decay_formula_depth_10_clamped` | penalty clamped to 0.10 |
| `decay_never_exceeds_clean_replacement` | All depths 1..10 in range |
| `all_active_no_penalty` | 5 active nodes, 0 edges, absent node returns 1.0 |
| `terminal_active_starting_node_is_active` | Starting node is already terminal → Some(self) |
| `two_successors_one_active_one_deprecated` | Two successors → PARTIAL_SUPERSESSION_PENALTY |
| `node_id_zero_not_in_graph` | node_id=0 absent → 1.0, no panic |
| `graph_penalty_entry_not_in_slice` | Node in graph but entries slice empty → 1.0 |

**Assertion**: `cargo test -p unimatrix-engine -- graph` passes with zero failures.
Zero test expectations may be modified.

---

## New Tests: RelationType Round-Trip (AC-02, AC-20)

### `test_relation_type_roundtrip_all_variants`
- Arrange: construct each of five variants
- Act: call `as_str()` on each; call `from_str(s)` on each result
- Assert: `from_str(v.as_str()) == Some(v)` for all five variants
- Assert: string values are exactly `"Supersedes"`, `"Contradicts"`, `"Supports"`,
  `"CoAccess"`, `"Prerequisite"` (case-sensitive)

### `test_relation_type_from_str_unknown_returns_none` (R-10)
- Arrange: unknown strings: `""`, `"unknown"`, `"supersedes"` (wrong case), `"SUPERSEDES"`
- Act: call `RelationType::from_str(s)` for each
- Assert: all return `None`, no panic

### `test_relation_type_prerequisite_no_write_path` (AC-20)
- Structural test: `RelationType::Prerequisite` exists in enum and round-trips
- Grep assertion (gate checklist): zero INSERT or analytics write references to `"Prerequisite"`
  in implementation files

---

## New Tests: RelationEdge and weight validation (AC-03, R-07)

### `test_relation_edge_weight_validation_rejects_nan`
- Assert: `f32::NAN.is_finite()` is false; the weight guard function returns an error
  or logs and refuses to proceed for NaN inputs

### `test_relation_edge_weight_validation_rejects_inf`
- Assert: `f32::INFINITY.is_finite()` is false; the weight guard rejects `+Inf`

### `test_relation_edge_weight_validation_rejects_neg_inf`
- Assert: `f32::NEG_INFINITY.is_finite()` is false; the weight guard rejects `-Inf`

### `test_relation_edge_weight_validation_passes_valid`
- Assert: `0.0_f32`, `0.5_f32`, `1.0_f32`, `f32::MAX` all pass validation (`.is_finite()` true)

---

## New Tests: Mixed Edge Type Regression (R-01, R-02, AC-11)

### `test_graph_penalty_identical_with_mixed_edge_types`
- Arrange: build `TypedRelationGraph` with a Supersedes chain A→B (B active);
  then add Contradicts edge A→B and CoAccess edge A→C to the same graph
- Act: call `graph_penalty(A.id, &graph, &entries)`
- Assert: penalty equals `CLEAN_REPLACEMENT_PENALTY` — same as graph with Supersedes only

### `test_find_terminal_active_ignores_non_supersedes_edges`
- Arrange: build graph where node A has:
  - CoAccess edge to C (C is active terminal)
  - No Supersedes edges from A
- Act: call `find_terminal_active(A.id, &graph, &entries)`
- Assert: returns `None` (CoAccess edge is not followed), not `Some(C.id)`

### `test_edges_of_type_filters_correctly`
- Arrange: build a `TypedRelationGraph` node with edges of types: Supersedes, CoAccess, Contradicts
- Act: call `edges_of_type(node_idx, RelationType::Supersedes, Direction::Outgoing)`
- Assert: iterator yields only the Supersedes edge, count = 1

### `test_cycle_detection_on_supersedes_subgraph_only`
- Arrange: build entries where CoAccess edges would form a cycle if traversed (A↔B),
  but Supersedes edges form a valid DAG (C→D only)
- Act: call `build_typed_relation_graph(entries, edges_including_coaccess)`
- Assert: returns `Ok` — cycle detection operates only on Supersedes edges, CoAccess edges
  do not trigger false cycle detection

---

## New Tests: bootstrap_only Structural Exclusion (R-03, AC-12)

### `test_build_typed_graph_excludes_bootstrap_only_edges`
- Arrange: `GraphEdgeRow` list with one Supersedes edge where `bootstrap_only=true`
- Act: call `build_typed_relation_graph(entries, &edges)`
- Assert: resulting graph has zero edges in `inner`

### `test_build_typed_graph_includes_confirmed_excludes_bootstrap`
- Arrange: two `GraphEdgeRow`s for the same source node:
  - one Supersedes edge `bootstrap_only=false`
  - one Supersedes edge `bootstrap_only=true`
- Act: call `build_typed_relation_graph(entries, &edges)`
- Assert: graph has exactly one edge (the confirmed one)

### `test_graph_penalty_with_bootstrap_only_supersedes_returns_no_chain_penalty`
- Arrange: entries A and B where A would be superseded by B;
  `GRAPH_EDGES` row has `bootstrap_only=true` for this edge
- Act: call `build_typed_relation_graph(entries, &edges_with_bootstrap_only)`;
  then call `graph_penalty(A.id, &graph, &entries)`
- Assert: penalty is NOT `CLEAN_REPLACEMENT_PENALTY` (bootstrap edge absent from graph,
  so A has no outgoing Supersedes edges — returns orphan or 1.0 based on A's status)

---

## New Tests: edges_of_type Filter Boundary (R-02)

### `test_edges_of_type_empty_graph_returns_empty_iterator`
- Build an empty `TypedRelationGraph`, call `edges_of_type` on a node
- Assert: iterator is empty, no panic

### `test_no_direct_edges_directed_calls_at_penalty_sites`
- Code review gate: assert that `graph_penalty`, `find_terminal_active`,
  `dfs_active_reachable`, `bfs_chain_depth` do NOT contain `.edges_directed(` or
  `.neighbors_directed(` calls directly — all traversal goes via `edges_of_type`
- Enforcement: `grep` in CI gate checklist

---

## New Tests: Supersedes Edge Source Authority (R-12)

### `test_supersedes_edges_from_entries_not_graph_edges_table`
- Arrange: entry A with `supersedes = Some(B.id)` in `all_entries`;
  provide NO matching `GraphEdgeRow` in the edges slice
- Act: call `build_typed_relation_graph(all_entries, &empty_edges)`
- Assert: resulting graph contains the A→B Supersedes edge
  (confirming `entries.supersedes` is the authoritative source for Supersedes edges, not GRAPH_EDGES rows)

---

## New Tests: Empty Graph and Edge Cases

### `test_build_typed_graph_with_zero_edges_returns_valid_empty_graph`
- Arrange: non-empty entries, empty `edges: &[]`
- Act: call `build_typed_relation_graph(&entries, &[])`
- Assert: returns `Ok`, graph has correct node count, zero edges

### `test_graph_penalty_on_orphan_node_with_no_supersedes_edges`
- Arrange: entry with status=Deprecated, no edges in graph
- Assert: `graph_penalty` returns `ORPHAN_PENALTY`, no panic

### `test_build_typed_graph_skips_edge_with_unmapped_node_id`
- Arrange: `GraphEdgeRow` referencing a `source_id` not in `all_entries`
- Act: `build_typed_relation_graph`
- Assert: returns `Ok`, unmapped edge is skipped with a warning log, no panic

---

## Test Helpers

All engine-types tests use `make_entry(id, status, supersedes, superseded_by)` helper from
the existing test module (no change needed).

`GraphEdgeRow` construction for test use:
```rust
fn make_edge_row(source_id: u64, target_id: u64, relation_type: &str,
                 weight: f32, bootstrap_only: bool) -> GraphEdgeRow {
    GraphEdgeRow {
        source_id, target_id,
        relation_type: relation_type.to_string(),
        weight,
        created_at: 0,
        created_by: "test".to_string(),
        source: "test".to_string(),
        bootstrap_only,
    }
}
```

All tests are synchronous (`#[test]`), no async needed — engine functions are pure.
