//! Injection event log persistence for SQLite backend.
//!
//! SQLite equivalent of the redb injection_log.rs module. Provides batch write
//! and scan operations on the injection_log table.

use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::injection_log::InjectionLogRecord;

use super::db::Store;

/// Serialize an InjectionLogRecord to bytes using bincode.
fn serialize_injection_log(record: &InjectionLogRecord) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(record, bincode::config::standard())
        .map_err(|e| StoreError::Serialization(e.to_string()))
}

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
            let base_id: u64 = conn
                .query_row(
                    "SELECT value FROM counters WHERE name = 'next_log_id'",
                    [],
                    |row| Ok(row.get::<_, i64>(0)? as u64),
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .unwrap_or(0);

            let next_id = base_id + records.len() as u64;
            conn.execute(
                "INSERT OR REPLACE INTO counters (name, value) VALUES ('next_log_id', ?1)",
                rusqlite::params![next_id as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // Insert each record with allocated log_id
            for (i, record) in records.iter().enumerate() {
                let mut r = record.clone();
                r.log_id = base_id + i as u64;
                let bytes = serialize_injection_log(&r)?;
                conn.execute(
                    "INSERT INTO injection_log (log_id, data) VALUES (?1, ?2)",
                    rusqlite::params![r.log_id as i64, bytes],
                )
                .map_err(StoreError::Sqlite)?;
            }

            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Scan all injection log records for a given session_id.
    ///
    /// Full table scan + in-process filter. Acceptable at current volumes.
    pub fn scan_injection_log_by_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<InjectionLogRecord>> {
        let conn = self.lock_conn();
        let mut stmt = conn
            .prepare("SELECT data FROM injection_log ORDER BY log_id")
            .map_err(StoreError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(StoreError::Sqlite)?;
        let mut results = Vec::new();
        for row in rows {
            let bytes = row.map_err(StoreError::Sqlite)?;
            let record = crate::injection_log::deserialize_injection_log(&bytes)?;
            if record.session_id == session_id {
                results.push(record);
            }
        }
        Ok(results)
    }
}
