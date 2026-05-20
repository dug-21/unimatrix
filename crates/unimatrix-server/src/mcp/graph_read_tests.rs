use super::*;

// -----------------------------------------------------------------------
// validate_no_unsupported_params tests
// -----------------------------------------------------------------------

#[test]
fn test_validate_chain_rejects_resolve_supersessions() {
    // AC-15c, R-08: resolve_supersessions on chain mode is semantically circular.
    // This check must fire inside validate_no_unsupported_params, NOT handle_chain.
    let params = GraphParams {
        mode: "chain".to_string(),
        resolve_supersessions: Some(true),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "resolve_supersessions is not applicable to chain mode — chain IS the supersession audit"
    );
}

#[test]
fn test_validate_unrecognized_mode_fires_before_field_check() {
    // R-04: unrecognized mode must fire BEFORE any field-level check.
    // mode="walk" (unrecognized) with seed_ids present must return "unrecognized mode",
    // NOT "seed_ids not supported".
    // NOTE: mode="subgraph" was used in vnc-018; it is now a recognized mode (vnc-019 R-05).
    let params = GraphParams {
        mode: "walk".to_string(),
        seed_ids: Some(vec![1, 2, 3]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("unrecognized mode"), "got: {msg}");
    assert!(
        !msg.contains("seed_ids"),
        "forward-compat check must not fire first, got: {msg}"
    );
}

#[test]
fn test_validate_walk_mode_error_lists_valid_modes() {
    // AC-14, FR-20: unrecognized mode error must list the supported modes including subgraph.
    let params = GraphParams {
        mode: "walk".to_string(),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("chain"), "got: {msg}");
    assert!(msg.contains("current"), "got: {msg}");
    assert!(msg.contains("neighbors"), "got: {msg}");
    assert!(
        msg.contains("subgraph"),
        "subgraph must be listed after vnc-019, got: {msg}"
    );
}

#[test]
fn test_validate_neighbors_rejects_seed_ids() {
    // AC-15b, R-16: seed_ids in neighbors mode → error with "seed_ids" and "subgraph".
    let params = GraphParams {
        mode: "neighbors".to_string(),
        seed_ids: Some(vec![1]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("seed_ids"), "got: {msg}");
    assert!(msg.contains("subgraph"), "got: {msg}");
}

#[test]
fn test_validate_neighbors_rejects_from_id() {
    // AC-15b: from_id in neighbors mode → error with "from_id" and "path".
    let params = GraphParams {
        mode: "neighbors".to_string(),
        from_id: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("from_id"), "got: {msg}");
    assert!(msg.contains("path"), "got: {msg}");
}

#[test]
fn test_validate_neighbors_rejects_to_id() {
    // AC-15b: to_id in neighbors mode → error with "to_id" and "path".
    let params = GraphParams {
        mode: "neighbors".to_string(),
        to_id: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("to_id"), "got: {msg}");
    assert!(msg.contains("path"), "got: {msg}");
}

#[test]
fn test_validate_neighbors_rejects_max_nodes() {
    // AC-15b: max_nodes in neighbors mode → error with "max_nodes" and "subgraph".
    let params = GraphParams {
        mode: "neighbors".to_string(),
        max_nodes: Some(10),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("max_nodes"), "got: {msg}");
    assert!(msg.contains("subgraph"), "got: {msg}");
}

#[test]
fn test_validate_chain_rejects_seed_ids() {
    // Forward-compat: seed_ids rejected on chain mode.
    let params = GraphParams {
        mode: "chain".to_string(),
        seed_ids: Some(vec![1]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("seed_ids"), "got: {msg}");
    assert!(msg.contains("chain"), "got: {msg}");
}

#[test]
fn test_validate_chain_rejects_from_id() {
    let params = GraphParams {
        mode: "chain".to_string(),
        from_id: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("from_id"), "got: {msg}");
    assert!(msg.contains("path"), "got: {msg}");
}

#[test]
fn test_validate_chain_rejects_to_id() {
    let params = GraphParams {
        mode: "chain".to_string(),
        to_id: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("to_id"), "got: {msg}");
}

#[test]
fn test_validate_chain_rejects_max_nodes() {
    let params = GraphParams {
        mode: "chain".to_string(),
        max_nodes: Some(100),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("max_nodes"), "got: {msg}");
}

#[test]
fn test_validate_valid_modes_pass() {
    for mode in &["chain", "current", "neighbors"] {
        let params = GraphParams {
            mode: mode.to_string(),
            ..Default::default()
        };
        let result = validate_no_unsupported_params(&params);
        assert!(
            result.is_ok(),
            "mode={mode} should be valid, got: {result:?}"
        );
    }
}

// -----------------------------------------------------------------------
// EdgeRecord serialization tests
// -----------------------------------------------------------------------

#[test]
fn test_edge_record_metadata_serializes_as_null() {
    // R-15, NFR-07: metadata must appear in JSON as `null`, not be absent.
    // skip_serializing_if = "Option::is_none" is PROHIBITED on this field (ADR-004).
    let record = EdgeRecord {
        source_id: 1,
        target_id: 2,
        relation_type: "Supports".to_string(),
        direction: "outgoing".to_string(),
        depth: 1,
        metadata: None,
    };
    let json = serde_json::to_string(&record).expect("serialize");
    assert!(
        json.contains("\"metadata\":null"),
        "metadata must serialize as null, not be absent. JSON: {json}"
    );
    assert!(
        !json.contains(r#""metadata":{"#),
        "metadata must be null, not an object. JSON: {json}"
    );
}

#[test]
fn test_truncated_serializes_as_struct_not_flat_bool() {
    // R-02, AC-03b: Truncated must serialize as {"forward":bool,"backward":bool}.
    // A flat bool would break the wire format contract (ADR-002).
    let t = Truncated {
        forward: true,
        backward: false,
    };
    let json = serde_json::to_string(&t).expect("serialize");
    assert!(
        json.contains("\"forward\":true"),
        "truncated.forward missing. JSON: {json}"
    );
    assert!(
        json.contains("\"backward\":false"),
        "truncated.backward missing. JSON: {json}"
    );
    assert_ne!(json, "true", "Truncated must not serialize as a flat bool");
}

// -----------------------------------------------------------------------
// TypedRelationGraph::node_index_for tests (ADR-008)
// -----------------------------------------------------------------------

use unimatrix_core::{EntryRecord, Status};

fn make_entry_for_test(id: u64) -> EntryRecord {
    EntryRecord {
        id,
        title: format!("Entry {id}"),
        content: String::new(),
        topic: String::new(),
        category: "pattern".to_string(),
        tags: vec![],
        source: String::new(),
        status: Status::Active,
        confidence: 0.5,
        created_at: 0,
        updated_at: 0,
        last_accessed_at: 0,
        access_count: 0,
        supersedes: None,
        superseded_by: None,
        correction_count: 0,
        embedding_dim: 0,
        created_by: String::new(),
        modified_by: String::new(),
        content_hash: String::new(),
        previous_hash: String::new(),
        version: 1,
        feature_cycle: String::new(),
        trust_source: "agent".to_string(),
        helpful_count: 0,
        unhelpful_count: 0,
        pre_quarantine_status: None,
    }
}

#[test]
fn test_node_index_for_known_node_returns_index() {
    // R-07, AC-11: node_index_for on a known ID returns Some(index).
    use unimatrix_engine::graph::build_typed_relation_graph;

    let entry = make_entry_for_test(42);
    let graph = build_typed_relation_graph(&[entry], &[]).expect("build graph");
    let result = graph.node_index_for(42);
    assert!(
        result.is_some(),
        "known node 42 must return Some(NodeIndex)"
    );
}

#[test]
fn test_node_index_for_unknown_node_returns_none() {
    // R-07: node_index_for on unknown ID returns None.
    let graph = unimatrix_engine::graph::TypedRelationGraph::empty();
    let result = graph.node_index_for(999_999);
    assert!(result.is_none(), "unknown node must return None");
}

// -----------------------------------------------------------------------
// vnc-019 tests (graph_read.rs changes: subgraph mode, max_depth, SubgraphResponse)
// Declared in a child module to keep this file under 500 lines (500-line rule).
// -----------------------------------------------------------------------

#[path = "graph_read_tests_vnc019.rs"]
mod vnc019;

// -----------------------------------------------------------------------
// vnc-020 tests (graph_read.rs changes: 8 new GraphParams fields, 3 new response types,
// validate_no_unsupported_params expansion, 3 new dispatch arms).
// -----------------------------------------------------------------------

#[path = "graph_read_tests_vnc020.rs"]
mod vnc020;
