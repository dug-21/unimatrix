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
