# Pseudocode: schema-migration

**Wave**: 3 (parallel with detection-rules and metrics-extension)
**Crate**: `unimatrix-store`
**File**: `crates/unimatrix-store/src/migration.rs`

## Purpose

Add the v13 → v14 migration: `ALTER TABLE OBSERVATION_METRICS ADD COLUMN
domain_metrics_json TEXT NULL`. Increment `CURRENT_SCHEMA_VERSION` to 14.
This is the only schema change in col-023. The `observations` table is NOT modified.

## Changes

### CURRENT_SCHEMA_VERSION

```
-- OLD:
pub const CURRENT_SCHEMA_VERSION: i64 = 13;

-- NEW:
pub const CURRENT_SCHEMA_VERSION: i64 = 14;
```

### Migration entry for v13 → v14

The migration system runs a sequence of versioned migration functions. Add a new
entry for v14. The implementation pattern follows the existing migration sequence.

```
fn migrate_v13_to_v14(conn: &Connection) -> Result<()>:
    conn.execute_batch(
        "ALTER TABLE OBSERVATION_METRICS ADD COLUMN domain_metrics_json TEXT NULL;"
    )?
    Ok(())
```

The migration runner calls this when the current DB version is 13 and the target is 14.

### Idempotency / FM-05 handling

SQLite's `ALTER TABLE ADD COLUMN` fails if the column already exists (SQLite does not
support `IF NOT EXISTS` for `ADD COLUMN` in older versions). To handle a partially-
migrated database (FM-05):

```
fn migrate_v13_to_v14(conn: &Connection) -> Result<()>:
    -- Check if column already exists before applying
    -- Use PRAGMA table_info to inspect columns
    let column_exists = check_column_exists(conn, "OBSERVATION_METRICS", "domain_metrics_json")?

    if !column_exists:
        conn.execute_batch(
            "ALTER TABLE OBSERVATION_METRICS ADD COLUMN domain_metrics_json TEXT NULL;"
        )?

    Ok(())
```

### check_column_exists helper

```
fn check_column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool>:
    -- PRAGMA table_info(TABLE_NAME) returns rows with columns: cid, name, type, notnull, dflt_value, pk
    let exists = conn.query_row(
        &format!("PRAGMA table_info({})", table),
        [],
        |_| Ok(())
    )
    -- Actually: collect all rows and check if any has name == column
    -- Use rusqlite query_map:
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?
    let exists = stmt.query_map([], |row| {
        let name: String = row.get(1)?  -- index 1 is the column name
        Ok(name)
    })?.any(|r| r.map(|n| n == column).unwrap_or(false))
    Ok(exists)
```

Note: If the existing migration infrastructure already has a column-existence check
utility, use it rather than implementing a new one. Check the existing code for a
pattern like `column_exists` or `pragma_table_info`.

## Complete Migration Sequence Context

The existing migration runner chains:
```
v0 → v1 (nxs-004)
v1 → v2 (crt-001)
v2 → v3 (crt-005)
...
v12 → v13 (col-022: feature_cycle in OBSERVATION_METRICS or similar)
v13 → v14 (col-023: domain_metrics_json in OBSERVATION_METRICS)   -- ADD THIS
```

The runner pattern (based on the memory entry):
```
fn run_migrations(conn: &Connection, current: i64) -> Result<()>:
    if current < 1: migrate_v0_to_v1(conn)?
    if current < 2: migrate_v1_to_v2(conn)?
    ...
    if current < 14: migrate_v13_to_v14(conn)?
    set_schema_version(conn, CURRENT_SCHEMA_VERSION)?
    Ok(())
```

## Fresh Database Schema

For a fresh database (no prior rows), the `OBSERVATION_METRICS` CREATE TABLE
statement must include `domain_metrics_json TEXT NULL` directly (no migration needed,
but the schema creation path must be updated):

```
CREATE TABLE IF NOT EXISTS OBSERVATION_METRICS (
    feature_cycle TEXT PRIMARY KEY,
    computed_at INTEGER NOT NULL,
    -- ... 21 UniversalMetrics columns ... (unchanged)
    domain_metrics_json TEXT NULL    -- ADD THIS to CREATE TABLE statement
);
```

## Error Handling

- `migrate_v13_to_v14()` returns `Err` if the SQLite statement fails for a reason other
  than column already existing (FM-05 handles the already-existing case).
- A failed migration aborts server startup — no partial state.
- Rollback risk (R-12): downgrading from v14 to a v13 binary encounters the extra column.
  Named-column queries are unaffected. The risk is documented in test comments.

## Key Test Scenarios

1. **Schema v14 fresh database**: open a new database; assert `PRAGMA user_version == 14`
   and `domain_metrics_json` column exists in `OBSERVATION_METRICS`.

2. **v13 → v14 migration**: start with a v13 schema (no `domain_metrics_json` column);
   run migration; assert column exists and all prior rows are readable (AC-09).

3. **Round-trip after migration**: insert a row with `domain_metrics_json = NULL` in
   v13, run migration, verify the row still reads correctly with all 21 original fields
   at correct values (R-05).

4. **Idempotency (FM-05)**: call `migrate_v13_to_v14()` twice; assert no error on
   the second call.

5. **R-12 rollback documentation**: test file includes a comment that explains the
   rollback risk and that named-column queries protect against field-offset errors.

6. **PRAGMA user_version**: after all migrations on a v0 database, `PRAGMA user_version`
   returns 14.

7. **R-11 structural test update**: `UNIVERSAL_METRICS_FIELDS.len() == 22` (this test
   lives in `unimatrix-store/src/metrics.rs`, not migration.rs, but it is triggered by
   this component's column addition).
