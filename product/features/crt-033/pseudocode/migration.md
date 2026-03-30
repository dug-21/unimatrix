# Pseudocode: migration v17→v18

## Purpose

Schema migration from v17 to v18 introducing the `cycle_review_index` table.
All seven cascade touchpoints must be updated (R-01, AC-02b, entry #3539).

## Touchpoint Map (All 7 Required)

| # | File | Change |
|---|------|--------|
| 1 | `migration.rs` | `CURRENT_SCHEMA_VERSION` constant: 17 → 18 |
| 2 | `migration.rs` | Add `if current_version < 18` block in `run_main_migrations()` |
| 3 | `db.rs` | Add `cycle_review_index` DDL to `create_tables_if_needed()` |
| 4 | `db.rs` | `schema_version` INSERT binds `CURRENT_SCHEMA_VERSION` already via `crate::migration::CURRENT_SCHEMA_VERSION as i64` — no literal to change |
| 5 | `tests/sqlite_parity.rs` (or `sqlite_parity_specialized.rs`) | Update table-count and named-table assertions |
| 6 | `crates/unimatrix-server/src/server.rs` | Update `assert_eq!(version, N)` assertions from 17 to 18 |
| 7 | Previous migration test file | Rename `test_current_schema_version_is_17` → `test_current_schema_version_is_at_least_17` with `>= 17` assertion |

Gate check (must pass before merge, AC-02b):
    `grep -r 'schema_version.*== 17' crates/` MUST return zero matches

Note on touchpoint 4: `db.rs` uses `bind(crate::migration::CURRENT_SCHEMA_VERSION as i64)` already
— the INSERT automatically picks up the bumped constant. Verify this is the case and no literal `17`
is present in the `create_tables_if_needed()` schema_version INSERT.

---

## migration.rs Changes

### Constant Bump

```
// BEFORE:
pub const CURRENT_SCHEMA_VERSION: u64 = 17;

// AFTER:
/// Current schema version. Incremented from 17 to 18 by crt-033 (CYCLE_REVIEW_INDEX).
pub const CURRENT_SCHEMA_VERSION: u64 = 18;
```

### New Migration Block in run_main_migrations()

Insert this block AFTER the existing `if current_version < 17` block and BEFORE
the final `INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)`
statement at the end of `run_main_migrations()`.

```
// v17 → v18: cycle_review_index table (crt-033).
//
// Stores memoized RetrospectiveReport JSON keyed by feature_cycle.
// Used as a purge gate by GH #409 (retention pass).
// CREATE TABLE IF NOT EXISTS: idempotent on re-run (NFR-06).
if current_version < 18 {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cycle_review_index (
            feature_cycle         TEXT    PRIMARY KEY,
            schema_version        INTEGER NOT NULL,
            computed_at           INTEGER NOT NULL,
            raw_signals_available INTEGER NOT NULL DEFAULT 1,
            summary_json          TEXT    NOT NULL
        )"
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?

    // Update schema_version to 18 before the final INSERT OR REPLACE below.
    // (The final statement at end of run_main_migrations already upserts to
    //  CURRENT_SCHEMA_VERSION, so this intermediate UPDATE is redundant but
    //  follows the existing v16→v17 pattern for consistency.)
    sqlx::query("UPDATE counters SET value = 18 WHERE name = 'schema_version'")
        .execute(&mut **txn)
        .await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?
}
```

The final `INSERT OR REPLACE INTO counters (name, value) VALUES ('schema_version', ?1)`
that already exists at the end of `run_main_migrations()` will bind 18 (the new
`CURRENT_SCHEMA_VERSION`). No change needed to that statement — it is parameterized.

---

## db.rs Changes

### create_tables_if_needed() — Add cycle_review_index DDL

The DDL in `create_tables_if_needed()` MUST mirror the migration block exactly
(same column names, same types, same DEFAULT, same PRIMARY KEY). Any divergence
between the two paths creates a fresh-database/migrated-database inconsistency.

Append after the last existing table DDL block (cycle_events or query_log,
whichever is last) and before the `schema_version` INSERT:

```
// cycle_review_index: memoized RetrospectiveReport archive (crt-033).
// No FOREIGN KEY clause — consistent with all other tables (C-09).
sqlx::query(
    "CREATE TABLE IF NOT EXISTS cycle_review_index (
        feature_cycle         TEXT    PRIMARY KEY,
        schema_version        INTEGER NOT NULL,
        computed_at           INTEGER NOT NULL,
        raw_signals_available INTEGER NOT NULL DEFAULT 1,
        summary_json          TEXT    NOT NULL
    )"
)
.execute(&mut *conn)
.await?
```

### schema_version INSERT — No Literal Change Needed

The existing INSERT already uses the constant:
```
sqlx::query("INSERT OR IGNORE INTO counters (name, value) VALUES ('schema_version', ?1)")
    .bind(crate::migration::CURRENT_SCHEMA_VERSION as i64)
    .execute(&mut *conn)
    .await?
```
Bumping `CURRENT_SCHEMA_VERSION` to 18 automatically propagates here. Verify
no literal `17` appears anywhere in this function.

---

## New Migration Integration Test

File: `tests/migration_v17_to_v18.rs`
Pattern: follows `tests/migration_v16_to_v17.rs`

```
FUNCTION test_v17_to_v18_migration_creates_table():
    // Step 1: Build a v17-shaped database using the v17 DDL snapshot.
    // Use test_helpers to create a temp DB and insert the v17 schema manually
    // (all tables except cycle_review_index, schema_version counter = 17).

    // Step 2: Open the v17 database with SqlxStore::open().
    // Migration should run automatically.

    // Step 3: Assert cycle_review_index exists.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'cycle_review_index'"
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("sqlite_master query failed")

    assert_eq!(count, 1, "cycle_review_index table must exist after v17→v18 migration")

    // Step 4: Assert all five columns exist.
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('cycle_review_index') ORDER BY cid"
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("pragma_table_info failed")

    assert!(columns.contains(&"feature_cycle".to_string()))
    assert!(columns.contains(&"schema_version".to_string()))
    assert!(columns.contains(&"computed_at".to_string()))
    assert!(columns.contains(&"raw_signals_available".to_string()))
    assert!(columns.contains(&"summary_json".to_string()))

    // Step 5: Assert schema_version counter = 18.
    let version: i64 = sqlx::query_scalar(
        "SELECT value FROM counters WHERE name = 'schema_version'"
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("schema_version query failed")

    assert_eq!(version, 18)

    // Step 6: Assert pre-existing rows in other tables are unchanged
    // (e.g., entries count unchanged, no data loss).

FUNCTION test_v17_to_v18_migration_idempotent():
    // Open the same DB twice. Assert no error and schema_version = 18 both times.
    // CREATE TABLE IF NOT EXISTS guarantees idempotency (NFR-06).

FUNCTION test_current_schema_version_is_18():
    // Assert CURRENT_SCHEMA_VERSION == 18 at compile time via constant check.
    assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 18)

FUNCTION test_current_schema_version_is_at_least_17():
    // Rename from test_current_schema_version_is_17 (existing test, touchpoint 7).
    // Change assertion from == 17 to >= 17.
    assert!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 17)
```

---

## Error Handling

| Scenario | Response |
|----------|----------|
| `CREATE TABLE IF NOT EXISTS` fails (disk full, permissions) | `Err(StoreError::Migration{...})` — transaction rolls back; DB stays at v17 |
| `UPDATE counters SET value = 18` fails | Same — transaction rolls back |
| Migration runs twice (idempotent) | `IF NOT EXISTS` swallows no-op; counter upsert is idempotent |
| v17 DB opened with v18 code | Migration runs once, then `current_version >= CURRENT_SCHEMA_VERSION` early-return |

## Key Test Scenarios

1. Fresh database: `cycle_review_index` exists with all 5 columns, `schema_version = 18`.
2. v17 database migrated: `cycle_review_index` created, pre-existing tables intact.
3. Migration twice on same DB: no error, `schema_version = 18`.
4. `CURRENT_SCHEMA_VERSION == 18` unit test.
5. Previous `test_current_schema_version_is_17` renamed to `>= 17` predicate.
6. `sqlite_parity` tests include `cycle_review_index` in named-table list.
7. `server.rs` `assert_eq!(version, N)` updated to 18.
8. CI grep check: `grep -r 'schema_version.*== 17' crates/` returns zero matches.
