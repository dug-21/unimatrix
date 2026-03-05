//! Schema migration for the SQLite backend.
//!
//! Fresh SQLite databases start at schema v6 (all tables created by
//! `create_tables`). Migration is needed when opening an existing
//! database created at an older schema version.

use std::path::Path;

use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::migration_compat;
use crate::schema::{deserialize_entry, serialize_entry};

use crate::db::Store;

/// Current schema version.
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 7;

/// Run migration if schema_version is behind CURRENT_SCHEMA_VERSION.
/// Called from Store::open() after table creation.
pub(crate) fn migrate_if_needed(store: &Store, db_path: &Path) -> Result<()> {
    let conn = store.lock_conn();

    // Check if this is a fresh database (no entries table yet).
    // Fresh databases are initialized by create_tables, not migration.
    let has_entries_table: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='entries'",
            [],
            |row| Ok(row.get::<_, i64>(0)? > 0),
        )
        .unwrap_or(false);

    if !has_entries_table {
        return Ok(());
    }

    let current_version: u64 = conn
        .query_row(
            "SELECT value FROM counters WHERE name = 'schema_version'",
            [],
            |row| Ok(row.get::<_, i64>(0)? as u64),
        )
        .optional()
        .map_err(StoreError::Sqlite)?
        .unwrap_or(0);

    if current_version >= CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(StoreError::Sqlite)?;

    let result = (|| -> Result<()> {
        // Entry-rewriting migrations: if starting from v0, v1, or v2,
        // attempt to re-serialize all entries to current format.
        if current_version <= 2 {
            migrate_entries_to_current_schema(&conn)?;
        }

        // Table-creation migrations (idempotent -- tables already exist via create_tables)
        if current_version < 4 {
            conn.execute(
                "INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0)",
                [],
            )
            .map_err(StoreError::Sqlite)?;
        }

        if current_version < 5 {
            conn.execute(
                "INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0)",
                [],
            )
            .map_err(StoreError::Sqlite)?;
        }

        // v5 -> v6: full schema normalization
        if current_version <= 5 {
            // Backup is done outside the transaction since it's a file copy
            // We need to commit the current txn, backup, then re-start
        }

        // v6 -> v7: observations table (col-012)
        if current_version < 7 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS observations (
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
                CREATE INDEX IF NOT EXISTS idx_observations_ts ON observations(ts_millis);"
            ).map_err(StoreError::Sqlite)?;
        }

        // Update schema version
        conn.execute(
            "INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)",
            rusqlite::params![CURRENT_SCHEMA_VERSION as i64],
        )
        .map_err(StoreError::Sqlite)?;

        Ok(())
    })();

    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;

            // If upgrading from v5, run the normalization migration after the
            // version bump transaction. The v5->v6 migration needs its own
            // transaction because it drops and renames tables.
            if current_version <= 5 && current_version > 0 {
                // Check if old-style blob tables still exist
                let has_blob_entries: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM pragma_table_info('entries') WHERE name = 'data'",
                        [],
                        |row| Ok(row.get::<_, i64>(0)? > 0),
                    )
                    .unwrap_or(false);

                if has_blob_entries {
                    migrate_v5_to_v6(&conn, db_path)?;
                }
            }

            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Migrate database from schema v5 (bincode blobs) to v6 (SQL columns).
/// Creates backup at {path}.v5-backup before starting.
/// Runs in a single transaction.
fn migrate_v5_to_v6(conn: &rusqlite::Connection, db_path: &Path) -> Result<()> {
    // Step 1: Backup database file
    let path_str = db_path.to_str().unwrap_or("");
    if !path_str.is_empty() && path_str != ":memory:" {
        let backup_path = format!("{}.v5-backup", path_str);
        std::fs::copy(db_path, &backup_path)
            .map_err(|e| StoreError::Deserialization(format!("backup failed: {e}")))?;
    }

    conn.execute_batch("PRAGMA foreign_keys = ON")
        .map_err(StoreError::Sqlite)?;

    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(StoreError::Sqlite)?;

    let result = (|| -> Result<()> {
        // Step 2: Create new tables with _v6 suffix
        conn.execute_batch(
            "CREATE TABLE entries_v6 (
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
                unhelpful_count INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS entry_tags (
                entry_id INTEGER NOT NULL,
                tag      TEXT    NOT NULL,
                PRIMARY KEY (entry_id, tag),
                FOREIGN KEY (entry_id) REFERENCES entries_v6(id) ON DELETE CASCADE
            );
            CREATE TABLE co_access_v6 (
                entry_id_a   INTEGER NOT NULL,
                entry_id_b   INTEGER NOT NULL,
                count        INTEGER NOT NULL DEFAULT 1,
                last_updated INTEGER NOT NULL,
                PRIMARY KEY (entry_id_a, entry_id_b),
                CHECK (entry_id_a < entry_id_b)
            );
            CREATE TABLE sessions_v6 (
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
            CREATE TABLE injection_log_v6 (
                log_id     INTEGER PRIMARY KEY,
                session_id TEXT    NOT NULL,
                entry_id   INTEGER NOT NULL,
                confidence REAL    NOT NULL,
                timestamp  INTEGER NOT NULL
            );
            CREATE TABLE signal_queue_v6 (
                signal_id     INTEGER PRIMARY KEY,
                session_id    TEXT    NOT NULL,
                created_at    INTEGER NOT NULL,
                entry_ids     TEXT    NOT NULL DEFAULT '[]',
                signal_type   INTEGER NOT NULL,
                signal_source INTEGER NOT NULL
            );
            CREATE TABLE agent_registry_v6 (
                agent_id           TEXT    PRIMARY KEY,
                trust_level        INTEGER NOT NULL,
                capabilities       TEXT    NOT NULL DEFAULT '[]',
                allowed_topics     TEXT,
                allowed_categories TEXT,
                enrolled_at        INTEGER NOT NULL,
                last_seen_at       INTEGER NOT NULL,
                active             INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE audit_log_v6 (
                event_id   INTEGER PRIMARY KEY,
                timestamp  INTEGER NOT NULL,
                session_id TEXT    NOT NULL,
                agent_id   TEXT    NOT NULL,
                operation  TEXT    NOT NULL,
                target_ids TEXT    NOT NULL DEFAULT '[]',
                outcome    INTEGER NOT NULL,
                detail     TEXT    NOT NULL DEFAULT ''
            );"
        ).map_err(StoreError::Sqlite)?;

        // Step 3: Migrate entries
        migrate_entries_v5_to_v6(conn)?;

        // Step 4: Migrate co_access
        migrate_co_access_v5_to_v6(conn)?;

        // Step 5: Migrate sessions
        migrate_sessions_v5_to_v6(conn)?;

        // Step 6: Migrate injection_log
        migrate_injection_log_v5_to_v6(conn)?;

        // Step 7: Migrate signal_queue
        migrate_signal_queue_v5_to_v6(conn)?;

        // Step 8: Migrate agent_registry
        migrate_agent_registry_v5_to_v6(conn)?;

        // Step 9: Migrate audit_log
        migrate_audit_log_v5_to_v6(conn)?;

        // Step 10: Drop old tables
        conn.execute_batch(
            "DROP TABLE entries;
             DROP TABLE IF EXISTS topic_index;
             DROP TABLE IF EXISTS category_index;
             DROP TABLE IF EXISTS tag_index;
             DROP TABLE IF EXISTS time_index;
             DROP TABLE IF EXISTS status_index;
             DROP TABLE co_access;
             DROP TABLE sessions;
             DROP TABLE injection_log;
             DROP TABLE signal_queue;
             DROP TABLE agent_registry;
             DROP TABLE audit_log;"
        ).map_err(StoreError::Sqlite)?;

        // Step 11: Rename new tables
        conn.execute_batch(
            "ALTER TABLE entries_v6 RENAME TO entries;
             ALTER TABLE co_access_v6 RENAME TO co_access;
             ALTER TABLE sessions_v6 RENAME TO sessions;
             ALTER TABLE injection_log_v6 RENAME TO injection_log;
             ALTER TABLE signal_queue_v6 RENAME TO signal_queue;
             ALTER TABLE agent_registry_v6 RENAME TO agent_registry;
             ALTER TABLE audit_log_v6 RENAME TO audit_log;"
        ).map_err(StoreError::Sqlite)?;

        // Step 12: Create indexes
        conn.execute_batch(
            "CREATE INDEX idx_entries_topic ON entries(topic);
             CREATE INDEX idx_entries_category ON entries(category);
             CREATE INDEX idx_entries_status ON entries(status);
             CREATE INDEX idx_entries_created_at ON entries(created_at);
             CREATE INDEX idx_entry_tags_tag ON entry_tags(tag);
             CREATE INDEX idx_entry_tags_entry_id ON entry_tags(entry_id);
             CREATE INDEX idx_co_access_b ON co_access(entry_id_b);
             CREATE INDEX idx_sessions_feature_cycle ON sessions(feature_cycle);
             CREATE INDEX idx_sessions_started_at ON sessions(started_at);
             CREATE INDEX idx_injection_log_session ON injection_log(session_id);
             CREATE INDEX idx_injection_log_entry ON injection_log(entry_id);
             CREATE INDEX idx_audit_log_agent ON audit_log(agent_id);
             CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);"
        ).map_err(StoreError::Sqlite)?;

        // Step 13: Update schema version
        conn.execute(
            "INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', 6)",
            [],
        ).map_err(StoreError::Sqlite)?;

        Ok(())
    })();

    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn migrate_entries_v5_to_v6(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT id, data FROM entries")
        .map_err(StoreError::Sqlite)?;
    let rows: Vec<(i64, Vec<u8>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(StoreError::Sqlite)?
        .collect::<rusqlite::Result<_>>()
        .map_err(StoreError::Sqlite)?;
    drop(stmt);

    let mut insert_entry = conn
        .prepare(
            "INSERT INTO entries_v6 (id, title, content, topic, category, source,
                status, confidence, created_at, updated_at, last_accessed_at,
                access_count, supersedes, superseded_by, correction_count,
                embedding_dim, created_by, modified_by, content_hash,
                previous_hash, version, feature_cycle, trust_source,
                helpful_count, unhelpful_count)
            VALUES (:id, :title, :content, :topic, :category, :source,
                :status, :confidence, :created_at, :updated_at, :last_accessed_at,
                :access_count, :supersedes, :superseded_by, :correction_count,
                :embedding_dim, :created_by, :modified_by, :content_hash,
                :previous_hash, :version, :feature_cycle, :trust_source,
                :helpful_count, :unhelpful_count)"
        )
        .map_err(StoreError::Sqlite)?;

    let mut insert_tag = conn
        .prepare("INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)")
        .map_err(StoreError::Sqlite)?;

    for (id, data) in &rows {
        let record = migration_compat::deserialize_entry_v5(data)?;
        insert_entry
            .execute(rusqlite::named_params! {
                ":id": *id,
                ":title": &record.title,
                ":content": &record.content,
                ":topic": &record.topic,
                ":category": &record.category,
                ":source": &record.source,
                ":status": record.status as u8 as i64,
                ":confidence": record.confidence,
                ":created_at": record.created_at as i64,
                ":updated_at": record.updated_at as i64,
                ":last_accessed_at": record.last_accessed_at as i64,
                ":access_count": record.access_count as i64,
                ":supersedes": record.supersedes.map(|v| v as i64),
                ":superseded_by": record.superseded_by.map(|v| v as i64),
                ":correction_count": record.correction_count as i64,
                ":embedding_dim": record.embedding_dim as i64,
                ":created_by": &record.created_by,
                ":modified_by": &record.modified_by,
                ":content_hash": &record.content_hash,
                ":previous_hash": &record.previous_hash,
                ":version": record.version as i64,
                ":feature_cycle": &record.feature_cycle,
                ":trust_source": &record.trust_source,
                ":helpful_count": record.helpful_count as i64,
                ":unhelpful_count": record.unhelpful_count as i64,
            })
            .map_err(StoreError::Sqlite)?;

        for tag in &record.tags {
            insert_tag
                .execute(rusqlite::params![*id, tag])
                .map_err(StoreError::Sqlite)?;
        }
    }

    Ok(())
}

fn migrate_co_access_v5_to_v6(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT entry_id_a, entry_id_b, data FROM co_access")
        .map_err(StoreError::Sqlite)?;
    let rows: Vec<(i64, i64, Vec<u8>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(StoreError::Sqlite)?
        .collect::<rusqlite::Result<_>>()
        .map_err(StoreError::Sqlite)?;
    drop(stmt);

    for (a, b, data) in &rows {
        let record = migration_compat::deserialize_co_access_v5(data)?;
        conn.execute(
            "INSERT INTO co_access_v6 (entry_id_a, entry_id_b, count, last_updated)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![a, b, record.count as i64, record.last_updated as i64],
        )
        .map_err(StoreError::Sqlite)?;
    }

    Ok(())
}

fn migrate_sessions_v5_to_v6(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT session_id, data FROM sessions")
        .map_err(StoreError::Sqlite)?;
    let rows: Vec<(String, Vec<u8>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(StoreError::Sqlite)?
        .collect::<rusqlite::Result<_>>()
        .map_err(StoreError::Sqlite)?;
    drop(stmt);

    for (session_id, data) in &rows {
        let record = migration_compat::deserialize_session_v5(data)?;
        let status_int = match record.status {
            crate::sessions::SessionLifecycleStatus::Active => 0_i64,
            crate::sessions::SessionLifecycleStatus::Completed => 1,
            crate::sessions::SessionLifecycleStatus::TimedOut => 2,
            crate::sessions::SessionLifecycleStatus::Abandoned => 3,
        };
        conn.execute(
            "INSERT INTO sessions_v6 (session_id, feature_cycle, agent_role,
                started_at, ended_at, status, compaction_count, outcome, total_injections)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                session_id,
                &record.feature_cycle,
                &record.agent_role,
                record.started_at as i64,
                record.ended_at.map(|v| v as i64),
                status_int,
                record.compaction_count as i64,
                &record.outcome,
                record.total_injections as i64,
            ],
        )
        .map_err(StoreError::Sqlite)?;
    }

    Ok(())
}

fn migrate_injection_log_v5_to_v6(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT log_id, data FROM injection_log")
        .map_err(StoreError::Sqlite)?;
    let rows: Vec<(i64, Vec<u8>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(StoreError::Sqlite)?
        .collect::<rusqlite::Result<_>>()
        .map_err(StoreError::Sqlite)?;
    drop(stmt);

    for (log_id, data) in &rows {
        let record = migration_compat::deserialize_injection_log_v5(data)?;
        conn.execute(
            "INSERT INTO injection_log_v6 (log_id, session_id, entry_id, confidence, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                log_id,
                &record.session_id,
                record.entry_id as i64,
                record.confidence,
                record.timestamp as i64,
            ],
        )
        .map_err(StoreError::Sqlite)?;
    }

    Ok(())
}

fn migrate_signal_queue_v5_to_v6(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT signal_id, data FROM signal_queue")
        .map_err(StoreError::Sqlite)?;
    let rows: Vec<(i64, Vec<u8>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(StoreError::Sqlite)?
        .collect::<rusqlite::Result<_>>()
        .map_err(StoreError::Sqlite)?;
    drop(stmt);

    for (signal_id, data) in &rows {
        let record = migration_compat::deserialize_signal_v5(data)?;
        let entry_ids_json = serde_json::to_string(&record.entry_ids)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        conn.execute(
            "INSERT INTO signal_queue_v6 (signal_id, session_id, created_at, entry_ids, signal_type, signal_source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                signal_id,
                &record.session_id,
                record.created_at as i64,
                &entry_ids_json,
                record.signal_type as u8 as i64,
                record.signal_source as u8 as i64,
            ],
        )
        .map_err(StoreError::Sqlite)?;
    }

    Ok(())
}

fn migrate_agent_registry_v5_to_v6(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT agent_id, data FROM agent_registry")
        .map_err(StoreError::Sqlite)?;
    let rows: Vec<(String, Vec<u8>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(StoreError::Sqlite)?
        .collect::<rusqlite::Result<_>>()
        .map_err(StoreError::Sqlite)?;
    drop(stmt);

    for (agent_id, data) in &rows {
        let record = migration_compat::deserialize_agent_v5(data)?;
        let cap_ints: Vec<u8> = record.capabilities.iter().map(|c| *c as u8).collect();
        let caps_json = serde_json::to_string(&cap_ints)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let topics_json: Option<String> = record
            .allowed_topics
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let cats_json: Option<String> = record
            .allowed_categories
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| StoreError::Serialization(e.to_string()))?;
        let trust_int = record.trust_level as u8 as i64;

        conn.execute(
            "INSERT INTO agent_registry_v6 (agent_id, trust_level, capabilities,
                allowed_topics, allowed_categories, enrolled_at, last_seen_at, active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                agent_id,
                trust_int,
                &caps_json,
                &topics_json,
                &cats_json,
                record.enrolled_at as i64,
                record.last_seen_at as i64,
                if record.active { 1_i64 } else { 0_i64 },
            ],
        )
        .map_err(StoreError::Sqlite)?;
    }

    Ok(())
}

fn migrate_audit_log_v5_to_v6(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT event_id, data FROM audit_log")
        .map_err(StoreError::Sqlite)?;
    let rows: Vec<(i64, Vec<u8>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(StoreError::Sqlite)?
        .collect::<rusqlite::Result<_>>()
        .map_err(StoreError::Sqlite)?;
    drop(stmt);

    for (event_id, data) in &rows {
        let record = migration_compat::deserialize_audit_event_v5(data)?;
        let target_ids_json = serde_json::to_string(&record.target_ids)
            .map_err(|e| StoreError::Serialization(e.to_string()))?;

        conn.execute(
            "INSERT INTO audit_log_v6 (event_id, timestamp, session_id, agent_id,
                operation, target_ids, outcome, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                event_id,
                record.timestamp as i64,
                &record.session_id,
                &record.agent_id,
                &record.operation,
                &target_ids_json,
                record.outcome as u8 as i64,
                &record.detail,
            ],
        )
        .map_err(StoreError::Sqlite)?;
    }

    Ok(())
}

/// Re-serialize all entries to current EntryRecord format.
/// Used for v0-v2 migration path.
fn migrate_entries_to_current_schema(conn: &rusqlite::Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT id, data FROM entries")
        .map_err(StoreError::Sqlite)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)? as u64, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(StoreError::Sqlite)?;

    let mut updates: Vec<(u64, Vec<u8>)> = Vec::new();
    for row in rows {
        let (id, bytes) = row.map_err(StoreError::Sqlite)?;
        match deserialize_entry(&bytes) {
            Ok(_record) => {
                let new_bytes = serialize_entry(&_record)?;
                if new_bytes != bytes {
                    updates.push((id, new_bytes));
                }
            }
            Err(_) => {
                continue;
            }
        }
    }
    drop(stmt);

    for (id, bytes) in updates {
        conn.execute(
            "UPDATE entries SET data = ?1 WHERE id = ?2",
            rusqlite::params![bytes, id as i64],
        )
        .map_err(StoreError::Sqlite)?;
    }

    Ok(())
}
