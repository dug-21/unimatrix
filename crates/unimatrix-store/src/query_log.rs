//! Query log persistence for nxs-010.
//!
//! Provides insert and scan operations on the `query_log` table.
//! The table uses SQLite AUTOINCREMENT for primary key allocation.
//! All operations are async.

use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::Row;

use crate::analytics::AnalyticsWrite;
use crate::db::SqlxStore;
use crate::error::{Result, StoreError};

// -- Types --

/// A single query log entry capturing search telemetry.
///
/// `query_id` is 0 on insert (AUTOINCREMENT allocates) and populated on read.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryLogRecord {
    /// Auto-allocated primary key. Set to 0 on insert; populated on read.
    pub query_id: i64,
    pub session_id: String,
    pub query_text: String,
    pub ts: u64,
    pub result_count: i64,
    pub result_entry_ids: String,
    pub similarity_scores: String,
    pub retrieval_mode: String,
    pub source: String,
    pub phase: Option<String>, // col-028: workflow phase at query time; None for UDS rows
}

/// Transient row returned by `query_phase_freq_table`.
///
/// Used only during `PhaseFreqTable::rebuild`; not stored or returned to callers.
///
/// `freq` is i64 because SQLite `COUNT(*)` maps to i64 via sqlx 0.8.
/// Do NOT use u64 — sqlx deserialization will fail silently at runtime (R-13).
#[derive(Debug, Clone, PartialEq)]
pub struct PhaseFreqRow {
    pub phase: String,
    pub category: String,
    /// entry_id read as i64 from SQL (CAST result), then cast to u64.
    /// The SQL CAST(je.value AS INTEGER) guarantees a non-negative integer value.
    pub entry_id: u64,
    /// COUNT(*) result — always i64 in sqlx 0.8 SQLite mapping.
    pub freq: i64,
}

// -- Shared constructor --

impl QueryLogRecord {
    /// Construct a new `QueryLogRecord` with consistent field population.
    pub fn new(
        session_id: String,
        query_text: String,
        entry_ids: &[u64],
        similarity_scores: &[f64],
        retrieval_mode: &str,
        source: &str,
        phase: Option<String>, // col-028: workflow phase at query time; final parameter
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        QueryLogRecord {
            query_id: 0,
            session_id,
            query_text,
            ts: now,
            result_count: entry_ids.len() as i64,
            result_entry_ids: serde_json::to_string(entry_ids).unwrap_or_default(),
            similarity_scores: serde_json::to_string(similarity_scores).unwrap_or_default(),
            retrieval_mode: retrieval_mode.to_string(),
            source: source.to_string(),
            phase,
        }
    }
}

// -- Store methods --

impl SqlxStore {
    /// Enqueue a query log record (analytics write via enqueue_analytics).
    pub fn insert_query_log(&self, record: &QueryLogRecord) {
        self.enqueue_analytics(AnalyticsWrite::QueryLog {
            session_id: record.session_id.clone(),
            query_text: record.query_text.clone(),
            ts: record.ts as i64,
            result_count: record.result_count,
            result_entry_ids: if record.result_entry_ids.is_empty() {
                None
            } else {
                Some(record.result_entry_ids.clone())
            },
            similarity_scores: if record.similarity_scores.is_empty() {
                None
            } else {
                Some(record.similarity_scores.clone())
            },
            retrieval_mode: if record.retrieval_mode.is_empty() {
                None
            } else {
                Some(record.retrieval_mode.clone())
            },
            source: record.source.clone(),
            phase: record.phase.clone(), // col-028
        });
    }

    /// Scan query log records for multiple sessions, ordered by timestamp ascending.
    ///
    /// Session IDs are batched into chunks of 50 to avoid large IN clauses (R-11).
    /// Returns an empty Vec if `session_ids` is empty or no rows match.
    pub async fn scan_query_log_by_sessions(
        &self,
        session_ids: &[&str],
    ) -> Result<Vec<QueryLogRecord>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_results: Vec<QueryLogRecord> = Vec::new();

        for chunk in session_ids.chunks(50) {
            let placeholders: String = chunk
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(",");

            let sql = format!(
                "SELECT query_id, session_id, query_text, ts, result_count, \
                        result_entry_ids, similarity_scores, retrieval_mode, source, phase \
                 FROM query_log \
                 WHERE session_id IN ({placeholders}) \
                 ORDER BY ts ASC"
            );

            let mut query = sqlx::query(&sql);
            for &id in chunk {
                query = query.bind(id);
            }

            let rows = query
                .fetch_all(self.read_pool())
                .await
                .map_err(|e| StoreError::Database(e.into()))?;

            for row in rows {
                all_results.push(row_to_query_log(&row)?);
            }
        }

        Ok(all_results)
    }

    /// Scan all query log records for a given session, ordered by timestamp ascending.
    pub async fn scan_query_log_by_session(&self, session_id: &str) -> Result<Vec<QueryLogRecord>> {
        let rows = sqlx::query(
            "SELECT query_id, session_id, query_text, ts, result_count, \
                    result_entry_ids, similarity_scores, retrieval_mode, source, phase \
             FROM query_log \
             WHERE session_id = ?1 \
             ORDER BY ts ASC",
        )
        .bind(session_id)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter().map(row_to_query_log).collect()
    }

    /// Query (phase, category, entry_id, freq) aggregates from query_log within
    /// a time window, joined to entries for category lookup.
    ///
    /// # SQL
    ///
    /// The SQL uses CROSS JOIN json_each to expand the JSON array in
    /// `result_entry_ids`. CAST(je.value AS INTEGER) is MANDATORY — omitting it
    /// causes a text-to-integer JOIN mismatch that returns zero rows silently (R-05).
    /// Verified against mcp/knowledge_reuse.rs json_each usage (Unimatrix #3681).
    ///
    /// Results are ordered by (phase, category, freq DESC) — the caller uses this
    /// ordering directly for rank-based normalization without re-sorting.
    ///
    /// # Parameters
    ///
    /// `lookback_days` is bound as i64 (sqlx 0.8 INTEGER mapping requirement).
    /// Validated to [1, 3650] by InferenceConfig::validate() at startup (R-08).
    ///
    /// # Returns
    ///
    /// Empty Vec when:
    ///   - No query_log rows have non-null phase within the time window.
    ///   - All result_entry_ids are null.
    ///   - The entries table has no rows matching any entry_id in the log.
    ///
    /// Caller (`PhaseFreqTable::rebuild`) treats an empty Vec as use_fallback=true.
    pub async fn query_phase_freq_table(&self, lookback_days: u32) -> Result<Vec<PhaseFreqRow>> {
        // The SQL is specified verbatim — do NOT modify the CAST forms or WHERE clause.
        // Any change to CAST(je.value AS INTEGER) risks returning zero rows silently (R-05).
        let sql = "
            SELECT
                q.phase,
                e.category,
                CAST(je.value AS INTEGER)  AS entry_id,
                COUNT(*)                   AS freq
            FROM query_log q
              CROSS JOIN json_each(q.result_entry_ids) AS je
              JOIN entries e ON CAST(je.value AS INTEGER) = e.id
            WHERE q.phase IS NOT NULL
              AND q.result_entry_ids IS NOT NULL
              AND q.ts > strftime('%s', 'now') - ?1 * 86400
            GROUP BY q.phase, e.category, CAST(je.value AS INTEGER)
            ORDER BY q.phase, e.category, freq DESC
        ";

        // Bind lookback_days as i64 (sqlx 0.8 INTEGER mapping — u32 would fail).
        let rows = sqlx::query(sql)
            .bind(lookback_days as i64)
            .fetch_all(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Deserialize using positional index access, matching existing query_log.rs pattern.
        // Column order from SELECT:
        //   0: q.phase       -> String
        //   1: e.category    -> String
        //   2: entry_id      -> i64  (CAST result is INTEGER in SQLite)
        //   3: freq          -> i64  (COUNT(*) is always i64 in sqlx 0.8)
        rows.iter().map(row_to_phase_freq_row).collect()
    }
}

/// Deserialize a single SQL row into PhaseFreqRow.
///
/// Column positions must match the SELECT clause in query_phase_freq_table:
///   0: phase    (String)
///   1: category (String)
///   2: entry_id (i64, cast to u64)
///   3: freq     (i64)
///
/// entry_id is read as i64 and cast to u64 because:
///   - SQLite INTEGER is always signed i64 in sqlx 0.8
///   - Entry IDs are non-negative by construction
///   - The CAST(je.value AS INTEGER) SQL expression produces INTEGER affinity
fn row_to_phase_freq_row(row: &sqlx::sqlite::SqliteRow) -> Result<PhaseFreqRow> {
    Ok(PhaseFreqRow {
        phase: row
            .try_get::<String, _>(0)
            .map_err(|e| StoreError::Database(e.into()))?,
        category: row
            .try_get::<String, _>(1)
            .map_err(|e| StoreError::Database(e.into()))?,
        entry_id: row
            .try_get::<i64, _>(2)
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        freq: row
            .try_get::<i64, _>(3)
            .map_err(|e| StoreError::Database(e.into()))?,
    })
}

fn row_to_query_log(row: &sqlx::sqlite::SqliteRow) -> Result<QueryLogRecord> {
    Ok(QueryLogRecord {
        query_id: row.try_get(0).map_err(|e| StoreError::Database(e.into()))?,
        session_id: row.try_get(1).map_err(|e| StoreError::Database(e.into()))?,
        query_text: row.try_get(2).map_err(|e| StoreError::Database(e.into()))?,
        ts: row
            .try_get::<i64, _>(3)
            .map_err(|e| StoreError::Database(e.into()))? as u64,
        result_count: row.try_get(4).map_err(|e| StoreError::Database(e.into()))?,
        result_entry_ids: row
            .try_get::<Option<String>, _>(5)
            .map_err(|e| StoreError::Database(e.into()))?
            .unwrap_or_default(),
        similarity_scores: row
            .try_get::<Option<String>, _>(6)
            .map_err(|e| StoreError::Database(e.into()))?
            .unwrap_or_default(),
        retrieval_mode: row
            .try_get::<Option<String>, _>(7)
            .map_err(|e| StoreError::Database(e.into()))?
            .unwrap_or_default(),
        source: row.try_get(8).map_err(|e| StoreError::Database(e.into()))?,
        // col-028: phase at index 9 — must match SELECT column list order (AC-17, SR-01 guard).
        // source is at index 8; phase is at index 9. Do NOT swap.
        phase: row
            .try_get::<Option<String>, _>(9)
            .map_err(|e| StoreError::Database(e.into()))?,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
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
        let dir = tempfile::TempDir::new().expect("tempdir");
        let _ = dir; // keep dir alive — actually we use the store's db, not a new one
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

        // Insert entry (id assigned by AUTOINCREMENT counter — first entry = 1).
        // We need id=42 specifically; seed 42 entries and use the last one.
        // Simpler: just use whatever id is assigned and verify round-trip.
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
}
