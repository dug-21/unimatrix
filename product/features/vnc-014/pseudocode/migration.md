# Component: Schema Migration (migration.rs + db.rs)

## Purpose

Add the v24→v25 migration block that installs four new columns, two indexes,
and two append-only DDL triggers on `audit_log`. Update `create_tables_if_needed`
in `db.rs` so fresh databases match the migrated schema exactly.

**Files modified:**
- `crates/unimatrix-store/src/migration.rs` — new migration block, `CURRENT_SCHEMA_VERSION = 25`
- `crates/unimatrix-store/src/db.rs` — updated `audit_log` DDL in `create_tables_if_needed`

---

## Initialization Sequence

`migrate_if_needed` is called from `SqlxStore::open()` on a dedicated
non-pooled connection. The new block runs inside the existing `run_main_migrations`
transaction. The schema version counter is bumped inside that same transaction.

---

## Modified Functions

### `CURRENT_SCHEMA_VERSION` (migration.rs)

```
// Change from:
pub const CURRENT_SCHEMA_VERSION: u64 = 24;
// To:
pub const CURRENT_SCHEMA_VERSION: u64 = 25;
```

### `run_main_migrations` (migration.rs)

Append a new `if current_version < 25` block at the end of the function,
after the existing `if current_version < 24` block.

```
// v24 → v25: four-column audit_log migration (vnc-014 / ASS-050)
if current_version < 25:

    // --- Pre-flight: check all four columns before any ALTER ---
    // ADR-004: ALL four checks run first; then ALL four ALTERs run.
    // This ensures a database that crashed between ALTER-1 and version bump
    // will skip already-added columns on re-run rather than failing.

    has_credential_type = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('audit_log')
         WHERE name = 'credential_type'"
    ).fetch_one(&mut **txn).await.map(|c| c > 0).unwrap_or(false)

    has_capability_used = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('audit_log')
         WHERE name = 'capability_used'"
    ).fetch_one(&mut **txn).await.map(|c| c > 0).unwrap_or(false)

    has_agent_attribution = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('audit_log')
         WHERE name = 'agent_attribution'"
    ).fetch_one(&mut **txn).await.map(|c| c > 0).unwrap_or(false)

    has_metadata = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('audit_log')
         WHERE name = 'metadata'"
    ).fetch_one(&mut **txn).await.map(|c| c > 0).unwrap_or(false)

    // --- ALTER TABLE: only if column not yet present ---

    if NOT has_credential_type:
        sqlx::query(
            "ALTER TABLE audit_log ADD COLUMN credential_type TEXT NOT NULL DEFAULT 'none'"
        ).execute(&mut **txn).await.map_err(migration_err)?

    if NOT has_capability_used:
        sqlx::query(
            "ALTER TABLE audit_log ADD COLUMN capability_used TEXT NOT NULL DEFAULT ''"
        ).execute(&mut **txn).await.map_err(migration_err)?

    if NOT has_agent_attribution:
        sqlx::query(
            "ALTER TABLE audit_log ADD COLUMN agent_attribution TEXT NOT NULL DEFAULT ''"
        ).execute(&mut **txn).await.map_err(migration_err)?

    if NOT has_metadata:
        sqlx::query(
            "ALTER TABLE audit_log ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}'"
        ).execute(&mut **txn).await.map_err(migration_err)?

    // --- Indexes: idempotent (IF NOT EXISTS) ---

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_audit_log_session
         ON audit_log(session_id)"
    ).execute(&mut **txn).await.map_err(migration_err)?

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_audit_log_cred
         ON audit_log(credential_type)"
    ).execute(&mut **txn).await.map_err(migration_err)?

    // --- Append-only DDL triggers: idempotent (IF NOT EXISTS) ---

    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS audit_log_no_update
         BEFORE UPDATE ON audit_log
         BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END"
    ).execute(&mut **txn).await.map_err(migration_err)?

    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS audit_log_no_delete
         BEFORE DELETE ON audit_log
         BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END"
    ).execute(&mut **txn).await.map_err(migration_err)?

    // --- Schema version bump (final step in transaction) ---

    sqlx::query(
        "UPDATE counters SET value = 25 WHERE name = 'schema_version'"
    ).execute(&mut **txn).await.map_err(migration_err)?
```

**Ordering rule (ADR-004, SR-02)**: The four `pragma_table_info` checks all
execute before any `ALTER TABLE`. A partial-apply state (crash after ALTER-1
but before version bump) is fully handled because re-run skips already-present
columns and proceeds to install missing ones plus the indexes and triggers.

### `create_tables_if_needed` (db.rs)

Update the `CREATE TABLE IF NOT EXISTS audit_log` DDL to include all four
new columns and the two append-only triggers. This DDL is used for fresh
databases; it must be byte-semantically identical to the migrated schema
(R-11 mitigation).

The existing DDL creates `audit_log` with 8 columns. Replace it with:

```
-- Updated audit_log DDL (fresh database path)
CREATE TABLE IF NOT EXISTS audit_log (
    event_id          INTEGER NOT NULL,
    timestamp         INTEGER NOT NULL,
    session_id        TEXT    NOT NULL,
    agent_id          TEXT    NOT NULL,
    operation         TEXT    NOT NULL,
    target_ids        TEXT    NOT NULL,
    outcome           INTEGER NOT NULL,
    detail            TEXT    NOT NULL,
    credential_type   TEXT    NOT NULL DEFAULT 'none',
    capability_used   TEXT    NOT NULL DEFAULT '',
    agent_attribution TEXT    NOT NULL DEFAULT '',
    metadata          TEXT    NOT NULL DEFAULT '{}'
);

-- Indexes (idempotent — also created by migration on existing DBs)
CREATE INDEX IF NOT EXISTS idx_audit_log_session ON audit_log(session_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_cred    ON audit_log(credential_type);

-- Append-only triggers (idempotent — also created by migration on existing DBs)
CREATE TRIGGER IF NOT EXISTS audit_log_no_update
    BEFORE UPDATE ON audit_log
    BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END;

CREATE TRIGGER IF NOT EXISTS audit_log_no_delete
    BEFORE DELETE ON audit_log
    BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END;
```

Note: existing `audit_log` DDL in `create_tables_if_needed` also creates an
`idx_audit_log_timestamp` index. Confirm whether that index exists in the
current DDL and preserve it if so. The two new indexes are additions only.

---

## Schema Cascade Checklist (R-02)

The delivery agent MUST address all cascade points after bumping the version:

1. `CURRENT_SCHEMA_VERSION = 25` in `migration.rs` (done in this component)
2. `sqlite_parity.rs` column count assertion for `audit_log` — update to 12
3. Migration test file rename — the `at_least_N` pattern: rename any
   `test_migration_at_least_24.rs` (or equivalent) to cover through v25
4. The existing `test_schema_version_initialized_to_current_on_fresh_db`
   test passes automatically when `CURRENT_SCHEMA_VERSION = 25`

Run `cargo test --workspace` immediately after bumping the constant to catch
all cascade failures before writing the migration test file.

---

## Error Handling

All `sqlx::query(...).execute(...)` calls use `.map_err(|e| StoreError::Migration { source: Box::new(e) })`.
This is the existing pattern used by every other migration block in the file.
A helper closure `migration_err` can be defined locally within `run_main_migrations`
for brevity — consistent with how other blocks handle it.

On error, `migrate_if_needed` rolls back the transaction and returns. The
database remains at v24. Re-run is safe because all ALTERs are guarded by
pragma checks and all indexes/triggers use `IF NOT EXISTS`.

---

## Key Test Scenarios

1. **Fresh DB (AC-04, R-11)**: Open a new database via `SqlxStore::open`.
   `pragma_table_info('audit_log')` returns 12 columns with correct names and
   NOT NULL constraints. `SELECT name FROM sqlite_master WHERE type='trigger'`
   returns both trigger names.

2. **Migration from v24 with rows (AC-09, EC-08)**: Create a v24 DB with
   existing audit rows. Run `migrate_if_needed`. Confirm: row count unchanged,
   new columns have correct defaults, schema_version = 25.

3. **Migration from v24 with zero rows (EC-07)**: Same as above on empty table.

4. **Idempotency: partial apply (R-04, NFR-04)**:
   - Manually execute `ALTER TABLE audit_log ADD COLUMN credential_type TEXT NOT NULL DEFAULT 'none'`
     on a v24 DB (simulates crash after first ALTER).
   - Run `migrate_if_needed`. Confirm: completes without error, all four
     columns present, version = 25.
   - Repeat with 2-of-4 and 4-of-4 columns pre-added.

5. **Idempotency: full apply (FM-03)**: Apply all four ALTERs manually without
   bumping version. Run `migrate_if_needed`. Confirm: all pragma checks skip
   ALTERs, indexes and triggers installed, version = 25.

6. **Append-only enforcement (AC-05b, R-01)**: After migration, execute
   `DELETE FROM audit_log WHERE event_id = 1` via `sqlx::query`. Assert
   returned error message contains `"audit_log is append-only: DELETE not permitted"`.
   Repeat for UPDATE.

7. **Trigger existence (SEC-04)**: After fresh DB creation and after migration,
   query `sqlite_master` for both trigger names. Both must be present.

8. **Schema parity (R-11)**: Create one fresh DB and one migrated-from-v24 DB.
   Compare `pragma_table_info('audit_log')` output row-by-row — column names,
   types, `notnull` flags, and `dflt_value` must be identical for all 12 columns.
