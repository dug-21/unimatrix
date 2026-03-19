# Test Plan: store-migration (unimatrix-store/src/migration.rs)

Covers: v12→v13 migration block in `run_main_migrations`; Supersedes bootstrap INSERT;
CoAccess bootstrap INSERT with threshold and weight normalization; schema_version update;
R-06 empty co_access guard; idempotency; no Contradicts at bootstrap; W1-2 promotion path.

Risks addressed: R-06 (Critical), R-08, R-13, R-15, AC-05, AC-06, AC-07, AC-08, AC-18, AC-21

---

## Test Infrastructure

All migration tests follow the existing pattern in `migration.rs`:

1. Open an in-memory or tempdir SQLite database using `SqlxStore` infrastructure
2. Manually set `schema_version` counter to 12 (bypassing `create_tables_if_needed`)
3. Call `migrate_if_needed(conn, db_path)` (or trigger via `SqlxStore::open` on the path)
4. Query the migrated database to assert outcomes

The synthetic v12 database setup helper (to be added):
```rust
async fn make_v12_db() -> (SqlxStore, tempfile::TempDir) {
    // open store (creates v13 by default via create_tables_if_needed)
    // then manually set schema_version=12 and drop the graph_edges table
    // to simulate a pre-v13 database
}
```

All tests are `#[tokio::test]` async. Use `PoolConfig::test_default()`.

---

## Test 1: v12→v13 Supersedes bootstrap (MANDATORY, AC-05, AC-06, AC-18, R-01)

### `test_v12_to_v13_supersedes_bootstrap`
- Arrange: synthetic v12 database with 3 entries:
  - entry A (id=1, supersedes=NULL)
  - entry B (id=2, supersedes=Some(1)) — B supersedes A
  - entry C (id=3, supersedes=Some(2)) — C supersedes B
- Act: run `migrate_if_needed`
- Assert:
  - `schema_version` counter = 13
  - `graph_edges` table exists
  - `SELECT COUNT(*) FROM graph_edges WHERE relation_type='Supersedes'` = 2
  - Row for B→A: `source_id=1`, `target_id=2` (source_id=entry.supersedes, target_id=entry.id)
  - Row for C→B: `source_id=2`, `target_id=3`
  - Both rows: `bootstrap_only=0`, `source='bootstrap'`, `created_by='bootstrap'`
  - `weight = 1.0` for all Supersedes rows
  - `metadata IS NULL` for all rows

---

## Test 2: R-06 — empty co_access migration succeeds (MANDATORY, R-06, AC-07)

### `test_v12_to_v13_empty_co_access_succeeds`
- Arrange: synthetic v12 database, `co_access` table has zero rows
- Act: run `migrate_if_needed`
- Assert:
  - Migration completes without error (no NULL weight constraint violation)
  - `schema_version` = 13
  - `SELECT COUNT(*) FROM graph_edges WHERE relation_type='CoAccess'` = 0
  - No `weight REAL NOT NULL` constraint violation was triggered
  - Supersedes bootstrap still ran (if any entries have supersedes set)

**This is the highest-priority migration test. Failure blocks all new deployments.**

---

## Test 3: CoAccess threshold and weight normalization (AC-07, R-15)

### `test_v12_to_v13_co_access_threshold_and_weights`
- Arrange: synthetic v12 database with:
  - `co_access` rows: `(1, 2, count=2)`, `(1, 3, count=3)`, `(1, 4, count=5)`
  - (count=2 is below threshold; count=3 and count=5 are at/above)
- Act: run `migrate_if_needed`
- Assert:
  - Pair `(1, 2)` produces NO row in `graph_edges` (count < 3)
  - Pair `(1, 3)` produces exactly one `relation_type='CoAccess'` row
  - Pair `(1, 4)` produces exactly one `relation_type='CoAccess'` row
  - Weight for `(1, 4)`: `abs(weight - 1.0) < 1e-6` (count=5 is max → normalized to 1.0)
  - Weight for `(1, 3)`: `abs(weight - 0.6) < 1e-6` (count=3/max=5 = 0.6)
  - All CoAccess rows: `bootstrap_only=0`, `source='bootstrap'`, `created_by='bootstrap'`
  - All weights in range `(0.0, 1.0]` (exclusive 0.0, inclusive 1.0)

**Flat `weight=1.0` implementation would make (1,3) weight == 1.0, failing this test.**

---

## Test 4: CoAccess all-below-threshold produces no edges

### `test_v12_to_v13_co_access_all_below_threshold`
- Arrange: `co_access` rows with count=1 and count=2 only
- Act: run `migrate_if_needed`
- Assert: zero CoAccess edges in `graph_edges`, migration completes without error

---

## Test 5: No Contradicts edges bootstrapped (AC-08)

### `test_v12_to_v13_no_contradicts_bootstrapped`
- Act: run migration on any synthetic v12 database (with or without entries)
- Assert: `SELECT COUNT(*) FROM graph_edges WHERE relation_type='Contradicts'` = 0

---

## Test 6: Idempotency — double run (R-08, AC-05)

### `test_v12_to_v13_idempotent_double_run`
- Arrange: synthetic v12 database with entries having supersedes links and co_access
- Act: run `migrate_if_needed` (migration bumps to v13)
- Act: run `migrate_if_needed` again on the same database
- Assert:
  - Row counts in `graph_edges` are identical after both runs
  - No `UNIQUE constraint failed` errors
  - `schema_version` = 13 after both runs
  - `CREATE TABLE IF NOT EXISTS` prevents DDL error on second run

---

## Test 7: Bootstrap-to-confirmed promotion path (AC-21)

### `test_v13_bootstrap_only_promotion_delete_insert`
- This test does NOT run migration; it validates the schema supports the W1-2 promotion
  pattern after migration
- Arrange: run migration to get a v13 database; insert a `bootstrap_only=1` CoAccess edge
- Act step 1: DELETE the bootstrap row by `(source_id, target_id, relation_type, bootstrap_only=1)`
- Act step 2: INSERT with same `(source_id, target_id, relation_type)` and `bootstrap_only=0`
- Assert: final row has `bootstrap_only=0`, `source='nli'`
- Act step 3: repeat the INSERT (idempotency via `INSERT OR IGNORE`)
- Assert: still exactly one row, no error

---

## Test 8: Fresh database with no entries (edge case from RISK-TEST-STRATEGY)

### `test_v12_to_v13_empty_entries_and_co_access`
- Arrange: synthetic v12 database with zero entries, zero co_access rows
- Act: run `migrate_if_needed`
- Assert:
  - Migration completes without error
  - `schema_version` = 13
  - `graph_edges` exists with zero rows

---

## Test 9: CURRENT_SCHEMA_VERSION constant = 13 (AC-18)

### `test_current_schema_version_is_13`
- Assert: `CURRENT_SCHEMA_VERSION == 13` (compile-time constant check)

---

## Test 10: Bootstrap inserts use direct SQL, not analytics queue (R-13, code inspection)

### `inspect_migration_no_analytics_write_calls`
- Code inspection gate: `grep` for `AnalyticsWrite` references in `migration.rs`
- Assert: zero occurrences — bootstrap inserts use raw sqlx queries, not the analytics queue
- Document: R-13 accepted risk is eliminated for bootstrap path by this design

---

## Key Assertion Values for Weight Tests

| co_access count | MAX(count) in set | normalized weight | Expected |
|----------------|-------------------|------------------|---------|
| 5 | 5 | 5/5 = 1.0 | 1.0 |
| 3 | 5 | 3/5 = 0.6 | 0.6 |
| 2 | 5 | excluded by WHERE count>=3 | no row |
| (empty table) | NULL → COALESCE → 1.0 | no rows pass WHERE | 0 rows |

---

## Test Module Location

All tests in `crates/unimatrix-store/src/migration.rs` `#[cfg(test)]` module.
Follow the `run_main_migrations` async test pattern. Use `#[tokio::test]`.
Use tempdir databases for tests that need file-path backup behavior; `:memory:` is fine
for all tests in this plan (no file backup needed).
