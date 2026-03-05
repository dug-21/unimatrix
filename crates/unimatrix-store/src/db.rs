use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::error::{Result, StoreError};
use crate::schema::DatabaseConfig;

pub use crate::txn::{SqliteReadTransaction, SqliteWriteTransaction};

/// The storage engine handle. Wraps a Mutex<rusqlite::Connection>.
///
/// `Store` is `Send + Sync` and shareable via `Arc<Store>`.
/// All read/write operations are methods on this struct.
pub struct Store {
    pub(crate) conn: Mutex<Connection>,
}

impl Store {
    /// Open or create a database at the given path with default configuration.
    ///
    /// All 17 tables are created if they don't already exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_config(path, DatabaseConfig::default())
    }

    /// Open or create a database at the given path with custom configuration.
    ///
    /// All 17 tables are created if they don't already exist.
    pub fn open_with_config(path: impl AsRef<Path>, _config: DatabaseConfig) -> Result<Self> {
        let conn = Connection::open(path.as_ref()).map_err(StoreError::Sqlite)?;

        // Configure PRAGMAs (ADR-003)
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA wal_autocheckpoint = 1000;
             PRAGMA foreign_keys = OFF;
             PRAGMA busy_timeout = 5000;
             PRAGMA cache_size = -16384;",
        )
        .map_err(StoreError::Sqlite)?;

        // Create all 17 tables (idempotent)
        create_tables(&conn)?;

        let store = Store {
            conn: Mutex::new(conn),
        };

        // Run schema migration if needed
        crate::migration::migrate_if_needed(&store)?;

        Ok(store)
    }

    /// Compact the database file.
    ///
    /// For SQLite with WAL mode, this is a no-op. WAL auto-checkpoint
    /// handles space management (ADR-003).
    pub fn compact(&mut self) -> Result<()> {
        Ok(())
    }

    /// Begin a read transaction.
    ///
    /// Returns a wrapper around the connection mutex guard that exposes
    /// table-access methods compatible with server usage patterns (ADR-001).
    pub fn begin_read(&self) -> Result<SqliteReadTransaction<'_>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(SqliteReadTransaction { guard })
    }

    /// Begin a write transaction.
    ///
    /// Returns a wrapper around the connection mutex guard that exposes
    /// table-access methods compatible with server usage patterns (ADR-001).
    pub fn begin_write(&self) -> Result<SqliteWriteTransaction<'_>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        SqliteWriteTransaction::new(guard)
    }

    /// Acquire the connection lock for internal operations.
    pub(crate) fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entries (
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
        );",
    )
    .map_err(StoreError::Sqlite)?;

    // Initialize counters that other modules expect
    conn.execute_batch(
        "INSERT OR IGNORE INTO counters (name, value) VALUES ('schema_version', 5);
         INSERT OR IGNORE INTO counters (name, value) VALUES ('next_entry_id', 1);
         INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0);
         INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0);
         INSERT OR IGNORE INTO counters (name, value) VALUES ('next_audit_event_id', 0);",
    )
    .map_err(StoreError::Sqlite)?;

    Ok(())
}
