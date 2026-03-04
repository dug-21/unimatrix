# Test Plan: C6 Parity Testing and Migration Tooling

## Parity Verification

### AC-02: All 234 Store Tests on SQLite
```
cargo test -p unimatrix-store --features backend-sqlite
-- All 234 tests pass
```

### AC-02: All 234 Store Tests on redb (Regression)
```
cargo test -p unimatrix-store
-- All 234 tests still pass (no redb regression)
```

### AC-03: Full Workspace on SQLite
```
cargo test --workspace --features unimatrix-store/backend-sqlite
-- All workspace tests pass
```

### AC-14: Dual Compilation Check
```
cargo check -p unimatrix-store
cargo check -p unimatrix-store --features backend-sqlite
cargo check --workspace
cargo check --workspace --features unimatrix-store/backend-sqlite
-- All four succeed
```

### AC-16: infra-001 Full Harness
```
cargo build --release --features unimatrix-store/backend-sqlite
cd product/test/infra-001
python -m pytest suites/ -v --timeout=60
-- All 157 tests pass across 8 suites
```

## Migration Tool Tests

### AC-06: Healthy Migration
```
test_migrate_redb_to_sqlite:
  create redb Store, populate with:
    - 10 entries across topics/categories/tags
    - 5 co-access pairs
    - 3 vector mappings
    - 2 sessions with injection logs
    - 1 signal
    - 1 observation metric
  run migrate_redb_to_sqlite(redb_path, sqlite_path)
  verify: row counts match per table (17 tables)
  verify: entries deserialize correctly from SQLite
```

### R-09: Empty Database Migration
```
test_migrate_empty_redb:
  create redb Store (empty, just tables)
  run migration
  verify: SQLite DB has all 17 tables, 0 rows each
  verify: report shows 0 rows migrated
```

### R-09: Corrupt Entry Handling
```
test_migrate_corrupt_entry:
  create redb Store, insert valid entry
  directly corrupt one entry's blob in redb
  run migration
  verify: valid entries migrated, corrupt entry skipped
  verify: report includes skipped entry details
```

## New Risk Tests (SQLite-specific)

### R-01: u64 Boundary
```
test_sqlite_u64_max_counter:
  -- Test that counters work near u64::MAX
  -- (Not directly testable by inserting u64::MAX entries,
  --  but verify counter arithmetic doesn't overflow)
```

### R-02: Poison Recovery
```
test_sqlite_mutex_poison_recovery:
  open Store (Arc)
  spawn thread that panics while holding lock (via insert that panics)
  next operation from main thread:
    -- unwrap_or_else(|e| e.into_inner()) recovers the mutex
    -- operation succeeds
```

### R-08: Clippy Clean
```
cargo clippy -p unimatrix-store --features backend-sqlite -- -D warnings
-- Zero warnings
```

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-01 | Full parity suite + boundary tests |
| R-02 | Poison recovery test |
| R-03 | Workspace compile-check |
| R-08 | Dual-backend compile + clippy |
| R-09 | Migration tool with healthy, empty, corrupt inputs |
