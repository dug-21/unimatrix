//! Store-tier tests for the cycle_tags write primitive and getter (vnc-047).
//!
//! C2 `insert_cycle_start_with_tags` (BEGIN IMMEDIATE txn: cycle_start event INSERT +
//! whole-set-once EXISTS guard) and C3 `get_cycle_tags` (ORDER BY tag).
//!
//! Covers (test-plan/store-write-primitive.md, store-read-getter.md):
//!   Atomicity (R-05): start row + tag rows share one commit; intra-set dup no abort.
//!   Whole-set-once (R-08): changed/subset/superset re-starts are wholesale no-ops;
//!     tagless start does not lock; exact stored-set equality.
//!   Concurrency (R-15): two same-FC starts → exactly one intact whole set, never merged.
//!   Value-opacity (R-11): empty rejected; colon-prefixed and bare stored identically;
//!     large/unicode/whitespace-only stored verbatim; SQL metacharacters stored verbatim
//!     (parameterized binds are the only SQLi defense).
//!   Getter: sorted, empty-when-none, scoped-to-feature_cycle, verbatim round-trip.
//!
//! NOTE (BEGIN IMMEDIATE, R-15): the write primitive opens its transaction with
//! `BEGIN IMMEDIATE` on a single dedicated connection (db.rs), NOT sqlx's default
//! DEFERRED `pool.begin()`. That is verified by source review; the functional guarantee
//! is proven by `test_concurrent_same_cycle_starts_one_whole_set` below.

#![cfg(feature = "test-support")]

use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

async fn open_store(dir: &TempDir) -> SqlxStore {
    let db_path = dir.path().join("unimatrix.db");
    SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store")
}

fn tags(vals: &[&str]) -> Vec<String> {
    vals.iter().map(|s| s.to_string()).collect()
}

/// Count cycle_start event rows for a cycle_id.
async fn count_cycle_start_rows(store: &SqlxStore, cycle_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM cycle_events WHERE cycle_id = ?1 AND event_type = 'cycle_start'",
    )
    .bind(cycle_id)
    .fetch_one(store.write_pool_test())
    .await
    .expect("count cycle_start rows")
}

// ===========================================================================
// C2 — atomicity (R-05 / AC-EXTRA-3)
// ===========================================================================

#[tokio::test]
async fn test_start_row_and_tag_rows_share_commit() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    store
        .insert_cycle_start_with_tags(
            "fc-1",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["A", "B"]),
        )
        .await
        .expect("insert");

    // Both the cycle_start row and both cycle_tags rows are visible after the one call.
    assert_eq!(count_cycle_start_rows(&store, "fc-1").await, 1);
    assert_eq!(
        store.get_cycle_tags("fc-1").await.unwrap(),
        tags(&["A", "B"])
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_dup_tag_in_set_no_txn_abort() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // ["A","A","B"] → ON CONFLICT DO NOTHING absorbs the intra-set dup; no abort.
    store
        .insert_cycle_start_with_tags(
            "fc-dup",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["A", "A", "B"]),
        )
        .await
        .expect("insert must not abort on intra-set dup");

    assert_eq!(
        store.get_cycle_tags("fc-dup").await.unwrap(),
        tags(&["A", "B"])
    );
    assert_eq!(count_cycle_start_rows(&store, "fc-dup").await, 1);

    store.close().await.unwrap();
}

// ===========================================================================
// C2 — whole-set-once exact equality (R-08 / AC-02a)
// ===========================================================================

#[tokio::test]
async fn test_first_call_inserts_full_set() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    store
        .insert_cycle_start_with_tags(
            "fc-full",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["A", "B", "C"]),
        )
        .await
        .expect("insert");

    assert_eq!(
        store.get_cycle_tags("fc-full").await.unwrap(),
        tags(&["A", "B", "C"])
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_whole_set_once_changed_set_is_noop() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // {A,B} then {C} (different) → stored EXACTLY {A,B}.
    store
        .insert_cycle_start_with_tags(
            "fc-x",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["A", "B"]),
        )
        .await
        .expect("first");
    store
        .insert_cycle_start_with_tags("fc-x", 1, None, None, None, 1700000001, None, &tags(&["C"]))
        .await
        .expect("second (frozen)");

    assert_eq!(
        store.get_cycle_tags("fc-x").await.unwrap(),
        tags(&["A", "B"])
    );
    // The cycle_start event row IS appended on every start (only the tag write freezes).
    assert_eq!(count_cycle_start_rows(&store, "fc-x").await, 2);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_whole_set_once_single_then_single() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // {A} then {B} → stored EXACTLY {A} (first tags win).
    store
        .insert_cycle_start_with_tags(
            "fc-ab",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["A"]),
        )
        .await
        .expect("first");
    store
        .insert_cycle_start_with_tags(
            "fc-ab",
            1,
            None,
            None,
            None,
            1700000001,
            None,
            &tags(&["B"]),
        )
        .await
        .expect("second");

    assert_eq!(store.get_cycle_tags("fc-ab").await.unwrap(), tags(&["A"]));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_whole_set_once_subset_and_superset_noop() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // Freeze {A,B}.
    store
        .insert_cycle_start_with_tags(
            "fc-ss",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["A", "B"]),
        )
        .await
        .expect("freeze");
    // Superset {A,B,C} → no-op.
    store
        .insert_cycle_start_with_tags(
            "fc-ss",
            1,
            None,
            None,
            None,
            1700000001,
            None,
            &tags(&["A", "B", "C"]),
        )
        .await
        .expect("superset");
    assert_eq!(
        store.get_cycle_tags("fc-ss").await.unwrap(),
        tags(&["A", "B"])
    );
    // Subset {A} → no-op.
    store
        .insert_cycle_start_with_tags(
            "fc-ss",
            2,
            None,
            None,
            None,
            1700000002,
            None,
            &tags(&["A"]),
        )
        .await
        .expect("subset");
    assert_eq!(
        store.get_cycle_tags("fc-ss").await.unwrap(),
        tags(&["A", "B"])
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_tagless_call_does_not_lock() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // Empty tag set → writes no cycle_tags rows and creates no lock sentinel.
    store
        .insert_cycle_start_with_tags("fc-tl", 0, None, None, None, 1700000000, None, &[])
        .await
        .expect("tagless start");
    assert!(store.get_cycle_tags("fc-tl").await.unwrap().is_empty());
    assert_eq!(count_cycle_start_rows(&store, "fc-tl").await, 1);

    // A later {A} start still locks {A} (first *tags* win, not first start).
    store
        .insert_cycle_start_with_tags(
            "fc-tl",
            1,
            None,
            None,
            None,
            1700000001,
            None,
            &tags(&["A"]),
        )
        .await
        .expect("later tagged start");
    assert_eq!(store.get_cycle_tags("fc-tl").await.unwrap(), tags(&["A"]));

    store.close().await.unwrap();
}

// ===========================================================================
// C2 — concurrency / TOCTOU (R-15 / AC-02b)
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent_same_cycle_starts_one_whole_set() {
    let dir = TempDir::new().unwrap();
    let store = std::sync::Arc::new(open_store(&dir).await);

    let s1 = store.clone();
    let s2 = store.clone();
    let h1 = tokio::spawn(async move {
        s1.insert_cycle_start_with_tags(
            "fc-cc",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["A", "B"]),
        )
        .await
    });
    let h2 = tokio::spawn(async move {
        s2.insert_cycle_start_with_tags(
            "fc-cc",
            1,
            None,
            None,
            None,
            1700000001,
            None,
            &tags(&["C", "D"]),
        )
        .await
    });

    // Neither call errors or panics.
    h1.await.expect("join1").expect("call1 ok");
    h2.await.expect("join2").expect("call2 ok");

    // Stored set is EXACTLY one intact whole set — never a merge {A,B,C,D} or partial mix.
    let stored = store.get_cycle_tags("fc-cc").await.unwrap();
    assert!(
        stored == tags(&["A", "B"]) || stored == tags(&["C", "D"]),
        "expected exactly one intact whole set, got {stored:?}"
    );

    // s1/s2 were moved into the spawned tasks and dropped; `store` is the sole owner.
    std::sync::Arc::into_inner(store)
        .expect("sole owner")
        .close()
        .await
        .unwrap();
}

// ===========================================================================
// C2 — value-opacity (R-11 / AC-01 / AC-07) + security
// ===========================================================================

#[tokio::test]
async fn test_empty_string_tag_rejected_others_stored() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    store
        .insert_cycle_start_with_tags(
            "fc-empty",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["workflow:v1.3", "", "foo"]),
        )
        .await
        .expect("insert");

    // Empty rejected; the other two stored verbatim (sorted by tag).
    assert_eq!(
        store.get_cycle_tags("fc-empty").await.unwrap(),
        tags(&["foo", "workflow:v1.3"])
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_colon_and_bare_stored_identically_no_branching() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    store
        .insert_cycle_start_with_tags(
            "fc-op",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["arm:A", "arm", "foo"]),
        )
        .await
        .expect("insert");

    // No namespace derivation, no prefix branching — all stored verbatim.
    assert_eq!(
        store.get_cycle_tags("fc-op").await.unwrap(),
        tags(&["arm", "arm:A", "foo"])
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_large_and_unicode_and_whitespace_tag_stored_verbatim() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    let large = "x".repeat(50_000);
    let unicode = "実験:アームⒶ🔬";
    let whitespace_only = "   "; // non-empty → stored (only emptiness is rejected)
    let submitted = vec![
        large.clone(),
        unicode.to_string(),
        whitespace_only.to_string(),
    ];

    store
        .insert_cycle_start_with_tags("fc-big", 0, None, None, None, 1700000000, None, &submitted)
        .await
        .expect("insert");

    let mut expected = submitted.clone();
    expected.sort();
    assert_eq!(store.get_cycle_tags("fc-big").await.unwrap(), expected);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_tag_write_uses_parameterized_binds() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // A tag full of SQL metacharacters must be stored verbatim — parameterized binds are
    // the ONLY SQLi defense (opacity forbids validation).
    let evil = "'); DROP TABLE cycle_tags;--";
    store
        .insert_cycle_start_with_tags(
            "fc-sqli",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&[evil]),
        )
        .await
        .expect("insert");

    assert_eq!(
        store.get_cycle_tags("fc-sqli").await.unwrap(),
        tags(&[evil])
    );
    // Table still exists and is queryable → no injection executed.
    assert_eq!(count_cycle_start_rows(&store, "fc-sqli").await, 1);

    store.close().await.unwrap();
}

// ===========================================================================
// C3 — get_cycle_tags
// ===========================================================================

#[tokio::test]
async fn test_get_cycle_tags_returns_sorted() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    store
        .insert_cycle_start_with_tags(
            "fc-sort",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["arm:B", "arm:A", "foo"]),
        )
        .await
        .expect("insert");

    assert_eq!(
        store.get_cycle_tags("fc-sort").await.unwrap(),
        tags(&["arm:A", "arm:B", "foo"])
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_get_cycle_tags_empty_when_none() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    // No rows → Ok(vec![]), not an error, not a spurious row.
    assert_eq!(
        store.get_cycle_tags("fc-none").await.unwrap(),
        Vec::<String>::new()
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_get_cycle_tags_scoped_to_feature_cycle() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    store
        .insert_cycle_start_with_tags(
            "fc-1",
            0,
            None,
            None,
            None,
            1700000000,
            None,
            &tags(&["A", "B"]),
        )
        .await
        .expect("fc-1");
    store
        .insert_cycle_start_with_tags("fc-2", 0, None, None, None, 1700000000, None, &tags(&["C"]))
        .await
        .expect("fc-2");

    assert_eq!(
        store.get_cycle_tags("fc-1").await.unwrap(),
        tags(&["A", "B"])
    );
    assert_eq!(store.get_cycle_tags("fc-2").await.unwrap(), tags(&["C"]));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_get_cycle_tags_verbatim() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    let submitted = tags(&["ns:value", "実験🔬"]);
    store
        .insert_cycle_start_with_tags("fc-vb", 0, None, None, None, 1700000000, None, &submitted)
        .await
        .expect("insert");

    let mut expected = submitted.clone();
    expected.sort();
    assert_eq!(store.get_cycle_tags("fc-vb").await.unwrap(), expected);

    store.close().await.unwrap();
}
