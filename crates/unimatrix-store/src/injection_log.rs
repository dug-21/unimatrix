//! Injection event log persistence for col-010.
//!
//! Provides batch write and scan operations on the injection_log table.
//! `insert_injection_log_batch` is the sole public write API — never insert single records.
//! All operations are async.

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::analytics::AnalyticsWrite;
use crate::db::SqlxStore;
use crate::error::{Result, StoreError};

// -- Types --

/// A single injection event: one entry served to an agent during a ContextSearch.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct InjectionLogRecord {
    /// Monotonic log ID allocated by `insert_injection_log_batch`.
    pub log_id: u64,
    /// Session that received this injection.
    pub session_id: String,
    /// Entry that was injected.
    pub entry_id: u64,
    /// Reranked similarity/confidence score at injection time.
    pub confidence: f64,
    /// Unix epoch seconds.
    pub timestamp: u64,
}

// -- Store methods (sqlx backend) --

impl SqlxStore {
    /// Enqueue a batch of injection log records (analytics write via enqueue_analytics).
    ///
    /// Each record is enqueued as a separate `AnalyticsWrite::InjectionLog` event.
    /// The `log_id` field is ignored — the drain task uses SQLite AUTOINCREMENT.
    /// Returns immediately (no-op) if `records` is empty.
    pub fn insert_injection_log_batch(&self, records: &[InjectionLogRecord]) {
        for record in records {
            self.enqueue_analytics(AnalyticsWrite::InjectionLog {
                session_id: record.session_id.clone(),
                entry_id: record.entry_id,
                confidence: record.confidence,
                timestamp: record.timestamp as i64,
            });
        }
    }

    /// Scan injection log records for multiple sessions, ordered by log_id.
    ///
    /// Session IDs are batched into chunks of 50 to avoid large IN clauses (R-11).
    /// Returns an empty Vec if `session_ids` is empty or no rows match.
    pub async fn scan_injection_log_by_sessions(
        &self,
        session_ids: &[&str],
    ) -> Result<Vec<InjectionLogRecord>> {
        if session_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_results: Vec<InjectionLogRecord> = Vec::new();

        for chunk in session_ids.chunks(50) {
            let placeholders: String = chunk
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(",");

            let sql = format!(
                "SELECT log_id, session_id, entry_id, confidence, timestamp \
                 FROM injection_log \
                 WHERE session_id IN ({placeholders}) \
                 ORDER BY log_id"
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
                all_results.push(InjectionLogRecord {
                    log_id: row
                        .try_get::<i64, _>("log_id")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    session_id: row
                        .try_get("session_id")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    entry_id: row
                        .try_get::<i64, _>("entry_id")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    confidence: row
                        .try_get("confidence")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    timestamp: row
                        .try_get::<i64, _>("timestamp")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                });
            }
        }

        Ok(all_results)
    }

    /// Scan all injection log records for a given session_id using the index.
    pub async fn scan_injection_log_by_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<InjectionLogRecord>> {
        let rows = sqlx::query(
            "SELECT log_id, session_id, entry_id, confidence, timestamp \
             FROM injection_log WHERE session_id = ?1 ORDER BY log_id",
        )
        .bind(session_id)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        rows.iter()
            .map(|row| -> Result<InjectionLogRecord> {
                Ok(InjectionLogRecord {
                    log_id: row
                        .try_get::<i64, _>("log_id")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    session_id: row
                        .try_get("session_id")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    entry_id: row
                        .try_get::<i64, _>("entry_id")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    confidence: row
                        .try_get("confidence")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    timestamp: row
                        .try_get::<i64, _>("timestamp")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                })
            })
            .collect()
    }
}
