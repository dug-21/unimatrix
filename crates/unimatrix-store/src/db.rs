use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::error::{Result, StoreError};
use crate::schema::DatabaseConfig;

pub use crate::txn::SqliteWriteTransaction;

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
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA cache_size = -16384;",
        )
        .map_err(StoreError::Sqlite)?;

        // Ensure counters table exists so migration can read schema_version
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS counters (
                name TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );"
        ).map_err(StoreError::Sqlite)?;

        let store = Store {
            conn: Mutex::new(conn),
        };

        // Run schema migration BEFORE creating v6 tables/indexes,
        // since the existing v5 entries table has (id, data) not 24 columns.
        crate::migration::migrate_if_needed(&store, path.as_ref())?;

        // Create all tables (idempotent) — safe now that migration has
        // transformed any v5 tables to v6 schema.
        create_tables(&*store.lock_conn())?;

        Ok(store)
    }

    /// Compact the database file.
    ///
    /// For SQLite with WAL mode, this is a no-op. WAL auto-checkpoint
    /// handles space management (ADR-003).
    pub fn compact(&mut self) -> Result<()> {
        Ok(())
    }

    /// Begin a write transaction.
    ///
    /// Returns a wrapper around the connection mutex guard that exposes
    /// table-access methods compatible with server usage patterns (ADR-001).
    pub fn begin_write(&self) -> Result<SqliteWriteTransaction<'_>> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        SqliteWriteTransaction::new(guard)
    }

    /// Acquire the connection lock for direct SQL access.
    pub fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entries (
            id              INTEGER PRIMARY KEY,
            title           TEXT    NOT NULL,
            content         TEXT    NOT NULL,
            topic           TEXT    NOT NULL,
            category        TEXT    NOT NULL,
            source          TEXT    NOT NULL,
            status          INTEGER NOT NULL DEFAULT 0,
            confidence      REAL    NOT NULL DEFAULT 0.0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            last_accessed_at INTEGER NOT NULL DEFAULT 0,
            access_count    INTEGER NOT NULL DEFAULT 0,
            supersedes      INTEGER,
            superseded_by   INTEGER,
            correction_count INTEGER NOT NULL DEFAULT 0,
            embedding_dim   INTEGER NOT NULL DEFAULT 0,
            created_by      TEXT    NOT NULL DEFAULT '',
            modified_by     TEXT    NOT NULL DEFAULT '',
            content_hash    TEXT    NOT NULL DEFAULT '',
            previous_hash   TEXT    NOT NULL DEFAULT '',
            version         INTEGER NOT NULL DEFAULT 0,
            feature_cycle   TEXT    NOT NULL DEFAULT '',
            trust_source    TEXT    NOT NULL DEFAULT '',
            helpful_count   INTEGER NOT NULL DEFAULT 0,
            unhelpful_count INTEGER NOT NULL DEFAULT 0,
            pre_quarantine_status INTEGER
        );
        CREATE TABLE IF NOT EXISTS entry_tags (
            entry_id INTEGER NOT NULL,
            tag      TEXT    NOT NULL,
            PRIMARY KEY (entry_id, tag),
            FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_entries_topic      ON entries(topic);
        CREATE INDEX IF NOT EXISTS idx_entries_category   ON entries(category);
        CREATE INDEX IF NOT EXISTS idx_entries_status     ON entries(status);
        CREATE INDEX IF NOT EXISTS idx_entries_created_at ON entries(created_at);
        CREATE INDEX IF NOT EXISTS idx_entry_tags_tag      ON entry_tags(tag);
        CREATE INDEX IF NOT EXISTS idx_entry_tags_entry_id ON entry_tags(entry_id);
        CREATE TABLE IF NOT EXISTS vector_map (
            entry_id INTEGER PRIMARY KEY,
            hnsw_data_id INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS counters (
            name TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS feature_entries (
            feature_id TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (feature_id, entry_id)
        );
        CREATE TABLE IF NOT EXISTS co_access (
            entry_id_a   INTEGER NOT NULL,
            entry_id_b   INTEGER NOT NULL,
            count        INTEGER NOT NULL DEFAULT 1,
            last_updated INTEGER NOT NULL,
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
            signal_id     INTEGER PRIMARY KEY,
            session_id    TEXT    NOT NULL,
            created_at    INTEGER NOT NULL,
            entry_ids     TEXT    NOT NULL DEFAULT '[]',
            signal_type   INTEGER NOT NULL,
            signal_source INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS sessions (
            session_id       TEXT    PRIMARY KEY,
            feature_cycle    TEXT,
            agent_role       TEXT,
            started_at       INTEGER NOT NULL,
            ended_at         INTEGER,
            status           INTEGER NOT NULL DEFAULT 0,
            compaction_count INTEGER NOT NULL DEFAULT 0,
            outcome          TEXT,
            total_injections INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_sessions_feature_cycle ON sessions(feature_cycle);
        CREATE INDEX IF NOT EXISTS idx_sessions_started_at    ON sessions(started_at);
        CREATE TABLE IF NOT EXISTS injection_log (
            log_id     INTEGER PRIMARY KEY,
            session_id TEXT    NOT NULL,
            entry_id   INTEGER NOT NULL,
            confidence REAL    NOT NULL,
            timestamp  INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_injection_log_session ON injection_log(session_id);
        CREATE INDEX IF NOT EXISTS idx_injection_log_entry   ON injection_log(entry_id);
        CREATE TABLE IF NOT EXISTS agent_registry (
            agent_id           TEXT    PRIMARY KEY,
            trust_level        INTEGER NOT NULL,
            capabilities       TEXT    NOT NULL DEFAULT '[]',
            allowed_topics     TEXT,
            allowed_categories TEXT,
            enrolled_at        INTEGER NOT NULL,
            last_seen_at       INTEGER NOT NULL,
            active             INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS audit_log (
            event_id   INTEGER PRIMARY KEY,
            timestamp  INTEGER NOT NULL,
            session_id TEXT    NOT NULL,
            agent_id   TEXT    NOT NULL,
            operation  TEXT    NOT NULL,
            target_ids TEXT    NOT NULL DEFAULT '[]',
            outcome    INTEGER NOT NULL,
            detail     TEXT    NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_audit_log_agent     ON audit_log(agent_id);
        CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp ON audit_log(timestamp);
        CREATE TABLE IF NOT EXISTS observations (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id      TEXT    NOT NULL,
            ts_millis       INTEGER NOT NULL,
            hook            TEXT    NOT NULL,
            tool            TEXT,
            input           TEXT,
            response_size   INTEGER,
            response_snippet TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id);
        CREATE INDEX IF NOT EXISTS idx_observations_ts ON observations(ts_millis);
        CREATE TABLE IF NOT EXISTS shadow_evaluations (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp         INTEGER NOT NULL,
            rule_name         TEXT    NOT NULL,
            rule_category     TEXT    NOT NULL,
            neural_category   TEXT    NOT NULL,
            neural_confidence REAL    NOT NULL,
            convention_score  REAL    NOT NULL,
            rule_accepted     INTEGER NOT NULL,
            digest            BLOB
        );
        CREATE INDEX IF NOT EXISTS idx_shadow_eval_ts ON shadow_evaluations(timestamp);",
    )
    .map_err(StoreError::Sqlite)?;

    // Initialize counters that other modules expect
    conn.execute_batch(
        "INSERT OR IGNORE INTO counters (name, value) VALUES ('schema_version', 8);
         INSERT OR IGNORE INTO counters (name, value) VALUES ('next_entry_id', 1);
         INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0);
         INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0);
         INSERT OR IGNORE INTO counters (name, value) VALUES ('next_audit_event_id', 0);",
    )
    .map_err(StoreError::Sqlite)?;

    Ok(())
}
