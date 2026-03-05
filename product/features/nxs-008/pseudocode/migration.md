# Component: migration (Wave 0)

## File: `crates/unimatrix-store/src/migration.rs`

**Action**: EXTEND (add `migrate_v5_to_v6`)
**Risk**: HIGH (RISK-01, CRITICAL)
**ADR**: ADR-005, ADR-008

## Purpose

Add v5-to-v6 migration: deserialize all bincode blobs across 7 tables, INSERT as SQL columns into new `_v6` tables, drop old tables, rename, create indexes. Update schema version to 6. Backup database file before starting.

## Constants Change

```rust
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 6;  // was 5
```

## Pseudocode: migrate_v5_to_v6

```rust
/// Migrate database from schema v5 (bincode blobs) to v6 (SQL columns).
/// Creates backup at {path}.v5-backup before starting.
/// Runs in single transaction.
pub(crate) fn migrate_v5_to_v6(conn: &Connection, db_path: &Path) -> Result<()> {
    // Step 1: Backup database file
    let backup_path = db_path.with_extension("db.v5-backup");
    // We need the path from Store::open - passed through migrate_if_needed
    // For in-memory databases (tests), skip backup
    if db_path.to_str() != Some(":memory:") {
        std::fs::copy(db_path, &backup_path)
            .map_err(|e| StoreError::Migration(format!("backup failed: {e}")))?;
    }

    // Step 2: Enable foreign keys for this connection
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .map_err(StoreError::Sqlite)?;

    // Step 3: Create new tables with _v6 suffix
    conn.execute_batch("
        CREATE TABLE entries_v6 (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            ... -- all 24 columns per Architecture
        );
        CREATE TABLE entry_tags (
            entry_id INTEGER NOT NULL,
            tag TEXT NOT NULL,
            PRIMARY KEY (entry_id, tag),
            FOREIGN KEY (entry_id) REFERENCES entries_v6(id) ON DELETE CASCADE
        );
        CREATE TABLE co_access_v6 (
            entry_id_a INTEGER NOT NULL,
            entry_id_b INTEGER NOT NULL,
            count INTEGER NOT NULL DEFAULT 1,
            last_updated INTEGER NOT NULL,
            PRIMARY KEY (entry_id_a, entry_id_b),
            CHECK (entry_id_a < entry_id_b)
        );
        CREATE TABLE sessions_v6 ( ... 9 columns ... );
        CREATE TABLE injection_log_v6 ( ... 5 columns ... );
        CREATE TABLE signal_queue_v6 ( ... 6 columns ... );
        CREATE TABLE agent_registry_v6 ( ... 8 columns ... );
        CREATE TABLE audit_log_v6 ( ... 8 columns ... );
    ").map_err(StoreError::Sqlite)?;

    // Step 4: Migrate entries
    {
        let mut stmt = conn.prepare("SELECT id, data FROM entries")?;
        let rows: Vec<(i64, Vec<u8>)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?.collect::<rusqlite::Result<_>>()?;
        drop(stmt);

        let mut insert_entry = conn.prepare("
            INSERT INTO entries_v6 (id, title, content, topic, category, source,
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
                :helpful_count, :unhelpful_count)
        ")?;
        let mut insert_tag = conn.prepare(
            "INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)"
        )?;

        for (id, data) in &rows {
            let record = migration_compat::deserialize_entry_v5(data)?;
            insert_entry.execute(named_params! {
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
            })?;
            for tag in &record.tags {
                insert_tag.execute(rusqlite::params![*id, tag])?;
            }
        }
    }

    // Step 5: Migrate co_access
    {
        let mut stmt = conn.prepare("SELECT entry_id_a, entry_id_b, data FROM co_access")?;
        let rows: Vec<(i64, i64, Vec<u8>)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?.collect::<rusqlite::Result<_>>()?;
        drop(stmt);

        for (a, b, data) in &rows {
            let record = migration_compat::deserialize_co_access_v5(data)?;
            conn.execute(
                "INSERT INTO co_access_v6 (entry_id_a, entry_id_b, count, last_updated)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![a, b, record.count as i64, record.last_updated as i64],
            )?;
        }
    }

    // Step 6: Migrate sessions, injection_log, signal_queue (similar pattern)
    // Each: SELECT old -> deserialize -> INSERT new with SQL columns
    // signal_queue.entry_ids -> serde_json::to_string(&record.entry_ids)

    // Step 7: Migrate agent_registry, audit_log
    // agent_registry: capabilities -> JSON array, allowed_* -> JSON or NULL
    // audit_log: target_ids -> JSON array, outcome -> integer

    // Step 8: Drop old tables
    conn.execute_batch("
        DROP TABLE entries;
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
        DROP TABLE audit_log;
    ")?;

    // Step 9: Rename new tables
    conn.execute_batch("
        ALTER TABLE entries_v6 RENAME TO entries;
        ALTER TABLE co_access_v6 RENAME TO co_access;
        ALTER TABLE sessions_v6 RENAME TO sessions;
        ALTER TABLE injection_log_v6 RENAME TO injection_log;
        ALTER TABLE signal_queue_v6 RENAME TO signal_queue;
        ALTER TABLE agent_registry_v6 RENAME TO agent_registry;
        ALTER TABLE audit_log_v6 RENAME TO audit_log;
    ")?;

    // Step 10: Create indexes
    conn.execute_batch("
        CREATE INDEX idx_entries_topic ON entries(topic);
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
        CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);
    ")?;

    // Step 11: Update schema version
    conn.execute(
        "INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', 6)",
        [],
    )?;

    Ok(())
}
```

## Changes to migrate_if_needed

```rust
pub(crate) fn migrate_if_needed(store: &Store, db_path: &Path) -> Result<()> {
    // ... existing version check ...

    if current_version < 6 {
        // Run existing v0-v2 migrations first
        if current_version <= 2 {
            migrate_entries_to_current_schema(&conn)?;
        }
        // ... existing v3, v4 counter init ...

        // v5 -> v6: full schema normalization
        if current_version == 5 || current_version < 5 {
            migrate_v5_to_v6(&conn, db_path)?;
        }
    }
}
```

**CRITICAL**: `migrate_if_needed` must receive `db_path` from `Store::open` to create the backup. The `Store::open_with_config` method needs to pass the path through.

## Changes to db.rs

`Store::open_with_config` must pass the path to `migrate_if_needed`:
```rust
crate::migration::migrate_if_needed(&store, path.as_ref())?;
```

## Changes to create_tables (db.rs)

Fresh databases (v6) get the new DDL directly. The old `CREATE TABLE entries (id, data)` is replaced with the 24-column version. See `schema-ddl` component for details.
