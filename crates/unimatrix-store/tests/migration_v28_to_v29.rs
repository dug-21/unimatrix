//! Integration tests for the v28→v29 schema migration (crt-054, ADR-007/ADR-008).
//!
//! Adds the Surface A `compaction_events` table + `idx_compaction_events_session`
//! index — a durable, content-free, insert-only compaction-event ledger. The
//! `compacted_at` column is Unix SECONDS (server wall clock); the PostToolUse
//! ts/1000 gate normalization is crt-055's, not crt-054's. `CREATE TABLE/INDEX
//! IF NOT EXISTS` makes the upgrade idempotent on re-run. crt-054 touches NO other
//! table and does NOT bump SUMMARY_SCHEMA_VERSION or ALTER cycle_review_index.
//!
//! Covers (test-plan/compaction-events-migration.md):
//!   test_migration_v28_to_v29_adds_compaction_events — v28 DB → table+index, v29
//!   test_migration_v28_to_v29_idempotent             — re-run is a no-op, >= v29
//!   test_compaction_events_columns_match_contract    — column names/types/null/default
//!   test_current_schema_version_is_at_least_29       — constant bumped
//!
//! Pattern: build a minimal v28-shaped database (entries table presence gates the
//! migration; counters(schema_version=28); compaction_events intentionally absent).
//! Open with the current SqlxStore to trigger the v28→v29 transition.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V28 database builder (minimal — only what gates + survives the v29 block)
// ---------------------------------------------------------------------------

/// Create a minimal v28-shaped database at the given path.
///
/// The migration entry point gates on the presence of an `entries` table and
/// reads `schema_version` from `counters`. From v28, every `if current_version <
/// N` block with N <= 28 is skipped, so only the `entries` + `counters` tables
/// are required to drive the v28→v29 block under test. `compaction_events` is
/// intentionally ABSENT so its creation by the v29 block is observable.
async fn create_v28_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v28 setup conn");

    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&mut conn)
        .await
        .expect("wal");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut conn)
        .await
        .expect("fk");

    for ddl in &[
        "CREATE TABLE counters (
            name TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        )",
        // entries presence gates migration (migrate_if_needed step 1).
        "CREATE TABLE entries (
            id              INTEGER PRIMARY KEY,
            title           TEXT    NOT NULL,
            content         TEXT    NOT NULL,
            topic           TEXT    NOT NULL,
            category        TEXT    NOT NULL,
            source          TEXT    NOT NULL,
            status          INTEGER NOT NULL DEFAULT 0,
            confidence      REAL    NOT NULL DEFAULT 0.0,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL,
            last_accessed_at INTEGER NOT NULL DEFAULT 0,
            access_count    INTEGER NOT NULL DEFAULT 0,
            supersedes      INTEGER,
            superseded_by   INTEGER,
            correction_count INTEGER NOT NULL DEFAULT 0,
            embedding_dim   INTEGER NOT NULL DEFAULT 0,
            created_by      TEXT    NOT NULL DEFAULT '',
            modified_by     TEXT    NOT NULL DEFAULT '',
            content_hash    TEXT    NOT NULL DEFAULT '',
            previous_hash   TEXT    NOT NULL DEFAULT '',
            version         INTEGER NOT NULL DEFAULT 0,
            feature_cycle   TEXT    NOT NULL DEFAULT '',
            trust_source    TEXT    NOT NULL DEFAULT '',
            helpful_count   INTEGER NOT NULL DEFAULT 0,
            unhelpful_count INTEGER NOT NULL DEFAULT 0,
            pre_quarantine_status INTEGER
        )",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table");
    }

    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 28)",
        "INSERT INTO counters (name, value) VALUES ('next_entry_id', 1)",
    ] {
        sqlx::query(seed)
            .execute(&mut conn)
            .await
            .expect("seed counters");
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn read_schema_version(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'schema_version'")
        .fetch_one(store.read_pool_test())
        .await
        .expect("read schema_version")
}

async fn table_exists(store: &SqlxStore, name: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
    )
    .bind(name)
    .fetch_one(store.read_pool_test())
    .await
    .expect("sqlite_master table lookup")
        > 0
}

async fn index_exists(store: &SqlxStore, name: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name=?1",
    )
    .bind(name)
    .fetch_one(store.read_pool_test())
    .await
    .expect("sqlite_master index lookup")
        > 0
}

async fn open_store(dir: &TempDir) -> SqlxStore {
    let db_path = dir.path().join("unimatrix.db");
    SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store")
}

// ---------------------------------------------------------------------------
// test_current_schema_version_is_at_least_29 (R-04)
// ---------------------------------------------------------------------------

#[test]
#[allow(clippy::assertions_on_constants)] // version constant is compile-time; assertion guards the bump
fn test_current_schema_version_is_at_least_29() {
    const {
        assert!(
            unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 29,
            "CURRENT_SCHEMA_VERSION must be >= 29 after crt-054"
        )
    };
}

// ---------------------------------------------------------------------------
// test_migration_v28_to_v29_adds_compaction_events (FR-A7, AC-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v28_to_v29_adds_compaction_events() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");

    // Arrange: a v28 DB without compaction_events.
    create_v28_database(&db_path).await;

    // Act: open with the current binary → triggers v28→v29.
    let store = open_store(&dir).await;

    // Assert: the upgrade block added the table + index, and stamped v29.
    assert!(
        table_exists(&store, "compaction_events").await,
        "v29 migration must add the compaction_events table"
    );
    assert!(
        index_exists(&store, "idx_compaction_events_session").await,
        "v29 migration must add idx_compaction_events_session"
    );
    assert!(
        read_schema_version(&store).await >= 29,
        "schema_version must be stamped >= 29 after the v28→v29 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// test_migration_v28_to_v29_idempotent (#4373 idempotency)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v28_to_v29_idempotent() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");

    create_v28_database(&db_path).await;

    // First open applies v28→v29.
    let store = open_store(&dir).await;
    assert!(table_exists(&store, "compaction_events").await);
    store.close().await.unwrap();

    // Re-open: at v29 the migration is a no-op (CREATE TABLE IF NOT EXISTS is
    // idempotent and the version match short-circuits). Must not error.
    let store = open_store(&dir).await;
    assert!(
        table_exists(&store, "compaction_events").await,
        "table must persist across re-open"
    );
    assert!(
        index_exists(&store, "idx_compaction_events_session").await,
        "index must persist across re-open"
    );
    assert!(
        read_schema_version(&store).await >= 29,
        "re-open must remain at >= v29"
    );
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// test_compaction_events_columns_match_contract (R-05, AC-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_compaction_events_columns_match_contract() {
    use sqlx::Row;

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v28_database(&db_path).await;
    let store = open_store(&dir).await;

    // pragma_table_info: (cid, name, type, notnull, dflt_value, pk)
    let rows = sqlx::query(
        "SELECT name, type, \"notnull\", dflt_value, pk \
         FROM pragma_table_info('compaction_events') ORDER BY cid",
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("pragma_table_info compaction_events");

    let cols: Vec<(String, String, i64, Option<String>, i64)> = rows
        .iter()
        .map(|r| {
            (
                r.try_get::<String, _>(0).unwrap(),
                r.try_get::<String, _>(1).unwrap(),
                r.try_get::<i64, _>(2).unwrap(),
                r.try_get::<Option<String>, _>(3).unwrap(),
                r.try_get::<i64, _>(4).unwrap(),
            )
        })
        .collect();

    // Exactly four columns, in contract order.
    assert_eq!(
        cols.len(),
        4,
        "compaction_events must have exactly 4 columns (id, session_id, compacted_at, high_water)"
    );

    // id INTEGER PRIMARY KEY
    assert_eq!(cols[0].0, "id");
    assert_eq!(cols[0].1, "INTEGER");
    assert_eq!(cols[0].4, 1, "id must be PRIMARY KEY");

    // session_id TEXT NOT NULL
    assert_eq!(cols[1].0, "session_id");
    assert_eq!(cols[1].1, "TEXT");
    assert_eq!(cols[1].2, 1, "session_id must be NOT NULL");

    // compacted_at INTEGER NOT NULL  (Unix SECONDS)
    assert_eq!(cols[2].0, "compacted_at");
    assert_eq!(cols[2].1, "INTEGER");
    assert_eq!(cols[2].2, 1, "compacted_at must be NOT NULL");

    // high_water INTEGER NOT NULL DEFAULT 0
    assert_eq!(cols[3].0, "high_water");
    assert_eq!(cols[3].1, "INTEGER");
    assert_eq!(cols[3].2, 1, "high_water must be NOT NULL");
    assert_eq!(cols[3].3.as_deref(), Some("0"), "high_water must DEFAULT 0");

    // Content-opacity (AC-03): no feature_cycle / content column.
    assert!(
        !cols.iter().any(|(n, ..)| n == "feature_cycle"),
        "compaction_events must NOT carry a feature_cycle column"
    );

    store.close().await.unwrap();
}
