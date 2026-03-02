use std::path::Path;

use redb::{ReadableDatabase, ReadableTable, ReadableTableMetadata};

use crate::error::{Result, StoreError};
use crate::schema::{
    AGENT_REGISTRY, AUDIT_LOG, CATEGORY_INDEX, CO_ACCESS, COUNTERS, DatabaseConfig, ENTRIES,
    FEATURE_ENTRIES, OBSERVATION_METRICS, OUTCOME_INDEX, SIGNAL_QUEUE, STATUS_INDEX, TAG_INDEX,
    TIME_INDEX, TOPIC_INDEX, VECTOR_MAP,
};
use crate::signal::{SignalRecord, SignalType, deserialize_signal, serialize_signal};

/// The storage engine handle. Wraps a redb::Database.
///
/// `Store` is `Send + Sync` and shareable via `Arc<Store>`.
/// All read/write operations are methods on this struct.
pub struct Store {
    pub(crate) db: redb::Database,
}

impl Store {
    /// Open or create a database at the given path with default configuration.
    ///
    /// All 14 tables are created if they don't already exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_config(path, DatabaseConfig::default())
    }

    /// Open or create a database at the given path with custom configuration.
    ///
    /// All 14 tables are created if they don't already exist.
    pub fn open_with_config(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self> {
        let builder = redb::Builder::new();
        // Note: redb v3.1 Builder cache_size is managed internally.
        // We keep DatabaseConfig for API compatibility with future versions.
        let _ = config.cache_size;
        let db = builder
            .create(path.as_ref())
            .map_err(StoreError::Database)?;

        // Ensure all 15 tables exist by opening them in a write transaction.
        let txn = db.begin_write().map_err(StoreError::Transaction)?;
        {
            txn.open_table(ENTRIES).map_err(StoreError::Table)?;
            txn.open_table(TOPIC_INDEX).map_err(StoreError::Table)?;
            txn.open_table(CATEGORY_INDEX).map_err(StoreError::Table)?;
            txn.open_multimap_table(TAG_INDEX).map_err(StoreError::Table)?;
            txn.open_table(TIME_INDEX).map_err(StoreError::Table)?;
            txn.open_table(STATUS_INDEX).map_err(StoreError::Table)?;
            txn.open_table(VECTOR_MAP).map_err(StoreError::Table)?;
            txn.open_table(COUNTERS).map_err(StoreError::Table)?;
            txn.open_table(AGENT_REGISTRY).map_err(StoreError::Table)?;
            txn.open_table(AUDIT_LOG).map_err(StoreError::Table)?;
            txn.open_multimap_table(FEATURE_ENTRIES).map_err(StoreError::Table)?;
            txn.open_table(CO_ACCESS).map_err(StoreError::Table)?;
            txn.open_table(OUTCOME_INDEX).map_err(StoreError::Table)?;
            txn.open_table(OBSERVATION_METRICS).map_err(StoreError::Table)?;
            txn.open_table(SIGNAL_QUEUE).map_err(StoreError::Table)?;
        }
        txn.commit().map_err(StoreError::Commit)?;

        // Run schema migration if needed (after tables exist)
        crate::migration::migrate_if_needed(&db)?;

        Ok(Store { db })
    }

    /// Begin a read transaction.
    ///
    /// Exposes raw redb read access for subsystems (registry, audit)
    /// that manage their own tables.
    pub fn begin_read(&self) -> Result<redb::ReadTransaction> {
        self.db.begin_read().map_err(StoreError::Transaction)
    }

    /// Begin a write transaction.
    ///
    /// Exposes raw redb write access for subsystems (registry, audit)
    /// that manage their own tables.
    pub fn begin_write(&self) -> Result<redb::WriteTransaction> {
        self.db.begin_write().map_err(StoreError::Transaction)
    }

    /// Compact the database file, reclaiming space from COW pages.
    ///
    /// Intended for clean shutdown. Returns `Ok(())` on success.
    /// Requires mutable access since redb compaction needs `&mut Database`.
    pub fn compact(&mut self) -> Result<()> {
        self.db.compact().map_err(StoreError::Compaction)?;
        Ok(())
    }

    // -- col-009: Signal queue methods --

    /// Insert a SignalRecord into SIGNAL_QUEUE.
    ///
    /// Allocates a new signal_id from the COUNTERS table (next_signal_id).
    /// Enforces the 10,000-record cap: if the queue is at or above 10,000 records,
    /// deletes the oldest record (lowest signal_id) before inserting.
    /// Returns the allocated signal_id.
    pub fn insert_signal(&self, record: &SignalRecord) -> Result<u64> {
        let txn = self.db.begin_write().map_err(StoreError::Transaction)?;

        let next_id = {
            // 1. Read and increment next_signal_id
            let mut counters = txn.open_table(COUNTERS).map_err(StoreError::Table)?;
            let next_id = match counters.get("next_signal_id").map_err(StoreError::Storage)? {
                Some(guard) => guard.value(),
                None => 0u64,
            };
            counters
                .insert("next_signal_id", next_id + 1)
                .map_err(StoreError::Storage)?;
            next_id
        };

        {
            let mut queue = txn.open_table(SIGNAL_QUEUE).map_err(StoreError::Table)?;

            // 2. Enforce cap: if queue >= 10_000, delete the oldest (lowest signal_id)
            let current_len = queue.len().map_err(StoreError::Storage)?;
            if current_len >= 10_000 {
                // Find the oldest record (first in ascending key order)
                let oldest_key = queue
                    .iter()
                    .map_err(StoreError::Storage)?
                    .next()
                    .and_then(|r| r.ok())
                    .map(|(k, _)| k.value());
                if let Some(k) = oldest_key {
                    queue.remove(k).map_err(StoreError::Storage)?;
                }
            }

            // 3. Insert new record with allocated signal_id
            let mut full_record = record.clone();
            full_record.signal_id = next_id;
            let bytes = serialize_signal(&full_record)?;
            queue
                .insert(next_id, bytes.as_slice())
                .map_err(StoreError::Storage)?;
        }

        txn.commit().map_err(StoreError::Commit)?;
        Ok(next_id)
    }

    /// Drain all SignalRecords of the given signal_type from SIGNAL_QUEUE.
    ///
    /// Reads all matching records and deletes them in a single write transaction.
    /// Returns the drained records. Idempotent on empty queue.
    pub fn drain_signals(&self, signal_type: SignalType) -> Result<Vec<SignalRecord>> {
        let txn = self.db.begin_write().map_err(StoreError::Transaction)?;
        let mut drained = Vec::new();
        let mut keys_to_delete: Vec<u64> = Vec::new();

        {
            let queue = txn.open_table(SIGNAL_QUEUE).map_err(StoreError::Table)?;
            for entry in queue.iter().map_err(StoreError::Storage)? {
                let (k, v) = entry.map_err(StoreError::Storage)?;
                let key = k.value();
                let bytes = v.value();
                match deserialize_signal(bytes) {
                    Ok(record) if record.signal_type == signal_type => {
                        keys_to_delete.push(key);
                        drained.push(record);
                    }
                    Ok(_) => {
                        // Different signal_type — leave it in the queue
                    }
                    Err(_) => {
                        // Corrupted record: remove to avoid perpetual re-processing
                        keys_to_delete.push(key);
                    }
                }
            }
        }

        if !keys_to_delete.is_empty() {
            let mut queue = txn.open_table(SIGNAL_QUEUE).map_err(StoreError::Table)?;
            for key in &keys_to_delete {
                queue.remove(*key).map_err(StoreError::Storage)?;
            }
        }

        txn.commit().map_err(StoreError::Commit)?;
        Ok(drained)
    }

    /// Return the total count of all records in SIGNAL_QUEUE (any signal_type).
    pub fn signal_queue_len(&self) -> Result<u64> {
        let txn = self.db.begin_read().map_err(StoreError::Transaction)?;
        let queue = txn.open_table(SIGNAL_QUEUE).map_err(StoreError::Table)?;
        Ok(queue.len().map_err(StoreError::Storage)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::{ReadableDatabase, ReadableTable};

    #[test]
    fn test_open_creates_all_tables() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        // Verify all 14 tables exist by opening each in a read transaction
        let txn = store.db.begin_read().unwrap();
        txn.open_table(ENTRIES).unwrap();
        txn.open_table(TOPIC_INDEX).unwrap();
        txn.open_table(CATEGORY_INDEX).unwrap();
        txn.open_multimap_table(TAG_INDEX).unwrap();
        txn.open_table(TIME_INDEX).unwrap();
        txn.open_table(STATUS_INDEX).unwrap();
        txn.open_table(VECTOR_MAP).unwrap();
        txn.open_table(COUNTERS).unwrap();
        txn.open_table(AGENT_REGISTRY).unwrap();
        txn.open_table(AUDIT_LOG).unwrap();
        txn.open_multimap_table(FEATURE_ENTRIES).unwrap();
        txn.open_table(CO_ACCESS).unwrap();
        txn.open_table(OUTCOME_INDEX).unwrap();
        txn.open_table(OBSERVATION_METRICS).unwrap();
    }

    #[test]
    fn test_outcome_index_accessible_after_open() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let txn = store.db.begin_read().unwrap();
        let table = txn.open_table(OUTCOME_INDEX).unwrap();
        assert_eq!(table.iter().unwrap().count(), 0);
    }

    #[test]
    fn test_outcome_index_insert_and_read() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        // Write
        let txn = store.db.begin_write().unwrap();
        {
            let mut table = txn.open_table(OUTCOME_INDEX).unwrap();
            table.insert(("col-001", 42u64), ()).unwrap();
        }
        txn.commit().unwrap();

        // Read
        let txn = store.db.begin_read().unwrap();
        let table = txn.open_table(OUTCOME_INDEX).unwrap();
        assert!(table.get(("col-001", 42u64)).unwrap().is_some());

        // Range scan
        let range = table
            .range::<(&str, u64)>(("col-001", 0u64)..=("col-001", u64::MAX))
            .unwrap();
        assert_eq!(range.count(), 1);
    }

    #[test]
    fn test_observation_metrics_accessible_after_open() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let txn = store.db.begin_read().unwrap();
        let table = txn.open_table(OBSERVATION_METRICS).unwrap();
        assert_eq!(table.iter().unwrap().count(), 0);
    }

    #[test]
    fn test_open_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        assert!(!path.exists());
        let _store = Store::open(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_open_with_custom_cache() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let config = DatabaseConfig {
            cache_size: 128 * 1024 * 1024,
        };
        let _store = Store::open_with_config(&path, config).unwrap();
    }

    #[test]
    fn test_compact_succeeds() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let mut store = Store::open(&path).unwrap();
        store.compact().unwrap();
    }

    #[test]
    fn test_open_already_open_returns_database_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let _store1 = Store::open(&path).unwrap();
        let result = Store::open(&path);
        match result {
            Err(StoreError::Database(redb::DatabaseError::DatabaseAlreadyOpen)) => {}
            Err(e) => panic!("expected DatabaseAlreadyOpen, got: {e}"),
            Ok(_) => panic!("expected error, got Ok"),
        }
    }

    #[test]
    fn test_store_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Store>();
        assert_sync::<Store>();
    }

    #[test]
    fn test_open_creates_all_15_tables() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let txn = store.db.begin_read().unwrap();
        txn.open_table(ENTRIES).unwrap();
        txn.open_table(TOPIC_INDEX).unwrap();
        txn.open_table(CATEGORY_INDEX).unwrap();
        txn.open_multimap_table(TAG_INDEX).unwrap();
        txn.open_table(TIME_INDEX).unwrap();
        txn.open_table(STATUS_INDEX).unwrap();
        txn.open_table(VECTOR_MAP).unwrap();
        txn.open_table(COUNTERS).unwrap();
        txn.open_table(AGENT_REGISTRY).unwrap();
        txn.open_table(AUDIT_LOG).unwrap();
        txn.open_multimap_table(FEATURE_ENTRIES).unwrap();
        txn.open_table(CO_ACCESS).unwrap();
        txn.open_table(OUTCOME_INDEX).unwrap();
        txn.open_table(OBSERVATION_METRICS).unwrap();
        txn.open_table(SIGNAL_QUEUE).unwrap();
    }

    fn make_signal(signal_id: u64, session_id: &str, entry_ids: Vec<u64>, signal_type: crate::signal::SignalType) -> crate::signal::SignalRecord {
        use crate::signal::{SignalRecord, SignalSource};
        SignalRecord {
            signal_id,
            session_id: session_id.to_string(),
            created_at: 1000,
            entry_ids,
            signal_type,
            signal_source: SignalSource::ImplicitOutcome,
        }
    }

    #[test]
    fn test_insert_signal_returns_monotonic_ids() {
        use crate::signal::SignalType;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let s1 = make_signal(0, "sess-1", vec![1], SignalType::Helpful);
        let s2 = make_signal(0, "sess-2", vec![2], SignalType::Helpful);
        let id1 = store.insert_signal(&s1).unwrap();
        let id2 = store.insert_signal(&s2).unwrap();
        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
    }

    #[test]
    fn test_insert_signal_data_persists() {
        use crate::signal::SignalType;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let s = make_signal(0, "sess-abc", vec![10, 20], SignalType::Helpful);
        store.insert_signal(&s).unwrap();
        let drained = store.drain_signals(SignalType::Helpful).unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].session_id, "sess-abc");
        assert_eq!(drained[0].entry_ids, vec![10, 20]);
        assert_eq!(drained[0].signal_type, SignalType::Helpful);
    }

    #[test]
    fn test_signal_queue_len_counts_all_types() {
        use crate::signal::SignalType;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let h1 = make_signal(0, "s1", vec![1], SignalType::Helpful);
        let h2 = make_signal(0, "s2", vec![2], SignalType::Helpful);
        let f1 = make_signal(0, "s3", vec![3], SignalType::Flagged);
        store.insert_signal(&h1).unwrap();
        store.insert_signal(&h2).unwrap();
        store.insert_signal(&f1).unwrap();
        assert_eq!(store.signal_queue_len().unwrap(), 3);
    }

    #[test]
    fn test_drain_signals_idempotent_on_empty() {
        use crate::signal::SignalType;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let result = store.drain_signals(SignalType::Helpful).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_drain_signals_returns_matching_type() {
        use crate::signal::SignalType;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        for i in 0..3 {
            let s = make_signal(0, &format!("h{i}"), vec![i], SignalType::Helpful);
            store.insert_signal(&s).unwrap();
        }
        for i in 0..2 {
            let s = make_signal(0, &format!("f{i}"), vec![i + 10], SignalType::Flagged);
            store.insert_signal(&s).unwrap();
        }

        let helpful = store.drain_signals(SignalType::Helpful).unwrap();
        assert_eq!(helpful.len(), 3);
        let flagged = store.drain_signals(SignalType::Flagged).unwrap();
        assert_eq!(flagged.len(), 2);
    }

    #[test]
    fn test_drain_signals_deletes_drained_records() {
        use crate::signal::SignalType;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let s1 = make_signal(0, "s1", vec![1], SignalType::Helpful);
        let s2 = make_signal(0, "s2", vec![2], SignalType::Helpful);
        store.insert_signal(&s1).unwrap();
        store.insert_signal(&s2).unwrap();

        let first = store.drain_signals(SignalType::Helpful).unwrap();
        assert_eq!(first.len(), 2);

        let second = store.drain_signals(SignalType::Helpful).unwrap();
        assert!(second.is_empty());
        assert_eq!(store.signal_queue_len().unwrap(), 0);
    }

    #[test]
    fn test_drain_signals_leaves_other_type() {
        use crate::signal::SignalType;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let h = make_signal(0, "sh", vec![1], SignalType::Helpful);
        let f = make_signal(0, "sf", vec![2], SignalType::Flagged);
        store.insert_signal(&h).unwrap();
        store.insert_signal(&f).unwrap();

        store.drain_signals(SignalType::Helpful).unwrap();
        assert_eq!(store.signal_queue_len().unwrap(), 1);

        let remaining = store.drain_signals(SignalType::Flagged).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].signal_type, SignalType::Flagged);
    }

    #[test]
    fn test_signal_queue_cap_at_10001_drops_oldest() {
        use crate::signal::SignalType;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        for i in 0..10_001u64 {
            let s = make_signal(0, &format!("s{i}"), vec![i], SignalType::Helpful);
            store.insert_signal(&s).unwrap();
        }

        assert_eq!(store.signal_queue_len().unwrap(), 10_000);

        let drained = store.drain_signals(SignalType::Helpful).unwrap();
        let ids: std::collections::HashSet<u64> = drained.iter().map(|r| r.signal_id).collect();
        assert!(!ids.contains(&0), "oldest signal_id=0 should have been dropped");
        assert!(ids.contains(&10_000), "newest signal_id=10000 should be present");
    }

    #[test]
    fn test_insert_drain_full_roundtrip() {
        use crate::signal::{SignalType, SignalSource};
        use crate::signal::SignalRecord;
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        let s = SignalRecord {
            signal_id: 0,
            session_id: "rt-session".to_string(),
            created_at: 9999,
            entry_ids: vec![1, 2, 3],
            signal_type: SignalType::Helpful,
            signal_source: SignalSource::ImplicitOutcome,
        };
        let assigned_id = store.insert_signal(&s).unwrap();
        assert_eq!(assigned_id, 0);

        let drained = store.drain_signals(SignalType::Helpful).unwrap();
        assert_eq!(drained.len(), 1);
        let r = &drained[0];
        assert_eq!(r.session_id, "rt-session");
        assert_eq!(r.entry_ids, vec![1, 2, 3]);
        assert_eq!(r.signal_type, SignalType::Helpful);
        assert_eq!(r.signal_source, SignalSource::ImplicitOutcome);
    }
}
