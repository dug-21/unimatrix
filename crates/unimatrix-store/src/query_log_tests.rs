//! Unit tests for `query_log.rs` — `query_phase_freq_table` method.
//!
//! Extracted to a separate file to keep `query_log.rs` under the 500-line limit.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::SqlxStore;
use crate::query_log::PhaseFreqRow;
use crate::test_helpers::{TestEntry, open_test_store};

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Insert a query_log row directly (bypasses analytics queue for test determinism).
async fn insert_query_log_row(
    store: &SqlxStore,
    session_id: &str,
    phase: Option<&str>,
    result_entry_ids: Option<&str>,
    ts: i64,
) {
    sqlx::query(
        "INSERT INTO query_log
             (session_id, query_text, ts, result_count,
              result_entry_ids, similarity_scores, retrieval_mode, source, phase)
         VALUES (?1, '', ?2, 0, ?3, NULL, NULL, 'test', ?4)",
    )
    .bind(session_id)
    .bind(ts)
    .bind(result_entry_ids)
    .bind(phase)
    .execute(&store.write_pool)
    .await
    .expect("insert query_log row");
}

/// Insert an entry and return its assigned id.
async fn insert_entry(store: &SqlxStore, category: &str) -> u64 {
    store
        .insert(TestEntry::new("test-topic", category).build())
        .await
        .expect("insert entry")
}

// AC-08 / primary R-05 and R-13 guard
#[tokio::test]
async fn test_query_phase_freq_table_returns_correct_entry_id() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    // Insert entry (id assigned by AUTOINCREMENT counter).
    // Use whatever id is assigned and verify the round-trip (R-05 guard).
    let entry_id = insert_entry(&store, "decision").await;

    let ts = now_secs() - 1000; // within 30-day window
    for _ in 0..10 {
        insert_query_log_row(
            &store,
            "sess-ac08",
            Some("delivery"),
            Some(&format!("[{entry_id}]")),
            ts,
        )
        .await;
    }

    let rows = store
        .query_phase_freq_table(30)
        .await
        .expect("query_phase_freq_table");

    assert_eq!(rows.len(), 1, "expected exactly one aggregated row");
    let row = &rows[0];
    assert_eq!(row.phase, "delivery");
    assert_eq!(row.category, "decision");
    assert_eq!(row.entry_id, entry_id, "entry_id round-trip (R-05 guard)");
    assert_eq!(row.freq, 10i64, "freq must be i64 = 10 (R-13 guard)");
}

#[tokio::test]
async fn test_query_phase_freq_table_absent_entry_not_returned() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    // entry_id 99999 does not exist in entries
    let ts = now_secs() - 1000;
    insert_query_log_row(&store, "sess-absent", Some("delivery"), Some("[99999]"), ts).await;

    let rows = store
        .query_phase_freq_table(30)
        .await
        .expect("query_phase_freq_table");

    assert!(
        rows.is_empty(),
        "orphaned entry_id should be dropped by JOIN on entries"
    );
}

#[tokio::test]
async fn test_query_phase_freq_table_null_phase_rows_excluded() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let ts = now_secs() - 1000;

    // Row with null phase — must be excluded.
    insert_query_log_row(
        &store,
        "sess-null-phase",
        None,
        Some(&format!("[{entry_id}]")),
        ts,
    )
    .await;
    // Row with non-null phase — must be included.
    insert_query_log_row(
        &store,
        "sess-with-phase",
        Some("delivery"),
        Some(&format!("[{entry_id}]")),
        ts,
    )
    .await;

    let rows = store
        .query_phase_freq_table(30)
        .await
        .expect("query_phase_freq_table");

    assert_eq!(rows.len(), 1, "only non-null phase rows contribute");
    assert_eq!(rows[0].phase, "delivery");
}

#[tokio::test]
async fn test_query_phase_freq_table_null_result_entry_ids_excluded() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;
    let ts = now_secs() - 1000;

    // Row with null result_entry_ids — must be excluded.
    insert_query_log_row(&store, "sess-null-ids", Some("delivery"), None, ts).await;
    // Row with a valid result_entry_ids — must be counted.
    insert_query_log_row(
        &store,
        "sess-valid-ids",
        Some("delivery"),
        Some(&format!("[{entry_id}]")),
        ts,
    )
    .await;

    let rows = store
        .query_phase_freq_table(30)
        .await
        .expect("query_phase_freq_table");

    assert_eq!(rows.len(), 1, "null result_entry_ids row must be excluded");
    assert_eq!(rows[0].freq, 1i64);
}

#[tokio::test]
async fn test_query_phase_freq_table_outside_lookback_window_excluded() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let entry_id = insert_entry(&store, "decision").await;

    // Unix epoch 0 — far outside any lookback window.
    insert_query_log_row(
        &store,
        "sess-old",
        Some("delivery"),
        Some(&format!("[{entry_id}]")),
        0,
    )
    .await;

    let rows_30 = store
        .query_phase_freq_table(30)
        .await
        .expect("query_phase_freq_table 30d");
    assert!(rows_30.is_empty(), "old row excluded from 30-day window");

    let rows_1 = store
        .query_phase_freq_table(1)
        .await
        .expect("query_phase_freq_table 1d");
    assert!(rows_1.is_empty(), "old row excluded from 1-day window");
}

#[tokio::test]
async fn test_query_phase_freq_table_ordered_by_freq_desc() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let id_a = insert_entry(&store, "decision").await;
    let id_b = insert_entry(&store, "decision").await;

    let ts = now_secs() - 1000;

    // id_a accessed 10 times, id_b accessed 3 times — same phase and category.
    for _ in 0..10 {
        insert_query_log_row(
            &store,
            "sess-ord",
            Some("delivery"),
            Some(&format!("[{id_a}]")),
            ts,
        )
        .await;
    }
    for _ in 0..3 {
        insert_query_log_row(
            &store,
            "sess-ord",
            Some("delivery"),
            Some(&format!("[{id_b}]")),
            ts,
        )
        .await;
    }

    let rows = store
        .query_phase_freq_table(30)
        .await
        .expect("query_phase_freq_table");

    assert_eq!(rows.len(), 2, "expected two rows");
    assert_eq!(rows[0].entry_id, id_a, "highest freq entry must come first");
    assert_eq!(rows[0].freq, 10i64);
    assert_eq!(rows[1].entry_id, id_b);
    assert_eq!(rows[1].freq, 3i64);
}

#[tokio::test]
async fn test_query_phase_freq_table_multiple_phase_category_groups() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let id_decision = insert_entry(&store, "decision").await;
    let id_lesson = insert_entry(&store, "lesson-learned").await;

    let ts = now_secs() - 1000;

    insert_query_log_row(
        &store,
        "sess-multi",
        Some("delivery"),
        Some(&format!("[{id_decision}]")),
        ts,
    )
    .await;
    insert_query_log_row(
        &store,
        "sess-multi",
        Some("scope"),
        Some(&format!("[{id_lesson}]")),
        ts,
    )
    .await;

    let rows = store
        .query_phase_freq_table(30)
        .await
        .expect("query_phase_freq_table");

    assert_eq!(rows.len(), 2, "expected two rows from different groups");
    let has_delivery = rows
        .iter()
        .any(|r| r.phase == "delivery" && r.category == "decision");
    let has_scope = rows
        .iter()
        .any(|r| r.phase == "scope" && r.category == "lesson-learned");
    assert!(has_delivery, "delivery/decision group must be present");
    assert!(has_scope, "scope/lesson-learned group must be present");
}

#[tokio::test]
async fn test_query_phase_freq_table_empty_query_log_returns_empty() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;

    let rows = store
        .query_phase_freq_table(30)
        .await
        .expect("query_phase_freq_table");

    assert!(rows.is_empty(), "empty query_log must return empty Vec");
}

// Compile-time type assertions for PhaseFreqRow fields.
// These verify the struct field types match sqlx 0.8 mapping requirements.
#[allow(dead_code)]
fn assert_phase_freq_row_field_types(r: PhaseFreqRow) {
    let _phase: String = r.phase;
    let _category: String = r.category;
    let _entry_id: u64 = r.entry_id;
    let _freq: i64 = r.freq; // must be i64, NOT u64 (R-13)
}
