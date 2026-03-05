//! Injection event log persistence for col-010.
//!
//! Provides batch write and scan operations on the injection_log table.
//! `insert_injection_log_batch` is the sole public write API — never insert single records.
//! All operations are synchronous; callers in async contexts use `tokio::task::spawn_blocking`.

use serde::{Deserialize, Serialize};

use crate::db::Store;
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

// -- Store methods (SQLite backend) --

impl Store {
    /// Insert a batch of injection log records in a single write transaction.
    ///
    /// Atomically allocates a contiguous range of `log_id` values from the
    /// `next_log_id` counter, writes all records, and commits.
    /// Incoming `log_id` fields are ignored and overwritten.
    ///
    /// Returns immediately (no-op) if `records` is empty.
    pub fn insert_injection_log_batch(&self, records: &[InjectionLogRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<()> {
            // Read and update counter
            let base_id = crate::counters::read_counter(&conn, "next_log_id")?;
            let next_id = base_id + records.len() as u64;
            crate::counters::set_counter(&conn, "next_log_id", next_id)?;

            // Insert each record with allocated log_id
            let mut stmt = conn
                .prepare(
                    "INSERT INTO injection_log (log_id, session_id, entry_id, confidence, timestamp) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(StoreError::Sqlite)?;

            for (i, record) in records.iter().enumerate() {
                let log_id = base_id + i as u64;
                stmt.execute(rusqlite::params![
                    log_id as i64,
                    &record.session_id,
                    record.entry_id as i64,
                    record.confidence,
                    record.timestamp as i64,
                ])
                .map_err(StoreError::Sqlite)?;
            }

            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Scan all injection log records for a given session_id using the index.
    pub fn scan_injection_log_by_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<InjectionLogRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare(
                "SELECT log_id, session_id, entry_id, confidence, timestamp \
                 FROM injection_log WHERE session_id = ?1 ORDER BY log_id",
            )
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map(rusqlite::params![session_id], |row| {
                Ok(InjectionLogRecord {
                    log_id: row.get::<_, i64>("log_id")? as u64,
                    session_id: row.get("session_id")?,
                    entry_id: row.get::<_, i64>("entry_id")? as u64,
                    confidence: row.get("confidence")?,
                    timestamp: row.get::<_, i64>("timestamp")? as u64,
                })
            })
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(StoreError::Sqlite)?);
        }
        Ok(results)
    }
}
