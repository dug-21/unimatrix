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

// ---------------------------------------------------------------------------
// GH #623: neighbors_bfs depth>1 falls back to DB when use_fallback=true
// ---------------------------------------------------------------------------

/// GH #623 regression: neighbors_bfs depth>1 returns empty when use_fallback=true.
///
/// Real SqlxStore, entries A and B with A→B Supports edge in GRAPH_EDGES.
/// TypedGraphState::new() directly (use_fallback=true, no rebuild).
/// handle_neighbors(id=A, depth=2) must return at least one edge (B at depth 1).
#[tokio::test]
async fn test_neighbors_bfs_use_fallback_true_depth_gt1_falls_back_to_db() {
    use unimatrix_store::NewEntry;

    let (store_impl, _dir) = open_test_store().await;

    // Step 1: Insert entries A and B.
    let id_a = store_impl
        .insert(NewEntry {
            title: "Entry A".to_string(),
            content: "content-a".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-623".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert A");

    let id_b = store_impl
        .insert(NewEntry {
            title: "Entry B".to_string(),
            content: "content-b".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-623".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert B");

    // Step 2: Insert A→B Supports edge directly into GRAPH_EDGES.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at,
              created_by, source, bootstrap_only)
         VALUES (?1, ?2, 'Supports', 1.0, ?3, 'test', '', 0)",
    )
    .bind(id_a as i64)
    .bind(id_b as i64)
    .bind(now as i64)
    .execute(store_impl.write_pool_server())
    .await
    .expect("insert Supports edge A→B");

    // Step 3: Cold-start state — TypedGraphState::new(), use_fallback=true.
    let handle = Arc::new(std::sync::RwLock::new(
        crate::services::typed_graph::TypedGraphState::new(),
    ));
    {
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            guard.use_fallback,
            "TypedGraphState::new() must have use_fallback=true"
        );
    }

    // Step 4: Call handle_neighbors with id=A, depth=2 — exercises neighbors_bfs path.
    let params = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(id_a),
        depth: Some(2),
        edge_types: Some(vec!["Supports".to_string()]),
        ..Default::default()
    };
    let result = handle_neighbors(&store_impl, &handle, &params, id_a).await;
    assert!(
        result.is_ok(),
        "DB-fallback neighbors (depth=2) must return Ok, got: {result:?}"
    );
    let resp = result.unwrap();

    // Step 5: Assert at least one edge found (B at depth 1).
    assert!(
        !resp.edges.is_empty(),
        "DB-fallback neighbors_bfs depth>1 must discover A→B Supports edge \
         when use_fallback=true; got empty. GH #623 regression guard."
    );
    let edge = &resp.edges[0];
    assert_eq!(edge.source_id, id_a, "edge source must be A");
    assert_eq!(edge.target_id, id_b, "edge target must be B");
    assert_eq!(edge.relation_type, "Supports");
    assert_eq!(edge.depth, 1, "B is at depth 1 from A");

    store_impl.close().await.unwrap();
}
