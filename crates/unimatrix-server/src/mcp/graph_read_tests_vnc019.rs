//! vnc-019 tests for `graph_read.rs` changes.
//!
//! Covers: subgraph mode validation, max_depth field, SubgraphResponse serialization,
//! and tool description disclosure assertions.
//!
//! Declared as a child module inside `graph_read_tests.rs`.

use super::super::*;

// -----------------------------------------------------------------------
// subgraph mode — validate_no_unsupported_params (R-05, FR-20)
// -----------------------------------------------------------------------

#[test]
fn test_validate_subgraph_mode_recognized() {
    // R-05, AC-11, FR-20: mode="subgraph" with no unsupported params passes validate.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "subgraph with seed_ids should pass validate, got: {result:?}"
    );
}

#[test]
fn test_validate_subgraph_mode_recognized_no_seed_ids() {
    // FR-20: mode="subgraph" with no fields passes structural validation (range errors
    // are deferred to handle_subgraph; validate_no_unsupported_params only checks
    // field compatibility, not range constraints).
    let params = GraphParams {
        mode: "subgraph".to_string(),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "subgraph with no params should pass structural validation, got: {result:?}"
    );
}

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

#[test]
fn test_validate_subgraph_permits_max_depth() {
    // ADR-001 vnc-019: max_depth is permitted on subgraph mode.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        max_depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "max_depth should be permitted on subgraph, got: {result:?}"
    );
}

#[test]
fn test_validate_subgraph_permits_max_nodes() {
    // ADR-001 vnc-019: max_nodes is permitted on subgraph mode.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![1]),
        max_nodes: Some(50),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "max_nodes should be permitted on subgraph, got: {result:?}"
    );
}

// -----------------------------------------------------------------------
// max_depth rejected on chain/current/neighbors (ADR-001)
// -----------------------------------------------------------------------

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
    assert!(
        msg.contains("subgraph"),
        "must direct to subgraph, got: {msg}"
    );
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
    assert!(
        msg.contains("max_depth") && msg.contains("subgraph"),
        "got: {msg}"
    );
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
    assert!(
        msg.contains("max_depth") && msg.contains("subgraph"),
        "got: {msg}"
    );
}

#[test]
fn test_validate_current_rejects_seed_ids() {
    // AC-11: seed_ids rejected on current mode (regression coverage).
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
    assert!(
        msg.contains("subgraph"),
        "subgraph must be listed in supported modes, got: {msg}"
    );
    assert!(msg.contains("chain"), "got: {msg}");
    assert!(msg.contains("current"), "got: {msg}");
    assert!(msg.contains("neighbors"), "got: {msg}");
}

// -----------------------------------------------------------------------
// GraphParams.max_depth field deserialization
// -----------------------------------------------------------------------

#[test]
fn test_graph_params_max_depth_none_deserializes() {
    // ADR-001 vnc-019: backward compat — JSON without max_depth deserializes to None.
    let json = r#"{"mode":"subgraph","seed_ids":[1]}"#;
    let params: GraphParams = serde_json::from_str(json).expect("deserialize");
    assert!(
        params.max_depth.is_none(),
        "absent max_depth must deserialize as None"
    );
}

#[test]
fn test_graph_params_max_depth_some_deserializes() {
    // ADR-001 vnc-019: max_depth=5 deserializes correctly.
    let json = r#"{"mode":"subgraph","seed_ids":[1],"max_depth":5}"#;
    let params: GraphParams = serde_json::from_str(json).expect("deserialize");
    assert_eq!(
        params.max_depth,
        Some(5),
        "max_depth must deserialize as Some(5)"
    );
}

// -----------------------------------------------------------------------
// SubgraphResponse struct (AC-01)
// -----------------------------------------------------------------------

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
    assert!(json.contains("\"nodes\""), "missing nodes: {json}");
    assert!(json.contains("\"edges\""), "missing edges: {json}");
    assert!(json.contains("\"truncated\""), "missing truncated: {json}");
    assert!(json.contains("\"seed_ids\""), "missing seed_ids: {json}");
    assert!(
        json.contains("\"depth_reached\""),
        "missing depth_reached: {json}"
    );
}

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
    assert!(
        json.contains("\"truncated\":true"),
        "truncated must be flat bool true, got: {json}"
    );
}

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

#[test]
fn test_subgraph_response_seed_ids_echoed() {
    // AC-01: seed_ids in response must echo the input seed IDs.
    let resp = SubgraphResponse {
        nodes: vec![],
        edges: vec![],
        truncated: false,
        seed_ids: vec![10, 20, 30],
        depth_reached: 0,
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(
        json.contains("\"seed_ids\":[10,20,30]"),
        "seed_ids must echo input IDs, got: {json}"
    );
}

// -----------------------------------------------------------------------
// Tool description staleness disclosures (AC-13, R-11)
// -----------------------------------------------------------------------

#[test]
fn test_tool_description_contains_staleness_disclosures() {
    // AC-13, R-11: tool description must include all required staleness disclosures.
    let desc = crate::mcp::tools::CONTEXT_GRAPH_DESCRIPTION;
    // (a) in-memory BFS tick-window staleness
    assert!(
        desc.contains("tick"),
        "description must mention tick-window staleness, got: {desc}"
    );
    // (b) depth_reached + truncated semantics
    assert!(
        desc.contains("depth_reached"),
        "description must explain depth_reached, got: {desc}"
    );
    assert!(
        desc.contains("truncated"),
        "description must explain truncated, got: {desc}"
    );
    // (c) unknown seed behavior
    assert!(
        desc.contains("empty result"),
        "description must document empty result for unknown seeds, got: {desc}"
    );
    // (d) direction always outgoing
    assert!(
        desc.contains("outgoing"),
        "description must note direction is always outgoing, got: {desc}"
    );
    // max_nodes range documentation
    assert!(
        desc.contains("200"),
        "description must document max_nodes limit, got: {desc}"
    );
}
