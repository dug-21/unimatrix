//! Integration tests for graph_read_inverse.rs (vnc-020).
//!
//! Covers AC-27, AC-28, limit-default behavioral test, and edge cases.
//! These tests require a live SqlxStore with entries and graph edges.
//! Declared as a child module via `#[path]` in graph_read_inverse_tests.rs.

use unimatrix_store::{NewEntry, PoolConfig, SqlxStore, Status};

use super::super::super::GraphParams;
use super::super::*;

// ---------------------------------------------------------------------------
// Test helpers (duplicated from parent module — child modules cannot inherit)
// ---------------------------------------------------------------------------

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

async fn insert_entry(store: &SqlxStore, category: &str) -> u64 {
    store
        .insert(NewEntry {
            title: format!("test-{category}"),
            content: String::new(),
            topic: String::new(),
            category: category.to_string(),
            tags: vec![],
            source: String::new(),
            status: Status::Active,
            created_by: String::new(),
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
        })
        .await
        .expect("insert entry")
}

async fn insert_edge(store: &SqlxStore, source_id: u64, target_id: u64, rel: &str) {
    let now = 1_700_000_000_i64;
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges \
         (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) \
         VALUES (?1, ?2, ?3, 1.0, ?4, '', '', 0)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(rel)
    .bind(now)
    .execute(store.write_pool_test())
    .await
    .expect("insert edge");
}

async fn deprecate_entry(store: &SqlxStore, entry_id: u64) {
    sqlx::query("UPDATE entries SET status = 1, updated_at = 1700000001 WHERE id = ?1")
        .bind(entry_id as i64)
        .execute(store.write_pool_test())
        .await
        .expect("deprecate entry");
}

/// Deprecate an entry and record it as superseded by another entry.
/// Sets status=1 (Deprecated) and superseded_by=successor_id directly in the DB.
async fn set_superseded_by(store: &SqlxStore, deprecated_id: u64, successor_id: u64) {
    sqlx::query(
        "UPDATE entries SET status = 1, superseded_by = ?1, updated_at = 1700000002 WHERE id = ?2",
    )
    .bind(successor_id as i64)
    .bind(deprecated_id as i64)
    .execute(store.write_pool_test())
    .await
    .expect("set superseded_by");
}

fn inverse_params(
    category: Option<&str>,
    missing_edge_types: Option<Vec<&str>>,
    limit: Option<u32>,
) -> GraphParams {
    GraphParams {
        mode: "inverse".to_string(),
        category: category.map(|s| s.to_string()),
        missing_edge_types: missing_edge_types
            .map(|v| v.into_iter().map(|s| s.to_string()).collect()),
        limit,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// AC-27 — Single-type integration (status guard + with/without edge)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_context_graph_inverse_single_type() {
    let (store, _dir) = open_test_store().await;

    let donor_id = insert_entry(&store, "decision").await;

    // active, no incoming Cites → should appear.
    let id_no_edge = insert_entry(&store, "source").await;

    // active, has incoming Cites → should NOT appear.
    let id_with_edge = insert_entry(&store, "source").await;
    insert_edge(&store, donor_id, id_with_edge, "Cites").await;

    // deprecated, no incoming Cites → should NOT appear (R-10).
    let id_deprecated = insert_entry(&store, "source").await;
    deprecate_entry(&store, id_deprecated).await;

    let params = inverse_params(Some("source"), Some(vec!["Cites"]), None);
    let resp = handle_inverse(&store, &params)
        .await
        .expect("handle_inverse");

    let ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(
        ids.contains(&id_no_edge),
        "active entry without edge must appear; ids={ids:?}"
    );
    assert!(
        !ids.contains(&id_with_edge),
        "active entry with Cites must not appear; ids={ids:?}"
    );
    assert!(
        !ids.contains(&id_deprecated),
        "deprecated entry must not appear; ids={ids:?}"
    );
    assert_eq!(resp.total_returned, resp.entries.len());

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-28 — AND semantics (4-state fixture)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_context_graph_inverse_and_semantics() {
    // 4-state fixture:
    // entry_a: no Cites, no Supports → should appear (missing BOTH — AND semantics).
    // entry_b: has Cites, no Supports → should NOT appear.
    // entry_c: no Cites, has Supports → should NOT appear.
    // entry_d: has Cites, has Supports → should NOT appear.
    let (store, _dir) = open_test_store().await;

    let donor_id = insert_entry(&store, "decision").await;

    let id_a = insert_entry(&store, "source").await; // missing both
    let id_b = insert_entry(&store, "source").await; // has Cites
    let id_c = insert_entry(&store, "source").await; // has Supports
    let id_d = insert_entry(&store, "source").await; // has both

    insert_edge(&store, donor_id, id_b, "Cites").await;
    insert_edge(&store, donor_id, id_d, "Cites").await;
    insert_edge(&store, donor_id, id_c, "Supports").await;
    insert_edge(&store, donor_id, id_d, "Supports").await;

    let params = inverse_params(Some("source"), Some(vec!["Cites", "Supports"]), None);
    let resp = handle_inverse(&store, &params)
        .await
        .expect("handle_inverse");

    let ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(ids.contains(&id_a), "entry_a (missing both) must appear");
    assert!(!ids.contains(&id_b), "entry_b (has Cites) must not appear");
    assert!(
        !ids.contains(&id_c),
        "entry_c (has Supports) must not appear"
    );
    assert!(!ids.contains(&id_d), "entry_d (has both) must not appear");
    assert_eq!(resp.total_returned, 1, "only entry_a should be returned");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-05 — limit default is 100 (behavioral)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_inverse_limit_default_is_100() {
    // 110 active entries, no Cites edges; default limit must cap at 100.
    let (store, _dir) = open_test_store().await;

    for _ in 0..110 {
        insert_entry(&store, "source").await;
    }

    let params = inverse_params(Some("source"), Some(vec!["Cites"]), None);
    let resp = handle_inverse(&store, &params)
        .await
        .expect("handle_inverse");

    assert_eq!(
        resp.entries.len(),
        100,
        "default limit must cap result at 100; got {}",
        resp.entries.len()
    );
    assert_eq!(resp.total_returned, 100);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_inverse_duplicate_edge_types_not_an_error() {
    // ["Cites","Cites"] — duplicate type must not panic or error.
    let (store, _dir) = open_test_store().await;
    insert_entry(&store, "source").await;
    let params = inverse_params(Some("source"), Some(vec!["Cites", "Cites"]), None);
    let result = handle_inverse(&store, &params).await;
    assert!(
        result.is_ok(),
        "duplicate edge types must not cause an error"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_inverse_10_types_does_not_error() {
    // IR-01: 10 LEFT JOINs — SQLite can handle this.
    let (store, _dir) = open_test_store().await;
    insert_entry(&store, "source").await;
    let types = vec![
        "Cites",
        "Supports",
        "Advances",
        "About",
        "Asserts",
        "Contradicts",
        "DerivedFrom",
        "Informs",
        "Mentions",
        "Motivates",
    ];
    let params = inverse_params(Some("source"), Some(types), None);
    let result = handle_inverse(&store, &params).await;
    assert!(
        result.is_ok(),
        "10-type antijoin must execute without error"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_inverse_empty_category_in_db_returns_empty_not_error() {
    // Category with no entries → Ok with empty list, not an error.
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some("goal"), Some(vec!["Cites"]), None);
    let resp = handle_inverse(&store, &params)
        .await
        .expect("handle_inverse");
    assert_eq!(resp.entries.len(), 0);
    assert_eq!(resp.total_returned, 0);
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// #616B — Deprecated-superseded entry absent from inverse results
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_inverse_deprecated_superseded_entry_absent() {
    // #616B: A deprecated entry (even one that is part of a supersession chain)
    // must NOT appear in inverse results. The AND e.status = 0 guard excludes
    // all non-active entries regardless of edge presence.
    //
    // Fixture:
    //   entry_a — category="source", deprecated, superseded_by entry_b
    //   entry_b — category="source", active, has an incoming Cites edge
    //   donor   — category="decision", active, provides the Cites edge to entry_b
    //
    // Expected:
    //   entry_a absent — deprecated (status=1), excluded by status guard.
    //   entry_b absent — active but has an incoming Cites edge, excluded by antijoin.
    let (store, _dir) = open_test_store().await;

    let donor_id = insert_entry(&store, "decision").await;
    let id_a = insert_entry(&store, "source").await;
    let id_b = insert_entry(&store, "source").await;

    // Give entry_b an incoming Cites edge so it's excluded by the antijoin.
    insert_edge(&store, donor_id, id_b, "Cites").await;

    // Deprecate entry_a and mark it as superseded_by entry_b.
    set_superseded_by(&store, id_a, id_b).await;

    let params = inverse_params(Some("source"), Some(vec!["Cites"]), None);
    let resp = handle_inverse(&store, &params)
        .await
        .expect("handle_inverse");

    let ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();

    assert!(
        !ids.contains(&id_a),
        "deprecated-superseded entry_a must not appear (status guard); ids={ids:?}"
    );
    assert!(
        !ids.contains(&id_b),
        "active entry_b (has Cites edge) must not appear (antijoin); ids={ids:?}"
    );

    store.close().await.unwrap();
}
