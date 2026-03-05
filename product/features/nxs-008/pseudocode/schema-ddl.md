# Component: schema-ddl (Wave 1)

## Files Modified

- `crates/unimatrix-store/src/db.rs` - DDL rewrite, PRAGMA foreign_keys ON
- `crates/unimatrix-store/src/schema.rs` - Remove runtime bincode helpers from re-exports, add server types
- `crates/unimatrix-store/src/read.rs` - entry_from_row(), load_tags_for_entries()

**Risk**: Medium (DDL), HIGH for entry_from_row (RISK-02, RISK-04)
**ADR**: ADR-004, ADR-006

## db.rs: create_tables Rewrite

Replace the v5 DDL with v6 schema. All `CREATE TABLE IF NOT EXISTS` for normalized tables.

```rust
fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS entries (
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
            FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
        );

        -- Indexes on entries
        CREATE INDEX IF NOT EXISTS idx_entries_topic      ON entries(topic);
        CREATE INDEX IF NOT EXISTS idx_entries_category   ON entries(category);
        CREATE INDEX IF NOT EXISTS idx_entries_status     ON entries(status);
        CREATE INDEX IF NOT EXISTS idx_entries_created_at ON entries(created_at);

        -- Indexes on entry_tags
        CREATE INDEX IF NOT EXISTS idx_entry_tags_tag      ON entry_tags(tag);
        CREATE INDEX IF NOT EXISTS idx_entry_tags_entry_id ON entry_tags(entry_id);

        -- NO MORE: topic_index, category_index, tag_index, time_index, status_index

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
    ").map_err(StoreError::Sqlite)?;

    // Initialize counters
    conn.execute_batch("
        INSERT OR IGNORE INTO counters (name, value) VALUES ('schema_version', 6);
        INSERT OR IGNORE INTO counters (name, value) VALUES ('next_entry_id', 1);
        INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0);
        INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0);
        INSERT OR IGNORE INTO counters (name, value) VALUES ('next_audit_event_id', 0);
    ").map_err(StoreError::Sqlite)?;

    Ok(())
}
```

## db.rs: PRAGMA foreign_keys ON

```rust
conn.execute_batch(
    "PRAGMA journal_mode = WAL;
     PRAGMA synchronous = NORMAL;
     PRAGMA wal_autocheckpoint = 1000;
     PRAGMA foreign_keys = ON;   // *** CHANGED from OFF ***
     PRAGMA busy_timeout = 5000;
     PRAGMA cache_size = -16384;",
).map_err(StoreError::Sqlite)?;
```

## db.rs: Remove begin_read

```rust
// Remove begin_read() method entirely (ADR-001).
// SqliteReadTransaction is removed in Wave 4.
```

## read.rs: entry_from_row Helper

```rust
/// Construct EntryRecord from a SQLite row using column-by-name access.
/// Tags are set to vec![] -- caller MUST use load_tags_for_entries().
/// Uses named column access (not positional) to prevent column-swap bugs (ADR-004).
fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EntryRecord> {
    Ok(EntryRecord {
        id: row.get::<_, i64>("id")? as u64,
        title: row.get("title")?,
        content: row.get("content")?,
        topic: row.get("topic")?,
        category: row.get("category")?,
        tags: vec![],  // populated by load_tags_for_entries
        source: row.get("source")?,
        status: Status::try_from(row.get::<_, u8>("status")?)
            .unwrap_or(Status::Active),
        confidence: row.get("confidence")?,
        created_at: row.get::<_, i64>("created_at")? as u64,
        updated_at: row.get::<_, i64>("updated_at")? as u64,
        last_accessed_at: row.get::<_, i64>("last_accessed_at")? as u64,
        access_count: row.get::<_, i64>("access_count")? as u32,
        supersedes: row.get::<_, Option<i64>>("supersedes")?.map(|v| v as u64),
        superseded_by: row.get::<_, Option<i64>>("superseded_by")?.map(|v| v as u64),
        correction_count: row.get::<_, i64>("correction_count")? as u32,
        embedding_dim: row.get::<_, i64>("embedding_dim")? as u16,
        created_by: row.get("created_by")?,
        modified_by: row.get("modified_by")?,
        content_hash: row.get("content_hash")?,
        previous_hash: row.get("previous_hash")?,
        version: row.get::<_, i64>("version")? as u32,
        feature_cycle: row.get("feature_cycle")?,
        trust_source: row.get("trust_source")?,
        helpful_count: row.get::<_, i64>("helpful_count")? as u32,
        unhelpful_count: row.get::<_, i64>("unhelpful_count")? as u32,
    })
}
```

## read.rs: load_tags_for_entries Helper

```rust
use std::collections::HashMap;

/// Batch-load tags for multiple entries. Returns map of entry_id -> Vec<tag>.
/// Every code path constructing EntryRecord MUST call this (ADR-006, C-10).
fn load_tags_for_entries(
    conn: &Connection,
    ids: &[u64],
) -> Result<HashMap<u64, Vec<String>>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    // Build IN clause with placeholders
    let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
    let sql = format!(
        "SELECT entry_id, tag FROM entry_tags WHERE entry_id IN ({}) ORDER BY entry_id, tag",
        placeholders.join(",")
    );

    let mut stmt = conn.prepare(&sql).map_err(StoreError::Sqlite)?;
    let params: Vec<Box<dyn rusqlite::types::ToSql>> =
        ids.iter().map(|&id| Box::new(id as i64) as Box<dyn rusqlite::types::ToSql>).collect();

    let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
        Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?))
    }).map_err(StoreError::Sqlite)?;

    let mut map: HashMap<u64, Vec<String>> = HashMap::new();
    for row in rows {
        let (entry_id, tag) = row.map_err(StoreError::Sqlite)?;
        map.entry(entry_id).or_default().push(tag);
    }

    Ok(map)
}

/// Apply tags from the tag map to a Vec of EntryRecords.
fn apply_tags(entries: &mut [EntryRecord], tag_map: &HashMap<u64, Vec<String>>) {
    for entry in entries.iter_mut() {
        if let Some(tags) = tag_map.get(&entry.id) {
            entry.tags = tags.clone();
        }
    }
}
```

## schema.rs Changes

Remove runtime re-exports of `serialize_entry` and `deserialize_entry` from public API. They remain in the module for migration_compat but are no longer used in runtime paths.

Keep `status_counter_key`, `co_access_key`, `CoAccessRecord` (these are still used).

Add server-crate types (Wave 3 preparation, but moved in Wave 0 to support migration_compat):
- `AgentRecord`, `TrustLevel`, `Capability`
- `AuditEvent`, `Outcome`
- `TryFrom<u8>` for TrustLevel, Capability, Outcome

The SELECT column list constant:

```rust
pub(crate) const ENTRY_COLUMNS: &str =
    "id, title, content, topic, category, source, status, confidence, \
     created_at, updated_at, last_accessed_at, access_count, \
     supersedes, superseded_by, correction_count, embedding_dim, \
     created_by, modified_by, content_hash, previous_hash, \
     version, feature_cycle, trust_source, helpful_count, unhelpful_count";
```
