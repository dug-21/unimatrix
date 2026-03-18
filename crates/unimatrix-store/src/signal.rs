// Signal queue persistence for confidence and retrospective pipelines.
//
// Records are created at session end, consumed by dual consumers,
// then deleted. Uses SQL columns with JSON for entry_ids (ADR-007).

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::db::{SqlxStore, map_pool_timeout};
use crate::error::{PoolKind, Result, StoreError};

/// A single confidence signal record in the SIGNAL_QUEUE work queue.
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
pub fn serialize_signal(record: &SignalRecord) -> crate::error::Result<Vec<u8>> {
    let bytes = bincode::serde::encode_to_vec(record, bincode::config::standard())?;
    Ok(bytes)
}

/// Deserialize a SignalRecord from bincode bytes using the serde-compatible path.
///
/// Retained for migration compatibility and public re-export.
pub fn deserialize_signal(bytes: &[u8]) -> crate::error::Result<SignalRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<SignalRecord, _>(bytes, bincode::config::standard())?;
    Ok(record)
}

impl SqlxStore {
    /// Insert a SignalRecord (analytics write via enqueue_analytics).
    ///
    /// The `signal_id` field is ignored — the drain task serializes entry_ids to JSON
    /// and inserts using the queued values. Returns 0 for the allocated signal_id
    /// (ID allocation now happens asynchronously in the drain task via write_pool).
    /// Insert a signal record directly into the write pool.
    ///
    /// Writes directly (not via analytics drain) to ensure immediate visibility
    /// for consumers like `run_confidence_consumer` and `drain_signals` that
    /// read signals immediately after insertion.
    pub async fn insert_signal(&self, record: &SignalRecord) -> Result<()> {
        let entry_ids_json = serde_json::to_string(&record.entry_ids).unwrap_or_default();
        sqlx::query(
            "INSERT INTO signal_queue (session_id, created_at, entry_ids, signal_type, signal_source)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&record.session_id)
        .bind(record.created_at as i64)
        .bind(&entry_ids_json)
        .bind(record.signal_type as u8 as i64)
        .bind(record.signal_source as u8 as i64)
        .execute(&self.write_pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;
        Ok(())
    }

    /// Drain all SignalRecords of the given signal_type from signal_queue.
    ///
    /// Uses write_pool directly (DELETE is a write operation).
    pub async fn drain_signals(&self, signal_type: SignalType) -> Result<Vec<SignalRecord>> {
        let mut txn = self
            .write_pool
            .begin()
            .await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        let rows = sqlx::query(
            "SELECT signal_id, session_id, created_at, entry_ids, \
                signal_type, signal_source \
             FROM signal_queue WHERE signal_type = ?1 ORDER BY signal_id",
        )
        .bind(signal_type as u8 as i64)
        .fetch_all(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let records: Vec<SignalRecord> = rows
            .iter()
            .map(|row| -> Result<SignalRecord> {
                let entry_ids_json: String = row
                    .try_get("entry_ids")
                    .map_err(|e| StoreError::Database(e.into()))?;
                let entry_ids: Vec<u64> = serde_json::from_str(&entry_ids_json).unwrap_or_default();
                Ok(SignalRecord {
                    signal_id: row
                        .try_get::<i64, _>("signal_id")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    session_id: row
                        .try_get("session_id")
                        .map_err(|e| StoreError::Database(e.into()))?,
                    created_at: row
                        .try_get::<i64, _>("created_at")
                        .map_err(|e| StoreError::Database(e.into()))?
                        as u64,
                    entry_ids,
                    signal_type: SignalType::try_from(
                        row.try_get::<i64, _>("signal_type")
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u8,
                    )
                    .unwrap_or(SignalType::Helpful),
                    signal_source: SignalSource::try_from(
                        row.try_get::<i64, _>("signal_source")
                            .map_err(|e| StoreError::Database(e.into()))?
                            as u8,
                    )
                    .unwrap_or(SignalSource::ImplicitOutcome),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        sqlx::query("DELETE FROM signal_queue WHERE signal_type = ?1")
            .bind(signal_type as u8 as i64)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(records)
    }

    /// Return the total count of all records in signal_queue.
    pub async fn signal_queue_len(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM signal_queue")
            .fetch_one(self.read_pool())
            .await
            .map_err(|e| StoreError::Database(e.into()))?;
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
        assert_eq!(
            SignalSource::try_from(0u8).unwrap(),
            SignalSource::ImplicitOutcome
        );
        assert_eq!(
            SignalSource::try_from(1u8).unwrap(),
            SignalSource::ImplicitRework
        );
        assert!(SignalSource::try_from(2u8).is_err());
    }
}
