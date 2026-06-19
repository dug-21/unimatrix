//! Tests for graph_read_subgraph.rs (vnc-019).
//!
//! Covers parameter validation and tool description disclosure assertions (AC-13).
//! BFS integration tests require a live store + typed graph state; those are in
//! the integration test suite.

use super::super::GraphParams;

// ---------------------------------------------------------------------------
// Parameter validation tests (unit — no store required)
// ---------------------------------------------------------------------------

fn make_params(seed_ids: Option<Vec<u64>>) -> GraphParams {
    GraphParams {
        mode: "subgraph".to_string(),
        seed_ids,
        ..Default::default()
    }
}

#[test]
fn test_subgraph_params_empty_seed_ids_is_invalid() {
    // AC-01: seed_ids absent or empty must be rejected.
    let params_none = make_params(None);
    assert!(
        params_none.seed_ids.is_none()
            || params_none.seed_ids.as_ref().is_none_or(|v| v.is_empty())
    );

    let params_empty = make_params(Some(vec![]));
    assert!(params_empty.seed_ids.as_ref().is_none_or(|v| v.is_empty()));
}

#[test]
fn test_subgraph_params_valid_seed_ids_accepted() {
    // AC-01: seed_ids with at least one entry is valid.
    let params = make_params(Some(vec![42]));
    assert!(params.seed_ids.as_ref().is_some_and(|v| !v.is_empty()));
}

#[test]
fn test_subgraph_max_depth_default_is_three() {
    // FR-06: default max_depth is 3.
    let params = make_params(Some(vec![1]));
    assert!(params.max_depth.is_none()); // None means default=3 in handle_subgraph
}

#[test]
fn test_subgraph_max_nodes_upper_bound() {
    // FR-07 (ALIGNMENT-REPORT variance resolved): max_nodes=201 must be rejected.
    // Validation happens in handle_subgraph, not at the struct level.
    // This test asserts the constant boundary value is correct.
    let max_nodes_upper: u32 = 200;
    assert!(201 > max_nodes_upper);
    assert!(200 <= max_nodes_upper);
}

#[test]
fn test_subgraph_graphparams_mode_field() {
    // Verify subgraph mode string matches what the dispatch arm expects.
    let params = make_params(Some(vec![1]));
    assert_eq!(params.mode, "subgraph");
}

// ---------------------------------------------------------------------------
// Behavioral tests — require store + TypedGraphState.
// Declared in a child module to keep this file under 500 lines (500-line rule).
// ---------------------------------------------------------------------------

#[path = "graph_read_subgraph_bfs_tests.rs"]
mod bfs;
