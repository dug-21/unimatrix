# Test Plan: Schema Migration (Component 7)

File: `crates/unimatrix-store/tests/migration_v14_to_v15.rs` (new file)
Risks: R-05, R-10, AC-10, AC-11, FR-07, NFR-05

---

## Unit Test Expectations

One sync test verifies the Rust constant without I/O:

**`test_current_schema_version_is_15`**
- Assert: `unimatrix_store::migration::CURRENT_SCHEMA_VERSION == 15`
- Purpose: catches accidental off-by-one in the version bump (mirrors `test_current_schema_version_is_14`)

---

## Integration Test Expectations

Pattern: follows `crates/unimatrix-store/tests/migration_v13_to_v14.rs` exactly.
All tests use `tempfile::TempDir`, `SqlxStore::open`, and direct SQL queries via
`store.read_pool_test()` / `store.write_pool_test()`.

### Helper: `create_v14_database(path)` (private async function)

Build a v14-shaped database at the given path:
- All tables from v14 DDL (mirrors `create_v13_database` plus the `domain_metrics_json` column
  on `observation_metrics`)
- `feature_entries` table WITHOUT a `phase` column (that is what v14→v15 adds)
- NO `cycle_events` table (that is the other v14→v15 addition)
- Counter: `schema_version = 14`

### T-MIG-01: Fresh database creates schema v15 (AC-11, R-10)

**`test_fresh_db_creates_schema_v15`**
- Arrange: empty path (no prior DB)
- Act: `SqlxStore::open(&path, PoolConfig::default())`
- Assert: `schema_version == 15`
- Assert: `cycle_events` table exists (`SELECT COUNT(*) FROM sqlite_master WHERE name='cycle_events'` > 0)
- Assert: `feature_entries` has `phase` column
  (`SELECT COUNT(*) FROM pragma_table_info('feature_entries') WHERE name = 'phase'` == 1)

**`test_fresh_db_cycle_events_table_schema`** (R-10, FR-07.2)
- Open fresh DB
- Verify `cycle_events` DDL: columns `id`, `cycle_id`, `seq`, `event_type`, `phase`, `outcome`,
  `next_phase`, `timestamp` all present via `pragma_table_info('cycle_events')`
- Verify index `idx_cycle_events_cycle_id` exists
  (`SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_cycle_events_cycle_id'`)

### T-MIG-02: v14 → v15 adds `cycle_events` table and `feature_entries.phase` column (AC-10, R-05)

**`test_v14_to_v15_migration_adds_cycle_events_table`**
- Arrange: `create_v14_database(&path)` (no `cycle_events` table)
- Act: `SqlxStore::open(...)` triggers v14→v15 migration
- Assert: `cycle_events` table now exists
- Assert: `schema_version == 15`

**`test_v14_to_v15_migration_adds_phase_column_to_feature_entries`**
- Arrange: v14 database (no `phase` column on `feature_entries`)
- Act: open store
- Assert: `feature_entries.phase` column exists after migration

**`test_v14_pre_existing_rows_have_null_phase`** (C-05, FR-06.4)
- Arrange: v14 database with a pre-seeded `feature_entries` row:
  `INSERT INTO feature_entries (feature_id, entry_id) VALUES ('old-feature', 99)`
- Act: open store to trigger migration
- Query: `SELECT phase FROM feature_entries WHERE entry_id = 99`
- Assert: `phase IS NULL` — no backfill, correct historical data

### T-MIG-03: Idempotency (AC-10, R-05, NFR-05)

**`test_v14_to_v15_migration_idempotent`**
- Run 1: open v14 database → migration applies → schema_version=15
- Run 2: open the same database again → migration skips (already at v15)
- Assert: second open succeeds without error
- Assert: `schema_version == 15` (unchanged)
- Assert: exactly one `phase` column in `feature_entries`
  (`SELECT COUNT(*) FROM pragma_table_info('feature_entries') WHERE name='phase'` == 1)
- Assert: exactly one `cycle_events` table in `sqlite_master`

**`test_pragma_table_info_guard_prevents_duplicate_column`** (NFR-05, C-08)
- Manually add `phase` column to `feature_entries` on a v14 DB before opening
- Open the store (migration sees column already exists, skips ALTER TABLE)
- Assert: no error, `schema_version = 15`, column exists exactly once

### T-MIG-04: Schema version correct after migration (AC-10)

**`test_schema_version_is_15_after_migration`**
- Arrange: v14 database
- Act: open store
- Assert: `SELECT value FROM counters WHERE name = 'schema_version'` == 15
- Assert: `CURRENT_SCHEMA_VERSION` Rust const == 15

### T-MIG-05: Data round-trip after migration (regression — no column offset issue)

**`test_v15_feature_entries_round_trip_with_phase`**
- After migration, insert a row:
  `INSERT INTO feature_entries (feature_id, entry_id, phase) VALUES ('crt-025', 1, 'scope')`
- Select: `SELECT feature_id, entry_id, phase FROM feature_entries WHERE entry_id = 1`
- Assert: `feature_id = "crt-025"`, `entry_id = 1`, `phase = "scope"`

**`test_v15_cycle_events_round_trip`**
- Insert a cycle event row via `store.insert_cycle_event(...)`
- Query by `cycle_id`
- Assert all columns round-trip correctly (event_type, phase, outcome, next_phase, seq, timestamp)

---

## Edge Cases

| Edge Case | Test | Expected Outcome |
|-----------|------|-----------------|
| Pre-v15 `feature_entries` rows | `test_v14_pre_existing_rows_have_null_phase` | `phase IS NULL` (correct, no backfill) |
| Migration run twice | `test_v14_to_v15_migration_idempotent` | No error; schema unchanged |
| Fresh DB (no migration needed) | `test_fresh_db_creates_schema_v15` | v15 created directly |
| `phase` column already exists before migration | `test_pragma_table_info_guard_prevents_duplicate_column` | pragma guard skips ALTER TABLE |
| `CURRENT_SCHEMA_VERSION` constant | `test_current_schema_version_is_15` | 15 |

---

## Critical Implementation Pattern

Migration must follow the `pragma_table_info` pre-check pattern (C-08, pattern #1264):

```rust
// v14 → v15: add phase column to feature_entries
let has_phase_column: bool = sqlx::query_scalar::<_, i64>(
    "SELECT COUNT(*) FROM pragma_table_info('feature_entries') WHERE name = 'phase'",
).fetch_one(&mut **txn).await.map(|c| c > 0).unwrap_or(false);

if !has_phase_column {
    sqlx::query("ALTER TABLE feature_entries ADD COLUMN phase TEXT")
        .execute(&mut **txn).await?;
}
```

The `cycle_events` table uses `CREATE TABLE IF NOT EXISTS` which is inherently idempotent.
