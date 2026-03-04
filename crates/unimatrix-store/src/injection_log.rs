//! Injection event log persistence for col-010.
//!
//! Provides batch write and scan operations on the INJECTION_LOG table (schema v5).
//! `insert_injection_log_batch` is the sole public write API — never insert single records.
//! All operations are synchronous; callers in async contexts use `tokio::task::spawn_blocking`.

#[cfg(not(feature = "backend-sqlite"))]
use redb::{ReadableDatabase, ReadableTable};
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "backend-sqlite"))]
use crate::db::Store;
use crate::error::{Result, StoreError};
#[cfg(not(feature = "backend-sqlite"))]
use crate::schema::{COUNTERS, INJECTION_LOG};

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

// -- Serialization helpers --
//
// Exposed as `pub(crate)` so `sessions.rs` can deserialize INJECTION_LOG records
// during GC cascade without creating a circular module dependency.

#[cfg_attr(feature = "backend-sqlite", allow(dead_code))]
pub(crate) fn serialize_injection_log(record: &InjectionLogRecord) -> Result<Vec<u8>> {
    bincode::serde::encode_to_vec(record, bincode::config::standard())
        .map_err(|e| StoreError::Serialization(e.to_string()))
}

pub(crate) fn deserialize_injection_log(bytes: &[u8]) -> Result<InjectionLogRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<InjectionLogRecord, _>(bytes, bincode::config::standard())
            .map_err(|e| StoreError::Deserialization(e.to_string()))?;
    Ok(record)
}

// -- Store methods (redb backend) --

#[cfg(not(feature = "backend-sqlite"))]
impl Store {
    /// Insert a batch of injection log records in a single write transaction.
    ///
    /// Atomically allocates a contiguous range of `log_id` values from the
    /// `next_log_id` counter in COUNTERS, writes all records, and commits.
    /// Incoming `log_id` fields in `records` are ignored and overwritten.
    ///
    /// **This is the only public write API for INJECTION_LOG.** Never insert
    /// individual records — the batch guarantees that one ContextSearch response
    /// produces exactly one counter increment (ADR-003).
    ///
    /// Returns immediately (no-op) if `records` is empty.
    pub fn insert_injection_log_batch(&self, records: &[InjectionLogRecord]) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }

        let txn = self.db.begin_write().map_err(StoreError::Transaction)?;
        {
            let mut counters = txn.open_table(COUNTERS).map_err(StoreError::Table)?;
            let base_id: u64 = match counters.get("next_log_id").map_err(StoreError::Storage)? {
                Some(guard) => guard.value(),
                None => 0u64, // defensive: should not happen after v5 migration
            };
            let next_id = base_id + records.len() as u64;
            counters
                .insert("next_log_id", next_id)
                .map_err(StoreError::Storage)?;

            let mut log_table = txn.open_table(INJECTION_LOG).map_err(StoreError::Table)?;
            for (i, record) in records.iter().enumerate() {
                let mut r = record.clone();
                r.log_id = base_id + i as u64;
                let bytes = serialize_injection_log(&r)?;
                log_table
                    .insert(r.log_id, bytes.as_slice())
                    .map_err(StoreError::Storage)?;
            }
        }
        txn.commit().map_err(StoreError::Commit)?;
        Ok(())
    }

    /// Scan all injection log records for a given session_id.
    ///
    /// Full table scan + in-process filter. Acceptable at current volumes.
    /// Returns an empty vec if no records exist for the given session.
    pub fn scan_injection_log_by_session(&self, session_id: &str) -> Result<Vec<InjectionLogRecord>> {
        let txn = self.db.begin_read().map_err(StoreError::Transaction)?;
        let table = txn.open_table(INJECTION_LOG).map_err(StoreError::Table)?;
        let mut results = Vec::new();
        for entry in table.iter().map_err(StoreError::Storage)? {
            let (_, value_guard) = entry.map_err(StoreError::Storage)?;
            let record = deserialize_injection_log(value_guard.value())?;
            if record.session_id == session_id {
                results.push(record);
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
#[cfg(not(feature = "backend-sqlite"))]
mod tests {
    use super::*;

    fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();
        (dir, store)
    }

    fn make_record(session_id: &str, entry_id: u64) -> InjectionLogRecord {
        InjectionLogRecord {
            log_id: 0, // allocated by insert_injection_log_batch
            session_id: session_id.to_string(),
            entry_id,
            confidence: 0.8,
            timestamp: 1700000000,
        }
    }

    #[test]
    fn test_injection_log_record_roundtrip() {
        let record = InjectionLogRecord {
            log_id: 42,
            session_id: "sess-1".to_string(),
            entry_id: 100,
            confidence: 0.123456789012345,
            timestamp: 1700000000,
        };
        let bytes = serialize_injection_log(&record).unwrap();
        let back = deserialize_injection_log(&bytes).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn test_injection_log_batch_allocates_ids() {
        let (_dir, store) = open_store();
        let records = vec![
            make_record("sess-a", 1),
            make_record("sess-a", 2),
            make_record("sess-a", 3),
        ];
        store.insert_injection_log_batch(&records).unwrap();

        let got = store.scan_injection_log_by_session("sess-a").unwrap();
        assert_eq!(got.len(), 3);

        // IDs should be contiguous starting at 0
        let mut ids: Vec<u64> = got.iter().map(|r| r.log_id).collect();
        ids.sort();
        assert_eq!(ids, vec![0, 1, 2]);
    }

    #[test]
    fn test_injection_log_sequential_batches_no_overlap() {
        let (_dir, store) = open_store();
        let batch1 = vec![make_record("sess-a", 1), make_record("sess-a", 2)];
        let batch2 = vec![make_record("sess-b", 3), make_record("sess-b", 4)];
        store.insert_injection_log_batch(&batch1).unwrap();
        store.insert_injection_log_batch(&batch2).unwrap();

        let all_a = store.scan_injection_log_by_session("sess-a").unwrap();
        let all_b = store.scan_injection_log_by_session("sess-b").unwrap();

        let mut all_ids: Vec<u64> = all_a.iter().chain(all_b.iter()).map(|r| r.log_id).collect();
        all_ids.sort();
        // All 4 IDs must be distinct and contiguous
        assert_eq!(all_ids, vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_injection_log_session_isolation() {
        let (_dir, store) = open_store();
        let batch_a = vec![
            make_record("session-A", 1),
            make_record("session-A", 2),
            make_record("session-A", 3),
        ];
        let batch_b = vec![
            make_record("session-B", 10),
            make_record("session-B", 11),
        ];
        store.insert_injection_log_batch(&batch_a).unwrap();
        store.insert_injection_log_batch(&batch_b).unwrap();

        let a = store.scan_injection_log_by_session("session-A").unwrap();
        let b = store.scan_injection_log_by_session("session-B").unwrap();
        assert_eq!(a.len(), 3);
        assert_eq!(b.len(), 2);
        assert!(a.iter().all(|r| r.session_id == "session-A"));
        assert!(b.iter().all(|r| r.session_id == "session-B"));
    }

    #[test]
    fn test_injection_log_empty_batch_is_noop() {
        let (_dir, store) = open_store();
        store.insert_injection_log_batch(&[]).unwrap();

        // Counter should not have been updated
        let txn = store.db.begin_read().unwrap();
        let counters = txn.open_table(crate::schema::COUNTERS).unwrap();
        let next_log_id = counters.get("next_log_id").unwrap();
        assert_eq!(next_log_id.map(|g| g.value()), Some(0u64));
    }

    #[test]
    fn test_injection_log_scan_empty_store() {
        let (_dir, store) = open_store();
        let result = store.scan_injection_log_by_session("anything").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_injection_log_confidence_f64_precision() {
        let (_dir, store) = open_store();
        let mut record = make_record("precision-sess", 1);
        record.confidence = 0.123456789012345;
        store.insert_injection_log_batch(&[record]).unwrap();

        let got = store.scan_injection_log_by_session("precision-sess").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].confidence, 0.123456789012345);
    }

    #[test]
    fn test_injection_log_one_transaction_per_batch() {
        // Verify that a batch of 3 records increments next_log_id by exactly 3
        let (_dir, store) = open_store();
        let batch = vec![
            make_record("sess", 1),
            make_record("sess", 2),
            make_record("sess", 3),
        ];
        store.insert_injection_log_batch(&batch).unwrap();

        let txn = store.db.begin_read().unwrap();
        let counters = txn.open_table(crate::schema::COUNTERS).unwrap();
        let next = counters.get("next_log_id").unwrap().map(|g| g.value()).unwrap_or(0);
        assert_eq!(next, 3);
    }
}
