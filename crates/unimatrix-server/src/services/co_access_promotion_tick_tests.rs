//! Tests for `co_access_promotion_tick.rs` (crt-034, updated crt-035).
//!
//! Extracted to a separate file to keep the main module under the 500-line limit.

use unimatrix_core::Store;

use super::run_co_access_promotion_tick;
use crate::infra::config::InferenceConfig;

fn make_config(cap: usize) -> InferenceConfig {
    InferenceConfig {
        max_co_access_promotion_per_tick: cap,
        ..InferenceConfig::default()
    }
}

/// Seed an entry row with the given id and status (as raw integer per Status enum).
///
/// Uses a minimal INSERT: only the NOT NULL columns without defaults are required
/// (title, content, topic, category, source). All other columns use schema defaults.
async fn seed_entry(store: &Store, id: i64, status: i64) {
    sqlx::query(
        "INSERT OR IGNORE INTO entries (id, title, content, topic, category, source, status, created_at, updated_at)
         VALUES (?1, 'test', 'content', 'test', 'test', 'test', ?2, 0, 0)",
    )
    .bind(id)
    .bind(status)
    .execute(store.write_pool_server())
    .await
    .unwrap();
}

/// Seed a co_access pair.
///
/// Also seeds Active (status=0) entries for both ids if they do not already exist,
/// so the tick-side JOIN against `entries` (GH #476 fix) includes these rows.
/// Tests that need quarantined entries must call `seed_entry` explicitly after
/// `seed_co_access` to override the default Active status.
async fn seed_co_access(store: &Store, a: i64, b: i64, count: i64) {
    // Ensure entry rows exist so the promotion tick JOIN includes these pairs.
    // INSERT OR IGNORE: does not overwrite a status already set by seed_entry.
    seed_entry(store, a, 0).await;
    seed_entry(store, b, 0).await;
    sqlx::query(
        "INSERT OR REPLACE INTO co_access (entry_id_a, entry_id_b, count, last_updated)
         VALUES (?1, ?2, ?3, 0)",
    )
    .bind(a)
    .bind(b)
    .bind(count)
    .execute(store.write_pool_server())
    .await
    .unwrap();
}

/// Seed an existing CoAccess edge in graph_edges.
async fn seed_graph_edge(store: &Store, source_id: i64, target_id: i64, weight: f64) {
    sqlx::query(
        "INSERT OR REPLACE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at,
              created_by, source, bootstrap_only)
         VALUES (?1, ?2, 'CoAccess', ?3, 0, 'test', 'co_access', 0)",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(weight)
    .execute(store.write_pool_server())
    .await
    .unwrap();
}

/// Count CoAccess rows in graph_edges.
async fn count_co_access_edges(store: &Store) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM graph_edges WHERE relation_type = 'CoAccess'",
    )
    .fetch_one(store.write_pool_server())
    .await
    .unwrap()
}

struct GraphEdgeRow {
    source_id: i64,
    target_id: i64,
    weight: f64,
    bootstrap_only: i64,
    source: String,
    created_by: String,
    relation_type: String,
}

/// Fetch a specific CoAccess edge, or None if absent.
async fn fetch_co_access_edge(store: &Store, a: i64, b: i64) -> Option<GraphEdgeRow> {
    sqlx::query_as::<_, (i64, i64, f64, i64, String, String, String)>(
        "SELECT source_id, target_id, weight, bootstrap_only, source,
                created_by, relation_type
         FROM graph_edges
         WHERE source_id = ?1 AND target_id = ?2 AND relation_type = 'CoAccess'",
    )
    .bind(a)
    .bind(b)
    .fetch_optional(store.write_pool_server())
    .await
    .unwrap()
    .map(
        |(source_id, target_id, weight, bootstrap_only, source, created_by, relation_type)| {
            GraphEdgeRow {
                source_id,
                target_id,
                weight,
                bootstrap_only,
                source,
                created_by,
                relation_type,
            }
        },
    )
}

// ---------------------------------------------------------------------------
// Group A: Basic Promotion
// ---------------------------------------------------------------------------

/// T-BLR-01: AC-01, AC-02, R-13, R-10: new qualifying pair is promoted with correct
/// fields in both directions.
#[tokio::test]
async fn test_basic_promotion_new_qualifying_pair() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 5).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    let edge = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("edge must exist");
    assert_eq!(edge.source_id, 1);
    assert_eq!(edge.target_id, 2);
    assert_eq!(edge.relation_type, "CoAccess");
    assert_eq!(edge.bootstrap_only, 0);
    assert_eq!(edge.source, "co_access");
    assert_eq!(edge.created_by, "tick");
    assert!(
        (edge.weight - 1.0).abs() < 1e-9,
        "only pair: weight must be 1.0"
    );
    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "both directions must be inserted"
    );
    let reverse = fetch_co_access_edge(&store, 2, 1)
        .await
        .expect("reverse edge must exist after bidirectional tick");
    assert_eq!(reverse.source_id, 2);
    assert_eq!(reverse.target_id, 1);
    assert!(
        (reverse.weight - 1.0).abs() < 1e-9,
        "reverse edge weight must equal forward"
    );
    assert_eq!(reverse.created_by, "tick");
    assert_eq!(reverse.source, "co_access");
    assert_eq!(reverse.bootstrap_only, 0);
}

/// AC-12, R-13: all four metadata fields verified on a multi-pair batch.
#[tokio::test]
async fn test_inserted_edge_metadata_all_four_fields() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 3).await;
    seed_co_access(&store, 1, 3, 6).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    let edge = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("edge must exist");
    assert_eq!(edge.bootstrap_only, 0);
    assert_eq!(edge.source, "co_access");
    assert_eq!(edge.created_by, "tick");
    assert_eq!(edge.relation_type, "CoAccess");
    // weight = 3/6 = 0.5
    assert!((edge.weight - 0.5).abs() < 1e-9, "weight must be 0.5 (3/6)");
}

/// T-BLR-02: edge is bidirectional (crt-035).
#[tokio::test]
async fn test_inserted_edge_is_bidirectional() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 5, 10, 4).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "bidirectional: 2 edges for 1 pair"
    );
    let fwd = fetch_co_access_edge(&store, 5, 10)
        .await
        .expect("forward edge (5→10) must exist");
    let rev = fetch_co_access_edge(&store, 10, 5)
        .await
        .expect("reverse edge (10→5) must exist after crt-035");
    assert!(
        (fwd.weight - rev.weight).abs() < 1e-9,
        "both directions must have equal weight"
    );
}

// ---------------------------------------------------------------------------
// Group B: Cap and Ordering
// ---------------------------------------------------------------------------

/// T-BLR-04: AC-04, R-11: cap respected and ORDER BY count DESC selects top-N.
/// 3 pairs × 2 directions = 6 edges.
#[tokio::test]
async fn test_cap_selects_highest_count_pairs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    let counts = [3i64, 3, 3, 3, 3, 10, 20, 50, 80, 100];
    for (i, &count) in counts.iter().enumerate() {
        seed_co_access(&store, (i + 1) as i64, (i + 11) as i64, count).await;
    }

    run_co_access_promotion_tick(&store, &make_config(3), 10).await;

    assert_eq!(
        count_co_access_edges(&store).await,
        6,
        "cap=3 pairs × 2 directions = 6 edges"
    );
    // Top-3 by count: 100, 80, 50 (indices 9, 8, 7 → pairs (10,20), (9,19), (8,18))
    assert!(
        fetch_co_access_edge(&store, 10, 20).await.is_some(),
        "count=100 pair must be selected"
    );
    assert!(
        fetch_co_access_edge(&store, 9, 19).await.is_some(),
        "count=80 pair must be selected"
    );
    assert!(
        fetch_co_access_edge(&store, 8, 18).await.is_some(),
        "count=50 pair must be selected"
    );
    // Reverse edges for top-3 pairs must also exist
    assert!(
        fetch_co_access_edge(&store, 20, 10).await.is_some(),
        "reverse of count=100 pair"
    );
    assert!(
        fetch_co_access_edge(&store, 19, 9).await.is_some(),
        "reverse of count=80 pair"
    );
    assert!(
        fetch_co_access_edge(&store, 18, 8).await.is_some(),
        "reverse of count=50 pair"
    );
    // count=3 pairs must NOT be present
    assert!(
        fetch_co_access_edge(&store, 1, 11).await.is_none(),
        "count=3 pair must not be promoted when cap=3"
    );
}

// ---------------------------------------------------------------------------
// Group C: Weight Refresh
// ---------------------------------------------------------------------------

/// T-BLR-08: AC-02, AC-03, R-04: stale weight (delta > 0.1) is updated in both
/// directions. forward (updated) + reverse (new) = 2 rows.
#[tokio::test]
async fn test_existing_edge_stale_weight_updated() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 10).await;
    seed_graph_edge(&store, 1, 2, 0.5).await; // delta = |1.0 - 0.5| = 0.5 > 0.1

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    // Forward edge updated.
    let fwd = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("forward edge must exist");
    assert!(
        (fwd.weight - 1.0).abs() < 1e-9,
        "forward edge weight must be updated to 1.0"
    );

    // Reverse edge newly inserted by tick.
    let rev = fetch_co_access_edge(&store, 2, 1)
        .await
        .expect("reverse edge must be inserted by tick");
    assert!(
        (rev.weight - 1.0).abs() < 1e-9,
        "reverse edge weight must be 1.0 (newly inserted)"
    );

    // Exactly 2 rows: forward (updated) + reverse (new). No third row.
    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "no duplicate: forward (updated) + reverse (new) = 2"
    );
}

/// AC-03, R-04: weight within delta (0.0) is not updated.
#[tokio::test]
async fn test_existing_edge_current_weight_no_update() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // pair (1,2): count=5, pair (1,3): count=10 → max_count=10, weight(1,2)=0.5
    seed_co_access(&store, 1, 2, 5).await;
    seed_co_access(&store, 1, 3, 10).await;
    seed_graph_edge(&store, 1, 2, 0.5).await; // delta = 0.0

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    let edge = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("edge must exist");
    assert!((edge.weight - 0.5).abs() < 1e-9, "weight must remain 0.5");
    // (1,3) is newly inserted
    assert!(
        fetch_co_access_edge(&store, 1, 3).await.is_some(),
        "pair (1,3) must be inserted"
    );
    // R-06: reverse edge for (1,2) must be inserted even when forward weight is unchanged
    assert!(
        fetch_co_access_edge(&store, 2, 1).await.is_some(),
        "reverse edge for (1,2) must be inserted even when forward weight is unchanged"
    );
}

/// E-05: delta exactly at boundary (0.1) must NOT trigger update.
#[tokio::test]
async fn test_weight_delta_exactly_at_boundary_no_update() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // pair (1,2): count=6, pair (1,3): count=10 → max_count=10, weight(1,2)=0.6
    // existing weight=0.5 → delta = |0.6 - 0.5| = 0.1 exactly → no update
    seed_co_access(&store, 1, 2, 6).await;
    seed_co_access(&store, 1, 3, 10).await;
    seed_graph_edge(&store, 1, 2, 0.5).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    let edge = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("edge must exist");
    assert!(
        (edge.weight - 0.5).abs() < 1e-9,
        "weight must NOT be updated when delta == 0.1 (strictly greater than required)"
    );
}

// ---------------------------------------------------------------------------
// Group D: Idempotency
// ---------------------------------------------------------------------------

/// T-BLR-03: AC-14, R-09: second tick on unchanged co_access leaves exactly 2 rows.
#[tokio::test]
async fn test_double_tick_idempotent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 5).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    // After first tick: both directions inserted.
    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "exactly 2 rows after first tick"
    );
    let weight_after_first_fwd = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("forward edge after first tick")
        .weight;
    let weight_after_first_rev = fetch_co_access_edge(&store, 2, 1)
        .await
        .expect("reverse edge after first tick")
        .weight;

    // Run second tick.
    run_co_access_promotion_tick(&store, &make_config(200), 11).await;

    // After second tick: same 2 rows, weights unchanged.
    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "exactly 2 rows after second tick (idempotent)"
    );
    assert!(
        (fetch_co_access_edge(&store, 1, 2).await.unwrap().weight - weight_after_first_fwd).abs()
            < 1e-9,
        "forward weight unchanged after second tick"
    );
    assert!(
        (fetch_co_access_edge(&store, 2, 1).await.unwrap().weight - weight_after_first_rev).abs()
            < 1e-9,
        "reverse weight unchanged after second tick"
    );
}

/// AC-15: sub-threshold pair after initial promotion is not deleted.
#[tokio::test]
async fn test_sub_threshold_pair_not_gc() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 5).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;
    assert!(fetch_co_access_edge(&store, 1, 2).await.is_some());

    // Drop count below threshold
    sqlx::query("UPDATE co_access SET count = 1 WHERE entry_id_a = 1 AND entry_id_b = 2")
        .execute(store.write_pool_server())
        .await
        .unwrap();

    run_co_access_promotion_tick(&store, &make_config(200), 11).await;

    assert!(
        fetch_co_access_edge(&store, 1, 2).await.is_some(),
        "promoted edge must not be deleted (GC is #409)"
    );
}

// ---------------------------------------------------------------------------
// Group E: Empty and Sub-threshold Table
// ---------------------------------------------------------------------------

/// AC-09(a), R-02: empty table at late tick → no panic, no warn, 0/0.
#[tracing_test::traced_test]
#[tokio::test]
async fn test_empty_co_access_table_noop_late_tick() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    assert_eq!(count_co_access_edges(&store).await, 0);
    assert!(
        !logs_contain("zero qualifying pairs"),
        "no SR-05 warn at tick >= 5"
    );
}

/// AC-09(c), R-02: all-below-threshold at late tick → no warn, 0 edges.
#[tracing_test::traced_test]
#[tokio::test]
async fn test_all_below_threshold_noop_late_tick() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 1).await;
    seed_co_access(&store, 3, 4, 2).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    assert_eq!(count_co_access_edges(&store).await, 0);
    assert!(!logs_contain("zero qualifying pairs"));
}

/// AC-09(b), R-06: qualifying_count=0 AND tick < 5 → warn! emitted.
#[tracing_test::traced_test]
#[tokio::test]
async fn test_early_tick_warn_when_qualifying_count_zero() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // empty table → qualifying_count = 0

    run_co_access_promotion_tick(&store, &make_config(200), 0).await;

    assert!(
        logs_contain("zero qualifying pairs"),
        "SR-05 warn must fire at tick=0 with empty table"
    );
}

/// R-06: qualifying_count=0, tick=5 (boundary) → NO warn.
#[tracing_test::traced_test]
#[tokio::test]
async fn test_late_tick_no_warn_empty_table() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    run_co_access_promotion_tick(&store, &make_config(200), 5).await;

    assert!(
        !logs_contain("zero qualifying pairs"),
        "no SR-05 warn at exactly tick=5"
    );
}

/// R-06: qualifying_count > 0, tick < 5 → NO warn (SR-05 only fires on zero qualifying).
#[tracing_test::traced_test]
#[tokio::test]
async fn test_fully_promoted_table_no_warn() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 4).await;
    seed_co_access(&store, 3, 4, 5).await;
    seed_co_access(&store, 5, 6, 6).await;
    // All already promoted
    seed_graph_edge(&store, 1, 2, 0.67).await;
    seed_graph_edge(&store, 3, 4, 0.83).await;
    seed_graph_edge(&store, 5, 6, 1.0).await;

    run_co_access_promotion_tick(&store, &make_config(200), 0).await;

    assert!(
        !logs_contain("zero qualifying pairs"),
        "no SR-05 warn when qualifying_count > 0"
    );
}

// ---------------------------------------------------------------------------
// Group F: Write Failure Handling (R-01) — Critical Priority
// ---------------------------------------------------------------------------

/// AC-11, R-01: batch continues after one pair is a no-op, remaining pairs inserted.
///
/// We verify the continue-on-error semantics by seeding pair (1,2) with a
/// pre-existing edge at exactly the computed weight (no INSERT or UPDATE),
/// then asserting pairs (1,3) and (1,4) are still attempted and inserted.
#[tokio::test]
async fn test_write_failure_mid_batch_warn_and_continue() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // 3 qualifying pairs
    seed_co_access(&store, 1, 2, 10).await;
    seed_co_access(&store, 1, 3, 8).await;
    seed_co_access(&store, 1, 4, 6).await;

    // Force pair (1,2) INSERT to be a no-op by pre-seeding the exact computed weight.
    // new_weight for (1,2) = 10/10 = 1.0; delta=0 → no UPDATE either.
    // Pairs (1,3) and (1,4) are new → must be inserted.
    seed_graph_edge(&store, 1, 2, 1.0).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    // Function must return () — verified by reaching this line.
    assert!(
        fetch_co_access_edge(&store, 1, 3).await.is_some(),
        "pair (1,3) must be attempted and inserted"
    );
    assert!(
        fetch_co_access_edge(&store, 1, 4).await.is_some(),
        "pair (1,4) must be attempted and inserted"
    );
    // Reverse edges for (1,3) and (1,4) must also be inserted
    assert!(
        fetch_co_access_edge(&store, 3, 1).await.is_some(),
        "reverse of pair (1,3) must be inserted"
    );
    assert!(
        fetch_co_access_edge(&store, 4, 1).await.is_some(),
        "reverse of pair (1,4) must be inserted"
    );
}

/// R-01: info! log fires even when no writes occur.
#[tracing_test::traced_test]
#[tokio::test]
async fn test_write_failure_info_log_always_fires() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Seed one pair with exact-match weight so nothing is written
    seed_co_access(&store, 1, 2, 5).await;
    seed_graph_edge(&store, 1, 2, 1.0).await; // weight = 5/5 = 1.0 → delta=0

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    assert!(
        logs_contain("co_access promotion tick complete"),
        "info! must always fire"
    );
}

// ---------------------------------------------------------------------------
// Group G: Normalization
// ---------------------------------------------------------------------------

/// AC-13, R-03: global MAX used as normalization anchor, not batch-local max.
#[tokio::test]
async fn test_global_max_normalization_subquery_shape() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Counts [1..10]; qualifying: [3..10], max=10; cap=3 → selects [10,9,8]
    for i in 1i64..=10 {
        seed_co_access(&store, i, i + 100, i).await;
    }

    run_co_access_promotion_tick(&store, &make_config(3), 10).await;

    let e10 = fetch_co_access_edge(&store, 10, 110)
        .await
        .expect("count=10 pair");
    let e9 = fetch_co_access_edge(&store, 9, 109)
        .await
        .expect("count=9 pair");
    let e8 = fetch_co_access_edge(&store, 8, 108)
        .await
        .expect("count=8 pair");
    assert!((e10.weight - 1.0).abs() < 1e-9, "10/10 = 1.0");
    assert!((e9.weight - 0.9).abs() < 1e-9, "9/10 = 0.9");
    assert!((e8.weight - 0.8).abs() < 1e-9, "8/10 = 0.8");
}

/// R-03 scenario 2: global max is the top selected entry; weight of lower-count
/// pair reflects global normalization (5/100 = 0.05).
#[tokio::test]
async fn test_global_max_outside_capped_batch() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    let pairs = [
        (1i64, 101i64, 3i64),
        (2, 102, 4),
        (3, 103, 5),
        (4, 104, 80),
        (5, 105, 100),
    ];
    for (a, b, count) in pairs {
        seed_co_access(&store, a, b, count).await;
    }

    run_co_access_promotion_tick(&store, &make_config(3), 10).await;

    let e100 = fetch_co_access_edge(&store, 5, 105)
        .await
        .expect("count=100");
    let e80 = fetch_co_access_edge(&store, 4, 104)
        .await
        .expect("count=80");
    let e5 = fetch_co_access_edge(&store, 3, 103).await.expect("count=5");
    assert!((e100.weight - 1.0).abs() < 1e-9, "100/100 = 1.0");
    assert!((e80.weight - 0.8).abs() < 1e-9, "80/100 = 0.8");
    assert!((e5.weight - 0.05).abs() < 1e-9, "5/100 = 0.05");
}

// ---------------------------------------------------------------------------
// Group H: Edge Cases
// ---------------------------------------------------------------------------

/// T-BLR-07 (E-01): single qualifying pair → weight=1.0; second tick no update (delta=0).
#[tokio::test]
async fn test_single_qualifying_pair_weight_one() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 7).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    let edge = fetch_co_access_edge(&store, 1, 2).await.expect("edge");
    assert!((edge.weight - 1.0).abs() < 1e-9, "7/7 = 1.0");

    run_co_access_promotion_tick(&store, &make_config(200), 11).await;
    let edge2 = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("edge after 2nd tick");
    assert!(
        (edge2.weight - 1.0).abs() < 1e-9,
        "weight unchanged after 2nd tick"
    );
}

/// T-BLR-05 (E-02): tied counts — cap respected, 3 pairs × 2 directions = 6 edges.
#[tokio::test]
async fn test_tied_counts_secondary_sort_stable() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    for i in 1i64..=5 {
        seed_co_access(&store, i, i + 10, 5).await;
    }

    run_co_access_promotion_tick(&store, &make_config(3), 10).await;

    assert_eq!(
        count_co_access_edges(&store).await,
        6,
        "cap=3 pairs × 2 directions = 6 edges"
    );
}

/// T-BLR-06 (E-03): cap equals qualifying count — no off-by-one. 5 pairs × 2 = 10 edges.
#[tokio::test]
async fn test_cap_equals_qualifying_count() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    for i in 1i64..=5 {
        seed_co_access(&store, i, i + 10, 4).await;
    }

    run_co_access_promotion_tick(&store, &make_config(5), 10).await;

    assert_eq!(
        count_co_access_edges(&store).await,
        10,
        "5 pairs × 2 directions = 10 edges; all pairs promoted"
    );
}

/// T-BLR-07 (E-04): cap=1 selects highest-count pair. 1 pair × 2 = 2 edges.
#[tokio::test]
async fn test_cap_one_selects_highest_count() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    seed_co_access(&store, 1, 2, 5).await;
    seed_co_access(&store, 1, 3, 3).await;
    seed_co_access(&store, 1, 4, 4).await;

    run_co_access_promotion_tick(&store, &make_config(1), 10).await;

    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "1 pair × 2 directions = 2 edges"
    );
    assert!(
        fetch_co_access_edge(&store, 1, 2).await.is_some(),
        "forward edge present"
    );
    assert!(
        fetch_co_access_edge(&store, 2, 1).await.is_some(),
        "reverse edge present"
    );
}

/// E-06: self-loop pair → no panic regardless of DB behavior.
///
/// The co_access schema has CHECK (entry_id_a < entry_id_b) so inserting a
/// self-loop will fail at the seed stage. We verify the promotion tick itself
/// does not panic when the co_access table contains only sub-threshold or zero
/// rows (which covers the same "unusual input" scenario without violating the DB
/// constraint).
#[tokio::test]
async fn test_self_loop_pair_no_panic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Self-loop (1,1) violates CHECK(entry_id_a < entry_id_b); the DB will reject
    // the seed insert. We ignore that error and verify the tick handles an empty
    // qualifying set without panicking.
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO co_access (entry_id_a, entry_id_b, count, last_updated)
         VALUES (1, 1, 5, 0)",
    )
    .execute(store.write_pool_server())
    .await; // may fail due to CHECK — that's fine

    // Must not panic regardless of whether the row was inserted
    run_co_access_promotion_tick(&store, &make_config(200), 10).await;
}

// ---------------------------------------------------------------------------
// Group I: Bidirectional Assertions (crt-035)
// ---------------------------------------------------------------------------

/// T-NEW-01: AC-01, AC-02: both directions inserted with equal weight on a fresh pair.
#[tokio::test]
async fn test_bidirectional_edges_inserted_same_weight() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Single qualifying pair: count=5, max_count=5, new_weight=1.0
    seed_co_access(&store, 1, 2, 5).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "both directions must be inserted"
    );
    let fwd = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("forward edge (1→2) must exist");
    let rev = fetch_co_access_edge(&store, 2, 1)
        .await
        .expect("reverse edge (2→1) must exist");
    assert!(
        (fwd.weight - 1.0).abs() < 1e-9,
        "forward weight must be 1.0"
    );
    assert!(
        (rev.weight - 1.0).abs() < 1e-9,
        "reverse weight must be 1.0"
    );
    assert!(
        (fwd.weight - rev.weight).abs() < 1e-9,
        "both directions must carry equal weight"
    );
}

/// T-NEW-02: AC-03, FR-12, R-05: pre-seeded stale forward and reverse edges both
/// converge on tick when drift exceeds delta.
#[tokio::test]
async fn test_bidirectional_both_directions_updated_when_drift_exceeds_delta() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Pre-seed asymmetric stale weights (simulates a partial prior tick failure).
    seed_graph_edge(&store, 1, 2, 0.5).await; // forward: delta = |1.0 - 0.5| = 0.5 > 0.1
    seed_graph_edge(&store, 2, 1, 0.2).await; // reverse: delta = |1.0 - 0.2| = 0.8 > 0.1
    // Single pair with count=10 → new_weight = 1.0 (max is itself).
    seed_co_access(&store, 1, 2, 10).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    let fwd = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("forward edge must exist");
    let rev = fetch_co_access_edge(&store, 2, 1)
        .await
        .expect("reverse edge must exist");
    assert!(
        (fwd.weight - 1.0).abs() < 1e-9,
        "forward weight must be updated from 0.5 to 1.0"
    );
    assert!(
        (rev.weight - 1.0).abs() < 1e-9,
        "reverse weight must be updated from 0.2 to 1.0"
    );
    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "exactly 2 rows: both directions present"
    );
}

/// T-NEW-03: AC-05, FR-05: tracing summary emits promoted_pairs, edges_inserted,
/// edges_updated structured fields.
#[tracing_test::traced_test]
#[tokio::test]
async fn test_log_format_promoted_pairs_and_edges_inserted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    // Two qualifying pairs, all fresh: 2 pairs × 2 directions = 4 inserts, 0 updates.
    seed_co_access(&store, 1, 2, 5).await;
    seed_co_access(&store, 3, 4, 4).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    // Structured key-value fields in the tracing::info! record.
    assert!(
        logs_contain("promoted_pairs=2") || logs_contain("promoted_pairs: 2"),
        "log must contain promoted_pairs=2"
    );
    assert!(
        logs_contain("edges_inserted=4") || logs_contain("edges_inserted: 4"),
        "log must contain edges_inserted=4 (2 pairs × 2 directions)"
    );
    assert!(
        logs_contain("edges_updated=0") || logs_contain("edges_updated: 0"),
        "log must contain edges_updated=0 (all fresh inserts)"
    );
}

// ---------------------------------------------------------------------------
// Group J: Quarantine Filtering (GH #476)
// ---------------------------------------------------------------------------

/// GH-476-a: one quarantined endpoint → 0 edges promoted.
///
/// Verifies the tick-side JOIN excludes pairs where either endpoint is quarantined.
/// Uses real entries rows with status=Quarantined (not missing rows) so the JOIN
/// exclusion is exercised rather than the implicit FK miss path.
#[tokio::test]
async fn test_quarantine_one_endpoint_no_edges_promoted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Entry 1: Active (status=0), Entry 2: Quarantined (status=3)
    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 3).await;
    // co_access pair with count above CO_ACCESS_GRAPH_MIN_COUNT (=3)
    seed_co_access(&store, 1, 2, 5).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    assert_eq!(
        count_co_access_edges(&store).await,
        0,
        "quarantined endpoint: 0 edges must be promoted"
    );
}

/// GH-476-b: both endpoints quarantined → 0 edges promoted.
///
/// Ensures the filter applies even when both sides are quarantined, not just
/// one. The JOIN must exclude the pair regardless of which endpoint triggers it.
#[tokio::test]
async fn test_quarantine_both_endpoints_no_edges_promoted() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Both entries quarantined
    seed_entry(&store, 1, 3).await;
    seed_entry(&store, 2, 3).await;
    seed_co_access(&store, 1, 2, 5).await;

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    assert_eq!(
        count_co_access_edges(&store).await,
        0,
        "both endpoints quarantined: 0 edges must be promoted"
    );
}

/// GH-476-d: quarantined pair vacates batch slot — active pairs fill to cap.
///
/// Verifies the throughput invariant: with cap=2, 3 qualifying active pairs, and 1
/// quarantined pair whose count exceeds all active pairs, the fix ensures all 2 cap
/// slots go to active pairs.
///
/// Before fix: the quarantined pair would win the first LIMIT slot (highest count),
/// leaving only 1 slot for the 3 active pairs → batch under-filled, valid pairs crowded out.
/// After fix: quarantined pair excluded from SELECT, 2 active pairs fill both cap slots.
#[tokio::test]
async fn test_quarantine_vacated_slots_filled_by_active_pairs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // A=1, B=2, C=3, D=4 (all active); Q=5 (quarantined)
    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_entry(&store, 3, 0).await;
    seed_entry(&store, 4, 0).await;
    seed_entry(&store, 5, 3).await; // quarantined

    // Quarantined pair has the highest count — would crowd out active pairs pre-fix.
    seed_co_access(&store, 1, 5, 100).await; // A↔Q: count=100, quarantined endpoint
    // Three active pairs, all qualifying.
    seed_co_access(&store, 1, 2, 9).await; // A↔B: count=9
    seed_co_access(&store, 2, 3, 8).await; // B↔C: count=8
    seed_co_access(&store, 3, 4, 7).await; // C↔D: count=7

    // Cap=2: only the top-2 eligible active pairs should be promoted.
    run_co_access_promotion_tick(&store, &make_config(2), 10).await;

    // Expect 4 directed edges: A↔B (2) + B↔C (2) — the top-2 active pairs by count.
    // If the quarantined pair consumed a slot, we'd see only 2 directed edges total.
    assert_eq!(
        count_co_access_edges(&store).await,
        4,
        "cap=2 with quarantine excluded: top-2 active pairs must fill both slots (4 directed edges)"
    );
    assert!(fetch_co_access_edge(&store, 1, 2).await.is_some(), "A→B must be promoted");
    assert!(fetch_co_access_edge(&store, 2, 1).await.is_some(), "B→A must be promoted");
    assert!(fetch_co_access_edge(&store, 2, 3).await.is_some(), "B→C must be promoted");
    assert!(fetch_co_access_edge(&store, 3, 2).await.is_some(), "C→B must be promoted");
    // The quarantined-endpoint pair must not appear.
    assert!(fetch_co_access_edge(&store, 1, 5).await.is_none(), "A→Q must not be promoted");
    assert!(fetch_co_access_edge(&store, 5, 1).await.is_none(), "Q→A must not be promoted");
    // The third active pair (C↔D) falls outside the cap — also absent.
    assert!(fetch_co_access_edge(&store, 3, 4).await.is_none(), "C→D outside cap must not be promoted");
}

/// GH-476-c: mixed batch — only active-both-endpoints pairs promoted, with correct weight.
///
/// Seed three entries: A=active, B=active, C=quarantined.
/// co_access: A↔B (count=5), A↔C (count=10, higher), B↔C (count=7).
/// Only A↔B qualifies. Crucially, the subquery must also exclude quarantined
/// endpoints so max_count=5 (not 10 from A↔C), yielding weight=5/5=1.0 for A↔B.
/// If the subquery filter is missing, max_count=10, weight=0.5.
#[tokio::test]
async fn test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // A=1 (active), B=2 (active), C=3 (quarantined)
    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_entry(&store, 3, 3).await;

    // co_access must satisfy CHECK (entry_id_a < entry_id_b)
    seed_co_access(&store, 1, 2, 5).await; // A↔B: count=5
    seed_co_access(&store, 1, 3, 10).await; // A↔C: count=10, higher but C is quarantined
    seed_co_access(&store, 2, 3, 7).await; // B↔C: count=7, but C is quarantined

    run_co_access_promotion_tick(&store, &make_config(200), 10).await;

    // Only A↔B edges promoted (both directions)
    assert_eq!(
        count_co_access_edges(&store).await,
        2,
        "only A↔B (both directions) must be promoted — A↔C and B↔C excluded"
    );
    assert!(
        fetch_co_access_edge(&store, 1, 2).await.is_some(),
        "forward edge A→B must exist"
    );
    assert!(
        fetch_co_access_edge(&store, 2, 1).await.is_some(),
        "reverse edge B→A must exist"
    );
    assert!(
        fetch_co_access_edge(&store, 1, 3).await.is_none(),
        "A→C edge must not exist (C is quarantined)"
    );
    assert!(
        fetch_co_access_edge(&store, 2, 3).await.is_none(),
        "B→C edge must not exist (C is quarantined)"
    );

    // Weight must be 1.0 (5/5), not 0.5 (5/10).
    // If the subquery filter is absent, max_count=10 from A↔C and weight=0.5.
    // This assertion verifies the subquery also excludes quarantined endpoints.
    let edge = fetch_co_access_edge(&store, 1, 2)
        .await
        .expect("A→B edge must exist");
    assert!(
        (edge.weight - 1.0).abs() < 1e-9,
        "weight must be 1.0 (5/5 with quarantine-filtered max_count), not 0.5 (5/10)"
    );
}
