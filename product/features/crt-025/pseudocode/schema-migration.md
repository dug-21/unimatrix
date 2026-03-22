# Component 7: Schema Migration
## Files: `crates/unimatrix-store/src/migration.rs`, `crates/unimatrix-store/src/db.rs`

---

## Purpose

Advances schema from v14 to v15. Two structural changes:

1. New `cycle_events` table (append-only audit log for lifecycle events).
2. New `phase TEXT` nullable column on `feature_entries`.

Both paths must be covered:
- `run_main_migrations` (existing database upgrade path)
- `create_tables_if_needed` (fresh database creation path)

---

## 7a: `migration.rs` — `CURRENT_SCHEMA_VERSION` bump

```
// BEFORE:
pub const CURRENT_SCHEMA_VERSION: u64 = 14;

// AFTER:
pub const CURRENT_SCHEMA_VERSION: u64 = 15;
```

---

## 7b: `migration.rs` — v14→v15 block in `run_main_migrations`

Append the following block at the end of `run_main_migrations`, after the existing v13→v14 block and before the `schema_version` counter update.

```
// v14 → v15: CYCLE_EVENTS table + feature_entries.phase column (crt-025).
IF current_version < 15:

    // Step 1: Create cycle_events table (idempotent — CREATE TABLE IF NOT EXISTS)
    // DDL mirrors create_tables_if_needed in db.rs; both must be kept in sync.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cycle_events (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            cycle_id   TEXT    NOT NULL,
            seq        INTEGER NOT NULL,
            event_type TEXT    NOT NULL,
            phase      TEXT,
            outcome    TEXT,
            next_phase TEXT,
            timestamp  INTEGER NOT NULL
        )"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id ON cycle_events (cycle_id)"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    // Step 2: Add phase column to feature_entries (idempotent via pragma_table_info pre-check)
    //
    // SQLite does not support ALTER TABLE ADD COLUMN IF NOT EXISTS.
    // Pattern from v7→v8 (pre_quarantine_status) and v13→v14 (domain_metrics_json):
    has_phase_column = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('feature_entries') WHERE name = 'phase'"
    )
    .fetch_one(&mut **txn)
    .await
    .map(|count| count > 0)
    .unwrap_or(false)

    IF NOT has_phase_column:
        sqlx::query("ALTER TABLE feature_entries ADD COLUMN phase TEXT")
            .execute(&mut **txn)
            .await
            .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    // No backfill: pre-existing feature_entries rows get phase = NULL (C-05, FR-06.4)
```

The schema_version counter update at the end of `run_main_migrations` already uses `CURRENT_SCHEMA_VERSION` as the bound value. After bumping the constant to 15, it will write 15 automatically.

---

## 7c: `db.rs` — `create_tables_if_needed` updates

`create_tables_if_needed` creates all tables for fresh databases. It must be updated to include:

### Update 1: `feature_entries` DDL (add `phase` column)

```
// BEFORE:
sqlx::query(
    "CREATE TABLE IF NOT EXISTS feature_entries (
        feature_id TEXT NOT NULL,
        entry_id INTEGER NOT NULL,
        PRIMARY KEY (feature_id, entry_id)
    )"
)
.execute(&mut *conn)
.await?

// AFTER:
sqlx::query(
    "CREATE TABLE IF NOT EXISTS feature_entries (
        feature_id TEXT NOT NULL,
        entry_id   INTEGER NOT NULL,
        phase      TEXT,
        PRIMARY KEY (feature_id, entry_id)
    )"
)
.execute(&mut *conn)
.await?
```

### Update 2: Add `cycle_events` DDL (new table, placed after `feature_entries`)

```
sqlx::query(
    "CREATE TABLE IF NOT EXISTS cycle_events (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        cycle_id   TEXT    NOT NULL,
        seq        INTEGER NOT NULL,
        event_type TEXT    NOT NULL,
        phase      TEXT,
        outcome    TEXT,
        next_phase TEXT,
        timestamp  INTEGER NOT NULL
    )"
)
.execute(&mut *conn)
.await?

sqlx::query(
    "CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id ON cycle_events (cycle_id)"
)
.execute(&mut *conn)
.await?
```

Both DDL blocks use `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX IF NOT EXISTS`, making `create_tables_if_needed` idempotent for fresh calls.

---

## Update 3: `create_tables_if_needed` schema_version counter

`create_tables_if_needed` inserts the initial `schema_version` counter. After bumping `CURRENT_SCHEMA_VERSION` to 15, this write will correctly seed the value as 15 for fresh databases. No explicit change needed if the counter insert already binds `CURRENT_SCHEMA_VERSION`.

Verify: confirm `create_tables_if_needed` uses `CURRENT_SCHEMA_VERSION` (the constant) not a hardcoded integer for the initial counter insert. If it uses a hardcoded `14`, update to `CURRENT_SCHEMA_VERSION`.

---

## Idempotency Contract

Running migration twice must not fail (FR-07.4, R-05):

| Operation | Idempotent How |
|-----------|----------------|
| `CREATE TABLE IF NOT EXISTS cycle_events` | SQLite no-ops if table exists |
| `CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id` | SQLite no-ops if index exists |
| `ALTER TABLE feature_entries ADD COLUMN phase` | Guarded by `pragma_table_info` pre-check |
| Schema version counter update | `INSERT OR REPLACE` (already in use) |

---

## Migration Sequence

```
DB at v14
  → run_main_migrations(current_version=14)
  → current_version < 15: CREATE TABLE cycle_events, CREATE INDEX, ADD COLUMN phase
  → UPDATE counters SET schema_version = 15
DB now at v15
```

```
DB at v15 (second run)
  → run_main_migrations(current_version=15)
  → 15 >= CURRENT_SCHEMA_VERSION(15): early return Ok(())
  → no-op
```

---

## Error Handling

All migration errors follow the existing pattern:
```
.map_err(|e| StoreError::Migration { source: Box::new(e) })
```

On any error, the surrounding transaction in `migrate_if_needed` rolls back, and the error propagates to `SqlxStore::open`, failing server startup. This is consistent with how all other migration failures are handled.

---

## Key Test Scenarios

1. Open a v14 database → run `migrate_if_needed` → schema_version = 15, `cycle_events` table exists, `feature_entries.phase` column exists.
2. Run `migrate_if_needed` again on v15 database → no error, schema unchanged.
3. `CREATE TABLE IF NOT EXISTS cycle_events` on an existing v15 DB → no-op (idempotency of fresh DDL path).
4. `pragma_table_info('feature_entries')` shows `phase TEXT` column after migration.
5. Fresh database via `create_tables_if_needed` → schema_version = 15, `cycle_events` table present, `feature_entries.phase` column present.
6. INSERT into `cycle_events` after migration succeeds.
7. INSERT into `feature_entries` with `phase` column succeeds.
8. INSERT into `feature_entries` with `phase = NULL` succeeds (backward compatible rows).
