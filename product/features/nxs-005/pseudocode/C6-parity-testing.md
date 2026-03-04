# C6: Parity Testing and Migration Tooling

## Files
- `crates/unimatrix-store/src/test_helpers.rs` (edit)
- `crates/unimatrix-store/src/sqlite/migrate_redb_to_sqlite.rs` (new, or as function in migration.rs)

## Test Helpers Changes

The TestDb struct needs to create the store with the active backend:

```rust
impl TestDb {
    pub fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        #[cfg(not(feature = "backend-sqlite"))]
        let path = dir.path().join("test.redb");
        #[cfg(feature = "backend-sqlite")]
        let path = dir.path().join("test.db");
        let store = Store::open(&path).expect("failed to open test database");
        TestDb { _dir: dir, store }
    }
}
```

All other test helper functions (TestEntry, assert_index_consistent, seed_entries) are backend-agnostic -- they use Store methods only, not internal database handles.

## Backend-Specific Test Adjustments

Some existing tests directly access `store.db` (redb Database field). These tests need cfg gates:

1. `db.rs` tests that use `store.db.begin_read()` to verify tables -> cfg-gated
   - Under SQLite: equivalent tests query `sqlite_master` or use store methods
2. `read.rs` co-access tests that use `store.db.begin_write()` to seed CO_ACCESS table
   - Under SQLite: use Store's public API (record_co_access_pairs) or add test-only seeding method
3. `injection_log.rs` test that reads `next_log_id` counter via `store.db.begin_read()`
   - Under SQLite: use `store.read_counter("next_log_id")`

Strategy: Keep redb-specific tests under `#[cfg(not(feature = "backend-sqlite"))]` and add equivalent SQLite-specific tests where needed. The bulk of tests (those using Store public API) run unchanged on both backends.

## Migration Tooling

### migrate_redb_to_sqlite(redb_path, sqlite_path) -> Result<MigrationReport>

```
open source redb Database (read-only)
open destination SQLite Store (creates fresh DB)

BEGIN IMMEDIATE on SQLite conn

-- Table 1: entries
  redb read txn -> open entries table -> iter all
  for each (id, blob):
    INSERT INTO entries (id, data) VALUES (?, ?)
  count entries

-- Table 2: topic_index
  redb -> iter topic_index
  for each ((topic, entry_id), ()):
    INSERT INTO topic_index (topic, entry_id) VALUES (?, ?)

-- Table 3: category_index (same pattern)
-- Table 4: tag_index (multimap: iter all values per key)
-- Table 5: time_index
-- Table 6: status_index
-- Table 7: vector_map
-- Table 8: counters
-- Table 9: agent_registry
-- Table 10: audit_log
-- Table 11: feature_entries (multimap)
-- Table 12: co_access
-- Table 13: outcome_index
-- Table 14: observation_metrics
-- Table 15: signal_queue
-- Table 16: sessions
-- Table 17: injection_log

COMMIT

-- Verify row counts match
for each table:
  redb_count = redb read txn -> open table -> len() or iter().count()
  sqlite_count = SELECT COUNT(*) FROM table
  if redb_count != sqlite_count:
    report.mismatches.push(table, redb_count, sqlite_count)

return report
```

### MigrationReport

```rust
pub struct MigrationReport {
    pub tables_migrated: u32,
    pub total_rows: u64,
    pub skipped_entries: Vec<(u64, String)>,  // (id, error message)
    pub row_count_mismatches: Vec<(String, u64, u64)>,  // (table, redb, sqlite)
}
```

### Error Handling

- Corrupt entry blobs: log the entry_id and error, skip, continue
- Transaction failure: return error (caller can retry or investigate)
- Empty tables: migrate as empty (valid state)

## Parity Verification Strategy

1. `cargo test -p unimatrix-store` -- all 234 tests pass on redb (regression)
2. `cargo test -p unimatrix-store --features backend-sqlite` -- all 234 tests pass on SQLite
3. `cargo test --workspace --features unimatrix-store/backend-sqlite` -- full workspace on SQLite
4. Build binary with SQLite, run infra-001 harness -- 157 tests pass
5. Migration tool test: populate redb, migrate, verify row counts

## New Risk-Specific Tests

Added to the SQLite test suite (run under --features backend-sqlite):

- R-01: u64::MAX entry_id, empty strings, empty blob boundary tests
- R-02: Concurrent access stress test (10 threads, 100 ops each)
- R-05: co_access CHECK constraint violation test
- R-07: WAL mode verification (PRAGMA journal_mode returns 'wal')
- R-10: Concurrent counter atomicity test (10 threads, verify unique IDs)

These tests are in addition to the 234 existing tests that run on both backends.
