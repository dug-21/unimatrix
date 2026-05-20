# Test Plan: `graph_read_subgraph.rs`

## Scope

All behavioral tests for the BFS algorithm, parameter validation, and metadata hydration
live in `graph_read_subgraph_tests.rs` (new file, `#[path]`-declared from `graph_read_subgraph.rs`).
This is the bulk of the vnc-019 test surface.

Test helper: `open_test_store()` (same pattern as `graph_read_neighbors_tests.rs`).
Graph builder: `build_typed_relation_graph(&[entries], &[relation_edges])`.

---

## Section A: Parameter Validation

### A-1. seed_ids validation (AC-07)

```rust
#[tokio::test]
async fn test_validate_seed_ids_absent_error() {
    // AC-07: seed_ids absent → validation error with exact message.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: None,
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert_eq!(msg, "subgraph mode requires at least one entry ID in seed_ids",
        "exact error message required, got: {msg}");
}

#[tokio::test]
async fn test_validate_seed_ids_empty_error() {
    // AC-07: seed_ids=[] → same exact error.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert_eq!(msg, "subgraph mode requires at least one entry ID in seed_ids");
}
```

### A-2. max_depth validation (AC-06, R-06)

```rust
#[tokio::test]
async fn test_validate_max_depth_boundary_values() {
    // AC-06: max_depth=0 and max_depth=11 rejected; max_depth=1 and max_depth=10 accepted.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    for bad in [0u8, 11u8] {
        let params = GraphParams {
            mode: "subgraph".to_string(),
            seed_ids: Some(vec![1]),
            max_depth: Some(bad),
            ..Default::default()
        };
        let result = handle_subgraph(&store, &handle, &params).await;
        assert!(result.is_err(), "max_depth={bad} must be rejected");
        let msg = result.unwrap_err().message;
        assert!(msg.contains("max_depth") && msg.contains("1..=10"),
            "error must state range, got: {msg}");
    }

    for good in [1u8, 10u8] {
        let params = GraphParams {
            mode: "subgraph".to_string(),
            seed_ids: Some(vec![u64::MAX]), // absent from graph, returns empty result
            max_depth: Some(good),
            ..Default::default()
        };
        let result = handle_subgraph(&store, &handle, &params).await;
        assert!(result.is_ok(), "max_depth={good} must be accepted, got: {result:?}");
    }
}
```

### A-3. max_nodes validation (R-07, AC-05)

```rust
#[tokio::test]
async fn test_validate_max_nodes_above_200_rejected() {
    // R-07: max_nodes=201 → validation error with exact range message.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        max_nodes: Some(201),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("max_nodes") && msg.contains("1..=200"),
        "error must state range, got: {msg}");
    assert!(msg.contains("201"), "error must echo the bad value, got: {msg}");
}

#[tokio::test]
async fn test_validate_max_nodes_zero_rejected() {
    // R-07: max_nodes=0 → validation error.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        max_nodes: Some(0),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validate_max_nodes_200_accepted() {
    // R-07: max_nodes=200 is the valid upper bound.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![u64::MAX]),
        max_nodes: Some(200),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok(), "max_nodes=200 must be accepted, got: {result:?}");
}
```

### A-4. edge_types validation (AC-08, R-14)

```rust
#[tokio::test]
async fn test_validate_unknown_edge_type_error() {
    // AC-08: unrecognized edge_type string → validation error naming the value.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        edge_types: Some(vec!["BogusEdgeType".to_string()]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("BogusEdgeType"), "error must echo the bad value, got: {msg}");
    // Error must list recognized types (at least one as sample):
    assert!(msg.contains("Supports") || msg.contains("Contradicts"),
        "error must list recognized types, got: {msg}");
}

#[tokio::test]
async fn test_validate_edge_types_absent_defaults_to_all_non_supersedes() {
    // AC-08, R-14: absent edge_types expands to all non-Supersedes types.
    // Verify by checking that a Supports edge IS traversed when edge_types is absent.
    // (Full traversal test — requires a warm graph; use build_typed_relation_graph.)
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    // Graph: A Supports B
    let entry_a = make_entry_for_test(1);
    let entry_b = make_entry_for_test(2);
    let rel = make_supports_edge(1, 2);
    let graph = build_typed_relation_graph(&[entry_a, entry_b], &[rel]).expect("build");
    set_test_graph(&handle, graph).await;

    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        edge_types: None, // absent — should default to all non-Supersedes
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.edges.len(), 1, "Supports edge must be traversed by default");
}

#[tokio::test]
async fn test_validate_edge_types_empty_defaults_to_all_non_supersedes() {
    // AC-08, R-14: edge_types=[] behaves identically to absent edge_types.
    // (Same setup as above, but edge_types: Some(vec![]))
    // Assert: Supports edge is traversed.
}
```

### A-5. direction validation

```rust
#[tokio::test]
async fn test_validate_direction_invalid_rejected() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        direction: Some("forward".to_string()), // invalid for subgraph
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    assert!(result.is_err());
    let msg = result.unwrap_err().message;
    assert!(msg.contains("direction"), "got: {msg}");
}
```

---

## Section B: BFS Traversal — Core Algorithm

### B-1. R-01: resolve_supersessions ordering (Critical)

```rust
#[tokio::test]
async fn test_bfs_resolve_supersessions_before_visited_check() {
    // R-01, AC-09: substitution happens BEFORE visited check.
    // Graph: A(active) → B(deprecated, superseded_by C) → C(active).
    // Call with seed=[A], resolve_supersessions=true.
    // Arrange:
    //   entry_a: id=1, status=Active
    //   entry_b: id=2, status=Deprecated, superseded_by=3
    //   entry_c: id=3, status=Active
    //   edge: A Supports B (stored in graph_edges)
    //   DB: entry B has superseded_by=3 in ENTRIES
    // Act: handle_subgraph(seed_ids=[1], resolve_supersessions=true, max_depth=2)
    // Assert:
    //   - resp.nodes contains id=1 (A)
    //   - resp.nodes contains id=3 (C, the terminal)
    //   - resp.nodes does NOT contain id=2 (B, the deprecated node)
    //   - id=3 appears exactly once
}

#[tokio::test]
async fn test_bfs_resolve_supersessions_dedup_via_multiple_paths() {
    // R-01: terminal node reachable via two paths appears once.
    // Graph: A → B(deprecated→C); D → C directly.
    // Call with seed=[A, D], resolve_supersessions=true.
    // Assert: C appears exactly once in nodes.
}

#[tokio::test]
async fn test_bfs_resolve_supersessions_false_includes_deprecated() {
    // AC-10, R-01: resolve_supersessions=false leaves deprecated nodes as-is.
    // Same graph. Assert: B present in nodes; C also present if directly reachable.
}
```

### B-2. R-02: direction="both" deduplication (Critical)

```rust
#[tokio::test]
async fn test_bfs_direction_both_single_edge_no_duplicate() {
    // R-02, AC-12: single stored A→B edge; call with seed=[A,B], direction="both";
    // exactly one EdgeRecord in response.
    // Arrange: graph with single A Supports B edge.
    // Act: handle_subgraph(seed_ids=[A,B], direction="both")
    // Assert: len(resp.edges) == 1
}

#[tokio::test]
async fn test_bfs_direction_both_canonical_direction_on_edge_record() {
    // R-02, AC-12: direction field on returned EdgeRecord is always "outgoing".
    // Same setup. Assert: resp.edges[0].direction == "outgoing"
    // Assert: resp.edges[0].source_id == A, resp.edges[0].target_id == B
}

#[tokio::test]
async fn test_bfs_direction_both_multihop_no_duplicates() {
    // R-02: A→B, B→C; seed=[A], direction="both", max_depth=2.
    // Each edge appears once. Total edges: 2.
    // Assert: len(resp.edges) == 2
    // Assert: no edge with the same (source, target, rel_type) appears twice.
}
```

### B-3. R-03: Seed count at cap boundary (Critical)

```rust
#[tokio::test]
async fn test_bfs_seed_count_exceeds_max_nodes_truncated() {
    // R-03, AC-05: 201 seeds with max_nodes default (200) → truncated=true, depth_reached=0.
    // Note: in unit test context, use max_nodes=5 with 6 seed IDs to avoid 201-entry setup.
    // Arrange: add 6 entries to graph; max_nodes=5.
    // Act: handle_subgraph(seed_ids=[1..=6], max_nodes=5)
    // Assert: len(resp.nodes) == 5
    //         resp.truncated == true
    //         resp.depth_reached == 0 (BFS never ran)
}

#[tokio::test]
async fn test_bfs_seed_count_exactly_at_max_nodes_truncated() {
    // R-03: seeds exactly equal max_nodes → truncated=true, BFS skipped.
    // Arrange: 3 entries, max_nodes=3.
    // Assert: len(resp.nodes) == 3, truncated=true, depth_reached=0.
}

#[tokio::test]
async fn test_bfs_seed_partial_budget_bfs_expands_remainder() {
    // R-03: 2 seeds + max_nodes=5; BFS should expand up to 3 more nodes.
    // Arrange: 5 entries forming a star graph from seed A; 1 additional seed B.
    // Assert: len(resp.nodes) <= 5; truncated=false if graph has exactly 5.
}
```

### B-4. R-04: Empty OR-chain guard (High)

```rust
#[tokio::test]
async fn test_bfs_no_edges_skips_metadata_query() {
    // R-04, AC-19: isolated seed (no edges of requested type) → edges=[], no SQL error.
    // Arrange: single entry in graph; no edges.
    // Act: handle_subgraph(seed_ids=[1], edge_types=["Supports"])
    // Assert:
    //   - result is Ok (no SQL error)
    //   - resp.nodes == [entry_1]
    //   - resp.edges == []
    //   - resp.truncated == false
    //   - resp.depth_reached == 0
    // Verification of metadata query skip: structural — if the query were issued with
    // an empty WHERE clause, SQLite would return a syntax error, so Ok proves it was skipped.
}

#[tokio::test]
async fn test_bfs_empty_graph_cold_start_no_error() {
    // R-04, IR-05, AC-17: TypedRelationGraph is empty (cold start); all seeds absent.
    // Act: handle_subgraph(seed_ids=[u64::MAX], default params)
    // Assert:
    //   - result is Ok
    //   - resp.nodes == []
    //   - resp.edges == []
    //   - resp.truncated == false
    //   - resp.depth_reached == 0
    //   - resp.seed_ids == [u64::MAX]
}
```

### B-5. R-06: Circular supersession termination (High)

```rust
#[tokio::test]
async fn test_bfs_circular_supersession_terminates() {
    // R-06: A.superseded_by=B, B.superseded_by=A (circular).
    // Call with resolve_supersessions=true. Assert: completes without hang; no panic.
    // Test uses a short timeout via tokio::time::timeout(Duration::from_secs(5), ...).
    // Assert: result is Ok (fallback behavior) within timeout.
}

#[tokio::test]
async fn test_bfs_supersession_chain_50_hops_terminates() {
    // R-06: supersession chain of exactly 50 hops (A0→A1→...→A49).
    // follow_to_current should reach the terminal at A49.
    // Assert: call completes; no infinite loop.
}
```

### B-6. R-07: max_nodes cap — never exceeded

Already covered in A-3 (validation) and B-3 (seed phase). Additional BFS-phase cap test:

```rust
#[tokio::test]
async fn test_bfs_max_nodes_cap_during_bfs_truncates() {
    // R-07: cap fires mid-BFS (not during seed phase).
    // Arrange: 1 seed + dense graph expanding to 10 nodes, max_nodes=3.
    // Assert: len(resp.nodes) == 3; resp.truncated == true; resp.depth_reached >= 1.
}
```

---

## Section C: BFS Traversal — Correctness

### C-1. R-08: depth_reached accuracy

```rust
#[tokio::test]
async fn test_bfs_depth_reached_full_traversal() {
    // R-08, AC-15: A→B→C chain; max_depth=10. depth_reached should be 2.
    // Assert: resp.depth_reached == 2
}

#[tokio::test]
async fn test_bfs_depth_reached_under_truncation() {
    // R-08, AC-15b: A→B→C chain; max_nodes=2. Truncation at depth 1.
    // Assert: resp.truncated == true; resp.depth_reached == 1.
}

#[tokio::test]
async fn test_bfs_depth_reached_zero_no_edges() {
    // R-08, AC-15c: isolated seed. Assert: resp.depth_reached == 0.
}

#[tokio::test]
async fn test_bfs_depth_reached_bounded_by_max_depth() {
    // R-08, AC-15: deep graph; max_depth=1. Assert: resp.depth_reached == 1.
    // No node beyond 1 hop should appear in nodes.
}
```

### C-2. Dangling-edge filter (from ARCHITECTURE.md step 5b)

```rust
#[tokio::test]
async fn test_bfs_dangling_edges_removed_after_truncation() {
    // ARCHITECTURE step 5b: when cap fires, edges referencing truncated nodes
    // must be filtered out of collected_edges.
    // Arrange: A→B, A→C, B→D; max_nodes=3; seed=[A].
    // After BFS: nodes=[A,B,C], D was discovered but truncated.
    // Edge B→D: target D not in collected_node_ids → must be removed.
    // Assert: all resp.edges have source_id AND target_id present in resp.nodes.
    let node_ids: std::collections::HashSet<u64> = resp.nodes.iter().map(|n| n.id).collect();
    for edge in &resp.edges {
        assert!(node_ids.contains(&edge.source_id), "dangling source: {}", edge.source_id);
        assert!(node_ids.contains(&edge.target_id), "dangling target: {}", edge.target_id);
    }
}
```

### C-3. Seed entries always in nodes (AC-04)

```rust
#[tokio::test]
async fn test_bfs_isolated_seed_present_in_nodes() {
    // AC-04: seed entry with no matching edges is still in nodes.
    // Arrange: single entry in graph; no edges of requested type.
    // Act: handle_subgraph(seed_ids=[1])
    // Assert: resp.nodes contains entry with id=1.
}

#[tokio::test]
async fn test_bfs_duplicate_seed_ids_deduped() {
    // Edge case from RISK-TEST-STRATEGY: all seed_ids same value.
    // Arrange: one entry in graph; seed_ids=[1, 1, 1].
    // Assert: resp.nodes contains exactly one node with id=1.
}
```

### C-4. R-12: Edge depth under multi-path discovery

```rust
#[tokio::test]
async fn test_bfs_first_discovery_wins_depth() {
    // R-12: edge A→C discovered from seed A at depth=1;
    // also reachable from B at depth=2 (B is neighbor of A).
    // First discovery (depth=1) wins.
    // Assert: EdgeRecord for A→C has depth=1, not 2.
}

#[tokio::test]
async fn test_bfs_two_paths_to_same_node_single_edge() {
    // R-12: two seeds [A, B]; both have Supports edge to C.
    // C should appear once in nodes; edge A→C and B→C are different stored edges
    // (different source), so both may appear. Verify no duplicate for any single triple.
}
```

### C-5. R-13: follow_to_current None fallback

```rust
#[tokio::test]
async fn test_bfs_follow_to_current_none_fallback_to_original() {
    // R-13: follow_to_current returns None (superseded_by → non-existent ID).
    // With resolve_supersessions=true, the original deprecated node is used as fallback.
    // Arrange: entry B with superseded_by=99999 (does not exist in ENTRIES).
    //          edge A→B in graph.
    // Act: handle_subgraph(seed=[A], resolve_supersessions=true)
    // Assert: result is Ok; B present in resp.nodes (deprecated, but fallback used).
}
```

---

## Section D: Metadata Hydration

### D-1. R-15: Malformed metadata handling (AC-18)

```rust
#[tokio::test]
async fn test_bfs_malformed_metadata_returns_none() {
    // R-15: malformed JSON in GRAPH_EDGES.metadata → EdgeRecord.metadata is None.
    // Arrange: insert graph edge with metadata='invalid json{' directly in test DB.
    //          build graph with that edge's nodes.
    // Act: handle_subgraph(seed=[source_id])
    // Assert: result is Ok (no panic); resp.edges[0].metadata is None.
}

#[tokio::test]
async fn test_bfs_null_metadata_column_returns_json_null() {
    // R-15, AC-18: metadata column is NULL → EdgeRecord.metadata is JSON null.
    // Arrange: insert edge with no metadata (NULL column).
    // Assert: resp.edges[0].metadata is None (serializes as JSON null).
}

#[tokio::test]
async fn test_bfs_valid_metadata_json_parsed() {
    // R-15, AC-18: valid JSON metadata → parsed serde_json::Value.
    // Arrange: insert edge with metadata='{"key":"value"}'.
    // Act: handle_subgraph including that edge.
    // Assert: resp.edges[0].metadata is Some(json!({ "key": "value" })).
    //         NOT a raw string.
}
```

---

## Section E: Default Edge Types

### E-1. R-14: all_non_supersedes_types expansion

```rust
#[test]
fn test_all_non_supersedes_types_count() {
    // R-14: all_non_supersedes_types() returns exactly 15 types (16 total - Supersedes).
    // Using graph_read_neighbors::all_non_supersedes_types via pub(super).
    let types = super::graph_read_neighbors::all_non_supersedes_types();
    assert_eq!(types.len(), 15, "expected 15 non-Supersedes relation types");
    // Supersedes must not be in the list:
    use unimatrix_engine::graph::RelationType;
    assert!(!types.contains(&RelationType::Supersedes),
        "Supersedes must not be in default traversal types");
}

#[tokio::test]
async fn test_validate_supersedes_edge_type_explicit_accepted() {
    // AC-08: explicit edge_types=["Supersedes"] is accepted (not rejected like neighbors mode).
    // In subgraph mode, Supersedes edges can be explicitly traversed.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![u64::MAX]),
        edge_types: Some(vec!["Supersedes".to_string()]),
        ..Default::default()
    };
    let result = handle_subgraph(&store, &handle, &params).await;
    // Should not fail validation (empty result because seed not in graph, but no validation error)
    assert!(result.is_ok(), "Supersedes must be accepted in subgraph edge_types, got: {result:?}");
}
```

---

## Section F: Batch Node Hydration

### F-1. R-09: Missing ENTRIES row

```rust
// This test requires review of get_many behavior.
// If get_many skips missing IDs (returns partial result), test verifies no panic:
#[tokio::test]
async fn test_bfs_hydration_missing_entry_does_not_panic() {
    // R-09: node in collected_node_ids but deleted from ENTRIES between rebuild and hydration.
    // Simulate by inserting a node into the TypedRelationGraph state WITHOUT storing it in ENTRIES.
    // Act: handle_subgraph discovering that node.
    // Assert: result is Ok; missing node is absent from resp.nodes (silent omission);
    //         other nodes are present.
    // Note: If get_many returns an error on missing IDs (not skip), this test
    // documents the panic vector and the behavior must be changed in implementation.
}
```

---

## Test Helpers Required

The `graph_read_subgraph_tests.rs` file needs these helpers (defined within the test file
or imported from a test utility module):

```rust
// Create a minimal EntryRecord for graph construction
fn make_entry_for_test(id: u64) -> EntryRecord { ... }

// Create a RelationEdge for graph construction
fn make_supports_edge(source: u64, target: u64) -> RelationEdge { ... }

// Open an in-memory test store (same as neighbors tests)
async fn open_test_store() -> (SqlxStore, tempfile::TempDir) { ... }

// Inject a pre-built TypedRelationGraph into the handle (bypassing the tick)
async fn set_test_graph(handle: &Arc<TypedGraphState>, graph: TypedRelationGraph) { ... }
```

The `set_test_graph` helper is critical: without it, BFS tests that require a non-empty
graph must wait for a tick or mock the graph state. The delivery agent must determine
if `TypedGraphState` has an existing test injection method or if one must be added.

---

## Assertions Checklist

### Validation
- [ ] seed_ids absent → exact error string
- [ ] seed_ids=[] → exact error string
- [ ] max_depth=0 → error with "1..=10"
- [ ] max_depth=11 → error with "1..=10"
- [ ] max_depth=1 → accepted
- [ ] max_depth=10 → accepted
- [ ] max_nodes=0 → error with "1..=200"
- [ ] max_nodes=201 → error with "1..=200" and echo "201"
- [ ] max_nodes=200 → accepted
- [ ] unknown edge_type → error names the bad value and lists recognized types
- [ ] direction="forward" → validation error
- [ ] edge_types=None → Supports edges traversed (defaulted to all_non_supersedes_types)
- [ ] edge_types=[] → same as None
- [ ] Supersedes in edge_types (subgraph mode) → accepted (not blocked like neighbors mode)

### BFS Correctness
- [ ] resolve_supersessions=true: deprecated node absent, terminal present
- [ ] resolve_supersessions=true: terminal deduped via visited set when reachable via 2 paths
- [ ] resolve_supersessions=false: deprecated node present as-is
- [ ] direction="both": single stored edge appears once in edges
- [ ] direction="both": all EdgeRecord.direction == "outgoing"
- [ ] seed count > max_nodes: truncated=true, depth_reached=0
- [ ] seed count == max_nodes: truncated=true, depth_reached=0
- [ ] cap fires mid-BFS: truncated=true, depth_reached >= 1
- [ ] cap never exceeded: len(resp.nodes) <= max_nodes always
- [ ] dangling edges removed: all edge.source_id and edge.target_id in resp.nodes
- [ ] isolated seed: present in resp.nodes
- [ ] duplicate seeds: single node in result
- [ ] circular supersession: terminates within timeout
- [ ] 50-hop supersession chain: terminates

### depth_reached
- [ ] full traversal: equals actual max depth of edges
- [ ] truncated at depth N: equals N
- [ ] no edges: equals 0
- [ ] max_depth=1: equals 1 (bounded by param, not graph depth)

### Metadata
- [ ] malformed JSON in column: metadata=None, no panic
- [ ] NULL column: metadata=None (JSON null on wire)
- [ ] valid JSON: metadata parsed as serde_json::Value (not raw string)

### Cold-start / Missing entries
- [ ] all seeds absent from graph: empty result, no error
- [ ] no edges → metadata query skipped (no SQL error from empty WHERE)
- [ ] missing ENTRIES row in hydration: no panic, partial result returned
