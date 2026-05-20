//! Tests for graph_read_inverse.rs (vnc-020).
//!
//! Extracted to a separate file to keep graph_read_inverse.rs under the 500-line limit.
//! Covers all ACs from the component test plan: AC-02, AC-03, AC-03a, AC-04, AC-05,
//! AC-06, AC-27, AC-28, R-10, IR-01, SR-B.

use unimatrix_store::{NewEntry, PoolConfig, SqlxStore, Status};

use super::super::GraphParams;
use super::*;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

/// Insert a minimal active entry and return the assigned ID.
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

/// Insert a graph_edges row directly via the write pool.
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

/// Deprecate an entry by setting status=1 (Deprecated) directly in the DB.
async fn deprecate_entry(store: &SqlxStore, entry_id: u64) {
    sqlx::query("UPDATE entries SET status = 1, updated_at = 1700000001 WHERE id = ?1")
        .bind(entry_id as i64)
        .execute(store.write_pool_test())
        .await
        .expect("deprecate entry");
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
// AC-04 — category Required
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_inverse_missing_category_returns_error() {
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(None, Some(vec!["Cites"]), None);
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.message, "inverse mode requires category");
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_inverse_empty_category_returns_error() {
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some(""), Some(vec!["Cites"]), None);
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.message, "inverse mode requires category");
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-03 — missing_edge_types Absent or Empty
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_inverse_missing_edge_types_none_returns_error() {
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some("source"), None, None);
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.message,
        "inverse mode requires at least one edge type in missing_edge_types"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_inverse_missing_edge_types_empty_returns_error() {
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some("source"), Some(vec![]), None);
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.message,
        "inverse mode requires at least one edge type in missing_edge_types"
    );
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-02 — Unrecognized Edge Type
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_inverse_unrecognized_edge_type_returns_error() {
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some("source"), Some(vec!["NotAType"]), None);
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("NotAType"),
        "error must name the unrecognized type; got: {}",
        err.message
    );
    // All 16 recognized types must be listed in the error.
    for name in &[
        "Cites",
        "Supports",
        "Advances",
        "About",
        "Asserts",
        "CoAccess",
        "Contradicts",
        "DerivedFrom",
        "Informs",
        "Mentions",
        "Motivates",
        "Prerequisite",
        "Refutes",
        "RelatedTo",
        "Supersedes",
        "Tests",
    ] {
        assert!(
            err.message.contains(name),
            "error must list recognized type '{name}'; got: {}",
            err.message
        );
    }
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_inverse_sql_injection_rejected_by_type_validation() {
    // SR-B: crafted type string never reaches SQL construction.
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(
        Some("source"),
        Some(vec!["Cites'; DROP TABLE entries; --"]),
        None,
    );
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("unrecognized edge type"));
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-05 — limit Boundary Validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_inverse_limit_zero_returns_error() {
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some("source"), Some(vec!["Cites"]), Some(0));
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("1..=500"),
        "error must state allowed range; got: {}",
        err.message
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_inverse_limit_501_returns_error() {
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some("source"), Some(vec!["Cites"]), Some(501));
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("1..=500"),
        "error must state allowed range; got: {}",
        err.message
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_inverse_limit_500_accepted() {
    // Boundary: 500 is the valid maximum — empty DB is fine.
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some("source"), Some(vec!["Cites"]), Some(500));
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_ok(), "limit=500 must be accepted");
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_inverse_limit_1_accepted() {
    // Boundary: 1 is the valid minimum — empty DB is fine.
    let (store, _dir) = open_test_store().await;
    let params = inverse_params(Some("source"), Some(vec!["Cites"]), Some(1));
    let result = handle_inverse(&store, &params).await;
    assert!(result.is_ok(), "limit=1 must be accepted");
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-06 — total_returned Matches entries.len()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_handle_inverse_total_returned_matches_len() {
    // 3 active "source" entries with no Cites edges, 2 with Cites incoming.
    let (store, _dir) = open_test_store().await;

    let donor_id = insert_entry(&store, "decision").await;

    // 3 entries with NO incoming Cites.
    for _ in 0..3 {
        insert_entry(&store, "source").await;
    }

    // 2 entries WITH incoming Cites.
    for _ in 0..2 {
        let id = insert_entry(&store, "source").await;
        insert_edge(&store, donor_id, id, "Cites").await;
    }

    let params = inverse_params(Some("source"), Some(vec!["Cites"]), None);
    let resp = handle_inverse(&store, &params)
        .await
        .expect("handle_inverse");

    assert_eq!(resp.entries.len(), 3);
    assert_eq!(resp.total_returned, 3);
    assert_eq!(resp.total_returned, resp.entries.len());

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// R-10 — SQL Always Has status = 0 Guard
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_inverse_sql_includes_status_guard_n1() {
    // Behavioral: deprecated entry with no Cites edges must NOT appear.
    let (store, _dir) = open_test_store().await;

    let deprecated_id = insert_entry(&store, "source").await;
    let active_id = insert_entry(&store, "source").await;
    deprecate_entry(&store, deprecated_id).await;

    let params = inverse_params(Some("source"), Some(vec!["Cites"]), None);
    let resp = handle_inverse(&store, &params)
        .await
        .expect("handle_inverse");

    let ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(
        !ids.contains(&deprecated_id),
        "deprecated entry must not appear: {ids:?}"
    );
    assert!(
        ids.contains(&active_id),
        "active entry must appear: {ids:?}"
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_inverse_sql_includes_status_guard_n3() {
    // IR-01: N=3 LEFT JOINs — SQL still correct, status guard still present.
    let (store, _dir) = open_test_store().await;

    let deprecated_id = insert_entry(&store, "source").await;
    let active_id = insert_entry(&store, "source").await;
    deprecate_entry(&store, deprecated_id).await;

    let params = inverse_params(
        Some("source"),
        Some(vec!["Cites", "Supports", "Advances"]),
        None,
    );
    let resp = handle_inverse(&store, &params)
        .await
        .expect("handle_inverse");

    let ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(
        !ids.contains(&deprecated_id),
        "deprecated entry must not appear: {ids:?}"
    );
    assert!(
        ids.contains(&active_id),
        "active entry must appear: {ids:?}"
    );

    store.close().await.unwrap();
}

// Integration tests (AC-27, AC-28, limit default, edge cases) are in a child module.
#[path = "graph_read_inverse_integration_tests.rs"]
mod integration;
