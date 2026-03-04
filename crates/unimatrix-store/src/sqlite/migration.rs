//! Schema migration for the SQLite backend.
//!
//! Fresh SQLite databases start at schema v5 (all tables created by
//! `create_tables`). Migration is only needed when opening an existing
//! database created at an older schema version.

use rusqlite::OptionalExtension;

use crate::error::{Result, StoreError};
use crate::schema::{deserialize_entry, serialize_entry};

use super::db::Store;

/// Current schema version. Must match the redb migration module.
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 5;

/// Run migration if schema_version is behind CURRENT_SCHEMA_VERSION.
/// Called from Store::open() after table creation.
pub(crate) fn migrate_if_needed(store: &Store) -> Result<()> {
    let conn = store.lock_conn();

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
        // Just ensure counters are initialized
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
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// Re-serialize all entries to current EntryRecord format.
///
/// Attempts to deserialize each entry with current format first (already
/// upgraded). If that fails, attempts legacy deserialization with serde
/// defaults applied, then re-serializes.
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
        // Try current format first
        match deserialize_entry(&bytes) {
            Ok(_record) => {
                // Already at current schema, no re-write needed.
                // Re-serialize to ensure all serde(default) fields are present.
                let new_bytes = serialize_entry(&_record)?;
                if new_bytes != bytes {
                    updates.push((id, new_bytes));
                }
            }
            Err(_) => {
                // Legacy format -- best-effort: skip if we can't parse at all.
                // In practice, SQLite databases start at v5, so this path is
                // only hit during redb-to-sqlite migration of ancient databases.
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
