//! Early-return validation tests for `graph_read_filter.rs`.
//!
//! These tests cover input validation that fires before SQL construction:
//! AC-09 (edge_types required), AC-10 (category required), AC-11 (limit range),
//! and confidence finiteness (NaN/±Infinity guard added in #615).
//!
//! Declared as `validation_tests` sub-module of `graph_read_filter` via `#[path]`.

use super::super::GraphParams;
use super::*;
use unimatrix_store::{NewEntry, PoolConfig, SqlxStore, Status};

// -----------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

/// Insert an entry via the standard SqlxStore API; returns the assigned ID.
async fn store_entry(store: &SqlxStore, category: &str, title: &str) -> u64 {
    let entry = NewEntry {
        title: title.to_string(),
        content: format!("content for {title}"),
        topic: "test".to_string(),
        category: category.to_string(),
        tags: vec![],
        source: "test".to_string(),
        status: Status::Active,
        created_by: String::new(),
        feature_cycle: "vnc-020".to_string(),
        trust_source: "agent".to_string(),
    };
    store.insert(entry).await.expect("insert entry")
}

// -----------------------------------------------------------------------
// AC-10 — category required
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_handle_filter_missing_category_returns_error() {
    // AC-10: category=None must return exact error text.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: None,
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.message, "filter mode requires category",
        "exact error text required, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_filter_empty_category_returns_error() {
    // AC-10: category="" (empty) must return the same error.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("".to_string()),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.message, "filter mode requires category",
        "empty string must also trigger error, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// AC-09 — edge_types required when edge-count constraints present
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_handle_filter_min_edge_count_without_edge_types_returns_error() {
    // AC-09: min_edge_count present, edge_types=None → error.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_edge_count: Some(1),
        edge_types: None,
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.message, "filter mode requires edge_types when edge_count constraints are specified",
        "exact error text required, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_filter_min_edge_count_with_empty_edge_types_returns_error() {
    // AC-09: min_edge_count present, edge_types=Some(vec![]) → same error.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_edge_count: Some(1),
        edge_types: Some(vec![]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.message, "filter mode requires edge_types when edge_count constraints are specified",
        "empty edge_types must trigger error, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_filter_max_edge_count_without_edge_types_returns_error() {
    // AC-09: max_edge_count=0, edge_types=None → error.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        max_edge_count: Some(0),
        edge_types: None,
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(
        err.message, "filter mode requires edge_types when edge_count constraints are specified",
        "exact error text required, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// AC-11 — limit boundary validation
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_handle_filter_limit_zero_returns_error() {
    // AC-11: limit=0 is out of range.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        limit: Some(0),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("1..=500"),
        "error must state range [1,500], got: {}",
        err.message
    );
    assert!(
        err.message.contains("got 0"),
        "error must include the bad value, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_handle_filter_limit_501_returns_error() {
    // AC-11: limit=501 is out of range.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        limit: Some(501),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("1..=500"),
        "error must state range [1,500], got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// AC-11 — limit default (lightweight fixture, no edges)
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_handle_filter_limit_default_is_100() {
    // AC-11: limit=None → default 100; with exactly 3 entries we get 3 back.
    let (store_impl, _dir) = open_test_store().await;
    store_entry(&store_impl, "goal", "Goal A").await;
    store_entry(&store_impl, "goal", "Goal B").await;
    store_entry(&store_impl, "goal", "Goal C").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        limit: None,
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    let resp = result.unwrap();
    assert_eq!(
        resp.entries.len(),
        3,
        "expected 3 entries with default limit=100"
    );
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// R-11 — category-only query (no other filters) is valid
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_handle_filter_category_only_no_validation_error() {
    // R-11: category-only query returns all active entries in category.
    let (store_impl, _dir) = open_test_store().await;
    store_entry(&store_impl, "goal", "Goal 1").await;
    store_entry(&store_impl, "goal", "Goal 2").await;
    store_entry(&store_impl, "goal", "Goal 3").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(
        result.is_ok(),
        "category-only filter must succeed, got: {:?}",
        result
    );
    let resp = result.unwrap();
    assert_eq!(resp.entries.len(), 3);
    assert_eq!(resp.total_returned, 3);
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// AC-12 — total_returned == entries.len()
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_handle_filter_total_returned_matches_len() {
    // AC-12: total_returned must equal entries.len() in every response.
    let (store_impl, _dir) = open_test_store().await;
    for i in 0..5 {
        store_entry(&store_impl, "goal", &format!("Goal {i}")).await;
    }
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(
        resp.total_returned,
        resp.entries.len(),
        "total_returned must equal entries.len()"
    );
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// Unrecognized edge type — parse_relation_types fires before SQL
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_filter_unrecognized_edge_type_returns_error() {
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_edge_count: Some(1),
        edge_types: Some(vec!["BogusType".to_string()]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.message.contains("BogusType"),
        "error must name the unrecognized type, got: {}",
        err.message
    );
    assert!(
        err.message.contains("recognized types"),
        "error must list recognized types, got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// Confidence finiteness guard (#615)
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_filter_nan_min_confidence_returns_error() {
    // NaN passed as min_confidence must be rejected before SQL construction.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_confidence: Some(f64::NAN),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err(), "NaN min_confidence must return Err");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("finite"),
        "error must mention 'finite', got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_filter_nan_max_confidence_returns_error() {
    // NaN passed as max_confidence must be rejected before SQL construction.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        max_confidence: Some(f64::NAN),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err(), "NaN max_confidence must return Err");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("finite"),
        "error must mention 'finite', got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_filter_inf_min_confidence_returns_error() {
    // +Infinity passed as min_confidence must be rejected before SQL construction.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_confidence: Some(f64::INFINITY),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err(), "+Infinity min_confidence must return Err");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("finite"),
        "error must mention 'finite', got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_filter_neg_inf_max_confidence_returns_error() {
    // -Infinity passed as max_confidence must be rejected before SQL construction.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        max_confidence: Some(f64::NEG_INFINITY),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_err(), "-Infinity max_confidence must return Err");
    let err = result.unwrap_err();
    assert!(
        err.message.contains("finite"),
        "error must mention 'finite', got: {}",
        err.message
    );
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_filter_valid_finite_confidence_does_not_trigger_guard() {
    // A valid finite value (0.5) must not be rejected by the finiteness guard.
    let (store_impl, _dir) = open_test_store().await;
    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_confidence: Some(0.5),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(
        result.is_ok(),
        "finite min_confidence=0.5 must not be rejected, got: {:?}",
        result
    );
    store_impl.close().await.unwrap();
}
