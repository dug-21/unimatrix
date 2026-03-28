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
#[path = "query_log_tests.rs"]
mod tests;
