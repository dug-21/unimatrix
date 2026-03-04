# C1: SQLite Connection Manager

## Files
- `crates/unimatrix-store/Cargo.toml` (edit)
- `crates/unimatrix-store/src/sqlite/mod.rs` (new)
- `crates/unimatrix-store/src/sqlite/db.rs` (new)
- `crates/unimatrix-store/src/lib.rs` (edit)
- `crates/unimatrix-store/src/error.rs` (edit)

## Cargo.toml Changes

```toml
[features]
default = []
backend-sqlite = ["dep:rusqlite"]
test-support = ["dep:tempfile"]

[dependencies]
rusqlite = { version = "0.34", features = ["bundled"], optional = true }
# existing deps unchanged
```

## sqlite/mod.rs

```rust
mod db;
pub(crate) mod write;
pub(crate) mod read;
pub(crate) mod signal;
pub(crate) mod sessions;
pub(crate) mod injection_log;
pub(crate) mod migration;

pub use db::Store;
```

## sqlite/db.rs -- Store struct

```rust
use std::sync::Mutex;
use std::path::Path;
use rusqlite::Connection;
use crate::error::{Result, StoreError};
use crate::schema::DatabaseConfig;

pub struct Store {
    pub(crate) conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_config(path, DatabaseConfig::default())
    }

    pub fn open_with_config(path: impl AsRef<Path>, _config: DatabaseConfig) -> Result<Self> {
        let conn = Connection::open(path).map_err(StoreError::Sqlite)?;

        // PRAGMAs (ADR-003)
        conn.execute_batch("
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA wal_autocheckpoint = 1000;
            PRAGMA foreign_keys = OFF;
            PRAGMA busy_timeout = 5000;
            PRAGMA cache_size = -16384;
        ").map_err(StoreError::Sqlite)?;

        // Create all 17 tables (idempotent)
        create_tables(&conn)?;

        let store = Store { conn: Mutex::new(conn) };

        // Run schema migration if needed
        migration::migrate_if_needed(&store)?;

        Ok(store)
    }

    pub fn compact(&mut self) -> Result<()> {
        // No-op for SQLite (ADR-003: WAL auto-checkpoint handles space)
        Ok(())
    }

    /// Begin a "read transaction" -- returns a wrapper around MutexGuard.
    /// Server code uses this for AGENT_REGISTRY and AUDIT_LOG access.
    pub fn begin_read(&self) -> Result<SqliteReadTransaction<'_>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(SqliteReadTransaction { guard })
    }

    /// Begin a "write transaction" -- returns a wrapper around MutexGuard.
    pub fn begin_write(&self) -> Result<SqliteWriteTransaction<'_>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(SqliteWriteTransaction { guard })
    }
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS entries (
            id INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS topic_index (
            topic TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (topic, entry_id)
        );
        CREATE TABLE IF NOT EXISTS category_index (
            category TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (category, entry_id)
        );
        CREATE TABLE IF NOT EXISTS tag_index (
            tag TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (tag, entry_id)
        );
        CREATE TABLE IF NOT EXISTS time_index (
            timestamp INTEGER NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (timestamp, entry_id)
        );
        CREATE TABLE IF NOT EXISTS status_index (
            status INTEGER NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (status, entry_id)
        );
        CREATE TABLE IF NOT EXISTS vector_map (
            entry_id INTEGER PRIMARY KEY,
            hnsw_data_id INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS counters (
            name TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS agent_registry (
            agent_id TEXT PRIMARY KEY,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS audit_log (
            event_id INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS feature_entries (
            feature_id TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (feature_id, entry_id)
        );
        CREATE TABLE IF NOT EXISTS co_access (
            entry_id_a INTEGER NOT NULL,
            entry_id_b INTEGER NOT NULL,
            data BLOB NOT NULL,
            PRIMARY KEY (entry_id_a, entry_id_b),
            CHECK (entry_id_a < entry_id_b)
        );
        CREATE INDEX IF NOT EXISTS idx_co_access_b ON co_access(entry_id_b);
        CREATE TABLE IF NOT EXISTS outcome_index (
            feature_cycle TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (feature_cycle, entry_id)
        );
        CREATE TABLE IF NOT EXISTS observation_metrics (
            feature_cycle TEXT PRIMARY KEY,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS signal_queue (
            signal_id INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            session_id TEXT PRIMARY KEY,
            data BLOB NOT NULL
        );
        CREATE TABLE IF NOT EXISTS injection_log (
            log_id INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        );
    ").map_err(StoreError::Sqlite)?;

    // Initialize counters that migration expects
    conn.execute(
        "INSERT OR IGNORE INTO counters (name, value) VALUES ('schema_version', 5)",
        []
    ).map_err(StoreError::Sqlite)?;
    conn.execute(
        "INSERT OR IGNORE INTO counters (name, value) VALUES ('next_entry_id', 1)",
        []
    ).map_err(StoreError::Sqlite)?;
    conn.execute(
        "INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0)",
        []
    ).map_err(StoreError::Sqlite)?;
    conn.execute(
        "INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0)",
        []
    ).map_err(StoreError::Sqlite)?;
    conn.execute(
        "INSERT OR IGNORE INTO counters (name, value) VALUES ('next_audit_event_id', 0)",
        []
    ).map_err(StoreError::Sqlite)?;

    Ok(())
}
```

## Transaction Wrappers (ADR-001)

```rust
use std::sync::MutexGuard;

/// Read transaction wrapper for server compatibility.
pub struct SqliteReadTransaction<'a> {
    pub(crate) guard: MutexGuard<'a, Connection>,
}

/// Write transaction wrapper for server compatibility.
pub struct SqliteWriteTransaction<'a> {
    pub(crate) guard: MutexGuard<'a, Connection>,
}

// These provide open_table() methods that return SqliteTableHandle / SqliteMultimapTableHandle
// which expose get/insert/remove/iter methods using SQL.
// The server code opens AGENT_REGISTRY, AUDIT_LOG, FEATURE_ENTRIES, OUTCOME_INDEX.
```

## lib.rs Changes

```rust
#[cfg(not(feature = "backend-sqlite"))]
mod db;
#[cfg(not(feature = "backend-sqlite"))]
mod counter;
#[cfg(not(feature = "backend-sqlite"))]
mod write;
#[cfg(not(feature = "backend-sqlite"))]
mod read;
#[cfg(not(feature = "backend-sqlite"))]
mod query;
#[cfg(not(feature = "backend-sqlite"))]
pub mod sessions;
#[cfg(not(feature = "backend-sqlite"))]
pub mod injection_log;
#[cfg(not(feature = "backend-sqlite"))]
mod migration;

#[cfg(feature = "backend-sqlite")]
mod sqlite;

// Re-exports: Store comes from whichever backend is active
#[cfg(not(feature = "backend-sqlite"))]
pub use db::Store;
#[cfg(feature = "backend-sqlite")]
pub use sqlite::Store;

// Transaction type aliases (ADR-001)
#[cfg(not(feature = "backend-sqlite"))]
pub type ReadTransaction<'a> = redb::ReadTransaction;
#[cfg(not(feature = "backend-sqlite"))]
pub type WriteTransaction<'a> = redb::WriteTransaction;

#[cfg(feature = "backend-sqlite")]
pub type ReadTransaction<'a> = sqlite::db::SqliteReadTransaction<'a>;
#[cfg(feature = "backend-sqlite")]
pub type WriteTransaction<'a> = sqlite::db::SqliteWriteTransaction<'a>;

// Shared module exports remain unchanged
pub use schema::{...};  // all existing exports
pub use error::{StoreError, Result};
```

## error.rs Changes

```rust
#[derive(Debug)]
pub enum StoreError {
    EntryNotFound(u64),

    #[cfg(not(feature = "backend-sqlite"))]
    Database(redb::DatabaseError),
    #[cfg(not(feature = "backend-sqlite"))]
    Transaction(redb::TransactionError),
    #[cfg(not(feature = "backend-sqlite"))]
    Table(redb::TableError),
    #[cfg(not(feature = "backend-sqlite"))]
    Storage(redb::StorageError),
    #[cfg(not(feature = "backend-sqlite"))]
    Commit(redb::CommitError),
    #[cfg(not(feature = "backend-sqlite"))]
    Compaction(redb::CompactionError),

    #[cfg(feature = "backend-sqlite")]
    Sqlite(rusqlite::Error),

    Serialization(String),
    Deserialization(String),
    InvalidStatus(u8),
}
```

Display, Error, and From impls all get corresponding cfg gates.

## test_helpers.rs Changes

```rust
pub struct TestDb {
    _dir: tempfile::TempDir,
    store: Store,
}

impl TestDb {
    pub fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        #[cfg(not(feature = "backend-sqlite"))]
        let path = dir.path().join("test.redb");
        #[cfg(feature = "backend-sqlite")]
        let path = dir.path().join("test.db");
        let store = Store::open(&path).expect("failed to open test database");
        TestDb { _dir: dir, store }
    }
}
```
