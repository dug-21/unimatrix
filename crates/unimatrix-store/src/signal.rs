// Signal queue persistence for confidence and retrospective pipelines.
//
// Records are created at session end, consumed by dual consumers,
// then deleted. Uses SQL columns with JSON for entry_ids (ADR-007).

use serde::{Deserialize, Serialize};

use crate::db::Store;
use crate::error::{Result, StoreError};

/// A single confidence signal record in the SIGNAL_QUEUE work queue.
///
/// Created at session end (Stop hook), consumed by dual consumers
/// (confidence pipeline and retrospective pipeline), then deleted.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignalRecord {
    pub signal_id: u64,
    pub session_id: String,
    pub created_at: u64,
    pub entry_ids: Vec<u64>,
    pub signal_type: SignalType,
    pub signal_source: SignalSource,
}

/// Type of confidence signal.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalType {
    Helpful = 0,
    Flagged = 1,
}

impl TryFrom<u8> for SignalType {
    type Error = StoreError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Helpful),
            1 => Ok(Self::Flagged),
            other => Err(StoreError::InvalidStatus(other)),
        }
    }
}

/// Source of the implicit confidence signal.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalSource {
    ImplicitOutcome = 0,
    ImplicitRework = 1,
}

impl TryFrom<u8> for SignalSource {
    type Error = StoreError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::ImplicitOutcome),
            1 => Ok(Self::ImplicitRework),
            other => Err(StoreError::InvalidStatus(other)),
        }
    }
}

/// Serialize a SignalRecord to bincode bytes using the serde-compatible path.
///
/// Retained for migration compatibility and public re-export.
pub fn serialize_signal(record: &SignalRecord) -> Result<Vec<u8>> {
    let bytes = bincode::serde::encode_to_vec(record, bincode::config::standard())?;
    Ok(bytes)
}

/// Deserialize a SignalRecord from bincode bytes using the serde-compatible path.
///
/// Retained for migration compatibility and public re-export.
pub fn deserialize_signal(bytes: &[u8]) -> Result<SignalRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<SignalRecord, _>(bytes, bincode::config::standard())?;
    Ok(record)
}

/// Construct a SignalRecord from a SQL row.
fn signal_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SignalRecord> {
    let entry_ids_json: String = row.get("entry_ids")?;
    let entry_ids: Vec<u64> =
        serde_json::from_str(&entry_ids_json).unwrap_or_default();
    Ok(SignalRecord {
        signal_id: row.get::<_, i64>("signal_id")? as u64,
        session_id: row.get("session_id")?,
        created_at: row.get::<_, i64>("created_at")? as u64,
        entry_ids,
        signal_type: SignalType::try_from(row.get::<_, i64>("signal_type")? as u8)
            .unwrap_or(SignalType::Helpful),
        signal_source: SignalSource::try_from(row.get::<_, i64>("signal_source")? as u8)
            .unwrap_or(SignalSource::ImplicitOutcome),
    })
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
            let next_id = crate::counters::read_counter(&conn, "next_signal_id")?;
            crate::counters::set_counter(&conn, "next_signal_id", next_id + 1)?;

            // 2. Enforce cap: if queue >= 10_000, delete the oldest
            let current_len: i64 = conn
                .query_row("SELECT COUNT(*) FROM signal_queue", [], |row| row.get(0))
                .map_err(StoreError::Sqlite)?;
            if current_len >= 10_000 {
                conn.execute(
                    "DELETE FROM signal_queue WHERE signal_id = (\
                        SELECT MIN(signal_id) FROM signal_queue\
                    )",
                    [],
                )
                .map_err(StoreError::Sqlite)?;
            }

            // 3. Serialize entry_ids as JSON (ADR-007)
            let entry_ids_json = serde_json::to_string(&record.entry_ids)
                .map_err(|e| StoreError::Serialization(e.to_string()))?;

            // 4. Insert with SQL columns
            conn.execute(
                "INSERT INTO signal_queue (signal_id, session_id, created_at, \
                    entry_ids, signal_type, signal_source) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    next_id as i64,
                    &record.session_id,
                    record.created_at as i64,
                    &entry_ids_json,
                    record.signal_type as u8 as i64,
                    record.signal_source as u8 as i64,
                ],
            )
            .map_err(StoreError::Sqlite)?;

            Ok(next_id)
        })();

        match result {
            Ok(id) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
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
            // SELECT matching signals
            let mut stmt = conn
                .prepare(
                    "SELECT signal_id, session_id, created_at, entry_ids, \
                        signal_type, signal_source \
                     FROM signal_queue WHERE signal_type = ?1 ORDER BY signal_id",
                )
                .map_err(StoreError::Sqlite)?;

            let records: Vec<SignalRecord> = stmt
                .query_map(
                    rusqlite::params![signal_type as u8 as i64],
                    signal_from_row,
                )
                .map_err(StoreError::Sqlite)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(StoreError::Sqlite)?;

            drop(stmt);

            // DELETE matching signals
            conn.execute(
                "DELETE FROM signal_queue WHERE signal_type = ?1",
                rusqlite::params![signal_type as u8 as i64],
            )
            .map_err(StoreError::Sqlite)?;

            Ok(records)
        })();

        match result {
            Ok(drained) => {
                conn.execute_batch("COMMIT")
                    .map_err(StoreError::Sqlite)?;
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

    #[test]
    fn test_signal_type_try_from() {
        assert_eq!(SignalType::try_from(0u8).unwrap(), SignalType::Helpful);
        assert_eq!(SignalType::try_from(1u8).unwrap(), SignalType::Flagged);
        assert!(SignalType::try_from(2u8).is_err());
    }

    #[test]
    fn test_signal_source_try_from() {
        assert_eq!(SignalSource::try_from(0u8).unwrap(), SignalSource::ImplicitOutcome);
        assert_eq!(SignalSource::try_from(1u8).unwrap(), SignalSource::ImplicitRework);
        assert!(SignalSource::try_from(2u8).is_err());
    }
}
