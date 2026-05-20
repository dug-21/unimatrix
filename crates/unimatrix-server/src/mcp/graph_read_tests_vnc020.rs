//! vnc-020 tests for `graph_read.rs` changes.
//!
//! Covers: validate_no_unsupported_params — depth rejection on 5 newly-rejecting modes
//! (AC-25), unrecognized mode lists all 7 modes (AC-26), from_id/to_id on non-path modes
//! (AC-22), missing_edge_types on non-inverse modes (AC-23), filter-only params on wrong
//! modes (AC-24), edge_types on inverse mode (AC-03a), and R-04 8-field matrix.
//!
//! Declared as a child module inside `graph_read_tests.rs`.

use super::super::*;

// -----------------------------------------------------------------------
// AC-26 — Unrecognized mode lists all seven modes
// -----------------------------------------------------------------------

#[test]
fn test_graph_unrecognized_mode_error_lists_all_seven_modes() {
    // AC-26, R-04: unrecognized mode error must list all 7 supported mode names.
    let params = GraphParams {
        mode: "unknown_mode_xyz".to_string(),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("chain"), "missing chain, got: {msg}");
    assert!(msg.contains("current"), "missing current, got: {msg}");
    assert!(msg.contains("neighbors"), "missing neighbors, got: {msg}");
    assert!(msg.contains("subgraph"), "missing subgraph, got: {msg}");
    assert!(msg.contains("inverse"), "missing inverse, got: {msg}");
    assert!(msg.contains("filter"), "missing filter, got: {msg}");
    assert!(msg.contains("path"), "missing path, got: {msg}");
    // AC-26 exact fragment requirement.
    assert!(
        msg.contains("chain, current, neighbors, subgraph, inverse, filter, path"),
        "exact mode list not found, got: {msg}"
    );
}

// -----------------------------------------------------------------------
// AC-25 — depth rejected on five newly-rejecting modes
// -----------------------------------------------------------------------

#[test]
fn test_depth_rejected_on_chain_mode() {
    // AC-25, R-07: depth on chain mode now returns error (was silently ignored).
    let params = GraphParams {
        mode: "chain".to_string(),
        depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("depth is not supported in chain mode"),
        "got: {msg}"
    );
    assert!(
        msg.contains("neighbors or path mode"),
        "must direct to neighbors or path, got: {msg}"
    );
}

#[test]
fn test_depth_rejected_on_current_mode() {
    // AC-25, R-07: depth on current mode.
    let params = GraphParams {
        mode: "current".to_string(),
        depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("depth is not supported in current mode"),
        "got: {msg}"
    );
    assert!(msg.contains("neighbors or path mode"), "got: {msg}");
}

#[test]
fn test_depth_rejected_on_subgraph_mode() {
    // AC-25, R-07: depth on subgraph mode.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("depth is not supported in subgraph mode"),
        "got: {msg}"
    );
    assert!(msg.contains("neighbors or path mode"), "got: {msg}");
}

#[test]
fn test_depth_rejected_on_inverse_mode() {
    // AC-25, R-07: depth on inverse mode.
    let params = GraphParams {
        mode: "inverse".to_string(),
        depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("depth is not supported in inverse mode"),
        "got: {msg}"
    );
    assert!(msg.contains("neighbors or path mode"), "got: {msg}");
}

#[test]
fn test_depth_rejected_on_filter_mode() {
    // AC-25, R-07: depth on filter mode.
    let params = GraphParams {
        mode: "filter".to_string(),
        depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("depth is not supported in filter mode"),
        "got: {msg}"
    );
    assert!(msg.contains("neighbors or path mode"), "got: {msg}");
}

// Regression: neighbors still accepts depth.
#[test]
fn test_depth_accepted_on_neighbors_mode() {
    let params = GraphParams {
        mode: "neighbors".to_string(),
        depth: Some(3),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "depth must be accepted on neighbors, got: {result:?}"
    );
}

// Regression: path still accepts depth.
#[test]
fn test_depth_accepted_on_path_mode() {
    let params = GraphParams {
        mode: "path".to_string(),
        depth: Some(5),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "depth must be accepted on path, got: {result:?}"
    );
}

// -----------------------------------------------------------------------
// AC-22 — from_id / to_id rejected on non-path modes
// -----------------------------------------------------------------------

#[test]
fn test_from_id_rejected_on_chain_mode() {
    // AC-22: from_id on chain mode → error naming "path".
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
fn test_from_id_rejected_on_current_mode() {
    let params = GraphParams {
        mode: "current".to_string(),
        from_id: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("from_id") && msg.contains("path"),
        "got: {msg}"
    );
}

#[test]
fn test_from_id_rejected_on_neighbors_mode() {
    let params = GraphParams {
        mode: "neighbors".to_string(),
        from_id: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("from_id") && msg.contains("path"),
        "got: {msg}"
    );
}

#[test]
fn test_from_id_rejected_on_subgraph_mode() {
    let params = GraphParams {
        mode: "subgraph".to_string(),
        from_id: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("from_id") && msg.contains("path"),
        "got: {msg}"
    );
}

#[test]
fn test_from_id_rejected_on_filter_mode() {
    // AC-22 + test plan AC-24 extra: from_id was a forward-compat stub, now actively
    // rejected on filter mode.
    let params = GraphParams {
        mode: "filter".to_string(),
        from_id: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("from_id") && msg.contains("path"),
        "got: {msg}"
    );
}

// -----------------------------------------------------------------------
// AC-23 — missing_edge_types rejected on non-inverse modes
// -----------------------------------------------------------------------

#[test]
fn test_missing_edge_types_rejected_on_chain_mode() {
    // AC-23: missing_edge_types on chain → error naming "inverse".
    let params = GraphParams {
        mode: "chain".to_string(),
        missing_edge_types: Some(vec!["Cites".to_string()]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("missing_edge_types"), "got: {msg}");
    assert!(msg.contains("inverse"), "got: {msg}");
}

#[test]
fn test_missing_edge_types_rejected_on_current_mode() {
    let params = GraphParams {
        mode: "current".to_string(),
        missing_edge_types: Some(vec!["Cites".to_string()]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("missing_edge_types") && msg.contains("inverse"),
        "got: {msg}"
    );
}

#[test]
fn test_missing_edge_types_rejected_on_neighbors_mode() {
    let params = GraphParams {
        mode: "neighbors".to_string(),
        missing_edge_types: Some(vec!["Cites".to_string()]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("missing_edge_types") && msg.contains("inverse"),
        "got: {msg}"
    );
}

#[test]
fn test_missing_edge_types_rejected_on_subgraph_mode() {
    let params = GraphParams {
        mode: "subgraph".to_string(),
        missing_edge_types: Some(vec!["Cites".to_string()]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("missing_edge_types") && msg.contains("inverse"),
        "got: {msg}"
    );
}

#[test]
fn test_missing_edge_types_rejected_on_filter_mode() {
    // AC-23: filter mode must reject missing_edge_types (it uses edge_types instead).
    let params = GraphParams {
        mode: "filter".to_string(),
        missing_edge_types: Some(vec!["Cites".to_string()]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("missing_edge_types") && msg.contains("inverse"),
        "got: {msg}"
    );
}

#[test]
fn test_missing_edge_types_rejected_on_path_mode() {
    // AC-23: path mode must reject missing_edge_types (it uses edge_types instead).
    let params = GraphParams {
        mode: "path".to_string(),
        missing_edge_types: Some(vec!["Cites".to_string()]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("missing_edge_types") && msg.contains("inverse"),
        "got: {msg}"
    );
}

// -----------------------------------------------------------------------
// AC-03a — edge_types rejected on inverse mode
// -----------------------------------------------------------------------

#[test]
fn test_edge_types_rejected_on_inverse_mode() {
    // AC-03a, R-04: edge_types on inverse → error directing to missing_edge_types.
    let params = GraphParams {
        mode: "inverse".to_string(),
        edge_types: Some(vec!["Cites".to_string()]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("edge_types"), "got: {msg}");
    assert!(
        msg.contains("missing_edge_types"),
        "error must name missing_edge_types as the correct parameter, got: {msg}"
    );
}

// -----------------------------------------------------------------------
// AC-24 / R-04 — 8-field rejection matrix (one test per new field × one wrong mode)
// -----------------------------------------------------------------------

#[test]
fn test_category_rejected_on_path_mode() {
    // R-04: category on path → error naming inverse or filter.
    let params = GraphParams {
        mode: "path".to_string(),
        category: Some("pattern".to_string()),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("category"), "got: {msg}");
    assert!(
        msg.contains("inverse") || msg.contains("filter"),
        "must direct to inverse or filter, got: {msg}"
    );
}

#[test]
fn test_missing_edge_types_rejected_on_filter_mode_r04() {
    // R-04 matrix: missing_edge_types on filter → error naming "inverse mode".
    // (Duplicate of AC-23 filter test; kept as required R-04 coverage entry.)
    let params = GraphParams {
        mode: "filter".to_string(),
        missing_edge_types: Some(vec!["Cites".to_string()]),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(
        msg.contains("missing_edge_types") && msg.contains("inverse"),
        "got: {msg}"
    );
}

#[test]
fn test_limit_rejected_on_chain_mode() {
    // R-04: limit on chain → error naming inverse or filter.
    let params = GraphParams {
        mode: "chain".to_string(),
        limit: Some(50),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("limit"), "got: {msg}");
    assert!(
        msg.contains("inverse") || msg.contains("filter"),
        "must direct to inverse or filter, got: {msg}"
    );
}

#[test]
fn test_min_age_days_rejected_on_path_mode() {
    // R-04: min_age_days on path → error naming filter.
    let params = GraphParams {
        mode: "path".to_string(),
        min_age_days: Some(30),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("min_age_days"), "got: {msg}");
    assert!(msg.contains("filter"), "got: {msg}");
}

#[test]
fn test_min_confidence_rejected_on_subgraph_mode() {
    // R-04: min_confidence on subgraph → error naming filter.
    let params = GraphParams {
        mode: "subgraph".to_string(),
        min_confidence: Some(0.5),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("min_confidence"), "got: {msg}");
    assert!(msg.contains("filter"), "got: {msg}");
}

#[test]
fn test_max_confidence_rejected_on_current_mode() {
    // R-04: max_confidence on current → error naming filter.
    let params = GraphParams {
        mode: "current".to_string(),
        max_confidence: Some(0.9),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("max_confidence"), "got: {msg}");
    assert!(msg.contains("filter"), "got: {msg}");
}

#[test]
fn test_min_edge_count_rejected_on_inverse_mode() {
    // R-04 / AC-24: min_edge_count on inverse → error naming filter.
    let params = GraphParams {
        mode: "inverse".to_string(),
        min_edge_count: Some(1),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("min_edge_count"), "got: {msg}");
    assert!(msg.contains("filter"), "got: {msg}");
}

#[test]
fn test_max_edge_count_rejected_on_neighbors_mode() {
    // R-04: max_edge_count on neighbors → error naming filter.
    let params = GraphParams {
        mode: "neighbors".to_string(),
        max_edge_count: Some(5),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(result.is_err());
    let msg = result.unwrap_err();
    assert!(msg.contains("max_edge_count"), "got: {msg}");
    assert!(msg.contains("filter"), "got: {msg}");
}

// -----------------------------------------------------------------------
// New response type serialization (InverseResponse, FilterResponse, PathResponse)
// -----------------------------------------------------------------------

#[test]
fn test_inverse_response_serializes_correctly() {
    // AC-01 equivalent: InverseResponse must serialize entries and total_returned.
    let resp = InverseResponse {
        entries: vec![],
        total_returned: 0,
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("\"entries\""), "missing entries: {json}");
    assert!(
        json.contains("\"total_returned\""),
        "missing total_returned: {json}"
    );
}

#[test]
fn test_filter_response_serializes_correctly() {
    let resp = FilterResponse {
        entries: vec![],
        total_returned: 0,
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("\"entries\""), "missing entries: {json}");
    assert!(
        json.contains("\"total_returned\""),
        "missing total_returned: {json}"
    );
}

#[test]
fn test_path_response_found_false_serializes_correctly() {
    // ADR-005: PathResponse { found, from_id, to_id, hops, length }.
    let resp = PathResponse {
        found: false,
        from_id: 1,
        to_id: 2,
        hops: vec![],
        length: 0,
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("\"found\":false"), "got: {json}");
    assert!(json.contains("\"from_id\""), "missing from_id: {json}");
    assert!(json.contains("\"to_id\""), "missing to_id: {json}");
    assert!(json.contains("\"hops\":[]"), "missing hops: {json}");
    assert!(json.contains("\"length\":0"), "missing length: {json}");
}

#[test]
fn test_path_hop_serializes_correctly() {
    // ADR-005: PathHop { entry_id, relation_type } — relation_type is never null.
    let hop = PathHop {
        entry_id: 42,
        relation_type: "Cites".to_string(),
    };
    let json = serde_json::to_string(&hop).expect("serialize");
    assert!(json.contains("\"entry_id\":42"), "got: {json}");
    assert!(json.contains("\"relation_type\":\"Cites\""), "got: {json}");
    assert!(
        !json.contains("null"),
        "relation_type must never be null: {json}"
    );
}

#[test]
fn test_path_response_length_equals_hops_len() {
    // ADR-005: length always equals hops.len().
    let hops = vec![
        PathHop {
            entry_id: 10,
            relation_type: "Cites".to_string(),
        },
        PathHop {
            entry_id: 20,
            relation_type: "Supports".to_string(),
        },
    ];
    let length = hops.len() as u8;
    let resp = PathResponse {
        found: true,
        from_id: 1,
        to_id: 20,
        hops,
        length,
    };
    assert_eq!(resp.length as usize, resp.hops.len());
    let json = serde_json::to_string(&resp).expect("serialize");
    assert!(json.contains("\"found\":true"), "got: {json}");
    assert!(json.contains("\"length\":2"), "got: {json}");
}

// -----------------------------------------------------------------------
// New GraphParams fields deserialize correctly (backward-compat, ADR-002)
// -----------------------------------------------------------------------

#[test]
fn test_graph_params_new_fields_absent_deserialize_as_none() {
    // ADR-002: new fields must deserialize as None when absent (backward compat).
    let json = r#"{"mode":"chain","id":1}"#;
    let params: GraphParams = serde_json::from_str(json).expect("deserialize");
    assert!(
        params.category.is_none(),
        "category must be None when absent"
    );
    assert!(
        params.missing_edge_types.is_none(),
        "missing_edge_types must be None when absent"
    );
    assert!(params.limit.is_none(), "limit must be None when absent");
    assert!(
        params.min_age_days.is_none(),
        "min_age_days must be None when absent"
    );
    assert!(
        params.min_confidence.is_none(),
        "min_confidence must be None when absent"
    );
    assert!(
        params.max_confidence.is_none(),
        "max_confidence must be None when absent"
    );
    assert!(
        params.min_edge_count.is_none(),
        "min_edge_count must be None when absent"
    );
    assert!(
        params.max_edge_count.is_none(),
        "max_edge_count must be None when absent"
    );
}

#[test]
fn test_graph_params_inverse_fields_deserialize() {
    // ADR-002: new fields round-trip through deserialization.
    let json = r#"{
        "mode": "inverse",
        "category": "goal",
        "missing_edge_types": ["Cites", "Supports"],
        "limit": 50
    }"#;
    let params: GraphParams = serde_json::from_str(json).expect("deserialize");
    assert_eq!(params.category.as_deref(), Some("goal"));
    assert_eq!(
        params.missing_edge_types.as_deref(),
        Some(&["Cites".to_string(), "Supports".to_string()][..])
    );
    assert_eq!(params.limit, Some(50));
}

#[test]
fn test_graph_params_filter_fields_deserialize() {
    let json = r#"{
        "mode": "filter",
        "category": "decision",
        "min_age_days": 30,
        "min_confidence": 0.4,
        "max_confidence": 0.9,
        "min_edge_count": 1,
        "max_edge_count": 5,
        "limit": 100
    }"#;
    let params: GraphParams = serde_json::from_str(json).expect("deserialize");
    assert_eq!(params.category.as_deref(), Some("decision"));
    assert_eq!(params.min_age_days, Some(30));
    assert_eq!(params.min_confidence, Some(0.4));
    assert_eq!(params.max_confidence, Some(0.9));
    assert_eq!(params.min_edge_count, Some(1));
    assert_eq!(params.max_edge_count, Some(5));
    assert_eq!(params.limit, Some(100));
}

// -----------------------------------------------------------------------
// inverse/filter/path mode validation passes with clean params
// -----------------------------------------------------------------------

#[test]
fn test_validate_inverse_mode_with_valid_params_passes() {
    let params = GraphParams {
        mode: "inverse".to_string(),
        category: Some("goal".to_string()),
        missing_edge_types: Some(vec!["Cites".to_string()]),
        limit: Some(50),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "valid inverse params must pass, got: {result:?}"
    );
}

#[test]
fn test_validate_filter_mode_with_valid_params_passes() {
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("pattern".to_string()),
        min_confidence: Some(0.5),
        limit: Some(100),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "valid filter params must pass, got: {result:?}"
    );
}

#[test]
fn test_validate_path_mode_with_valid_params_passes() {
    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(42),
        depth: Some(5),
        ..Default::default()
    };
    let result = validate_no_unsupported_params(&params);
    assert!(
        result.is_ok(),
        "valid path params must pass, got: {result:?}"
    );
}
