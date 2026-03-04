# Test Plan: C1 Connection Manager

## Existing Tests (run on both backends)

These tests from db.rs must pass unchanged:
- test_open_creates_file -> verifies DB file created at path
- test_open_with_custom_cache -> verifies config accepted
- test_compact_succeeds -> verifies compact() returns Ok
- test_store_is_send_sync -> static assertion

## Modified Tests (cfg-gated)

- test_open_creates_all_tables: redb version opens table handles; SQLite version queries sqlite_master
- test_open_already_open_returns_database_error: redb-specific (DatabaseAlreadyOpen). SQLite allows multiple opens (WAL mode). This test is redb-only.

## New SQLite-Specific Tests

### AC-01: Schema Introspection
```
test_sqlite_all_17_tables_exist:
  open Store
  query: SELECT name FROM sqlite_master WHERE type='table' ORDER BY name
  verify all 17 table names present:
    agent_registry, audit_log, category_index, co_access, counters,
    entries, feature_entries, injection_log, observation_metrics,
    outcome_index, sessions, signal_queue, status_index, tag_index,
    time_index, topic_index, vector_map
```

### AC-07: WAL Mode
```
test_sqlite_wal_mode_enabled:
  open Store
  query: PRAGMA journal_mode -> should return "wal"
```

### AC-07: Concurrent Read+Write
```
test_sqlite_concurrent_read_write:
  open Store
  spawn writer thread: insert 100 entries
  spawn reader thread: query_by_status 100 times
  join both -> no errors, no SQLITE_BUSY
```

### AC-14: Feature Flag Compilation
```
test_sqlite_feature_flag_compiles:
  -- verified by cargo check --features backend-sqlite succeeding
  -- not a runtime test, but a build-system gate
```

### FR-09: Compact No-Op
```
test_sqlite_compact_is_noop:
  open Store
  insert 10 entries
  store.compact() -> Ok(())
  get all 10 entries -> still present (compact didn't delete anything)
```

### Counters Initialization
```
test_sqlite_counters_initialized:
  open Store
  read_counter("next_entry_id") -> 1 (first ID)
  read_counter("next_signal_id") -> 0
  read_counter("next_log_id") -> 0
  read_counter("schema_version") -> 5
```

### PRAGMA Verification
```
test_sqlite_pragmas:
  open Store
  PRAGMA synchronous -> 1 (NORMAL)
  PRAGMA busy_timeout -> 5000
  PRAGMA cache_size -> -16384
  PRAGMA foreign_keys -> 0 (OFF)
```

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-08 (cfg gaps) | Feature flag compilation check |
| R-01 (semantic) | Schema introspection, WAL mode |
| R-02 (deadlock) | Concurrent read+write |
