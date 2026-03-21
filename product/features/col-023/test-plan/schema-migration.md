# Test Plan: schema-migration

**Component**: `crates/unimatrix-store/src/migration.rs`
**AC Coverage**: AC-09
**Risk Coverage**: R-05 (v13→v14 positional access), R-12 (rollback), FM-05 (partial migration idempotency)

---

## Unit Test Expectations

### Location: inline `#[cfg(test)]` in `crates/unimatrix-store/src/migration.rs`

All migration tests follow the existing pattern in this file: open a test database,
apply migrations, assert schema state, perform round-trips.

---

### v13 → v14 Migration Tests

### T-MIG-01: Fresh database creates schema v14 directly (AC-09)

```rust
// test_fresh_db_creates_schema_v14
// Arrange: open a fresh in-memory SQLite database
// Act: create_tables_if_needed() (not migrate_if_needed, since no entries table)
// Assert: PRAGMA user_version == 14
// Assert: OBSERVATION_METRICS table has column domain_metrics_json
//   verified by: SELECT COUNT(*) FROM pragma_table_info('OBSERVATION_METRICS')
//                WHERE name = 'domain_metrics_json' == 1
```

### T-MIG-02: v13 → v14 migration adds domain_metrics_json column (AC-09, R-05)

```rust
// test_v13_to_v14_migration_adds_column
// Arrange: construct a v13 schema database (OBSERVATION_METRICS without domain_metrics_json)
//          by calling create_schema_at_version(13) or equivalent test helper
// Act: migrate_if_needed(&mut conn, db_path)
// Assert: PRAGMA user_version == 14
// Assert: column domain_metrics_json present in OBSERVATION_METRICS
// Assert: CURRENT_SCHEMA_VERSION == 14
```

### T-MIG-03: Round-trip — write and read back all 21 original fields after v14 migration (R-05)

```rust
// test_v14_migration_round_trip_all_original_fields
// Arrange: v13 → v14 migrated database
// Act: INSERT a row into OBSERVATION_METRICS with all 21 named original fields
//      (using named column bindings, not positional)
// Act: SELECT the row back
// Assert: each of the 21 original fields reads back with the expected value
// Assert: NO positional offset — field X is not reading what used to be field X-1
// R-05: guards against positional column indexing regression.
```

### T-MIG-04: v13 row reads back NULL for domain_metrics_json (AC-09, FR-05.4)

```rust
// test_v13_row_reads_null_domain_metrics_json
// Arrange: v14 database; INSERT a row using named columns EXCLUDING domain_metrics_json
//          (simulates a row inserted by a v13 binary)
// Act: SELECT domain_metrics_json FROM OBSERVATION_METRICS WHERE ...
// Assert: value IS NULL
// Act: deserialize via get_metrics()
// Assert: MetricVector.domain_metrics.is_empty()
```

### T-MIG-05: Schema version assertion post-migration

```rust
// test_schema_version_is_14_after_migration
// Assert: after migration, SELECT value FROM counters WHERE name='schema_version' == 14
// Assert: CURRENT_SCHEMA_VERSION == 14 (Rust const)
```

---

### Idempotency and Rollback Tests

### T-MIG-06: Migration is idempotent — running twice does not error (FM-05)

```rust
// test_v13_to_v14_migration_idempotent
// Arrange: v13 database
// Act: migrate_if_needed() (first run — applies migration)
// Act: migrate_if_needed() (second run — should be a no-op)
// Assert: no error on second run
// Assert: schema_version still == 14
// Assert: only one domain_metrics_json column exists
// FM-05: handles the case where v14 migration was partially applied before a crash
// Implementation note: the ALTER TABLE ADD COLUMN check should be idempotent via
// "IF NOT EXISTS" syntax or a pragma_table_info pre-check.
```

### T-MIG-07: Rollback safety — v14 schema read by reduced struct (R-12)

```rust
// test_v14_schema_named_column_readback_with_reduced_struct
// Arrange: v14 database with a row containing domain_metrics_json = '{"k":1.0}'
// Act: run a SELECT query using only the 21 original named columns
//      (simulating a v13 binary that doesn't know about domain_metrics_json)
// Assert: all 21 original fields return correct values
// Assert: no error, no panic
// R-12: a downgraded binary using named columns must not be affected by the extra column.
// Document in test comments: "SQLite named-column queries are NOT affected by
// additional columns; only positional indexing would be broken."
```

---

## CURRENT_SCHEMA_VERSION Constant Test

### T-MIG-08: CURRENT_SCHEMA_VERSION value

```rust
// test_current_schema_version_is_14
// Assert: CURRENT_SCHEMA_VERSION == 14
// Simple constant check to catch accidental off-by-one in version bump.
```

---

## Migration Helper Pattern (from existing migration.rs)

The existing `migration.rs` tests use the pattern:
1. Open an in-memory (`":memory:"`) SQLite connection.
2. Call `create_tables_if_needed()` for fresh DB tests.
3. For migration tests: manually construct a v13 schema by calling a helper that
   creates tables without the new column.

Extend this pattern rather than creating isolated scaffolding. Use
`unimatrix_store::test_helpers::TestDb` if it provides the necessary hooks, or
directly use `sqlx::SqlitePool::connect("sqlite::memory:")` following the existing
test pattern in this file.

---

## Edge Cases

- Migration called on a database with schema_version already == 14:
  `migrate_if_needed()` returns `Ok(())` immediately (idempotent path already covered
  by T-MIG-06).
- Migration called on a fresh database (no `entries` table):
  `migrate_if_needed()` returns `Ok(())` without applying any migration (the fresh DB
  path is handled by `create_tables_if_needed()`).
- `OBSERVATION_METRICS` table absent on an otherwise-populated database:
  This is an abnormal state. Document behavior (likely an error or no-op) but do not
  add a test for it unless the implementation explicitly handles it.
