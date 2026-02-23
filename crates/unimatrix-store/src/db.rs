use std::path::Path;

use redb::ReadableDatabase;

use crate::error::{Result, StoreError};
use crate::schema::{
    AGENT_REGISTRY, AUDIT_LOG, CATEGORY_INDEX, COUNTERS, DatabaseConfig, ENTRIES, STATUS_INDEX,
    TAG_INDEX, TIME_INDEX, TOPIC_INDEX, VECTOR_MAP,
};

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
    /// All 10 tables are created if they don't already exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_config(path, DatabaseConfig::default())
    }

    /// Open or create a database at the given path with custom configuration.
    ///
    /// All 10 tables are created if they don't already exist.
    pub fn open_with_config(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self> {
        let builder = redb::Builder::new();
        // Note: redb v3.1 Builder cache_size is managed internally.
        // We keep DatabaseConfig for API compatibility with future versions.
        let _ = config.cache_size;
        let db = builder
            .create(path.as_ref())
            .map_err(StoreError::Database)?;

        // Ensure all 10 tables exist by opening them in a write transaction.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::ReadableDatabase;

    #[test]
    fn test_open_creates_all_tables() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Store::open(&path).unwrap();

        // Verify all 10 tables exist by opening each in a read transaction
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
}
