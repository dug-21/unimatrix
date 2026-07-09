//! Integration tests for the v30→v31 schema migration (vnc-047, ADR-001, #940).
//!
//! Adds the `cycle_tags(feature_cycle, tag)` junction — the durable source of truth
//! for context_cycle whole-set-once run-identity tags — with PK `(feature_cycle, tag)`
//! and `idx_cycle_tags_tag` on `(tag)`. NO FK (feature_cycle is free-text, no parent
//! table). Idempotency guard is `CREATE TABLE/INDEX IF NOT EXISTS` (a brand-new table
//! needs no pragma pre-check — that is only for ALTER TABLE ADD COLUMN). This is
//! version cascade #1 (real DB migration); it does NOT touch SUMMARY_SCHEMA_VERSION.
//!
//! Covers (test-plan/cycle_tags-migration.md, AC-03a–d):
//!   test_current_schema_version_is_at_least_31        — constant bump (AC-03a)
//!   test_fresh_db_creates_cycle_tags_table            — fresh-create path (AC-03b)
//!   test_migration_v30_to_v31_creates_cycle_tags      — migration path (AC-03c)
//!   test_migration_v30_to_v31_idempotent              — re-run no-op (AC-03c, R-13)
//!   test_migration_from_populated_v30_data_intact     — existing data survives (#378)
//!   test_migration_with_stray_cycle_tags_no_error     — defensive IF NOT EXISTS
//!   test_fresh_create_and_migration_schemas_identical — DDL drift guard (AC-03d, #376)
//!   test_cycle_tags_has_no_foreign_key                — free-text contract
//!
//! Pattern mirrors migration_v29_to_v30.rs: build a minimal v30-shaped database
//! (entries gates the migration; counters(schema_version=30); a representative existing
//! table so post-migration data-intact is observable; cycle_tags intentionally ABSENT).

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::Row;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V30 database builder (minimal — only what gates + survives the v31 block)
// ---------------------------------------------------------------------------

/// Create a minimal v30-shaped database at the given path.
///
/// `entries` presence gates the migration; `counters(schema_version=30)` drives the
/// v30→v31 block under test. `cycle_events` is created and seeded so post-migration
/// data-intact is observable. `cycle_tags` is intentionally ABSENT so the v30→v31
/// CREATE is observable.
async fn create_v30_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v30 setup conn");

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
        // Representative existing table for the data-intact assertion.
        "CREATE TABLE cycle_events (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            cycle_id       TEXT    NOT NULL,
            seq            INTEGER NOT NULL,
            event_type     TEXT    NOT NULL,
            phase          TEXT,
            outcome        TEXT,
            next_phase     TEXT,
            timestamp      INTEGER NOT NULL,
            goal           TEXT,
            goal_embedding BLOB
        )",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table");
    }

    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 30)",
        "INSERT INTO counters (name, value) VALUES ('next_entry_id', 1)",
    ] {
        sqlx::query(seed)
            .execute(&mut conn)
            .await
            .expect("seed counters");
    }

    // Pre-existing cycle_events row — must survive the migration (#378).
    sqlx::query(
        "INSERT INTO cycle_events (cycle_id, seq, event_type, timestamp) \
         VALUES ('pre-existing-cycle', 0, 'cycle_start', 1700000000)",
    )
    .execute(&mut conn)
    .await
    .expect("seed cycle_events row");
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
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
    )
    .bind(name)
    .fetch_one(store.read_pool_test())
    .await
    .expect("sqlite_master table lookup")
        > 0
}

async fn index_exists(store: &SqlxStore, name: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
    )
    .bind(name)
    .fetch_one(store.read_pool_test())
    .await
    .expect("sqlite_master index lookup")
        > 0
}

/// (name, type, notnull, pk_ordinal) for every column of a table, ordered by cid.
async fn table_columns(store: &SqlxStore, table: &str) -> Vec<(String, String, i64, i64)> {
    // pragma_table_info does not accept a bound table identifier; table names here are
    // fixed test literals, never user input.
    let sql = format!(
        "SELECT name, type, \"notnull\", pk FROM pragma_table_info('{table}') ORDER BY cid"
    );
    let rows = sqlx::query(&sql)
        .fetch_all(store.read_pool_test())
        .await
        .expect("pragma_table_info");
    rows.into_iter()
        .map(|r| {
            (
                r.try_get::<String, _>(0).unwrap(),
                r.try_get::<String, _>(1).unwrap(),
                r.try_get::<i64, _>(2).unwrap(),
                r.try_get::<i64, _>(3).unwrap(),
            )
        })
        .collect()
}

async fn open_store(dir: &TempDir) -> SqlxStore {
    let db_path = dir.path().join("unimatrix.db");
    SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store")
}

// ---------------------------------------------------------------------------
// AC-03a — constant bump
// ---------------------------------------------------------------------------

#[test]
#[allow(clippy::assertions_on_constants)] // version constant is compile-time; assertion guards the bump
fn test_current_schema_version_is_at_least_31() {
    const {
        assert!(
            unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 31,
            "CURRENT_SCHEMA_VERSION must be >= 31 after vnc-047"
        )
    };
}

// ---------------------------------------------------------------------------
// AC-03b — fresh-create path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_cycle_tags_table() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    assert!(
        table_exists(&store, "cycle_tags").await,
        "fresh-create must create cycle_tags"
    );
    assert!(
        index_exists(&store, "idx_cycle_tags_tag").await,
        "fresh-create must create idx_cycle_tags_tag"
    );

    let cols = table_columns(&store, "cycle_tags").await;
    // Two columns, both NOT NULL, both in the composite PK.
    assert_eq!(cols.len(), 2, "cycle_tags must have exactly 2 columns");
    let names: Vec<&str> = cols.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(names, vec!["feature_cycle", "tag"]);
    for (name, _ty, notnull, pk) in &cols {
        assert_eq!(*notnull, 1, "{name} must be NOT NULL");
        assert!(*pk > 0, "{name} must be part of the PRIMARY KEY");
    }

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-03c — migration path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v30_to_v31_creates_cycle_tags() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v30_database(&db_path).await;

    // cycle_tags absent before opening with the current binary.
    let store = open_store(&dir).await;

    assert!(
        table_exists(&store, "cycle_tags").await,
        "v31 migration must create cycle_tags"
    );
    assert!(
        index_exists(&store, "idx_cycle_tags_tag").await,
        "v31 migration must create idx_cycle_tags_tag"
    );
    assert_eq!(
        read_schema_version(&store).await,
        31,
        "schema_version must be stamped 31 after the v30→v31 migration"
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_v30_to_v31_idempotent() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v30_database(&db_path).await;

    // First open applies v30→v31.
    let store = open_store(&dir).await;
    assert!(table_exists(&store, "cycle_tags").await);
    store.close().await.unwrap();

    // Re-open at v31: migration is a no-op (version match short-circuits; the
    // CREATE ... IF NOT EXISTS statements are no-ops even if reached). Must not error.
    let store = open_store(&dir).await;
    assert!(
        table_exists(&store, "cycle_tags").await,
        "cycle_tags must persist across re-open"
    );
    assert!(index_exists(&store, "idx_cycle_tags_tag").await);
    assert_eq!(
        read_schema_version(&store).await,
        31,
        "re-open must remain at v31"
    );
    // Exactly one table of this name.
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'cycle_tags'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count");
    assert_eq!(count, 1, "cycle_tags must not be duplicated on re-run");

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_from_populated_v30_data_intact() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v30_database(&db_path).await;

    let store = open_store(&dir).await;

    // The pre-existing cycle_events row must survive the migration (#378).
    let surviving = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM cycle_events WHERE cycle_id = 'pre-existing-cycle'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count cycle_events");
    assert_eq!(
        surviving, 1,
        "pre-existing cycle_events data must survive the v30→v31 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Edge case — migration against a DB already carrying a stray cycle_tags
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_with_stray_cycle_tags_no_error() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v30_database(&db_path).await;

    // Pre-plant a cycle_tags table on the v30 DB; the IF NOT EXISTS DDL must not error.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("reopen v30 conn");
        sqlx::query(
            "CREATE TABLE cycle_tags (
                feature_cycle TEXT NOT NULL,
                tag           TEXT NOT NULL,
                PRIMARY KEY (feature_cycle, tag)
            )",
        )
        .execute(&mut conn)
        .await
        .expect("pre-plant cycle_tags");
    }

    // Opening triggers the migration; IF NOT EXISTS makes it a no-op, not an error.
    let store = open_store(&dir).await;
    assert!(table_exists(&store, "cycle_tags").await);
    assert_eq!(read_schema_version(&store).await, 31);
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-03d — DDL drift guard between fresh-create and migration routes (#376)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_create_and_migration_schemas_identical() {
    // Migration path: v30 DB → v31.
    let up_dir = TempDir::new().unwrap();
    create_v30_database(&up_dir.path().join("unimatrix.db")).await;
    let up_store = open_store(&up_dir).await;
    let up_cols = table_columns(&up_store, "cycle_tags").await;
    let up_index = index_exists(&up_store, "idx_cycle_tags_tag").await;
    up_store.close().await.unwrap();

    // Fresh-create path: brand-new DB at CURRENT_SCHEMA_VERSION.
    let fresh_dir = TempDir::new().unwrap();
    let fresh_store = open_store(&fresh_dir).await;
    let fresh_cols = table_columns(&fresh_store, "cycle_tags").await;
    let fresh_index = index_exists(&fresh_store, "idx_cycle_tags_tag").await;
    fresh_store.close().await.unwrap();

    assert_eq!(
        fresh_cols, up_cols,
        "fresh-create and migration routes must produce structurally identical cycle_tags DDL"
    );
    assert_eq!(
        fresh_index, up_index,
        "idx_cycle_tags_tag must exist on both routes"
    );
    assert!(fresh_index, "idx_cycle_tags_tag must exist");
}

// ---------------------------------------------------------------------------
// Free-text contract — no FK on feature_cycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cycle_tags_has_no_foreign_key() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    let fk_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM pragma_foreign_key_list('cycle_tags')")
            .fetch_one(store.read_pool_test())
            .await
            .expect("pragma_foreign_key_list");
    assert_eq!(
        fk_count, 0,
        "cycle_tags must have NO foreign key (feature_cycle is free-text, no parent table)"
    );

    store.close().await.unwrap();
}
