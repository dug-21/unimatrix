//! bugfix-881 — cross-surface supersession-resolution consistency.
//!
//! Regression guard for the divergence that shipped invisibly: `context_get` resolved
//! supersessions BY DEFAULT (vnc-042) while `context_graph`'s traversal modes did not. Each
//! surface was previously tested only in isolation with an EXPLICIT flag, so the default-path
//! divergence never showed up in CI. This test supersedes X → X′ once and asserts that ALL
//! read surfaces surface X′ by DEFAULT (no `resolve_supersessions` flag): context_get
//! (resolve_effective_id anchor resolution), neighbors at DEFAULT depth 1 (the arm G1 fixed —
//! previously ignored the flag entirely), subgraph, and path.
//!
//! It also asserts the escape hatch: an explicit `resolve_supersessions:false` neighbors call
//! still returns the raw as-stored X.

use std::sync::{Arc, RwLock};

use unimatrix_core::Store;
use unimatrix_store::{NewEntry, PoolConfig, SqlxStore, Status};

use crate::mcp::graph_read::GraphParams;
use crate::mcp::graph_read::graph_read_neighbors::handle_neighbors;
use crate::mcp::graph_read::graph_read_path::handle_path;
use crate::mcp::graph_read::graph_read_subgraph::handle_subgraph;
use crate::mcp::tools::resolve_effective_id;
use crate::services::typed_graph::TypedGraphState;

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

async fn insert_active(store: &SqlxStore, title: &str) -> u64 {
    store
        .insert(NewEntry {
            title: title.to_string(),
            content: format!("content-{title}"),
            topic: "test".to_string(),
            category: "pattern".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "bugfix-881".to_string(),
            trust_source: "agent".to_string(),
        })
        .await
        .unwrap_or_else(|e| panic!("insert {title}: {e:?}"))
}

async fn insert_supports_edge(store: &SqlxStore, source_id: u64, target_id: u64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at,
              created_by, source, bootstrap_only)
         VALUES (?1, ?2, 'Supports', 1.0, ?3, 'test', 'agent', 0)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(now as i64)
    .execute(store.write_pool_server())
    .await
    .expect("insert Supports edge");
}

async fn deprecate_toward(store: &SqlxStore, id: u64, superseded_by: u64) {
    sqlx::query("UPDATE entries SET status = 1, superseded_by = ?2 WHERE id = ?1")
        .bind(id as i64)
        .bind(superseded_by as i64)
        .execute(store.write_pool_server())
        .await
        .expect("deprecate entry");
}

/// Cold-start handle (`use_fallback = true`) so subgraph/path take the live-SQL DB-fallback
/// path (no tick rebuild needed); neighbors depth=1 always uses live SQL.
fn cold_start_handle() -> Arc<RwLock<TypedGraphState>> {
    Arc::new(RwLock::new(TypedGraphState::new()))
}

/// Supersede X → X′ once; assert every read surface resolves to X′ BY DEFAULT.
#[tokio::test]
async fn test_cross_surface_default_resolves_to_terminal() {
    let (store, _dir) = open_test_store().await;
    let handle = cold_start_handle();

    let a = insert_active(&store, "A").await;
    let x = insert_active(&store, "X-stale").await;
    let x_prime = insert_active(&store, "X-prime").await;
    insert_supports_edge(&store, a, x).await;
    deprecate_toward(&store, x, x_prime).await;

    // --- context_get: anchor resolves X → X′ by default (None). ---
    let (eff, _note) = resolve_effective_id(&store as &Store, x, None).await;
    assert_eq!(eff, x_prime, "context_get default must resolve X → X′");

    // --- neighbors at DEFAULT depth 1 (depth omitted) — this is the arm G1 fixed. ---
    let neighbors_params = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(a),
        ..Default::default()
    };
    let neighbors = handle_neighbors(&store, &handle, &neighbors_params, a)
        .await
        .expect("neighbors ok");
    let edge = neighbors
        .edges
        .iter()
        .find(|e| e.relation_type == "Supports")
        .expect("A→ Supports edge present");
    assert_eq!(
        edge.target_id, x_prime,
        "neighbors (default depth 1) must surface X′, not deprecated X (G1 regression guard)"
    );

    // --- subgraph seeded on A. ---
    let subgraph_params = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(vec![a]),
        ..Default::default()
    };
    let subgraph = handle_subgraph(&store, &handle, &subgraph_params)
        .await
        .expect("subgraph ok");
    assert!(
        subgraph.edges.iter().any(|e| e.target_id == x_prime),
        "subgraph must surface an edge to X′ by default"
    );
    assert!(
        !subgraph.edges.iter().any(|e| e.target_id == x),
        "subgraph must NOT surface the deprecated X target by default"
    );
    assert!(
        subgraph.nodes.iter().any(|n| n.id == x_prime),
        "subgraph node set must include the terminal X′"
    );

    // --- path A → X (to_id given as the deprecated id). ---
    let path_params = GraphParams {
        mode: "path".to_string(),
        from_id: Some(a),
        to_id: Some(x),
        ..Default::default()
    };
    let path = handle_path(&store, &handle, &path_params)
        .await
        .expect("path ok");
    assert!(path.found, "path A→X must be found (to_id resolves to X′)");
    assert_eq!(path.to_id, x_prime, "path to_id must be resolved to X′");
    assert!(
        path.hops.iter().any(|h| h.entry_id == x_prime),
        "path hops must terminate at X′, not deprecated X"
    );
}

/// The explicit `resolve_supersessions:false` opt-out still returns the raw as-stored X —
/// the audit surface is preserved after the default flip.
#[tokio::test]
async fn test_neighbors_explicit_false_returns_raw() {
    let (store, _dir) = open_test_store().await;
    let handle = cold_start_handle();

    let a = insert_active(&store, "A").await;
    let x = insert_active(&store, "X-stale").await;
    let x_prime = insert_active(&store, "X-prime").await;
    insert_supports_edge(&store, a, x).await;
    deprecate_toward(&store, x, x_prime).await;

    let params = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(a),
        resolve_supersessions: Some(false),
        ..Default::default()
    };
    let neighbors = handle_neighbors(&store, &handle, &params, a)
        .await
        .expect("neighbors ok");
    let edge = neighbors
        .edges
        .iter()
        .find(|e| e.relation_type == "Supports")
        .expect("A→ Supports edge present");
    assert_eq!(
        edge.target_id, x,
        "explicit resolve_supersessions:false must return the raw as-stored X (audit opt-out)"
    );
}
