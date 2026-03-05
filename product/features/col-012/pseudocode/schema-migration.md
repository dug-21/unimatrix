# Pseudocode: schema-migration

## File: crates/unimatrix-store/src/migration.rs

### Change: CURRENT_SCHEMA_VERSION 6 -> 7

```
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 7;
```

### Change: Add v6->v7 migration step

In `migrate_if_needed`, after the v5->v6 block and before the schema version update:

```
if current_version < 7:
    conn.execute_batch(
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
    )
```

This is idempotent (IF NOT EXISTS). No data migration needed.

## File: crates/unimatrix-store/src/db.rs

### Change: Add observations table to create_tables()

After the audit_log block, add:

```
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
```

### Change: Update schema_version counter default

```
INSERT OR IGNORE INTO counters (name, value) VALUES ('schema_version', 7);
```

## Notes

- ADR-001: AUTOINCREMENT PK avoids timestamp collision risk
- Migration runs inside the existing BEGIN IMMEDIATE transaction in migrate_if_needed
- Fresh databases get the table from create_tables, existing databases get it from migration
- The v6->v7 migration block goes inside the existing transaction closure, before the schema version update
