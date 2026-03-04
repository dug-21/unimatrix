# Test Plan: C5 Migration

## Existing Migration Tests

From migration.rs (redb-specific, cfg-gated):
- test_fresh_db_sets_schema_version
- test_v0_to_current_migration
- test_v1_to_current_migration
- test_v2_to_current_migration
- test_v3_to_current_migration
- test_already_current_is_noop

## New SQLite Migration Tests

### AC-05: Fresh Database Schema Version
```
test_sqlite_fresh_db_has_schema_v5:
  open Store (fresh DB)
  read_counter("schema_version") -> 5
```

### AC-05: Migration Chain (v0 to v5)

Note: For SQLite, creating a DB at v0 requires manually:
1. Create DB with only the original tables
2. Set schema_version = 0
3. Insert entries in v0 format
4. Open with Store::open() which triggers migration

```
test_sqlite_migration_from_v0:
  create raw SQLite connection
  create minimal tables (entries, counters, indexes)
  set schema_version = 0
  insert entry with v0 EntryRecord format (minimal fields)
  close connection
  open with Store::open() -> triggers migration
  read_counter("schema_version") -> 5
  get(entry_id) -> has all current fields with defaults

test_sqlite_migration_empty_db_v0:
  create raw connection, set schema_version = 0, create tables
  open with Store::open()
  read_counter("schema_version") -> 5
  -- All tables exist, no entries to migrate
```

### R-04: Entry Field Preservation
```
test_sqlite_migration_preserves_entry_data:
  create DB at v0 with known entry data
  open -> migration runs
  get entry -> verify title, content, topic, category, tags, status preserved
  verify extension fields have defaults: confidence=0.0, access_count=0, etc.
```

### Table Creation Idempotency
```
test_sqlite_migration_creates_missing_tables:
  create DB at v3 (missing signal_queue, sessions, injection_log)
  open -> migration creates missing tables
  insert_signal -> works
  insert_session -> works
  insert_injection_log_batch -> works
```

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-04 | Migration chain from each version |
| R-08 | Table creation idempotency |
