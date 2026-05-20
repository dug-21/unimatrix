//! Unit tests for `graph_read_filter.rs`.
//!
//! Extracted to a separate file to keep `graph_read_filter.rs` under the 500-line limit.
//! Covers: AC-07 through AC-12, AC-29, AC-30, R-02, R-08, R-10, R-11, IR-04.

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

fn filter_params(category: Option<&str>) -> GraphParams {
    GraphParams {
        mode: "filter".to_string(),
        category: category.map(|s| s.to_string()),
        ..Default::default()
    }
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

/// Insert a graph edge directly via the write pool.
async fn add_edge(store: &SqlxStore, source_id: u64, target_id: u64, rel_type: &str) {
    let now = 1_700_000_000_i64;
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges \
         (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) \
         VALUES (?1, ?2, ?3, 1.0, ?4, '', 'test', 0)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(rel_type)
    .bind(now)
    .execute(store.write_pool_test())
    .await
    .expect("insert edge");
}

// -----------------------------------------------------------------------
// AC-10 — category required
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_handle_filter_missing_category_returns_error() {
    // AC-10: category=None must return exact error text.
    let (store_impl, _dir) = open_test_store().await;
    let params = filter_params(None);
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
    // Default limit=100 means all 3 are returned.
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
// R-02 — max_edge_count=0 uses <= binding, not special-cased (AC-29)
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_filter_max_edge_count_zero_uses_lte_binding() {
    // R-02 (Critical) / AC-29: max_edge_count=0 returns only 0-edge entries.
    // Behavioral verification: 4-entry fixture with 0/1/2/3 outgoing Advances edges.
    let (store_impl, _dir) = open_test_store().await;

    let entry_0 = store_entry(&store_impl, "goal", "Zero edges").await;
    let entry_1 = store_entry(&store_impl, "goal", "One edge").await;
    let entry_2 = store_entry(&store_impl, "goal", "Two edges").await;
    let entry_3 = store_entry(&store_impl, "goal", "Three edges").await;

    // Targets (different category so they don't pollute filter results).
    let t1 = store_entry(&store_impl, "other", "Target 1").await;
    let t2 = store_entry(&store_impl, "other", "Target 2").await;
    let t3 = store_entry(&store_impl, "other", "Target 3").await;
    let t4 = store_entry(&store_impl, "other", "Target 4").await;
    let t5 = store_entry(&store_impl, "other", "Target 5").await;
    let t6 = store_entry(&store_impl, "other", "Target 6").await;

    // entry_1 → 1 Advances edge.
    add_edge(&store_impl, entry_1, t1, "Advances").await;
    // entry_2 → 2 Advances edges.
    add_edge(&store_impl, entry_2, t2, "Advances").await;
    add_edge(&store_impl, entry_2, t3, "Advances").await;
    // entry_3 → 3 Advances edges.
    add_edge(&store_impl, entry_3, t4, "Advances").await;
    add_edge(&store_impl, entry_3, t5, "Advances").await;
    add_edge(&store_impl, entry_3, t6, "Advances").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        max_edge_count: Some(0),
        edge_types: Some(vec!["Advances".to_string()]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    let resp = result.unwrap();

    let returned_ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(
        returned_ids.contains(&entry_0),
        "entry_0 (0 edges) must be returned for max_edge_count=0"
    );
    assert!(
        !returned_ids.contains(&entry_1),
        "entry_1 (1 edge) must NOT be returned for max_edge_count=0"
    );
    assert!(
        !returned_ids.contains(&entry_2),
        "entry_2 (2 edges) must NOT be returned for max_edge_count=0"
    );
    assert!(
        !returned_ids.contains(&entry_3),
        "entry_3 (3 edges) must NOT be returned for max_edge_count=0"
    );
    assert_eq!(resp.total_returned, 1, "total_returned must be 1");
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// max_edge_count=1 — general <= N path
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_filter_max_edge_count_one_returns_zero_and_one_edge_entries() {
    // max_edge_count=1 → entries with 0 or 1 edge; 2 and 3 excluded.
    let (store_impl, _dir) = open_test_store().await;

    let entry_0 = store_entry(&store_impl, "goal", "Zero edges").await;
    let entry_1 = store_entry(&store_impl, "goal", "One edge").await;
    let entry_2 = store_entry(&store_impl, "goal", "Two edges").await;
    let entry_3 = store_entry(&store_impl, "goal", "Three edges").await;

    let t1 = store_entry(&store_impl, "other", "T1").await;
    let t2 = store_entry(&store_impl, "other", "T2").await;
    let t3 = store_entry(&store_impl, "other", "T3").await;
    let t4 = store_entry(&store_impl, "other", "T4").await;
    let t5 = store_entry(&store_impl, "other", "T5").await;
    let t6 = store_entry(&store_impl, "other", "T6").await;

    add_edge(&store_impl, entry_1, t1, "Advances").await;
    add_edge(&store_impl, entry_2, t2, "Advances").await;
    add_edge(&store_impl, entry_2, t3, "Advances").await;
    add_edge(&store_impl, entry_3, t4, "Advances").await;
    add_edge(&store_impl, entry_3, t5, "Advances").await;
    add_edge(&store_impl, entry_3, t6, "Advances").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        max_edge_count: Some(1),
        edge_types: Some(vec!["Advances".to_string()]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    let resp = result.unwrap();

    let returned_ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(
        returned_ids.contains(&entry_0),
        "0-edge entry must be included"
    );
    assert!(
        returned_ids.contains(&entry_1),
        "1-edge entry must be included"
    );
    assert!(
        !returned_ids.contains(&entry_2),
        "2-edge entry must be excluded"
    );
    assert!(
        !returned_ids.contains(&entry_3),
        "3-edge entry must be excluded"
    );
    assert_eq!(resp.total_returned, 2);
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// AC-30 — min_edge_count >= 2
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_filter_min_edge_count_gte2_returns_two_and_three_edge_entries() {
    // AC-30: min_edge_count=2 returns only entries with 2+ outgoing Advances edges.
    let (store_impl, _dir) = open_test_store().await;

    let entry_0 = store_entry(&store_impl, "decision", "Zero edges").await;
    let entry_1 = store_entry(&store_impl, "decision", "One edge").await;
    let entry_2 = store_entry(&store_impl, "decision", "Two edges").await;
    let entry_3 = store_entry(&store_impl, "decision", "Three edges").await;

    let t1 = store_entry(&store_impl, "other", "T1").await;
    let t2 = store_entry(&store_impl, "other", "T2").await;
    let t3 = store_entry(&store_impl, "other", "T3").await;
    let t4 = store_entry(&store_impl, "other", "T4").await;
    let t5 = store_entry(&store_impl, "other", "T5").await;
    let t6 = store_entry(&store_impl, "other", "T6").await;

    add_edge(&store_impl, entry_1, t1, "Advances").await;
    add_edge(&store_impl, entry_2, t2, "Advances").await;
    add_edge(&store_impl, entry_2, t3, "Advances").await;
    add_edge(&store_impl, entry_3, t4, "Advances").await;
    add_edge(&store_impl, entry_3, t5, "Advances").await;
    add_edge(&store_impl, entry_3, t6, "Advances").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("decision".to_string()),
        min_edge_count: Some(2),
        edge_types: Some(vec!["Advances".to_string()]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    let resp = result.unwrap();

    let returned_ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(
        !returned_ids.contains(&entry_0),
        "0-edge entry must be excluded"
    );
    assert!(
        !returned_ids.contains(&entry_1),
        "1-edge entry must be excluded"
    );
    assert!(
        returned_ids.contains(&entry_2),
        "2-edge entry must be included"
    );
    assert!(
        returned_ids.contains(&entry_3),
        "3-edge entry must be included"
    );
    assert_eq!(resp.total_returned, 2);
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// R-08 — both min and max edge_count produce two independent subqueries
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_filter_both_edge_count_bounds_two_subqueries_in_sql() {
    // R-08: min=2, max=3 → entries with exactly 2 or 3 outgoing edges.
    // 5-entry fixture: 0,1,2,3,4 outgoing edges. Only 2 and 3 should return.
    let (store_impl, _dir) = open_test_store().await;

    let entry_0 = store_entry(&store_impl, "goal", "Zero edges").await;
    let entry_1 = store_entry(&store_impl, "goal", "One edge").await;
    let entry_2 = store_entry(&store_impl, "goal", "Two edges").await;
    let entry_3 = store_entry(&store_impl, "goal", "Three edges").await;
    let entry_4 = store_entry(&store_impl, "goal", "Four edges").await;

    let mut t: Vec<u64> = Vec::with_capacity(10);
    for i in 0..10 {
        t.push(store_entry(&store_impl, "other", &format!("T{i}")).await);
    }

    add_edge(&store_impl, entry_1, t[0], "Advances").await;
    add_edge(&store_impl, entry_2, t[1], "Advances").await;
    add_edge(&store_impl, entry_2, t[2], "Advances").await;
    add_edge(&store_impl, entry_3, t[3], "Advances").await;
    add_edge(&store_impl, entry_3, t[4], "Advances").await;
    add_edge(&store_impl, entry_3, t[5], "Advances").await;
    add_edge(&store_impl, entry_4, t[6], "Advances").await;
    add_edge(&store_impl, entry_4, t[7], "Advances").await;
    add_edge(&store_impl, entry_4, t[8], "Advances").await;
    add_edge(&store_impl, entry_4, t[9], "Advances").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_edge_count: Some(2),
        max_edge_count: Some(3),
        edge_types: Some(vec!["Advances".to_string()]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    let resp = result.unwrap();

    let returned_ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(!returned_ids.contains(&entry_0), "0-edge must be excluded");
    assert!(!returned_ids.contains(&entry_1), "1-edge must be excluded");
    assert!(returned_ids.contains(&entry_2), "2-edge must be included");
    assert!(returned_ids.contains(&entry_3), "3-edge must be included");
    assert!(!returned_ids.contains(&entry_4), "4-edge must be excluded");
    assert_eq!(resp.total_returned, 2, "only entries with 2 or 3 edges");
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_filter_min_edge_count_only_one_gte_subquery() {
    // R-08: min_edge_count only — entries with 0 edge excluded; 2-edge included.
    let (store_impl, _dir) = open_test_store().await;

    let entry_0 = store_entry(&store_impl, "goal", "Zero edges").await;
    let entry_2 = store_entry(&store_impl, "goal", "Two edges").await;

    let t1 = store_entry(&store_impl, "other", "T1").await;
    let t2 = store_entry(&store_impl, "other", "T2").await;
    add_edge(&store_impl, entry_2, t1, "Advances").await;
    add_edge(&store_impl, entry_2, t2, "Advances").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_edge_count: Some(2),
        max_edge_count: None,
        edge_types: Some(vec!["Advances".to_string()]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok());
    let resp = result.unwrap();
    let returned_ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(!returned_ids.contains(&entry_0), "0-edge must be excluded");
    assert!(returned_ids.contains(&entry_2), "2-edge must be included");
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_filter_max_edge_count_only_one_lte_subquery() {
    // R-08: max_edge_count only — entry with 0 edges included; 2-edge excluded.
    let (store_impl, _dir) = open_test_store().await;

    let entry_0 = store_entry(&store_impl, "goal", "Zero edges").await;
    let entry_2 = store_entry(&store_impl, "goal", "Two edges").await;

    let t1 = store_entry(&store_impl, "other", "T1").await;
    let t2 = store_entry(&store_impl, "other", "T2").await;
    add_edge(&store_impl, entry_2, t1, "Advances").await;
    add_edge(&store_impl, entry_2, t2, "Advances").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_edge_count: None,
        max_edge_count: Some(0),
        edge_types: Some(vec!["Advances".to_string()]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok());
    let resp = result.unwrap();
    let returned_ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(returned_ids.contains(&entry_0), "0-edge must be included");
    assert!(!returned_ids.contains(&entry_2), "2-edge must be excluded");
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// IR-04 — multi-type edge_types IN clause uses push_bind
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_filter_multi_type_edge_types_push_bind_pattern() {
    // IR-04: Entries with edges of types NOT in edge_types count as 0 matching edges.
    // entry has 2 "RelatedTo" edges (not in filter set) → COUNT(*) = 0 for
    // edge_types=["Advances","Supports"], so it appears for max_edge_count=0.
    let (store_impl, _dir) = open_test_store().await;

    let entry = store_entry(&store_impl, "goal", "RelatedTo edges only").await;
    let t1 = store_entry(&store_impl, "other", "T1").await;
    let t2 = store_entry(&store_impl, "other", "T2").await;
    add_edge(&store_impl, entry, t1, "RelatedTo").await;
    add_edge(&store_impl, entry, t2, "RelatedTo").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        max_edge_count: Some(0),
        edge_types: Some(vec!["Advances".to_string(), "Supports".to_string()]),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    let resp = result.unwrap();
    let returned_ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    // RelatedTo edges are not Advances or Supports: COUNT(*) = 0 for our types.
    assert!(
        returned_ids.contains(&entry),
        "entry with only RelatedTo edges must appear for max_edge_count=0 \
         filtering on Advances/Supports — got: {:?}",
        returned_ids
    );
    store_impl.close().await.unwrap();
}

// -----------------------------------------------------------------------
// Unrecognized edge type
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
// Edge cases
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_filter_inverted_confidence_bounds_returns_empty() {
    // min_confidence > max_confidence: not a validation error; returns empty set.
    let (store_impl, _dir) = open_test_store().await;
    store_entry(&store_impl, "goal", "Goal A").await;

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        min_confidence: Some(0.9),
        max_confidence: Some(0.1),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok(), "inverted confidence bounds must not error");
    let resp = result.unwrap();
    assert_eq!(resp.entries.len(), 0, "inverted bounds must return empty");
    assert_eq!(resp.total_returned, 0);
    store_impl.close().await.unwrap();
}

#[tokio::test]
async fn test_filter_deprecated_entries_excluded() {
    // R-10: deprecated entries must not appear (status != 0 filter).
    let (store_impl, _dir) = open_test_store().await;
    let active_id = store_entry(&store_impl, "goal", "Active Goal").await;
    let deprecated_id = store_entry(&store_impl, "goal", "Deprecated Goal").await;

    // Deprecate the second entry directly via the write pool.
    sqlx::query("UPDATE entries SET status = 2 WHERE id = ?")
        .bind(deprecated_id as i64)
        .execute(store_impl.write_pool_test())
        .await
        .expect("deprecate entry");

    let params = GraphParams {
        mode: "filter".to_string(),
        category: Some("goal".to_string()),
        ..Default::default()
    };
    let result = handle_filter(&store_impl, &params).await;
    assert!(result.is_ok());
    let resp = result.unwrap();
    let returned_ids: Vec<u64> = resp.entries.iter().map(|e| e.id).collect();
    assert!(
        returned_ids.contains(&active_id),
        "active entry must appear"
    );
    assert!(
        !returned_ids.contains(&deprecated_id),
        "deprecated entry must not appear"
    );
    store_impl.close().await.unwrap();
}
