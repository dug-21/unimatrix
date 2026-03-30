# Component Test Plan: migration

## Component Scope

Two files modified:
- `crates/unimatrix-store/src/migration.rs` — `CURRENT_SCHEMA_VERSION` 17→18,
  `if current_version < 18` block.
- `crates/unimatrix-store/src/db.rs` — `create_tables_if_needed()` DDL,
  schema_version INSERT 17→18.

New file:
- `crates/unimatrix-store/tests/migration_v17_to_v18.rs` — integration test.

**AC coverage**: AC-01, AC-02, AC-02b, AC-13.
**Risk coverage**: R-01 (all six scenarios).

---

## Migration Pattern

Follow `tests/migration_v16_to_v17.rs` exactly:
- `create_v17_database(path: &Path)` helper builds the full v17 schema (all tables as
  they exist at v17) with `schema_version = 17` seeded in counters.
- V17 shape = V16 shape + `query_log.phase` column (the addition from v16→v17). The
  `cycle_review_index` table must NOT exist in the v17 shape.
- Tests open the v17 DB with `SqlxStore::open`, which triggers the migration.

---

## Unit Tests (in `tests/migration_v17_to_v18.rs`)

### MIG-U-01: CURRENT_SCHEMA_VERSION constant == 18 (AC-01, R-01)

```rust
#[test]
fn test_current_schema_version_is_18() {
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        18,
        "CURRENT_SCHEMA_VERSION must be 18"
    );
}
```

### MIG-U-02: Fresh database creates schema v18 (AC-01, R-01)

```rust
#[tokio::test]
async fn test_fresh_db_creates_schema_v18() {
    // open a fresh SqlxStore on an empty path
    // assert SELECT value FROM counters WHERE name='schema_version' == 18
    // assert cycle_review_index table exists via
    //   SELECT name FROM sqlite_master WHERE type='table' AND name='cycle_review_index'
    // assert returned row count == 1
}
```

### MIG-U-03: v17→v18 migration creates cycle_review_index table (AC-02, AC-13, R-01)

```rust
#[tokio::test]
async fn test_v17_to_v18_migration_creates_table() {
    // Arrange: create_v17_database (no cycle_review_index table).
    // Act: SqlxStore::open triggers migration.
    // Assert:
    //   SELECT name FROM sqlite_master WHERE name='cycle_review_index' returns 1 row.
    //   schema_version counter == 18.
}
```

### MIG-U-04: All five columns present after migration (AC-02, R-01)

```rust
#[tokio::test]
async fn test_v17_to_v18_migration_table_has_five_columns() {
    // Arrange: create_v17_database.
    // Act: open with SqlxStore.
    // Assert: SELECT COUNT(*) FROM pragma_table_info('cycle_review_index') == 5
    // Assert columns by name via pragma_table_info:
    //   'feature_cycle', 'schema_version', 'computed_at',
    //   'raw_signals_available', 'summary_json' — all present.
}
```

### MIG-U-05: Pre-existing data survives migration (AC-02, NFR-04, R-01)

```rust
#[tokio::test]
async fn test_v17_to_v18_migration_preserves_existing_data() {
    // Arrange: create_v17_database; insert one row into entries table.
    // Act: SqlxStore::open triggers migration.
    // Assert: the entry is still readable via store.get(id); no data loss.
    //   schema_version == 18 (confirming migration ran).
}
```

### MIG-U-06: Idempotency — running migration twice succeeds (NFR-06, R-01)

```rust
#[tokio::test]
async fn test_v17_to_v18_migration_idempotent() {
    // Arrange: create_v17_database.
    // Run 1: SqlxStore::open — migration fires.
    //   assert schema_version == 18, table exists.
    //   store.close().await.
    // Run 2: SqlxStore::open on same path — migration is a no-op.
    //   assert no error, schema_version still 18.
    //   SELECT COUNT(*) FROM sqlite_master WHERE name='cycle_review_index' == 1 (not 2).
}
```

### MIG-U-07: Previous migration test renamed — test_current_schema_version_is_at_least_17 (AC-02b, R-01)

The existing test `test_current_schema_version_is_17` in `migration_v16_to_v17.rs`
must be renamed to `test_current_schema_version_is_at_least_17` with predicate
`>= 17` rather than `== 17`. The Stage 3b implementor is responsible for this rename;
Stage 3c verifies it via grep.

```
grep -n 'test_current_schema_version_is_17' crates/unimatrix-store/tests/
```
Assert: zero matches (the == 17 test no longer exists).

```
grep -n 'test_current_schema_version_is_at_least_17' crates/unimatrix-store/tests/
```
Assert: one match in `migration_v16_to_v17.rs`.

---

## Schema Cascade Verification (AC-02b, R-01)

All seven cascade touchpoints must be verified. Six are grep-verifiable;
one requires reading `sqlite_parity.rs`:

### MIG-C-01: No == 17 schema_version assertions remain in crates/ (cascade grep gate)

```bash
grep -r 'schema_version.*== 17' crates/
```
Assert: **zero matches**. This is the mandatory gate check from entry #3539.

### MIG-C-02: server.rs schema version assertions reference 18

```bash
grep -n 'assert_eq!(version, 1' crates/unimatrix-server/src/server.rs
```
Assert: all found assertions show `18`, none show `17`.

### MIG-C-03: sqlite_parity.rs table-count / named-table assertion includes cycle_review_index

Verify that `tests/sqlite_parity.rs` (or `tests/sqlite_parity_specialized.rs`) contains
a reference to `cycle_review_index` in its table enumeration or count assertion.

### MIG-C-04: db.rs fresh DDL includes cycle_review_index

```bash
grep -n 'cycle_review_index' crates/unimatrix-store/src/db.rs
```
Assert: at least two matches (DDL creation + schema_version INSERT context).

### MIG-C-05: migration.rs CURRENT_SCHEMA_VERSION == 18

```bash
grep -n 'CURRENT_SCHEMA_VERSION' crates/unimatrix-store/src/migration.rs
```
Assert: shows `= 18`.

### MIG-C-06: migration.rs if current_version < 18 block exists

```bash
grep -n 'current_version < 18' crates/unimatrix-store/src/migration.rs
```
Assert: one match.

---

## v17 Database Builder Contract

The `create_v17_database` helper for the new test file must produce a database with:
- All tables from v16 (see `migration_v16_to_v17.rs::create_v16_database`)
- Plus `query_log.phase` column (the v16→v17 addition)
- **Without** `cycle_review_index` table
- `counters.schema_version = 17`

The helper directly reuses the v16 DDL from `create_v16_database` but adds the
`phase TEXT` column to the `query_log` CREATE TABLE statement. It does not call
`create_v16_database` — it must be a standalone builder for test isolation.

---

## Edge Cases

| Edge Case | Test | Expected |
|-----------|------|---------|
| `cycle_review_index` DDL in migration uses `CREATE TABLE IF NOT EXISTS` | MIG-U-06 idempotency | Idempotent; no "table already exists" error |
| v17 DB with pre-existing rows in entries, observations, cycle_events | MIG-U-05 | All rows readable; schema_version = 18 |
| Schema version already > 17 (skip-forward) | Not tested separately — covered by MIG-U-06 | Migration block is a no-op; `if current_version < 18` gate prevents re-run |
