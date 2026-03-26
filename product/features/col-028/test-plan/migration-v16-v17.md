# Test Plan: Schema Migration v16→v17 (Component 4/5)
# Files: crates/unimatrix-store/src/migration.rs, analytics.rs, query_log.rs
# New test file: crates/unimatrix-store/tests/migration_v16_to_v17.rs

## Risks Addressed

| Risk | AC | Priority |
|------|-----|----------|
| R-02 Positional column index drift | AC-17, AC-21 | Critical |
| R-05 Schema version cascade | AC-13, AC-22 | High |
| R-06 UDS compile break | AC-23 | High |
| R-11 Migration idempotency | AC-15, T-V17-04 | Medium |
| R-12 Pre-existing row deserialization | AC-18, T-V17-05 | Medium |

---

## Pattern to Follow

The migration test file must follow the structure of
`crates/unimatrix-store/tests/migration_v15_to_v16.rs` exactly:

1. A v16 database builder function `create_v16_database(path: &Path)` that creates
   all tables as they existed at v16 (with `query_log` having NO `phase` column),
   seeding counters at version 16.
2. Helper functions for common assertions: `read_schema_version`, `phase_column_exists`.
3. Tests named `T-V17-01` through `T-V17-06` matching the six required scenarios.

**Key v16 shape**: `query_log` table at v16 has columns:
`query_id, session_id, query_text, ts, result_count, result_entry_ids, similarity_scores, retrieval_mode, source`
(9 columns, no `phase`). The `create_v16_database` function must use this schema.

---

## migration_v16_to_v17.rs Tests

### Crate-level setup

```rust
#![cfg(feature = "test-support")]

use std::path::Path;
use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

async fn create_v16_database(path: &Path) { ... }

async fn read_schema_version(store: &SqlxStore) -> i64 { ... }

async fn phase_column_exists(store: &SqlxStore) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check query_log.phase column");
    count > 0
}
```

---

### T-V17-01: Fresh database creates schema v17 directly (AC-14, AC-13)

```rust
#[tokio::test]
async fn test_fresh_db_creates_schema_v17() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: empty path — no prior DB.
    // Act: SqlxStore::open calls create_tables_if_needed() for fresh DBs.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Assert: schema_version == 17
    assert_eq!(read_schema_version(&store).await, 17,
        "fresh database must be at schema v17");

    // Assert: phase column present (fresh schema has full DDL including phase)
    assert!(phase_column_exists(&store).await,
        "fresh database must have query_log.phase column");

    store.close().await.unwrap();
}
```

---

### T-V17-02: v16→v17 migration adds phase column (AC-14)

```rust
#[tokio::test]
async fn test_v16_to_v17_migration_adds_phase_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v16 database — query_log exists, phase column absent.
    create_v16_database(&db_path).await;

    // Act: open triggers v16→v17 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store after v16→v17 migration");

    // Assert: phase column now exists.
    assert!(phase_column_exists(&store).await,
        "query_log.phase column must exist after v16→v17 migration (AC-14)");

    // Assert: schema_version == 17.
    assert_eq!(read_schema_version(&store).await, 17);

    store.close().await.unwrap();
}
```

---

### T-V17-03: idx_query_log_phase index present after migration (AC-14)

```rust
#[tokio::test]
async fn test_v16_to_v17_migration_creates_phase_index() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v16_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Check index exists via sqlite_master
    let index_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='index' AND name='idx_query_log_phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check idx_query_log_phase");

    assert_eq!(index_exists, 1,
        "idx_query_log_phase must be created by v16→v17 migration");

    store.close().await.unwrap();
}
```

---

### T-V17-04: Idempotency — running migration twice succeeds (AC-15)

```rust
#[tokio::test]
async fn test_v16_to_v17_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v16_database(&db_path).await;

    // Run 1: applies v16→v17 migration.
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("first open");
        assert!(phase_column_exists(&store).await);
        assert_eq!(read_schema_version(&store).await, 17);
        store.close().await.unwrap();
    }

    // Run 2: must be a no-op — no errors, no duplicate column.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open must succeed (idempotency)");

    assert_eq!(read_schema_version(&store).await, 17);

    // Exactly one phase column (pragma_table_info guard prevents duplicate ALTER)
    let col_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count phase columns");
    assert_eq!(col_count, 1, "exactly one phase column after idempotent run");

    store.close().await.unwrap();
}
```

---

### T-V17-05: Pre-existing rows have phase=None after migration (AC-18)

```rust
#[tokio::test]
async fn test_v16_pre_existing_query_log_rows_have_null_phase() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v16 database with a pre-seeded query_log row (v16 columns only).
    create_v16_database(&db_path).await;
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        // Insert with 8 columns (no phase — this is the v16 schema)
        sqlx::query(
            "INSERT INTO query_log \
             (session_id, query_text, ts, result_count, \
              result_entry_ids, similarity_scores, retrieval_mode, source) \
             VALUES ('pre-migration-session', 'test query', 1700000000, 0, \
                     NULL, NULL, 'semantic', 'mcp')",
        )
        .execute(&mut conn)
        .await
        .expect("insert pre-existing row");
    }

    // Act: open triggers v16→v17 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: read the row back using the updated scan function.
    use unimatrix_store::query_log::scan_query_log_by_session; // or equivalent
    let rows = scan_query_log_by_session(&store, "pre-migration-session")
        .await
        .expect("scan_query_log_by_session must not error");

    assert_eq!(rows.len(), 1, "exactly one pre-existing row");
    assert!(
        rows[0].phase.is_none(),
        "pre-existing query_log row must have phase = None after migration (no backfill)"
    );

    store.close().await.unwrap();
}
```

---

### T-V17-06: schema_version counter = 17 after migration (AC-13, AC-19)

```rust
#[tokio::test]
async fn test_schema_version_is_17_after_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v16_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Assert: counters table carries schema_version = 17.
    assert_eq!(read_schema_version(&store).await, 17);

    // Assert: Rust const agrees.
    assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 17);

    store.close().await.unwrap();
}
```

---

## AC-13: CURRENT_SCHEMA_VERSION Unit Test (in migration.rs)

This constant check belongs in `migration.rs` as a `#[test]` function, following the
same pattern as `test_current_schema_version_is_16` in `migration_v15_to_v16.rs`.

```rust
#[test]
fn test_current_schema_version_is_17() {
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        17,
        "CURRENT_SCHEMA_VERSION must be 17"
    );
}
```

Note: The function `test_current_schema_version_is_16` in `migration_v15_to_v16.rs`
must be renamed to `test_current_schema_version_is_17` with its assertion updated.
AC-22 grep check (`grep -r 'schema_version.*== 16' crates/`) must return zero matches.

---

## AC-17: Phase Round-Trip Test (SR-01 Guard)

This is the primary guard against positional column drift across the four atomic
sites: analytics.rs INSERT, scan_query_log_by_sessions SELECT, scan_query_log_by_session
SELECT, and row_to_query_log index 9.

```rust
#[tokio::test]
async fn test_query_log_phase_round_trip_some() {
    // Arrange: fresh v17 store + real analytics drain
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Create analytics drain (real drain — no mock)
    let (drain_tx, drain_rx) = create_analytics_channel(); // existing infra
    let drain_task = spawn_analytics_drain(store.clone(), drain_rx);

    // Act: write a query_log row via insert_query_log with phase=Some("design")
    use unimatrix_store::query_log::QueryLogRecord;
    let record = QueryLogRecord::new(
        "session-rt-test".to_string(),
        "round trip query".to_string(),
        &[1, 2, 3],           // entry_ids
        &[0.9, 0.8, 0.7],    // similarity_scores
        "semantic",            // retrieval_mode
        "mcp",                 // source
        Some("design".to_string()), // phase — NEW param
    );
    store.insert_query_log(record, &drain_tx).await.expect("insert");

    // Flush analytics drain — wait for channel to drain
    drop(drain_tx); // signal no more writes
    drain_task.await.expect("drain task");

    // Assert: read back via scan_query_log_by_session
    let rows = store
        .scan_query_log_by_session("session-rt-test", None)
        .await
        .expect("scan");

    assert_eq!(rows.len(), 1, "exactly one row");
    assert_eq!(
        rows[0].phase,
        Some("design".to_string()),
        "phase must round-trip: written as Some('design'), read back as Some('design'). \
         Mismatch indicates positional drift in INSERT or SELECT or row_to_query_log."
    );
}

#[tokio::test]
async fn test_query_log_phase_round_trip_none() {
    // Same setup but with phase=None
    // Assert: rows[0].phase is None (not empty string, not panic)
    // This verifies NULL SQLite column maps to None in Option<String>
}

#[tokio::test]
async fn test_query_log_phase_round_trip_non_trivial_value() {
    // phase = Some("design/v2") — contains slash (EC-06)
    // Assert: rows[0].phase == Some("design/v2")
    // Verifies parameterized binding handles non-trivial characters
}
```

**If any of the four atomic sites is out of sync (AC-21 violation)**:
- INSERT missing phase bind: rows[0].phase = None even though Some("design") was written.
- SELECT missing phase column: `row_to_query_log` panics or returns wrong column at index 9.
- row_to_query_log reading index 8 instead of 9: `phase` field returns `source` value
  ("mcp") instead of "design" — detectable assertion failure.

---

## AC-22: Cascade Update Verification

The following files contain `schema_version == 16` assertions and must be updated:

| File | Required Change |
|------|----------------|
| `crates/unimatrix-store/tests/migration_v15_to_v16.rs` | All `assert_eq!(..., 16)` → 17; rename `test_current_schema_version_is_16` → `_is_17`; update inline comments |
| `crates/unimatrix-server/src/server.rs` lines 2059, 2084 | `assert_eq!(version, 16)` → 17 |

**Gate check**: `grep -r 'schema_version.*== 16' crates/` must return zero matches.

---

## AC-23: UDS Compile Fix

`uds/listener.rs:1324` — update `QueryLogRecord::new(...)` to pass `None` as the seventh
argument (phase). Verification: `cargo build --workspace` succeeds.

No behavioral test needed — a compile error is unmissable.

---

## IR-03/IR-04: eval helper update

These are not migration tests but are tracked here as store-layer obligations:

**IR-03**: `crates/unimatrix-server/src/eval/scenarios/tests.rs` — update
`insert_query_log_row` helper to include phase column binding (NULL):

```rust
// Before (8 binds):
"INSERT INTO query_log (session_id, query_text, ts, result_count, \
 result_entry_ids, similarity_scores, retrieval_mode, source) \
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"

// After (9 binds — phase added as ?9, NULL):
"INSERT INTO query_log (session_id, query_text, ts, result_count, \
 result_entry_ids, similarity_scores, retrieval_mode, source, phase) \
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
// .bind(Option::<String>::None) appended
```

All 15+ call sites use the shared helper — a single helper update fixes all of them.
Verification: `cargo test --workspace` compiles without "table has 9 columns but 8
values were supplied" runtime errors.

**IR-04**: `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` — update
`make_query_log` struct literal to include `phase: None`.
Verification: `cargo build --workspace` compiles without missing-field error.

---

## Assertions Summary

| AC | Test / Gate | Expected |
|----|------------|---------|
| AC-13 | `test_current_schema_version_is_17` | CURRENT_SCHEMA_VERSION == 17 |
| AC-14 | T-V17-01 (fresh) | phase column present; schema = 17 |
| AC-14 | T-V17-02 (migrate from v16) | phase column added |
| AC-14 | T-V17-03 (index) | idx_query_log_phase exists |
| AC-15 | T-V17-04 (idempotency) | No error on second open; col_count = 1 |
| AC-17 | `test_query_log_phase_round_trip_some` | phase reads back as Some("design") |
| AC-17 | `test_query_log_phase_round_trip_none` | phase reads back as None |
| AC-18 | T-V17-05 | Pre-existing row phase = None |
| AC-19 | All six T-V17-* tests pass | `cargo test -p unimatrix-store --test migration_v16_to_v17` |
| AC-22 | grep check | `schema_version.*== 16` → zero matches |
| AC-23 | `cargo build --workspace` | Compiles without error |
| IR-03 | `cargo test --workspace` | No runtime column count errors |
| IR-04 | `cargo build --workspace` | No missing-field compile error in knowledge_reuse.rs |
