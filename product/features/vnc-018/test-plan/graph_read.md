# vnc-018 Test Plan: mcp/graph_read.rs

## Component Scope

All logic in `crates/unimatrix-server/src/mcp/graph_read.rs`:
- `GraphParams` (struct, validation)
- `EdgeRecord`, `Truncated`, `ChainResult`, `CurrentResponse`, `NeighborsResponse`
- `handle_graph` — top-level entry point, validation ordering, mode dispatch
- `validate_no_unsupported_params` — centralized param check
- `handle_chain` — SQL CTE supersession chain walk
- `handle_current` — SQL CTE terminal-active lookup
- `handle_neighbors` — depth=1 SQL + depth>1 BFS dispatch
- `follow_to_current` — 50-hop helper
- `node_index_for` accessor (unimatrix-engine/src/graph.rs — tested here because BFS depends on it)

---

## Unit Test Expectations

### Module: `validate_no_unsupported_params`

**Test: `test_validate_chain_rejects_resolve_supersessions`** (AC-15c, R-08)

```rust
// Arrange
let params = GraphParams {
    mode: "chain".to_string(),
    resolve_supersessions: Some(true),
    ..Default::default()
};
// Act
let result = validate_no_unsupported_params(&params);
// Assert
assert!(result.is_err());
assert_eq!(
    result.unwrap_err(),
    "resolve_supersessions is not applicable to chain mode — chain IS the supersession audit"
);
// Note: must NOT be inside handle_chain; must fire in validate_no_unsupported_params
```

**Test: `test_validate_unrecognized_mode_fires_before_field_check`** (R-04)

```rust
// Arrange: unrecognized mode + forward-compat field present
let params = GraphParams {
    mode: "subgraph".to_string(),
    seed_ids: Some(vec![1, 2, 3]),
    ..Default::default()
};
// Act
let result = validate_no_unsupported_params(&params);
// Assert: error says "unrecognized mode", NOT "seed_ids not supported"
assert!(result.is_err());
let msg = result.unwrap_err();
assert!(msg.contains("unrecognized mode"), "got: {msg}");
assert!(!msg.contains("seed_ids"), "forward-compat check must not fire first, got: {msg}");
```

**Test: `test_validate_walk_mode_error_lists_valid_modes`** (AC-14)

```rust
let params = GraphParams { mode: "walk".to_string(), ..Default::default() };
let result = validate_no_unsupported_params(&params);
assert!(result.is_err());
let msg = result.unwrap_err();
assert!(msg.contains("chain"), "got: {msg}");
assert!(msg.contains("current"), "got: {msg}");
assert!(msg.contains("neighbors"), "got: {msg}");
```

**Tests: forward-compat fields (AC-15b / R-16) — four tests, one per field**

Each test passes one forward-compat field to neighbors mode and asserts the error
contains the field name and the correct future mode hint:

```rust
// seed_ids:
params.seed_ids = Some(vec![1]); → error contains "seed_ids" and "subgraph"
// from_id:
params.from_id = Some(1);       → error contains "from_id" and "path"
// to_id:
params.to_id = Some(1);         → error contains "to_id" and "path"
// max_nodes:
params.max_nodes = Some(10);    → error contains "max_nodes" and "subgraph"
```

Also: pass all four fields simultaneously to confirm no panic and the first-encountered
error is returned (or all four listed — either is acceptable).

---

### Module: `handle_chain`

**Test: `test_handle_chain_nonexistent_id_returns_empty`** (AC-04, R-21)

```rust
// Arrange: fresh database (no entries matching the ID)
// Act: call handle_chain with id=999999
// Assert: ChainResult.entries is empty, no error propagated
// Assert: ChainResult.truncated == Truncated { forward: false, backward: false }
// COMMENT: "chain mode returns empty for non-existent ID — intentionally asymmetric
//          with current mode which returns error. See R-21 and AC-04. Do not unify."
```

**Test: `test_handle_chain_five_entry_chain_both_directions`** (AC-01)

```rust
// Arrange: 5-entry chain A→B→C→D→E in test DB
// Act: call handle_chain with id=C, direction=Both
// Assert: entries = [A, B, C, D, E] ordered oldest to newest
// Assert: truncated == Truncated { forward: false, backward: false }
```

**Test: `test_handle_chain_direction_forward_from_mid_chain`** (AC-02)

```rust
// Same 5-entry chain; call with id=C, direction=Forward
// Assert: entries = [C, D, E], no ancestors
```

**Test: `test_handle_chain_direction_backward_from_mid_chain`** (AC-02)

```rust
// Same chain; call with id=C, direction=Backward
// Assert: entries = [A, B, C], no descendants
```

**Test: `test_handle_chain_truncated_forward_only`** (AC-03, AC-03b, R-02)

```rust
// Arrange: synthetic 55-hop forward chain from seed, 3-hop backward chain from seed
// Act: call with direction=Both
// Assert: truncated.forward == true, truncated.backward == false
// Assert: backward entries count == 3 (returned in full)
// Assert: forward entries count == 50 (capped)
// WIRE FORMAT CHECK: serialize the response to JSON; inspect the raw string
// Assert: JSON contains `"truncated":{"forward":true,"backward":false}`
// Assert: JSON does NOT contain `"truncated":true` (flat bool would fail here)
```

This is the definitive AC-03b test. The wire format check (not just deserialized struct)
is the only way to catch an accidental `#[serde(flatten)]` or wrong type.

---

### Module: `handle_current`

**Test: `test_handle_current_active_entry_returns_self`** (AC-05)

```rust
// Arrange: store active entry with no superseded_by
// Act: call handle_current with that ID
// Assert: Ok(CurrentResponse) with entry.id == input id, entry.status == "Active"
```

**Test: `test_handle_current_nonexistent_id_returns_error`** (AC-05a, R-21)

```rust
// Act: call handle_current with id=999999
// Assert: Err("no active terminal found" or equivalent)
// COMMENT: "Intentionally asymmetric with chain mode (returns empty for same ID).
//          This asymmetry is correct by design — current is a lookup that must
//          succeed or fail, not a traversal that can return empty. See R-21."
```

**Test: `test_handle_current_deprecated_resolves_to_active_terminal`** (AC-06)

```rust
// Arrange: A(deprecated, superseded_by=B), B(deprecated, superseded_by=C), C(active)
// Act: call handle_current with id=A
// Assert: Ok, returned entry.id == C.id, entry.status == "Active"
```

**Test: `test_handle_current_orphaned_deprecated_returns_error`** (AC-06b, R-20)

```rust
// Arrange: store entry D; set D.status = Deprecated, D.superseded_by = NULL
// (simulate context_deprecate with no successor)
// Act: call handle_current with id=D
// Assert: Err("no active terminal found" or equivalent)
// Assert: error is the same message produced for a non-existent ID (AC-05a)
// COMMENT: "This is the only test that catches an accidentally omitted
//          `AND e.status = 'Active'` filter in the CTE. Without this filter,
//          D would be returned as if it were an active terminal (R-20 Critical risk)."
```

**Test: `test_handle_current_50_hop_cap_returns_error`** (AC-07)

```rust
// Arrange: synthetic 55-hop chain with no active terminal (all superseded)
// Act: call handle_current with id of head entry
// Assert: Err containing "50-hop" or "safety cap"
```

---

### Module: `handle_neighbors`

**Test: `test_handle_neighbors_depth1_outgoing`** (AC-08)

```rust
// Arrange: write Prerequisite edges X→Y, X→Z in GRAPH_EDGES
// Act: call handle_neighbors id=X, edge_types=["Prerequisite"], direction=Outgoing, depth=1
// Assert: NeighborsResponse.edges contains records for Y and Z
// Assert: each record has direction="outgoing", depth=1, relation_type="Prerequisite"
```

**Test: `test_handle_neighbors_depth1_incoming`** (AC-09)

```rust
// Arrange: write Supports Y→X, Z→X
// Act: call handle_neighbors id=X, edge_types=["Supports"], direction=Incoming, depth=1
// Assert: edges contain Y and Z with direction="incoming"
```

**Test: `test_handle_neighbors_all_types_excludes_supersedes_silently`** (AC-10, AC-10a, R-06)

```rust
// Arrange: X→Y (Supports), X→Z (Informs), X→W (Supersedes)
// Act: call with edge_types=[], direction=Both, depth=1
// Assert: Y and Z in edges; W NOT in edges
// Assert: no "excluded_types" key in serialized JSON response
// Assert: no "warnings" key in serialized JSON response (AC-10a)
```

**Test: `test_handle_neighbors_depth2_bfs_multi_hop`** (AC-11)

```rust
// Arrange: X→Y (Supports), Y→Z (Informs) in GRAPH_EDGES and in-memory graph
// (for depth>1, must pre-load TypedRelationGraph via test harness or mock)
// Act: call handle_neighbors id=X, depth=2
// Assert: Y in edges at depth=1, Z in edges at depth=2
// Assert: EdgeRecord.depth field matches expected values
```

**Test: `test_handle_neighbors_bfs_deduplicates_by_node_id`** (AC-11a, R-18)

```rust
// Arrange: X→Y (direct, depth=1) AND X→Z→Y (Z at depth=1, Y also reachable at depth=2)
// In-memory graph must have both paths.
// Act: call handle_neighbors id=X, depth=2
// Assert: Y appears exactly once in edges
// Assert: Y's depth == 1 (shallowest path wins)
// Assert: no second EdgeRecord with target=Y at depth=2
// COMMENT: "Visited set is HashSet<u64> keyed by node_id only (not (node_id, depth)).
//          A duplicate at depth=2 means the visited set was incorrectly keyed by
//          (node_id, depth). This is the definitive test for AC-11a. See R-18."
```

**Test: `test_handle_neighbors_resolve_supersessions_true`** (AC-12, R-10)

```rust
// Arrange: write edge X→Y; correct Y→Z (Y now deprecated, superseded_by=Z)
// Act: call handle_neighbors id=X, resolve_supersessions=true, depth=1
// Assert: edges contain Z, not Y
// Assert: no panic when follow_to_current is called
```

**Test: `test_handle_neighbors_resolve_supersessions_false`** (AC-13)

```rust
// Same setup; call with resolve_supersessions=false
// Assert: edges contain Y (deprecated original target)
```

**Test: `test_handle_neighbors_resolve_supersessions_none_fallback_graceful`** (R-10)

```rust
// Arrange: write edge X→Y; Y is orphaned deprecated (superseded_by=NULL, status=Deprecated)
// Act: call handle_neighbors id=X, resolve_supersessions=true, depth=1
// Assert: Y appears in edges as-is (follow_to_current returns None, fallback to original)
// Assert: no panic, no error propagated
// COMMENT: "follow_to_current returns None for orphaned deprecated node;
//          caller must use original ID as fallback (ADR-005 spec)."
```

**Test: `test_handle_neighbors_depth_out_of_range`** (R-11)

```rust
// depth=0: assert error (valid range is 1..=10)
// depth=11: assert error
// depth=1: assert Ok (boundary valid)
// depth=10: assert Ok (boundary valid) — may return empty if graph sparse
```

**Test: `test_handle_neighbors_direction_invalid_for_mode`** (R-17)

```rust
// Call neighbors with direction="forward" (chain-mode vocabulary)
// Assert error; error message references valid neighbors directions: incoming, outgoing, both
// Call neighbors with direction="incoming" → Ok
// Call neighbors with direction="both" → Ok
```

**Test: `test_handle_neighbors_supersedes_explicit_rejection`** (AC-15a, R-06)

```rust
// Call neighbors with edge_types=["Supersedes"]
// Assert exact error string:
// "Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation"
```

**Test: `test_handle_neighbors_supersedes_in_mixed_list_rejected`** (R-06)

```rust
// Call neighbors with edge_types=["Supersedes", "Supports"]
// Assert error fires — one valid type alongside Supersedes does not bypass rejection
```

**Test: `test_handle_neighbors_unknown_edge_type`** (AC-15)

```rust
// Call neighbors with edge_types=["BogusEdge"]
// Assert error before traversal; no edges in response
```

---

### Module: `follow_to_current`

**Test: `test_follow_to_current_active_entry_returns_self`**

```rust
// Arrange: active entry with no superseded_by
// Act: follow_to_current(store, id)
// Assert: Some(id) — returns the entry itself
```

**Test: `test_follow_to_current_chain_resolves`**

```rust
// Arrange: A(superseded_by=B), B(superseded_by=C), C(active)
// Assert: Some(C.id)
```

**Test: `test_follow_to_current_orphaned_returns_none`** (R-10)

```rust
// Arrange: entry deprecated, superseded_by=NULL
// Assert: None
```

**Test: `test_follow_to_current_50_hop_cap_returns_none`** (R-10)

```rust
// Arrange: synthetic 55-hop chain with no active terminal
// Assert: None (cap fires at 50)
// Assert: no panic
```

---

### Struct: `EdgeRecord` serialization

**Test: `test_edge_record_metadata_serializes_as_null`** (R-15, NFR-07)

```rust
// Arrange: EdgeRecord with metadata: None
// Act: serde_json::to_string(&record)
// Assert: serialized JSON contains `"metadata":null` (the key is present)
// Assert: serialized JSON does NOT omit the metadata field
// COMMENT: "EdgeRecord.metadata must serialize as null, not be absent.
//          skip_serializing_if = 'Option::is_none' is prohibited on this field. (ADR-004)"
```

---

### Accessor: `TypedRelationGraph::node_index_for` (ADR-008)

**Test: `test_node_index_for_known_node_returns_index`** (R-07, AC-11)

```rust
// Arrange: TypedRelationGraph with node ID 42 registered
// Act: graph.node_index_for(42)
// Assert: Some(node_index) — index is correct for node 42
```

**Test: `test_node_index_for_unknown_node_returns_none`** (R-07)

```rust
// Act: graph.node_index_for(999999)
// Assert: None
```

---

## Integration Test Expectations (through MCP interface)

All of the following are exercised by the Python infra-001 suite.

### AC-20 coverage (all three modes)

- `test_graph_chain_basic` — chain mode returns entries in order
- `test_graph_current_resolves_deprecated` — current follows superseded_by to active terminal
- `test_graph_neighbors_outgoing_depth1` — neighbors returns typed-edge records

### Critical behavioral pair (AC-04 / AC-05a / R-21)

Both of these tests must exist as a matched pair. A comment in each must state that
the asymmetry is intentional design:

- `test_graph_chain_nonexistent_id_returns_empty` — chain mode, id=999999, empty result
- `test_graph_current_nonexistent_id_returns_error` — current mode, same id, error response

### R-20 orphaned deprecated terminal

- `test_graph_current_orphaned_deprecated_returns_error` — context_deprecate a node with no
  successor; call current; assert "no active terminal found" error; assert deprecated entry
  not returned. This is the only test that catches an omitted `AND e.status = 'Active'`.

### R-03 depth staleness (documents expected behavior)

- `test_graph_neighbors_depth1_sees_fresh_write` — depth=1 live SQL, fresh write visible
- `test_graph_neighbors_depth2_does_not_see_fresh_write` — depth=2 BFS, fresh write absent
  (marked with appropriate comment that absence is expected, not a bug)

### R-15 EdgeRecord.metadata null in wire format

- `test_graph_neighbors_edgerecord_metadata_is_null` — raw JSON check: metadata key present with value null

---

## Edge Cases Requiring Explicit Tests

| Edge Case | Test Name | Assertion |
|-----------|-----------|-----------|
| chain on mid-chain node (has both ancestors and descendants) | `test_handle_chain_five_entry_chain_both_directions` | Both directions returned |
| chain with single entry (no supersessions) | `test_handle_chain_single_entry_no_supersessions` | entries=[entry], truncated={false,false} |
| current on already-active entry | `test_handle_current_active_entry_returns_self` | Returns same entry |
| neighbors with 15 non-Supersedes types explicitly listed | `test_handle_neighbors_all_15_explicit_types_valid` | No error; same as edge_types=[] |
| neighbors depth=1 with zero edges | `test_handle_neighbors_zero_edges_returns_empty` | Empty NeighborsResponse, no error |
| neighbors non-existent anchor ID | `test_handle_neighbors_nonexistent_id_returns_empty` | Empty response (OQ-01 resolved: consistent with chain mode) |
| chain on id=0 or u64::MAX | `test_handle_chain_boundary_ids` | Empty result, no integer overflow |

---

## Risks Specifically Addressed in This Component

- R-01: All chain/current tests use SQL CTE path — any test using in-memory graph would
  reveal the in-memory path is being called (cold-start test validates this)
- R-02: AC-03b wire format test catches flat bool serialization
- R-04: `test_validate_unrecognized_mode_fires_before_field_check` catches wrong ordering
- R-06: Both Supersedes exclusion paths (AC-15a and AC-10/AC-10a) tested independently
- R-08: AC-15c check is in `validate_no_unsupported_params` unit test, not `handle_chain`
- R-10: `follow_to_current` None path tested with orphaned and 50-hop cases
- R-11: depth range validation both boundaries tested
- R-15: EdgeRecord.metadata serialization asserted via raw JSON inspection
- R-17: Invalid direction value for neighbors mode produces mode-specific error
- R-18: AC-11a BFS dedup test asserts single entry at minimum depth
- R-20: orphaned-deprecated test — the only guard for omitted `AND e.status = 'Active'`
- R-21: chain/current asymmetry pair with explicit test comments
