//! Supersession and endpoint resolution tests for `graph_read_path.rs` (vnc-020).
//!
//! These tests require DB-backed entries because they call `follow_to_current`,
//! which reads `superseded_by` from the store.
//!
//! Tests:
//! - R-03: BFS visited set keyed on resolved ID (double-enqueue prevention)
//! - R-06: Endpoint resolution reflected in response
//! - follow_to_current None fallback
//! - Self-path resolution to different target proceeds normally

use std::sync::Arc;

use unimatrix_core::EntryRecord;
use unimatrix_engine::graph::build_typed_relation_graph;
use unimatrix_store::SqlxStore;

use super::super::super::GraphParams;
use super::super::handle_path;
use super::{make_deprecated_entry, make_edge, make_entry, open_test_store, set_test_graph};
use crate::services::typed_graph::TypedGraphState;

// ---------------------------------------------------------------------------
// DB insert helper — bypasses counter-based ID assignment for test fixtures
// ---------------------------------------------------------------------------

/// Insert an entry directly into the DB with a known ID using `write_pool_server()`.
/// Bypasses the counter-based ID assignment so test fixtures can use specific IDs.
async fn insert_entry_with_id(store: &SqlxStore, entry: &EntryRecord) {
    let pool = store.write_pool_server();
    sqlx::query(
        "INSERT OR REPLACE INTO entries
         (id, title, content, topic, category, source, status, confidence,
          created_at, updated_at, last_accessed_at, access_count,
          supersedes, superseded_by, correction_count, embedding_dim,
          created_by, modified_by, content_hash, previous_hash, version,
          feature_cycle, trust_source, helpful_count, unhelpful_count)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)",
    )
    .bind(entry.id as i64)
    .bind(&entry.title)
    .bind(&entry.content)
    .bind(&entry.topic)
    .bind(&entry.category)
    .bind(&entry.source)
    .bind(entry.status as u8 as i64)
    .bind(entry.confidence)
    .bind(entry.created_at as i64)
    .bind(entry.updated_at as i64)
    .bind(entry.last_accessed_at as i64)
    .bind(entry.access_count as i64)
    .bind(entry.supersedes.map(|v| v as i64))
    .bind(entry.superseded_by.map(|v| v as i64))
    .bind(entry.correction_count as i64)
    .bind(entry.embedding_dim as i64)
    .bind(&entry.created_by)
    .bind(&entry.modified_by)
    .bind(&entry.content_hash)
    .bind(&entry.previous_hash)
    .bind(entry.version as i64)
    .bind(&entry.feature_cycle)
    .bind(&entry.trust_source)
    .bind(entry.helpful_count as i64)
    .bind(entry.unhelpful_count as i64)
    .execute(pool)
    .await
    .expect("insert entry with id");
}

// ---------------------------------------------------------------------------
// R-03: BFS visited set keyed on resolved ID (double-enqueue prevention)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_bfs_visited_set_keyed_on_resolved_id() {
    // Critical: R-03, pattern #4494.
    //
    // Fixture:
    //   from_node (id=1)
    //   D1_dep (id=2, deprecated, superseded_by=4)
    //   D2_dep (id=3, deprecated, superseded_by=4)
    //   C_active (id=4, active) — both D1 and D2 resolve to C_active
    //   Edges: 1→2 (Supports), 1→3 (Supports)
    //
    // With resolve_supersessions=true, D1 and D2 both resolve to C_active (id=4).
    // Expected: C_active appears exactly ONCE in hops (visited set keyed on 4,
    // not on raw IDs 2 and 3, prevents double-enqueue — pattern #4494).
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    // Store deprecated entries so follow_to_current can resolve them.
    let from_node = make_entry(1);
    let d1 = make_deprecated_entry(2, 4);
    let d2 = make_deprecated_entry(3, 4);
    let c_active = make_entry(4);
    insert_entry_with_id(&store, &from_node).await;
    insert_entry_with_id(&store, &d1).await;
    insert_entry_with_id(&store, &d2).await;
    insert_entry_with_id(&store, &c_active).await;

    // Graph snapshot: contains all 4 nodes + edges from 1 to D1 and D2.
    // C_active (4) IS in the graph snapshot (so BFS can continue after resolution).
    let graph = build_typed_relation_graph(
        &[from_node, d1.clone(), d2.clone(), c_active.clone()],
        &[make_edge(1, 2, "Supports"), make_edge(1, 3, "Supports")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(4), // C_active
        resolve_supersessions: Some(true),
        edge_types: Some(vec!["Supports".to_string()]),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();

    assert!(
        resp.found,
        "path from 1 to 4 (via D1 or D2 resolved to C_active) must be found"
    );
    assert_eq!(
        resp.hops.len(),
        1,
        "exactly 1 hop — C_active appears once, not twice"
    );
    assert_eq!(resp.hops[0].entry_id, 4, "hop[0] must be C_active=4");
    // No duplicate hops (double-enqueue prevention — R-03).
    let dup_check: Vec<_> = resp.hops.iter().filter(|h| h.entry_id == 4).collect();
    assert_eq!(
        dup_check.len(),
        1,
        "C_active must appear exactly once in hops"
    );
}

// ---------------------------------------------------------------------------
// R-06: Endpoint resolution reflected in response
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_resolve_supersessions_from_id_reflected() {
    // Deprecated D (id=10, superseded_by=11). Active A (id=11). Edge A→B(20).
    // resolve_supersessions=true → response.from_id == 11 (resolved, not 10).
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let d = make_deprecated_entry(10, 11);
    let a = make_entry(11);
    let b = make_entry(20);
    insert_entry_with_id(&store, &d).await;
    insert_entry_with_id(&store, &a).await;
    insert_entry_with_id(&store, &b).await;

    let graph = build_typed_relation_graph(
        &[d.clone(), a.clone(), b.clone()],
        &[make_edge(11, 20, "Supports")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(10), // deprecated D
        to_id: Some(20),
        resolve_supersessions: Some(true),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    assert_eq!(resp.from_id, 11, "resolved from_id must be 11, not 10");
    assert!(
        resp.found,
        "path from resolved A(11) to B(20) must be found"
    );
}

#[tokio::test]
async fn test_handle_path_resolve_supersessions_to_id_reflected() {
    // Deprecated D2 (id=30, superseded_by=31). Active T (id=31). Edge A(1)→T.
    // resolve_supersessions=true → response.to_id == 31 (resolved, not 30).
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let d2 = make_deprecated_entry(30, 31);
    let t = make_entry(31);
    let a = make_entry(1);
    insert_entry_with_id(&store, &d2).await;
    insert_entry_with_id(&store, &t).await;
    insert_entry_with_id(&store, &a).await;

    let graph = build_typed_relation_graph(
        &[d2.clone(), t.clone(), a.clone()],
        &[make_edge(1, 31, "Supports")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(1),
        to_id: Some(30), // deprecated D2 → resolves to 31
        resolve_supersessions: Some(true),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    assert_eq!(resp.to_id, 31, "resolved to_id must be 31, not 30");
    assert!(resp.found, "path from A(1) to resolved T(31) must be found");
}

#[tokio::test]
async fn test_handle_path_resolve_supersessions_false_uses_original_id() {
    // resolve_supersessions=false → from_id reflected as-is (no DB lookup).
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    // Graph does NOT contain id=10. Snapshot only has id=11 (active).
    let graph = build_typed_relation_graph(&[make_entry(11)], &[]).expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(10), // not in graph (no resolution performed)
        to_id: Some(11),
        resolve_supersessions: Some(false),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    // No resolution performed → from_id stays as original 10.
    assert_eq!(
        resp.from_id, 10,
        "without resolution, from_id must be original 10"
    );
    // from_id=10 not in graph snapshot → found: false.
    assert!(
        !resp.found,
        "from_id not in snapshot (no resolution) → found: false"
    );
}

// ---------------------------------------------------------------------------
// follow_to_current returns None (orphaned chain) → fallback to original ID
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_follow_to_current_none_fallback_uses_original_id() {
    // If follow_to_current returns None (50-hop cap exceeded or orphaned deprecated),
    // handler must fall back to original ID without panicking.
    // Here: from_id=999 not in store at all → follow_to_current returns None → fallback.
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    // Empty graph — no entries stored.
    let graph = build_typed_relation_graph(&[], &[]).expect("build graph");
    set_test_graph(&handle, graph);

    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(999),
        to_id: Some(1000),
        resolve_supersessions: Some(true),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    // follow_to_current returns None for 999 → fallback to 999; 999 not in graph → found: false
    assert!(
        result.is_ok(),
        "follow_to_current None must not panic, got: {result:?}"
    );
    let resp = result.unwrap();
    assert!(!resp.found, "orphaned chain fallback → found: false");
    // from_id in response must be the fallback (original 999), NOT a panic.
    assert_eq!(resp.from_id, 999, "fallback ID must be original 999");
}

// ---------------------------------------------------------------------------
// Edge case: from_id resolves to a different target — BFS runs normally
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_path_self_resolves_to_different_target_proceeds_normally() {
    // from_id=10 resolves to 11 via resolve_supersessions; to_id=12 ≠ 11.
    // After resolution, effective_from=11, effective_to=12 — not a self-path.
    // BFS should run normally and find the 1-hop path A(11)→B(12).
    let (store, _dir) = open_test_store().await;
    let handle = Arc::new(TypedGraphState::new_handle());

    let d = make_deprecated_entry(10, 11);
    let a = make_entry(11);
    let b = make_entry(12);
    insert_entry_with_id(&store, &d).await;
    insert_entry_with_id(&store, &a).await;
    insert_entry_with_id(&store, &b).await;

    let graph = build_typed_relation_graph(
        &[d.clone(), a.clone(), b.clone()],
        &[make_edge(11, 12, "Supports")],
    )
    .expect("build graph");
    set_test_graph(&handle, graph);

    // from_id=10 (deprecated, resolves to 11), to_id=12; not a self-path after resolution.
    let params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(10),
        to_id: Some(12),
        resolve_supersessions: Some(true),
        ..Default::default()
    };
    let result = handle_path(&store, &handle, &params).await;
    assert!(result.is_ok(), "got: {result:?}");
    let resp = result.unwrap();
    assert!(
        resp.found,
        "resolved from_id=11 → to_id=12 path must be found"
    );
    assert_eq!(resp.from_id, 11, "from_id in response must be resolved ID");
    assert_eq!(resp.to_id, 12);
}
