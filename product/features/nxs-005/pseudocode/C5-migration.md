# C5: Schema Migration

## File: `crates/unimatrix-store/src/sqlite/migration.rs`

## Overview

The SQLite migration module adapts the redb migration chain (v0 -> v5) for SQLite.

Key difference: When creating a FRESH SQLite database, all tables are already created at v5 schema with `CREATE TABLE IF NOT EXISTS`. The schema_version counter is initialized to 5. No migration runs.

Migration is only needed when:
1. Opening an EXISTING SQLite database created at an older schema version (future scenario after nxs-005 ships and schema evolves)
2. During the redb-to-SQLite data migration (C6), the target DB starts fresh at v5

For nxs-005, the migration module is a structural placeholder that:
- Reads schema_version from counters
- If version >= CURRENT_SCHEMA_VERSION (5), returns immediately
- If version < 5, runs the appropriate migration steps

## migrate_if_needed(store: &Store) -> Result<()>

```
lock conn
SELECT value FROM counters WHERE name = 'schema_version' -> version (default 0)
if version >= CURRENT_SCHEMA_VERSION (5):
  return Ok(())

-- Entry-rewriting migrations (same logic as redb, different I/O)
if version == 0:
  migrate_entries_to_current_schema(conn)
elif version == 1:
  migrate_entries_to_current_schema(conn)
elif version == 2:
  migrate_entries_to_current_schema(conn)

-- Table-creation migrations (idempotent CREATE TABLE IF NOT EXISTS)
if version < 4:
  -- signal_queue table (already created by create_tables, but ensure counter)
  INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0)
if version < 5:
  -- sessions + injection_log tables (already created)
  INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0)

UPDATE counters SET value = 5 WHERE name = 'schema_version'
```

## migrate_entries_to_current_schema(conn)

```
BEGIN IMMEDIATE
  SELECT id, data FROM entries
  for each (id, bytes):
    -- Attempt deserialization with current EntryRecord
    -- If fails, try legacy format and upgrade
    try deserialize as current EntryRecord:
      Ok(record) -> no change needed (already at current schema)
      Err -> try legacy deserialization:
        deserialize with defaults applied
        re-serialize as current EntryRecord
        UPDATE entries SET data = ? WHERE id = ?
COMMIT
```

## Design Notes

1. Fresh SQLite databases start at schema v5 -- no migration needed
2. The migration chain handles the same v0->v1->v2->v3->v4->v5 transitions as redb
3. Entry rewriting uses the same bincode serde path
4. Table creation is idempotent (IF NOT EXISTS) so migration is safe to re-run
5. Counter initialization uses INSERT OR IGNORE to be idempotent
6. All migration runs in a single transaction for atomicity
