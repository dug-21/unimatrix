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
// GH #612: DB-fallback BFS when use_fallback = true (cold-start / cycle-detected)
// ---------------------------------------------------------------------------

/// Mandatory regression test for GH #612.
///
/// Verifies that `handle_path` returns a correct result when `use_fallback = true`
/// (the initial state of `TypedGraphState::new()`) even though the in-memory graph
/// has NOT been rebuilt. Edges are written directly to the DB via `write_graph_edge`
/// (same code path as `context_edge add`); `rebuild_typed_graph()` is never called.
///
/// This simulates the cold-start scenario the existing unit tests bypass by
/// pre-building the graph via `set_test_graph()`.
#[tokio::test]
async fn test_handle_path_db_fallback_cold_start_finds_path() {
    use unimatrix_store::NewEntry;

    let (store, _dir) = open_test_store().await;
    let store = std::sync::Arc::new(store);

    // Step 1: Insert entries A, B, C via the store.
    let id_a = store
        .insert(NewEntry {
            title: "Entry A".to_string(),
            content: "content-a".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-612".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert A");

    let id_b = store
        .insert(NewEntry {
            title: "Entry B".to_string(),
            content: "content-b".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-612".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert B");

    let id_c = store
        .insert(NewEntry {
            title: "Entry C".to_string(),
            content: "content-c".to_string(),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-612".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert C");

    // Step 2: Write edges A→B (Advances) and B→C (Supports) via write_graph_edge —
    // same code path as context_edge add.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    crate::services::nli_detection::write_graph_edge(
        &*store, id_a, id_b, "Advances", 1.0, now, "test", "",
    )
    .await;

    crate::services::nli_detection::write_graph_edge(
        &*store, id_b, id_c, "Supports", 1.0, now, "test", "",
    )
    .await;

    // Step 3: Create TypedGraphState with use_fallback = true (initial state).
    // Do NOT call rebuild() — simulates cold-start where tick has not yet run.
    let handle = Arc::new(std::sync::RwLock::new(
        crate::services::typed_graph::TypedGraphState::new(),
    ));

    // Verify the handle is in cold-start state.
    {
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(guard.use_fallback, "must be cold-start (use_fallback=true)");
    }

    // Step 4: Call handle_path from A to C — must find path via DB fallback.
    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(id_a),
        to_id: Some(id_c),
        edge_types: Some(vec!["Advances".to_string(), "Supports".to_string()]),
        depth: Some(5),
        ..Default::default()
    };
    let result = handle_path(&*store, &handle, &params).await;
    assert!(
        result.is_ok(),
        "DB-fallback path must return Ok, got: {result:?}"
    );
    let resp = result.unwrap();

    // Step 5: Assert found: true and path contains B as intermediate.
    assert!(
        resp.found,
        "DB-fallback BFS must find path A→B→C with use_fallback=true; got found=false"
    );
    assert_eq!(
        resp.from_id, id_a,
        "from_id must be A={id_a}, got {}",
        resp.from_id
    );
    assert_eq!(
        resp.to_id, id_c,
        "to_id must be C={id_c}, got {}",
        resp.to_id
    );
    assert_eq!(
        resp.hops.len(),
        2,
        "A→B→C must yield exactly 2 hops; got {:?}",
        resp.hops
    );
    assert_eq!(
        resp.hops[0].entry_id, id_b,
        "hops[0] must be B={id_b}; got {:?}",
        resp.hops[0]
    );
    assert_eq!(
        resp.hops[0].relation_type, "Advances",
        "hops[0].relation_type must be Advances; got {}",
        resp.hops[0].relation_type
    );
    assert_eq!(
        resp.hops[1].entry_id, id_c,
        "hops[1] must be C={id_c}; got {:?}",
        resp.hops[1]
    );
    assert_eq!(
        resp.hops[1].relation_type, "Supports",
        "hops[1].relation_type must be Supports; got {}",
        resp.hops[1].relation_type
    );
    assert_eq!(resp.length, 2, "length must be 2; got {}", resp.length);

    // from_id must NOT appear in hops (ADR-005).
    assert!(
        resp.hops.iter().all(|h| h.entry_id != id_a),
        "from_id (A={id_a}) must NOT appear in hops; got {:?}",
        resp.hops
    );
}

// ---------------------------------------------------------------------------
// Supersession tests that require DB writes — delegated to child module
// ---------------------------------------------------------------------------

#[path = "graph_read_path_supersession_tests.rs"]
mod supersession;
