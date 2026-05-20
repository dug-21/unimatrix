# Test Plan: `graph_read.rs`

## Scope

This file covers tests for changes made to `graph_read.rs` in vnc-019:

1. `GraphParams` extension — new `max_depth: Option<u8>` field
2. `SubgraphResponse` struct — serialization correctness
3. `validate_no_unsupported_params` — subgraph arm added; existing-mode regressions
4. `handle_graph` dispatch — `"subgraph"` arm delegates to `handle_subgraph`
5. Unrecognized-mode error update — now lists `"subgraph"` as a supported mode

Tests live in `crates/unimatrix-server/src/mcp/graph_read_tests.rs` (existing file,
extended with new cases).

---

## 1. `GraphParams.max_depth` Extension

### Existing Test That Must Be Updated

**`test_validate_unrecognized_mode_fires_before_field_check`** currently uses
`mode="subgraph"` as the unrecognized-mode probe. After vnc-019 delivery, `"subgraph"`
is a recognized mode — this test MUST be updated to use `mode="walk"` or another
unrecognized string. Failure to update = the test silently tests wrong behavior.

### New Test: subgraph mode recognized in validate

```rust
#[test]
fn test_validate_subgraph_mode_recognized() {
    // R-05, AC-11: mode="subgraph" with no unsupported params passes validate.
    // Arrange: minimal valid GraphParams for subgraph
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        ..Default::default()
    };
    // Act
    let result = validate_no_unsupported_params(&params);
    // Assert: no error (subgraph is now a recognized mode)
    assert!(result.is_ok(), "subgraph with seed_ids should pass validate, got: {result:?}");
}
```

### New Test: subgraph mode rejects from_id

```rust
#[test]
fn test_validate_subgraph_rejects_from_id() {
    // R-05, AC-05: from_id is a path-mode parameter; rejected on subgraph.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        from_id: Some(42),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("from_id"), "got: {msg}");
}
```

### New Test: subgraph mode rejects to_id

```rust
#[test]
fn test_validate_subgraph_rejects_to_id() {
    // R-05: to_id is a path-mode parameter; rejected on subgraph.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        to_id: Some(42),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("to_id"), "got: {msg}");
}
```

### New Tests: max_depth rejected on chain/current/neighbors

```rust
#[test]
fn test_validate_chain_rejects_max_depth() {
    // R-05, AC-16: max_depth on chain mode → exact error message.
    let params = GraphParams {
        mode: "chain".to_string(),
        max_depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("max_depth"), "got: {msg}");
    assert!(msg.contains("chain"), "must name the mode, got: {msg}");
    assert!(msg.contains("subgraph"), "must direct to subgraph, got: {msg}");
}

#[test]
fn test_validate_current_rejects_max_depth() {
    // AC-16: max_depth on current mode → validation error.
    let params = GraphParams {
        mode: "current".to_string(),
        max_depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("max_depth") && msg.contains("subgraph"), "got: {msg}");
}

#[test]
fn test_validate_neighbors_rejects_max_depth() {
    // AC-16: max_depth on neighbors mode → validation error.
    let params = GraphParams {
        mode: "neighbors".to_string(),
        max_depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("max_depth") && msg.contains("subgraph"), "got: {msg}");
}
```

### New Test: current mode rejects seed_ids (regression coverage)

```rust
#[test]
fn test_validate_current_rejects_seed_ids() {
    // AC-11: seed_ids rejected on current mode (chain/neighbors already tested).
    let params = GraphParams {
        mode: "current".to_string(),
        seed_ids: Some(vec![1]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("seed_ids"), "got: {msg}");
}
```

### New Test: unrecognized mode lists subgraph

```rust
#[test]
fn test_validate_unrecognized_mode_lists_subgraph() {
    // FR-20: error message for unrecognized mode must list "subgraph" after vnc-019.
    let params = GraphParams {
        mode: "walk".to_string(),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("subgraph"), "subgraph must be listed in supported modes, got: {msg}");
    assert!(msg.contains("chain"), "got: {msg}");
    assert!(msg.contains("current"), "got: {msg}");
    assert!(msg.contains("neighbors"), "got: {msg}");
}
```

---

## 2. `SubgraphResponse` Struct

Tests verify serialization of the new `SubgraphResponse` wire type.

### Test: SubgraphResponse serializes all five fields

```rust
#[test]
fn test_subgraph_response_serializes_all_fields() {
    // AC-01: SubgraphResponse must produce a JSON object with exactly
    // nodes, edges, truncated, seed_ids, depth_reached.
    let resp = SubgraphResponse {
        nodes: vec![],
        edges: vec![],
        truncated: false,
        seed_ids: vec![42],
        depth_reached: 0,
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("\"nodes\""), "missing nodes");
    assert!(json.contains("\"edges\""), "missing edges");
    assert!(json.contains("\"truncated\""), "missing truncated");
    assert!(json.contains("\"seed_ids\""), "missing seed_ids");
    assert!(json.contains("\"depth_reached\""), "missing depth_reached");
}
```

### Test: SubgraphResponse.truncated serializes as bool (not struct)

```rust
#[test]
fn test_subgraph_response_truncated_is_bool() {
    // AC-01: truncated is a flat bool in SubgraphResponse (unlike ChainResult.Truncated).
    let resp = SubgraphResponse {
        nodes: vec![],
        edges: vec![],
        truncated: true,
        seed_ids: vec![1],
        depth_reached: 0,
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("\"truncated\":true"), "truncated must be bool true, got: {json}");
}
```

### Test: SubgraphResponse.depth_reached zero on empty

```rust
#[test]
fn test_subgraph_response_depth_reached_zero() {
    // AC-15c, R-08: depth_reached=0 when no edges traversed.
    let resp = SubgraphResponse {
        nodes: vec![],
        edges: vec![],
        truncated: false,
        seed_ids: vec![99],
        depth_reached: 0,
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("\"depth_reached\":0"), "got: {json}");
}
```

---

## 3. Tool Description — String Match

The tool description review is primarily a code-review task (AC-13), but a unit test
can assert the string contains required disclosure keywords. This test lives in
`graph_read_tests.rs` as a string-constant assertion against the static tool
description defined in `tools.rs`.

### Test: tool description contains all four disclosure categories

```rust
#[test]
fn test_tool_description_contains_staleness_disclosures() {
    // AC-13, R-11: tool description must include all four required disclosures.
    // The const CONTEXT_GRAPH_DESCRIPTION must be accessible here (pub(crate) or via re-export).
    // (a) in-memory BFS tick-window staleness
    assert!(crate::mcp::tools::CONTEXT_GRAPH_DESCRIPTION.contains("tick"),
        "description must mention tick-window staleness");
    // (b) depth_reached + truncated semantics
    assert!(crate::mcp::tools::CONTEXT_GRAPH_DESCRIPTION.contains("depth_reached"),
        "description must explain depth_reached");
    assert!(crate::mcp::tools::CONTEXT_GRAPH_DESCRIPTION.contains("truncated"),
        "description must explain truncated");
    // (c) unknown seed behavior
    assert!(crate::mcp::tools::CONTEXT_GRAPH_DESCRIPTION.contains("empty result"),
        "description must document empty result for unknown seeds");
    // (d) direction always outgoing
    assert!(crate::mcp::tools::CONTEXT_GRAPH_DESCRIPTION.contains("outgoing"),
        "description must note direction is always outgoing");
    // max_nodes range documentation
    assert!(crate::mcp::tools::CONTEXT_GRAPH_DESCRIPTION.contains("200"),
        "description must document max_nodes limit");
}
```

Note: If the description string is not exposed as a named constant, the delivery
agent must either add `pub(crate) const CONTEXT_GRAPH_DESCRIPTION: &str = ...;`
or move this assertion into a `tools_tests.rs` file with appropriate access.
The test plan author flags this as a delivery-time concern.

---

## 4. `handle_graph` Dispatch

The dispatch to `handle_subgraph` is tested behaviorally through `graph_read_subgraph_tests.rs`
and infra-001. No separate dispatch unit test is required: if `handle_subgraph` is not
called, the integration tests in `test_lifecycle.py::test_graph_subgraph_topology_traversal`
will fail to find any edges.

---

## Assertions Checklist

- [ ] `mode="subgraph"` passes `validate_no_unsupported_params` with valid params
- [ ] `mode="subgraph"` + `from_id` → error containing "from_id"
- [ ] `mode="subgraph"` + `to_id` → error containing "to_id"
- [ ] `mode="chain"` + `max_depth` → error containing "max_depth" + "subgraph"
- [ ] `mode="current"` + `max_depth` → error containing "max_depth" + "subgraph"
- [ ] `mode="neighbors"` + `max_depth` → error containing "max_depth" + "subgraph"
- [ ] `mode="current"` + `seed_ids` → error containing "seed_ids"
- [ ] Unrecognized mode error lists "subgraph" (regression from vnc-018 test)
- [ ] `SubgraphResponse` serializes all 5 fields
- [ ] `SubgraphResponse.truncated` is flat bool
- [ ] Tool description contains: "tick", "depth_reached", "truncated", "empty result", "outgoing", "200"
