//! Unit tests for `graph_read_neighbors.rs`.
//!
//! Extracted to a separate file to keep `graph_read_neighbors.rs` under the 500-line limit.

use std::sync::Arc;

use super::super::GraphParams;
use super::*;
use unimatrix_store::{PoolConfig, SqlxStore, Status};

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

#[tokio::test]
async fn test_handle_neighbors_supersedes_explicit_rejection() {
    // AC-15a, R-06: Supersedes in edge_types must produce exact error string.
    let (store_impl, _dir) = open_test_store().await;
    let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

    let params = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(1),
        edge_types: Some(vec!["Supersedes".to_string()]),
        ..Default::default()
    };

    let result = handle_neighbors(&store_impl, &handle, &params, 1).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message
            .contains("Supersedes edges are not traversable via neighbors mode"),
        "exact error string required, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_neighbors_supersedes_in_mixed_list_rejected() {
    // R-06: one valid type alongside Supersedes does not bypass rejection.
    let (store_impl, _dir) = open_test_store().await;
    let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

    let params = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(1),
        edge_types: Some(vec!["Supersedes".to_string(), "Supports".to_string()]),
        ..Default::default()
    };

    let result = handle_neighbors(&store_impl, &handle, &params, 1).await;
    assert!(result.is_err());
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_neighbors_unknown_edge_type() {
    // AC-15: unknown edge type string rejected before traversal.
    let (store_impl, _dir) = open_test_store().await;
    let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

    let params = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(1),
        edge_types: Some(vec!["BogusEdge".to_string()]),
        ..Default::default()
    };

    let result = handle_neighbors(&store_impl, &handle, &params, 1).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("unknown edge type"),
        "error must mention unknown edge type, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_neighbors_direction_invalid_for_mode() {
    // R-17: direction="forward" is chain-mode vocabulary; rejected for neighbors.
    let (store_impl, _dir) = open_test_store().await;
    let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

    let params = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(1),
        direction: Some("forward".to_string()),
        ..Default::default()
    };

    let result = handle_neighbors(&store_impl, &handle, &params, 1).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("invalid direction"),
        "error must mention invalid direction, got: {}",
        err.message
    );
    // Valid neighbors directions must be listed in the error.
    assert!(
        err.message.contains("incoming") || err.message.contains("outgoing"),
        "error must list valid directions, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_neighbors_depth_out_of_range() {
    // R-11: depth=0 and depth=11 rejected; depth=1 is a valid boundary.
    let (store_impl, _dir) = open_test_store().await;
    let handle = Arc::new(crate::services::typed_graph::TypedGraphState::new_handle());

    // depth=0 → error
    let params_zero = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(1),
        depth: Some(0),
        ..Default::default()
    };
    let result = handle_neighbors(&store_impl, &handle, &params_zero, 1).await;
    assert!(result.is_err(), "depth=0 must be rejected");

    // depth=11 → error
    let params_eleven = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(1),
        depth: Some(11),
        ..Default::default()
    };
    let result = handle_neighbors(&store_impl, &handle, &params_eleven, 1).await;
    assert!(result.is_err(), "depth=11 must be rejected");

    // depth=1 → OK (may return empty, but no validation error)
    let params_one = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(999_999),
        depth: Some(1),
        ..Default::default()
    };
    let result = handle_neighbors(&store_impl, &handle, &params_one, 999_999).await;
    assert!(result.is_ok(), "depth=1 is valid boundary");

    store_impl.close().await.unwrap();
}
