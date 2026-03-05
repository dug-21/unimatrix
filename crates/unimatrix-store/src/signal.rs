// LAYOUT FROZEN: bincode v2 positional encoding. Fields may only be APPENDED.
// See ADR-001 (col-009). Do not reorder or remove fields.

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::db::Store;
use crate::error::{Result, StoreError};

/// A single confidence signal record in the SIGNAL_QUEUE work queue.
///
/// Created at session end (Stop hook), consumed by dual consumers
/// (confidence pipeline and retrospective pipeline), then deleted.
///
/// Field order is frozen for bincode v2 positional compatibility (ADR-001).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignalRecord {
    pub signal_id: u64,              // field 0 — monotonic key, also stored as value
    pub session_id: String,          // field 1 — which session generated this signal
    pub created_at: u64,             // field 2 — Unix seconds
    pub entry_ids: Vec<u64>,         // field 3 — deduplicated entries receiving this signal
    pub signal_type: SignalType,     // field 4 — Helpful | Flagged
    pub signal_source: SignalSource, // field 5 — ImplicitOutcome | ImplicitRework
}

/// Type of confidence signal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalType {
    Helpful = 0,
    Flagged = 1,
}

/// Source of the implicit confidence signal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalSource {
    ImplicitOutcome = 0,
    ImplicitRework = 1,
}

/// Serialize a SignalRecord to bincode bytes using the serde-compatible path.
///
/// Uses `bincode::serde::encode_to_vec` with `standard()` config,
/// matching the workspace convention for EntryRecord.
pub fn serialize_signal(record: &SignalRecord) -> Result<Vec<u8>> {
    let bytes = bincode::serde::encode_to_vec(record, bincode::config::standard())?;
    Ok(bytes)
}

/// Deserialize a SignalRecord from bincode bytes using the serde-compatible path.
pub fn deserialize_signal(bytes: &[u8]) -> Result<SignalRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<SignalRecord, _>(bytes, bincode::config::standard())?;
    Ok(record)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signal(signal_type: SignalType) -> SignalRecord {
        SignalRecord {
            signal_id: 1,
            session_id: "test-session".to_string(),
            created_at: 1_700_000_000,
            entry_ids: vec![10, 20, 30],
            signal_type,
            signal_source: SignalSource::ImplicitOutcome,
        }
    }

    #[test]
    fn test_signal_record_roundtrip_helpful() {
        let record = make_signal(SignalType::Helpful);
        let bytes = serialize_signal(&record).expect("serialize");
        let deserialized = deserialize_signal(&bytes).expect("deserialize");
        assert_eq!(deserialized.signal_id, 1);
        assert_eq!(deserialized.session_id, "test-session");
        assert_eq!(deserialized.created_at, 1_700_000_000);
        assert_eq!(deserialized.entry_ids, vec![10, 20, 30]);
        assert_eq!(deserialized.signal_type, SignalType::Helpful);
        assert_eq!(deserialized.signal_source, SignalSource::ImplicitOutcome);
    }

    #[test]
    fn test_signal_record_roundtrip_flagged() {
        let record = SignalRecord {
            signal_id: 42,
            session_id: "session-2".to_string(),
            created_at: 2_000_000_000,
            entry_ids: vec![1, 2],
            signal_type: SignalType::Flagged,
            signal_source: SignalSource::ImplicitRework,
        };
        let bytes = serialize_signal(&record).expect("serialize");
        let deserialized = deserialize_signal(&bytes).expect("deserialize");
        assert_eq!(deserialized.signal_type, SignalType::Flagged);
        assert_eq!(deserialized.signal_source, SignalSource::ImplicitRework);
        assert_eq!(deserialized.signal_id, 42);
    }

    #[test]
    fn test_signal_type_discriminants() {
        assert_eq!(SignalType::Helpful as u8, 0);
        assert_eq!(SignalType::Flagged as u8, 1);
    }

    #[test]
    fn test_signal_source_discriminants() {
        assert_eq!(SignalSource::ImplicitOutcome as u8, 0);
        assert_eq!(SignalSource::ImplicitRework as u8, 1);
    }

    #[test]
    fn test_signal_record_empty_entry_ids() {
        let record = SignalRecord {
            signal_id: 0,
            session_id: String::new(),
            created_at: 0,
            entry_ids: vec![],
            signal_type: SignalType::Helpful,
            signal_source: SignalSource::ImplicitOutcome,
        };
        let bytes = serialize_signal(&record).expect("serialize");
        let deserialized = deserialize_signal(&bytes).expect("deserialize");
        assert!(deserialized.entry_ids.is_empty());
    }

    #[test]
    fn test_signal_record_roundtrip_max_values() {
        let record = SignalRecord {
            signal_id: u64::MAX,
            session_id: "x".repeat(256),
            created_at: u64::MAX,
            entry_ids: vec![u64::MAX],
            signal_type: SignalType::Flagged,
            signal_source: SignalSource::ImplicitRework,
        };
        let bytes = serialize_signal(&record).expect("serialize");
        let deserialized = deserialize_signal(&bytes).expect("deserialize");
        assert_eq!(deserialized.signal_id, u64::MAX);
    }
}
