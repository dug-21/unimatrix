//! Tests for `graph_read_path.rs` — BFS shortest-path handler (vnc-020).
//!
//! Parameter validation, basic BFS traversal, depth enforcement, cycle termination,
//! and path response shape. Tests that require DB-backed supersession resolution
//! live in `graph_read_path_supersession_tests.rs`.
//!
//! Declared as a sibling test module via `#[path]` in `graph_read_path.rs`.

use std::sync::Arc;

use unimatrix_core::{EntryRecord, Status};
use unimatrix_engine::graph::{GraphEdgeRow, TypedRelationGraph, build_typed_relation_graph};
use unimatrix_store::{PoolConfig, SqlxStore};

use super::super::GraphParams;
use super::handle_path;
use crate::services::typed_graph::TypedGraphState;

// ---------------------------------------------------------------------------
// Shared test helpers
// ---------------------------------------------------------------------------

pub(super) async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

pub(super) fn make_entry(id: u64) -> EntryRecord {
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

/// Make a deprecated entry pointing to `superseded_by`.
pub(super) fn make_deprecated_entry(id: u64, superseded_by: u64) -> EntryRecord {
    EntryRecord {
        id,
        status: Status::Deprecated,
        superseded_by: Some(superseded_by),
        ..make_entry(id)
    }
}

pub(super) fn make_edge(source_id: u64, target_id: u64, relation_type: &str) -> GraphEdgeRow {
    GraphEdgeRow {
        source_id,
        target_id,
        relation_type: relation_type.to_string(),
        weight: 1.0,
        created_at: 0,
        created_by: String::new(),
        source: String::new(),
        bootstrap_only: false,
    }
}

pub(super) fn set_test_graph(
    handle: &Arc<std::sync::RwLock<TypedGraphState>>,
    graph: TypedRelationGraph,
) {
    let mut state = handle.write().expect("write lock");
    state.typed_graph = graph;
    state.use_fallback = false;
}

fn path_params(from_id: u64, to_id: u64) -> GraphParams {
    GraphParams {
        mode: "path".to_string(),
        from_id: Some(from_id),
        to_id: Some(to_id),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// AC-16: from_id required
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_missing_from_id_returns_error() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "path".to_string(),
        from_id: None,
        to_id: Some(1),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_err(), "missing from_id must return Err");
    let msg = result.unwrap_err().message;
    assert_eq!(
        msg, "path mode requires from_id",
        "exact error message required, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// AC-17: to_id required
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_missing_to_id_returns_error() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: None,
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_err(), "missing to_id must return Err");
    let msg = result.unwrap_err().message;
    assert_eq!(
        msg, "path mode requires to_id",
        "exact error message required, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// AC-32: self-path (from_id == to_id) returns found: false
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_self_path_returns_not_found() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    // Inject a graph that contains entry A.
    let graph = build_typed_relation_graph(&[make_entry(1)], &[]).expect("build graph");
    set_test_graph(&handle, graph);

    let params = path_params(1, 1);
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_ok(), "self-path must return Ok, got: {result:?}");
    let resp = result.unwrap();
    assert!(!resp.found, "self-path must return found: false");
    assert!(resp.hops.is_empty(), "self-path must have empty hops");
    assert_eq!(resp.length, 0, "self-path must have length 0");
}

// ---------------------------------------------------------------------------
// AC-18: depth boundary validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_depth_zero_returns_error() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(2),
        depth: Some(0),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_err(), "depth=0 must be rejected");
    let msg = result.unwrap_err().message;
    assert!(
        msg.contains("1..=10"),
        "error must state range [1, 10], got: {msg}"
    );
}

#[tokio::test]
async fn test_handle_path_depth_11_returns_error() {
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());
    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(2),
        depth: Some(11),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_err(), "depth=11 must be rejected");
    let msg = result.unwrap_err().message;
    assert!(
        msg.contains("1..=10") && msg.contains("11"),
        "error must state range and echo value, got: {msg}"
    );
}

#[tokio::test]
async fn test_handle_path_depth_default_is_5() {
    // Build a 5-hop chain: 1→2→3→4→5→6
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let entries: Vec<_> = (1u64..=6).map(make_entry).collect();
    let edges: Vec<_> = (1u64..=5)
        .map(|i| make_edge(i, i + 1, "Supports"))
        .collect();
    let graph = build_typed_relation_graph(&entries, &edges).expect("build graph");
    set_test_graph(&handle, graph);

    // depth=None → default 5 → path from 1 to 6 is exactly 5 hops, should be found.
    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(6),
        depth: None,
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(
        result.is_ok(),
        "depth default must allow 5-hop path, got: {result:?}"
    );
    let resp = result.unwrap();
    assert!(resp.found, "5-hop path must be found with default depth=5");
    assert_eq!(resp.length, 5);
}

// ---------------------------------------------------------------------------
// AC-15: from/to_id not in snapshot → found: false, NOT Err (distinct from AC-14)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_from_id_not_in_snapshot_returns_not_found() {
    // AC-15 (R-09): from_id=99999 is NOT in the graph snapshot.
    // Result must be Ok(found: false), NOT Err(ErrorData).
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    // Graph has entries A(1) and B(2) only — entry 99999 is absent.
    let graph = build_typed_relation_graph(
        &[make_entry(1), make_entry(2)],
        &[make_edge(1, 2, "Supports")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(99999),
        to_id: Some(2),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    // Must be Ok, NOT Err (lesson #4497).
    assert!(
        result.is_ok(),
        "from_id not in snapshot must return Ok, not Err; got: {result:?}"
    );
    let resp = result.unwrap();
    assert!(
        !resp.found,
        "from_id not in snapshot must return found: false"
    );
    assert!(resp.hops.is_empty());
    assert_eq!(resp.length, 0);
}

#[tokio::test]
async fn test_handle_path_to_id_not_in_snapshot_returns_not_found() {
    // AC-15 (R-09): to_id=99999 NOT in snapshot. from_id IS present.
    // Distinct fixture from the no-path case (R-09 separation requirement).
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    // Graph has entries 1 and 2 only — to_id 99999 is absent.
    let graph = build_typed_relation_graph(
        &[make_entry(1), make_entry(2)],
        &[make_edge(1, 2, "Supports")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(99999),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(
        result.is_ok(),
        "to_id not in snapshot must return Ok, not Err; got: {result:?}"
    );
    let resp = result.unwrap();
    assert!(
        !resp.found,
        "to_id not in snapshot must return found: false"
    );
    assert!(resp.hops.is_empty());
    assert_eq!(resp.length, 0);
}

// ---------------------------------------------------------------------------
// R-12: Path response shape — 1-hop and 2-hop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_1hop_from_id_not_in_hops() {
    // R-12: A→B edge. hops=[{B, "Advances"}], from_id=A NOT in hops, length=1.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph = build_typed_relation_graph(
        &[make_entry(10), make_entry(20)],
        &[make_edge(10, 20, "Advances")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(10),
        to_id: Some(20),
        edge_types: Some(vec!["Advances".to_string()]),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();

    assert!(resp.found, "1-hop path must be found");
    assert_eq!(resp.from_id, 10, "from_id must be 10 (top-level)");
    assert_eq!(resp.to_id, 20);
    assert_eq!(resp.hops.len(), 1, "exactly 1 hop");
    assert_eq!(resp.hops[0].entry_id, 20, "hop[0] must be B=20");
    assert_eq!(
        resp.hops[0].relation_type, "Advances",
        "relation_type never null"
    );
    assert_eq!(resp.length, 1, "length must equal hops.len()");
    // from_id NOT in hops (AC-13, ADR-005).
    assert!(
        resp.hops.iter().all(|h| h.entry_id != 10),
        "from_id=10 must NOT appear in hops"
    );
}

#[tokio::test]
async fn test_handle_path_2hop_from_id_not_in_hops() {
    // R-12: A→B (Advances), B→C (Supports). from_id=A, to_id=C.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph = build_typed_relation_graph(
        &[make_entry(1), make_entry(2), make_entry(3)],
        &[make_edge(1, 2, "Advances"), make_edge(2, 3, "Supports")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(3),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();

    assert!(resp.found, "2-hop path must be found");
    assert_eq!(resp.from_id, 1, "from_id=1 must be top-level field");
    assert_eq!(resp.to_id, 3);
    assert_eq!(resp.hops.len(), 2, "exactly 2 hops");
    assert_eq!(resp.hops[0].entry_id, 2, "hop[0] must be B=2");
    assert_eq!(resp.hops[0].relation_type, "Advances");
    assert_eq!(resp.hops[1].entry_id, 3, "hop[1] must be C=3");
    assert_eq!(resp.hops[1].relation_type, "Supports");
    assert_eq!(resp.length, 2);
    // A is NOT present anywhere in hops (explicit AC-13 check).
    assert!(
        resp.hops.iter().all(|h| h.entry_id != 1),
        "from_id=1 (A) must NOT appear in hops; got: {:?}",
        resp.hops
    );
}

// ---------------------------------------------------------------------------
// SR-C: BFS cycle termination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_bfs_terminates_on_cyclic_graph() {
    // SR-C: Graph with cycle A→B→C→A; target D unreachable (not in graph).
    // BFS must terminate with found: false (no infinite loop).
    // Note: build_typed_relation_graph checks only Supersedes cycles, so
    // non-Supersedes cycles (Supports, etc.) are allowed in the graph.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph = build_typed_relation_graph(
        &[make_entry(1), make_entry(2), make_entry(3)],
        &[
            make_edge(1, 2, "Supports"),
            make_edge(2, 3, "Supports"),
            make_edge(3, 1, "Supports"), // cycle back to A
        ],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(9999), // D — not in graph
        depth: Some(5),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    // to_id=9999 not in snapshot → found: false immediately (snapshot guard fires).
    assert!(
        result.is_ok(),
        "cyclic graph BFS must return Ok, got: {result:?}"
    );
    assert!(
        !result.unwrap().found,
        "unreachable target must return found: false"
    );
}

// ---------------------------------------------------------------------------
// Edge case: depth=1 misses 2-hop path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_depth_1_misses_2hop_path() {
    // A→B→C chain. depth=1 — BFS explores only 1 hop, does not reach C.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let graph = build_typed_relation_graph(
        &[make_entry(1), make_entry(2), make_entry(3)],
        &[make_edge(1, 2, "Supports"), make_edge(2, 3, "Supports")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(3),
        depth: Some(1),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(
        result.is_ok(),
        "depth=1 missing 2-hop path must return Ok, got: {result:?}"
    );
    let resp = result.unwrap();
    assert!(!resp.found, "depth=1 must not find a 2-hop path");
    assert_eq!(resp.length, 0);
}

// ---------------------------------------------------------------------------
// Supersession tests that require DB writes — delegated to child module
// ---------------------------------------------------------------------------

#[path = "graph_read_path_supersession_tests.rs"]
mod supersession;

// ---------------------------------------------------------------------------
// DB-fallback BFS tests (GH #612) — delegated to child module
// ---------------------------------------------------------------------------

#[path = "graph_read_path_db_tests.rs"]
mod db_fallback;
