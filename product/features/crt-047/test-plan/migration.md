# Test Plan: Migration v23 → v24

Components:
- `crates/unimatrix-store/src/migration.rs` — v23→v24 block, `CURRENT_SCHEMA_VERSION = 24`
- `crates/unimatrix-store/src/db.rs` — fresh-schema DDL update
Risk coverage: R-03, R-10, AC-01, AC-14, AC-R04

---

## What Is Under Test

1. The v23→v24 migration block in `migration.rs` adds all seven new columns to
   `cycle_review_index` using per-column `pragma_table_info` pre-checks (ADR-004).
2. `CURRENT_SCHEMA_VERSION` is bumped from `23` to `24`.
3. `db.rs` fresh-schema DDL is updated to include all seven columns.
4. Pre-existing `cycle_review_index` rows get `DEFAULT 0` on all seven new columns.
5. Migration is atomic: partial failure leaves schema at v23; retry from v23 completes.

---

## New Test File: `crates/unimatrix-store/tests/migration_v23_to_v24.rs`

Pattern follows `migration_v22_to_v23.rs` exactly. The v23 database builder
(`create_v23_database()`) must include all DDL from the v22 test PLUS the compound
index `idx_entry_tags_tag_entry_id`, and must NOT include the seven new columns on
`cycle_review_index`. Seeds at least one `cycle_review_index` row to prove DEFAULT 0
on existing rows.

### MIG-V24-U-01: `CURRENT_SCHEMA_VERSION` is `>= 24`

```
test_current_schema_version_is_at_least_24
```

```rust
assert!(
    unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 24,
    "CURRENT_SCHEMA_VERSION must be >= 24 after crt-047"
);
```

### MIG-V24-U-02: Fresh database initializes directly to v24 (AC-R04)

```
test_fresh_db_creates_schema_v24
```

- Open a fresh `SqlxStore` (no prior DB file).
- Assert `read_schema_version(&store) == 24`.
- Assert `pragma_table_info('cycle_review_index')` includes all seven new columns.

### MIG-V24-U-03: v23→v24 migration adds all seven columns (AC-14, AC-01)

```
test_v23_to_v24_migration_adds_all_seven_columns
```

This is the primary AC-14 test. Must use `Store::open()` not the migration function
in isolation.

- Arrange: call `create_v23_database(&db_path)` to create a v23-shaped DB.
  Seed one `cycle_review_index` row using the v23 schema (5 columns only).
- Act: `SqlxStore::open(&db_path, PoolConfig::test_default()).await`.
- Assert `read_schema_version(&store) == 24`.
- For each of the seven new columns, query `pragma_table_info` and assert presence:

```rust
for col in &[
    "corrections_total", "corrections_agent", "corrections_human",
    "corrections_system", "deprecations_total", "orphan_deprecations",
    "first_computed_at",
] {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index') WHERE name = ?1"
    )
    .bind(col)
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma_table_info");
    assert_eq!(count, 1, "column {col} must exist after v23→v24 migration");
}
```

- Assert: the pre-existing `cycle_review_index` row has value `0` for all seven new columns:

```rust
let row = sqlx::query(
    "SELECT corrections_total, corrections_agent, corrections_human,
            corrections_system, deprecations_total, orphan_deprecations,
            first_computed_at
     FROM cycle_review_index WHERE feature_cycle = 'pre-existing-cycle'"
)
.fetch_one(store.read_pool_test())
.await
.expect("pre-existing row");
// Assert all seven are 0 (SQLite DEFAULT 0 on ALTER TABLE ADD COLUMN).
for col_idx in 0..7i32 {
    assert_eq!(row.get::<i64, _>(col_idx), 0);
}
```

### MIG-V24-U-04: Idempotency — re-open v24 database is a no-op

```
test_v24_migration_idempotent
```

- Arrange: `create_v23_database()`, then `SqlxStore::open()` to migrate to v24.
- Close store.
- Act: `SqlxStore::open()` again on the same file.
- Assert: `read_schema_version(&store2) >= 24`.
- Assert: no panic, no error.

### MIG-V24-U-05: Partial column pre-existence — idempotency (R-03, ADR-004)

```
test_v24_migration_idempotent_when_some_columns_pre_exist
```

- Arrange: create v23 DB, then manually add three of seven columns via raw SQLite
  (simulating a crashed mid-migration):

```rust
sqlx::query("ALTER TABLE cycle_review_index ADD COLUMN corrections_total INTEGER NOT NULL DEFAULT 0")
    .execute(&mut conn).await.expect("partial");
// ...add corrections_agent, corrections_human...
```

  Leave the remaining four columns absent. Schema_version counter stays at 23.
- Act: `SqlxStore::open()` — must complete the migration without error.
- Assert: all seven columns present.
- Assert: `read_schema_version == 24`.
- This directly tests the `pragma_table_info` pre-check path (ADR-004).

---

## Cascade Touchpoints (R-10, AC-R04, Entry #4125)

These existing tests break when `CURRENT_SCHEMA_VERSION` advances to 24. All must be
updated before `cargo test --workspace` is run after the bump.

### 1. `migration_v22_to_v23.rs` — assertion relaxation

| Current test | Required change |
|-------------|-----------------|
| `test_fresh_db_creates_schema_v23`: `assert_eq!(read_schema_version, 23)` | Change to `assert!(read_schema_version >= 23)` — idempotency assertion must not hardcode version |
| `test_v22_to_v23_migration_creates_compound_index`: `assert_eq!(read_schema_version, 23)` | Change to `assert!(read_schema_version >= 23)` |
| `test_v23_migration_idempotent` (second-open assertion): `assert_eq!(read_schema_version, 23, "schema_version must remain 23 on re-open")` | Change to `assert!(read_schema_version >= 23, ...)` |

Pattern from entry #4125: **every** `assert_eq!(read_schema_version, 23)` in all prior
migration test files must become `assert!(... >= 23)`.

### 2. `sqlite_parity.rs` — two assertions

| Test | Required change |
|------|-----------------|
| `test_schema_version_is_N` | Assert `24` instead of `23` |
| `test_schema_column_count` (for `cycle_review_index`) | Update column count by `+7` (the seven new columns) |

### 3. `server.rs` — schema version assertion sites

The v23 assertion appears at two sites in `server.rs` (pattern: `assert_eq!(version, 23)`
or similar). Both must be updated to `24`.

**Pre-delivery check** (AC-R04): `grep -r 'schema_version.*== 23' crates/` must return
zero matches after the bump. This includes comments — fix any comment that contains the
literal `== 23` pattern.

---

## `db.rs` Fresh-Schema DDL Verification

The fresh-schema DDL in `db.rs` must include all seven new columns with `DEFAULT 0`.
This is verified by `MIG-V24-U-02` (fresh DB initializes to v24 with all columns).

Additional assertion in `sqlite_parity.rs`:

```
test_create_tables_cycle_review_index_schema
```

- Assert `pragma_table_info('cycle_review_index')` returns the correct column count
  (existing columns + 7 new) and that each new column has `dflt_value = 0`.

---

## v23 Database Builder Reference

`create_v23_database()` in `migration_v23_to_v24.rs` must produce the v22 schema
(from `migration_v22_to_v23.rs`) PLUS:
- `CREATE INDEX idx_entry_tags_tag_entry_id ON entry_tags(tag, entry_id)` (added by v23)
- `INSERT INTO counters ... ('schema_version', 23)`
- At least one row in `cycle_review_index` (5-column shape):

```sql
INSERT INTO cycle_review_index
    (feature_cycle, schema_version, computed_at, raw_signals_available, summary_json)
VALUES
    ('pre-existing-cycle', 1, 1700000000, 1, '{"test":true}')
```

This seeded row proves DEFAULT 0 lands correctly on the seven new columns after migration.
