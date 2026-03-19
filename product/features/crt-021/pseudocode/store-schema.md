# store-schema — Pseudocode

**File**: `crates/unimatrix-store/src/db.rs`
**Function**: `create_tables_if_needed`
**Change**: Add GRAPH_EDGES DDL + three indexes at the end of the existing table creation sequence

---

## Purpose

Add the `graph_edges` table to the fresh-database initialization path so that every new
database has the table available without requiring migration. This is the standard pattern
for all tables in `create_tables_if_needed` — every table in the schema appears here
alongside the `migration.rs` blocks that handle upgrades of existing databases.

---

## Insertion Point

The new DDL block is appended after the existing `query_log` table DDL and its indexes,
and before the counter initialization `INSERT OR IGNORE` calls. Keep the DDL in the same
order used by the migration to aid readability. Update the `schema_version` counter
initialization value from `12` to `13`.

---

## DDL to Add

### graph_edges table

```sql
CREATE TABLE IF NOT EXISTS graph_edges (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id      INTEGER NOT NULL,
    target_id      INTEGER NOT NULL,
    relation_type  TEXT    NOT NULL,
    weight         REAL    NOT NULL DEFAULT 1.0,
    created_at     INTEGER NOT NULL,
    created_by     TEXT    NOT NULL DEFAULT '',
    source         TEXT    NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata       TEXT    DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```

Column notes:
- `id`: autoincrement surrogate key; not exposed in GraphEdgeRow
- `source_id`: references entries.id (the "old/replaced" entry for Supersedes)
- `target_id`: references entries.id (the "new/correcting" entry for Supersedes)
- `relation_type`: RelationType string value; validated by from_str() on read
- `weight`: f32; finite-validated before every write; DEFAULT 1.0 for Supersedes
- `created_at`: unix epoch seconds; NOT NULL; populated at write time
- `created_by`: agent_id or "bootstrap"; empty string default
- `source`: origin pipeline identifier; "entries.supersedes", "co_access", "nli"
- `bootstrap_only`: 0 or 1 SQLite boolean; 0 = confirmed, 1 = heuristic unconfirmed
- `metadata`: NULL for all crt-021 writes; reserved for W3-1 GNN per-edge features
- `UNIQUE(source_id, target_id, relation_type)`: enables `INSERT OR IGNORE` idempotency

### graph_edges indexes

```sql
CREATE INDEX IF NOT EXISTS idx_graph_edges_source_id
    ON graph_edges(source_id)

CREATE INDEX IF NOT EXISTS idx_graph_edges_target_id
    ON graph_edges(target_id)

CREATE INDEX IF NOT EXISTS idx_graph_edges_relation_type
    ON graph_edges(relation_type)
```

Purpose of indexes:
- `source_id`, `target_id`: make the orphaned-edge compaction DELETE efficient
  (`WHERE source_id NOT IN (SELECT id FROM entries) OR target_id NOT IN (...)`)
- `relation_type`: supports future W3-1 GNN queries filtered by edge type

---

## Counter Initialization Update

```
-- Change schema_version initialization from 12 to 13
-- This initializes fresh databases at the current schema version.
-- The migration path is not triggered for fresh databases.

INSERT OR IGNORE INTO counters (name, value) VALUES ('schema_version', 13)
```

---

## Pseudocode Insertion (full block)

Insert this block in `create_tables_if_needed` after the query_log index creation and
before the counter initialization section:

```
-- graph_edges table
sqlx::query(
    "CREATE TABLE IF NOT EXISTS graph_edges (
        id             INTEGER PRIMARY KEY AUTOINCREMENT,
        source_id      INTEGER NOT NULL,
        target_id      INTEGER NOT NULL,
        relation_type  TEXT    NOT NULL,
        weight         REAL    NOT NULL DEFAULT 1.0,
        created_at     INTEGER NOT NULL,
        created_by     TEXT    NOT NULL DEFAULT '',
        source         TEXT    NOT NULL DEFAULT '',
        bootstrap_only INTEGER NOT NULL DEFAULT 0,
        metadata       TEXT    DEFAULT NULL,
        UNIQUE(source_id, target_id, relation_type)
    )"
)
.execute(&mut *conn).await?

sqlx::query(
    "CREATE INDEX IF NOT EXISTS idx_graph_edges_source_id ON graph_edges(source_id)"
)
.execute(&mut *conn).await?

sqlx::query(
    "CREATE INDEX IF NOT EXISTS idx_graph_edges_target_id ON graph_edges(target_id)"
)
.execute(&mut *conn).await?

sqlx::query(
    "CREATE INDEX IF NOT EXISTS idx_graph_edges_relation_type ON graph_edges(relation_type)"
)
.execute(&mut *conn).await?
```

---

## Error Handling

Each `sqlx::query(...).execute(&mut *conn).await?` propagates `sqlx::Error` via the
`?` operator back to the caller (`create_tables_if_needed`), which itself returns
`Result<(), sqlx::Error>`. No additional error wrapping needed — matches existing pattern
for all other tables in this function.

---

## Key Test Scenarios

1. **GRAPH_EDGES DDL on fresh database** (AC-04):
   - Open a fresh in-memory SQLite database.
   - Call `create_tables_if_needed`.
   - Execute `SELECT name FROM sqlite_master WHERE type='table' AND name='graph_edges'`.
   - Assert the table exists.
   - Execute `PRAGMA table_info('graph_edges')` and assert all columns exist with
     correct types and constraints, including `metadata TEXT` with no NOT NULL.

2. **UNIQUE constraint present** (AC-04):
   - After creating the table, insert two rows with the same `(source_id, target_id, relation_type)`.
   - Use `INSERT OR IGNORE`. Assert only one row persists.
   - Use `INSERT` (without OR IGNORE). Assert a unique constraint violation error.

3. **schema_version counter initialized to 13 on fresh database**:
   - After `create_tables_if_needed`, query `SELECT value FROM counters WHERE name = 'schema_version'`.
   - Assert value = 13.

4. **All three indexes present**:
   - Execute `SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='graph_edges'`.
   - Assert `idx_graph_edges_source_id`, `idx_graph_edges_target_id`, `idx_graph_edges_relation_type`
     all appear.
