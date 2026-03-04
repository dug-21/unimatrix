use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::signal::{SignalRecord, SignalType, deserialize_signal, serialize_signal};

use super::db::Store;

impl Store {
    /// Insert a SignalRecord into signal_queue.
    ///
    /// Allocates a new signal_id from the counters table (next_signal_id).
    /// Enforces the 10,000-record cap: if the queue is at or above 10,000 records,
    /// deletes the oldest record (lowest signal_id) before inserting.
    /// Returns the allocated signal_id.
    pub fn insert_signal(&self, record: &SignalRecord) -> Result<u64> {
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<u64> {
            // 1. Read and increment next_signal_id
            let next_id: u64 = conn
                .query_row(
                    "SELECT value FROM counters WHERE name = 'next_signal_id'",
                    [],
                    |row| Ok(row.get::<_, i64>(0)? as u64),
                )
                .optional()
                .map_err(StoreError::Sqlite)?
                .unwrap_or(0);
            conn.execute(
                "INSERT OR REPLACE INTO counters (name, value) VALUES ('next_signal_id', ?1)",
                rusqlite::params![(next_id + 1) as i64],
            )
            .map_err(StoreError::Sqlite)?;

            // 2. Enforce cap: if queue >= 10_000, delete the oldest (lowest signal_id)
            let current_len: i64 = conn
                .query_row("SELECT COUNT(*) FROM signal_queue", [], |row| row.get(0))
                .map_err(StoreError::Sqlite)?;
            if current_len >= 10_000 {
                let oldest_key: Option<i64> = conn
                    .query_row(
                        "SELECT MIN(signal_id) FROM signal_queue",
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(StoreError::Sqlite)?
                    .flatten();
                if let Some(k) = oldest_key {
                    conn.execute(
                        "DELETE FROM signal_queue WHERE signal_id = ?1",
                        rusqlite::params![k],
                    )
                    .map_err(StoreError::Sqlite)?;
                }
            }

            // 3. Insert new record with allocated signal_id
            let mut full_record = record.clone();
            full_record.signal_id = next_id;
            let bytes = serialize_signal(&full_record)?;
            conn.execute(
                "INSERT INTO signal_queue (signal_id, data) VALUES (?1, ?2)",
                rusqlite::params![next_id as i64, bytes],
            )
            .map_err(StoreError::Sqlite)?;

            Ok(next_id)
        })();

        match result {
            Ok(id) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
                Ok(id)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Drain all SignalRecords of the given signal_type from signal_queue.
    pub fn drain_signals(&self, signal_type: SignalType) -> Result<Vec<SignalRecord>> {
        let conn = self.lock_conn();
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(StoreError::Sqlite)?;

        let result = (|| -> Result<Vec<SignalRecord>> {
            let mut drained = Vec::new();
            let mut keys_to_delete: Vec<i64> = Vec::new();

            let mut stmt = conn
                .prepare("SELECT signal_id, data FROM signal_queue ORDER BY signal_id")
                .map_err(StoreError::Sqlite)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .map_err(StoreError::Sqlite)?;

            for row in rows {
                let (key, bytes) = row.map_err(StoreError::Sqlite)?;
                match deserialize_signal(&bytes) {
                    Ok(record) if record.signal_type == signal_type => {
                        keys_to_delete.push(key);
                        drained.push(record);
                    }
                    Ok(_) => {
                        // Different signal_type -- leave it
                    }
                    Err(_) => {
                        // Corrupted record: remove
                        keys_to_delete.push(key);
                    }
                }
            }
            drop(stmt);

            for key in &keys_to_delete {
                conn.execute(
                    "DELETE FROM signal_queue WHERE signal_id = ?1",
                    rusqlite::params![key],
                )
                .map_err(StoreError::Sqlite)?;
            }

            Ok(drained)
        })();

        match result {
            Ok(drained) => {
                conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
                Ok(drained)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Return the total count of all records in signal_queue.
    pub fn signal_queue_len(&self) -> Result<u64> {
        let conn = self.lock_conn();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM signal_queue", [], |row| row.get(0))
            .map_err(StoreError::Sqlite)?;
        Ok(count as u64)
    }
}
