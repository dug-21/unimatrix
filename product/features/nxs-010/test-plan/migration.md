# Test Plan: migration (C2) + backfill edge cases

## Component

`crates/unimatrix-store/src/migration.rs` -- v10->v11 migration block. Creates both tables, backfills `topic_deliveries` from `sessions.feature_cycle`, bumps schema_version to 11.

## Risks Covered

| Risk ID | Priority | Scenarios |
|---------|----------|-----------|
| R-01 | High | Migration completeness and idempotency |
| R-02 | Critical | Backfill aggregate correctness |
| R-08 | Med | Fresh database skip |
| R-14 | High | NULL ended_at duration handling |

## Test Setup Pattern: Seeding a v10 Database

Migration tests need a database at schema v10 with controlled session data. Pattern:

```rust
// 1. Create temp dir and open a fresh store (gets current schema)
// 2. Manually downgrade schema_version to 10 via raw SQL
// 3. Drop topic_deliveries and query_log if they exist (simulate v10)
// 4. Insert session rows via raw SQL
// 5. Drop and re-open Store on same path -- triggers v10->v11 migration
```

Alternatively, build a v10 database from scratch using raw SQL (create counters table, set schema_version=10, create entries + sessions tables manually). This is more precise but verbose. Use whichever approach existing migration tests in the crate follow.

## Integration Tests

### test_migration_v10_to_v11_basic

**Arrange**: Create a v10 database with 3 sessions attributed to 2 topics:
- Session "s1": feature_cycle="topic-a", started_at=1000, ended_at=1100
- Session "s2": feature_cycle="topic-a", started_at=2000, ended_at=2300
- Session "s3": feature_cycle="topic-b", started_at=3000, ended_at=3050

**Act**: Open store with v11 code (triggers migration).

**Assert**:
- `pragma_table_info('topic_deliveries')` returns 9 columns.
- `pragma_table_info('query_log')` returns 9 columns.
- `SELECT * FROM topic_deliveries WHERE topic='topic-a'`:
  - `total_sessions = 2`
  - `total_duration_secs = 400` (100 + 300)
  - `created_at = 1000` (MIN of started_at)
  - `status = 'completed'`
- `SELECT * FROM topic_deliveries WHERE topic='topic-b'`:
  - `total_sessions = 1`
  - `total_duration_secs = 50`
  - `created_at = 3000`
  - `status = 'completed'`
- `SELECT value FROM counters WHERE name='schema_version'` returns 11.
- `query_log` table is empty (no backfill for query_log).

**AC**: AC-04, AC-18, AC-19

### test_migration_v10_to_v11_idempotent

**Arrange**: Create v10 database with 1 attributed session. Open store (migration runs).
**Act**: Close and re-open store on the same path.
**Assert**:
- No error.
- `topic_deliveries` still has exactly 1 row (no duplicates).
- `schema_version = 11`.

**AC**: AC-05

### test_migration_v10_to_v11_empty_sessions

**Arrange**: Create v10 database with sessions table but zero rows.
**Act**: Open store.
**Assert**:
- Migration succeeds.
- `topic_deliveries` has 0 rows.
- `schema_version = 11`.

**AC**: AC-06

### test_migration_v10_to_v11_no_attributed_sessions

**Arrange**: Create v10 database with 3 sessions where:
- Session "s1": feature_cycle=NULL
- Session "s2": feature_cycle=""
- Session "s3": feature_cycle=NULL

**Act**: Open store.
**Assert**:
- Migration succeeds.
- `topic_deliveries` has 0 rows (all excluded by WHERE clause).
- `schema_version = 11`.

**AC**: AC-06 (variant)

### test_migration_backfill_null_ended_at_mixed

**Arrange**: Create v10 database with 3 sessions for "topic-x":
- Session "s1": started_at=1000, ended_at=1200 (duration=200)
- Session "s2": started_at=2000, ended_at=2100 (duration=100)
- Session "s3": started_at=3000, ended_at=NULL (incomplete)

**Act**: Open store.
**Assert**:
- `topic_deliveries` for "topic-x":
  - `total_sessions = 3`
  - `total_duration_secs = 300` (200+100, NULL excluded from SUM but session counted)
  - `created_at = 1000`

**AC**: AC-18 (R-14 mitigation)

### test_migration_backfill_all_null_ended_at

**Arrange**: Create v10 database with 2 sessions for "topic-y", both with ended_at=NULL.
**Act**: Open store.
**Assert**:
- `topic_deliveries` for "topic-y":
  - `total_sessions = 2`
  - `total_duration_secs = 0` (COALESCE kicks in)

**AC**: AC-18 (R-14 mitigation)

### test_migration_fresh_database_skips

**Arrange**: Open a completely fresh database (no pre-existing tables).
**Act**: Verify `migrate_if_needed` returns early (no entries table).
**Assert**:
- Store opens successfully.
- `topic_deliveries` and `query_log` created by `create_tables()` (not migration).
- `schema_version` counter initialized correctly.

**AC**: AC-20 (no regression), R-08

### test_migration_v10_to_v11_partial_rerun

**Arrange**: Create v10 database with sessions. Manually create topic_deliveries and query_log tables (simulating partial migration) but leave schema_version at 10.
**Act**: Open store (migration guard fires because version < 11).
**Assert**:
- `CREATE TABLE IF NOT EXISTS` succeeds (no error on existing tables).
- `INSERT OR IGNORE` backfill does not duplicate existing topic_deliveries rows.
- `schema_version` updated to 11.

**AC**: AC-05 (R-01 mitigation)

## Edge Cases

| Edge Case | Test |
|-----------|------|
| feature_cycle = "" (empty string) | test_migration_v10_to_v11_no_attributed_sessions |
| feature_cycle = NULL | test_migration_v10_to_v11_no_attributed_sessions |
| All ended_at = NULL | test_migration_backfill_all_null_ended_at |
| Mix of NULL and valid ended_at | test_migration_backfill_null_ended_at_mixed |
| Zero sessions | test_migration_v10_to_v11_empty_sessions |
| Re-run on v11 | test_migration_v10_to_v11_idempotent |
| Partial migration (tables exist, version not bumped) | test_migration_v10_to_v11_partial_rerun |
