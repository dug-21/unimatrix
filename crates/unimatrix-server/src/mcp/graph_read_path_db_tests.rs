//! DB-fallback BFS regression tests for `graph_read_path.rs` (GH #612).
//!
//! These tests exercise `handle_path` when `use_fallback = true` (cold-start or
//! cycle-detected state), verifying that path mode falls through to `path_via_db()`
//! rather than the in-memory snapshot. All tests write edges directly to the DB via
//! `write_graph_edge` without calling `rebuild_typed_graph()`.
//!
//! Declared as a sibling test module via `#[path]` in `graph_read_path_tests.rs`.

use std::sync::Arc;

use unimatrix_store::NewEntry;

use super::super::super::GraphParams;
use super::super::handle_path;
use super::open_test_store;
use crate::services::typed_graph::TypedGraphState;

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
