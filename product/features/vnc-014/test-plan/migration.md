# Test Plan: Schema Migration (migration.rs + db.rs)

## Component Summary

`migration.rs` gets a new v24→v25 migration block. `db.rs` `create_tables_if_needed()` gets
the four new columns and trigger DDL added to the `audit_log` CREATE TABLE statement.
`CURRENT_SCHEMA_VERSION` bumps from 24 to 25.

The migration test file follows the established `migration_vN_to_vM.rs` pattern with a v24
database builder (8-column `audit_log`, no triggers, schema_version=24).

**Cascade checklist** (pattern #4125, applied to this feature):
1. `CURRENT_SCHEMA_VERSION = 25` in `migration.rs`
2. `sqlite_parity.rs` `audit_log` column count assertion updated to 12
3. New migration test file: `migration_v24_to_v25.rs`
4. Existing `migration_v23_to_v24.rs` schema_version assertions use `>= 24` and remain valid
5. `create_tables_if_needed()` DDL byte-identical to migration DDL (R-11 mitigation)

---

## Unit Tests

### MIG-U-01: `CURRENT_SCHEMA_VERSION` constant is 25

**Risk**: R-02
**Arrange**: N/A (compile-time constant).
**Assert**: `unimatrix_store::migration::CURRENT_SCHEMA_VERSION == 25`

The existing `test_schema_version_initialized_to_current_on_fresh_db` test in the store
crate validates this automatically. Verify it still passes after the bump.

---

## Migration Integration Tests (migration_v24_to_v25.rs)

This test file must be created at `crates/unimatrix-store/tests/migration_v24_to_v25.rs`.
It requires the `test-support` feature flag (consistent with all other migration test files).

### V24 Database Builder

```rust
async fn create_v24_database(path: &Path) {
    // Creates the full v24 schema with audit_log having 8 columns (no new fields),
    // schema_version = 24. Seeds at least one audit_log row and several entries rows
    // to verify DEFAULT landing and no data loss (R-02, AC-09).
}
```

The v24 `audit_log` DDL must have exactly 8 columns (`event_id`, `timestamp`, `session_id`,
`agent_id`, `operation`, `target_ids`, `outcome`, `detail`) and no append-only triggers.

---

### MIG-V25-U-01: `CURRENT_SCHEMA_VERSION` constant is at least 25

**Risk**: R-02
**Assert**: `CURRENT_SCHEMA_VERSION >= 25`
This is a non-async constant check added as a standalone `#[test]`.

---

### MIG-V25-U-02: Fresh database initializes directly to v25 — 12 columns, triggers present

**Risk**: R-02, R-11, AC-04
**Arrange**: Call `SqlxStore::open(in_memory_path, PoolConfig::default())`.
**Assert**:
- `pragma_table_info('audit_log')` returns exactly 12 rows
- Columns include `credential_type` (NOT NULL, DEFAULT 'none'), `capability_used` (NOT NULL, DEFAULT ''),
  `agent_attribution` (NOT NULL, DEFAULT ''), `metadata` (NOT NULL, DEFAULT '{}')
- `SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='audit_log'` returns
  both `audit_log_no_update` and `audit_log_no_delete`
- `SELECT value FROM counters WHERE name = 'schema_version'` returns 25

---

### MIG-V25-U-03: v24→v25 migration adds all four columns with correct defaults

**Risk**: R-02, AC-04
**Arrange**: Create a v24 database using the builder; seed one row in `audit_log`.
**Act**: Open with `SqlxStore::open(path, PoolConfig::default())` to trigger migration.
**Assert**:
- `pragma_table_info('audit_log')` returns 12 columns
- Column `credential_type`: `notnull=1`, `dflt_value='none'`
- Column `capability_used`: `notnull=1`, `dflt_value=''`
- Column `agent_attribution`: `notnull=1`, `dflt_value=''`
- Column `metadata`: `notnull=1`, `dflt_value='{}'`
- The seeded row's new column values equal their defaults (no data loss, AC-09)

---

### MIG-V25-U-04: Idempotency — re-opening v25 database is a no-op

**Risk**: R-04
**Arrange**: Open a v24 database to migrate it to v25; close and reopen the same file.
**Act**: Re-open with `SqlxStore::open(path, PoolConfig::default())`.
**Assert**:
- Returns `Ok(_)` — no error
- Column count remains 12 (no duplicate columns)
- `schema_version == 25`

---

### MIG-V25-U-05: Partial column pre-existence — idempotency after partial crash

**Risk**: R-04
**Sub-case A: one column already present**
**Arrange**: Create v24 database; manually execute
  `ALTER TABLE audit_log ADD COLUMN credential_type TEXT NOT NULL DEFAULT 'none'`
  (simulating crash after first ALTER). Set `schema_version = 24`.
**Act**: Open with `SqlxStore::open(path, PoolConfig::default())`.
**Assert**: Migration completes without error. All 12 columns present. No "duplicate column
  name" error.

**Sub-case B: all four columns present, version not bumped**
**Arrange**: Create v24 database; manually execute all four ALTERs; leave `schema_version = 24`.
**Act**: Open with `SqlxStore::open(path, PoolConfig::default())`.
**Assert**: Migration completes. All four pragma pre-checks skip their respective ALTERs.
  Triggers and indexes created (idempotent). `schema_version = 25`.

---

### MIG-V25-U-06: Schema version bumped to 25, row count unchanged — AC-09

**Risk**: AC-09
**Arrange**: Create v24 database with 5 `audit_log` rows and 3 `entries` rows.
**Act**: Migrate by opening with `SqlxStore::open`.
**Assert**:
- `SELECT COUNT(*) FROM audit_log` returns 5 (unchanged)
- `SELECT COUNT(*) FROM entries` returns 3 (unchanged)
- `SELECT value FROM counters WHERE name = 'schema_version'` returns 25

---

### MIG-V25-U-07: Fresh-DB schema identical to migrated-DB schema (R-11 parity)

**Risk**: R-11
**Arrange**:
- Create fresh database `A` via `SqlxStore::open(":memory:", ...)` (uses `create_tables_if_needed`).
- Create v24 database `B`; migrate to v25 via `SqlxStore::open`.
**Act**: Run `pragma_table_info('audit_log')` on both.
**Assert**:
- Both databases return the same 12 column records (name, type, notnull, dflt_value, pk)
- Both have `audit_log_no_update` and `audit_log_no_delete` triggers
- `sqlite_parity.rs` column count test covers this independently

---

### MIG-V25-U-08: Append-only triggers installed and fire on DELETE

**Risk**: R-01, AC-05b, SEC-04
**Arrange**: Open a fresh v25 database; insert one `audit_log` row.
**Act**:
1. Execute `DELETE FROM audit_log WHERE event_id = 1` via `sqlx::query`.
2. Execute `UPDATE audit_log SET detail = 'x' WHERE event_id = 1` via `sqlx::query`.
**Assert** for each:
- Returns `Err(_)` — not `Ok`
- Error message contains `"audit_log is append-only: DELETE not permitted"` (for DELETE)
- Error message contains `"audit_log is append-only: UPDATE not permitted"` (for UPDATE)

---

### MIG-V25-U-09: Triggers present in sqlite_master after migration

**Risk**: SEC-04
**Arrange**: Migrate a v24 database to v25.
**Act**: `SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='audit_log'`
**Assert**:
- Result set contains `"audit_log_no_update"`
- Result set contains `"audit_log_no_delete"`

---

### MIG-V25-U-10: v24 database with zero rows — migration succeeds cleanly

**Risk**: EC-07
**Arrange**: Create v24 database with empty `audit_log` table.
**Act**: Migrate.
**Assert**:
- All 12 columns present
- No error (empty table is not a special case for ALTER TABLE)
- Triggers installed

---

## sqlite_parity.rs Cascade

### MIG-V25-PARITY-01: `audit_log` column count is 12 in sqlite_parity.rs

**Risk**: R-02 (cascade checklist)
The existing `sqlite_parity.rs` file contains column count assertions per table. The
`audit_log` assertion must be updated from 8 to 12.

**Assert**: The `audit_log` count check in `sqlite_parity.rs` passes at 12 (not 8).
This is a cascade test that fails if the DDL in `create_tables_if_needed()` is not updated.
