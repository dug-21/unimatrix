//! Integration tests for the v31→v32 schema migration (vnc-049, ADR-001/ADR-002, C5).
//!
//! Adds two nullable TEXT columns to `observations`:
//!   source_domain — provider-first ingest stamp (opencode-only, ADR-001); NULL for
//!                   non-opencode + legacy rows (read-derived resolution, R-05).
//!   model_id      — backend model identity carrier (ADR-002); NULL when no model / legacy.
//!
//! Idempotency guard is `pragma_table_info` pre-check per column before any ALTER TABLE
//! ADD COLUMN (pattern #4092; SQLite has no ADD COLUMN IF NOT EXISTS). This is version
//! cascade #1 for vnc-049 (real DB migration); it does NOT touch SUMMARY_SCHEMA_VERSION.
//!
//! Covers (test-plan/c5-ingest-persistence.md, R-04 #4373 cascade):
//!   test_current_schema_version_is_at_least_32           — constant bump
//!   test_fresh_db_creates_source_domain_and_model_id_columns — fresh-create path
//!   test_v31_to_v32_migration_adds_source_domain_and_model_id — old-schema ALTER path
//!   test_fresh_and_migrated_column_parity                — #4373 column-count parity
//!   test_v31_to_v32_migration_idempotent                 — re-run no double-apply (R-04.4)
//!   test_migration_from_populated_v31_data_intact        — existing rows survive (#378)
//!
//! Pattern mirrors migration_v30_to_v31.rs: build a minimal v31-shaped database
//! (entries gates the migration; counters(schema_version=31); an observations table at
//! the pre-v32 shape so the ADD COLUMN is observable and a seeded row proves data-intact).

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::Row;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V31 database builder (minimal — only what gates + survives the v32 block)
// ---------------------------------------------------------------------------

/// Create a minimal v31-shaped database at the given path.
///
/// `entries` presence gates the migration; `counters(schema_version=31)` drives the
/// v31→v32 block under test. `observations` is created at its PRE-v32 shape (no
/// `source_domain`/`model_id`) and seeded so both the ADD COLUMN and the post-migration
/// data-intact checks are observable.
async fn create_v31_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v31 setup conn");

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
        // observations at the PRE-v32 shape (11 columns; no source_domain/model_id).
        // This is exactly the v30→v31-era observations table (topic_source is the last col).
        "CREATE TABLE observations (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id       TEXT    NOT NULL,
            ts_millis        INTEGER NOT NULL,
            hook             TEXT    NOT NULL,
            tool             TEXT,
            input            TEXT,
            response_size    INTEGER,
            response_snippet TEXT,
            topic_signal     TEXT,
            phase            TEXT,
            topic_source     TEXT
        )",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table");
    }

    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 31)",
        "INSERT INTO counters (name, value) VALUES ('next_entry_id', 1)",
    ] {
        sqlx::query(seed)
            .execute(&mut conn)
            .await
            .expect("seed counters");
    }

    // Pre-existing observations row — must survive the migration (#378).
    sqlx::query(
        "INSERT INTO observations (session_id, ts_millis, hook, topic_source) \
         VALUES ('pre-existing-session', 1700000000000, 'PreToolUse', 'declared')",
    )
    .execute(&mut conn)
    .await
    .expect("seed observations row");
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

/// Column names of a table, ordered by cid.
async fn column_names(store: &SqlxStore, table: &str) -> Vec<String> {
    // pragma_table_info does not accept a bound table identifier; table names here are
    // fixed test literals, never user input.
    let sql = format!("SELECT name FROM pragma_table_info('{table}') ORDER BY cid");
    let rows = sqlx::query(&sql)
        .fetch_all(store.read_pool_test())
        .await
        .expect("pragma_table_info");
    rows.into_iter()
        .map(|r| r.try_get::<String, _>(0).unwrap())
        .collect()
}

async fn has_column(store: &SqlxStore, table: &str, column: &str) -> bool {
    column_names(store, table).await.iter().any(|c| c == column)
}

async fn open_store(dir: &TempDir) -> SqlxStore {
    let db_path = dir.path().join("unimatrix.db");
    SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store")
}

// ---------------------------------------------------------------------------
// Constant bump
// ---------------------------------------------------------------------------

#[test]
#[allow(clippy::assertions_on_constants)] // version constant is compile-time; assertion guards the bump
fn test_current_schema_version_is_at_least_32() {
    const {
        assert!(
            unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 32,
            "CURRENT_SCHEMA_VERSION must be >= 32 after vnc-049"
        )
    };
}

// ---------------------------------------------------------------------------
// Fresh-create path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_source_domain_and_model_id_columns() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir).await;

    assert!(
        has_column(&store, "observations", "source_domain").await,
        "fresh-create must create observations.source_domain"
    );
    assert!(
        has_column(&store, "observations", "model_id").await,
        "fresh-create must create observations.model_id"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Migration (old-schema ALTER) path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v31_to_v32_migration_adds_source_domain_and_model_id() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v31_database(&db_path).await;

    // Both columns absent before opening with the current binary.
    let store = open_store(&dir).await;

    assert!(
        has_column(&store, "observations", "source_domain").await,
        "v32 migration must add observations.source_domain"
    );
    assert!(
        has_column(&store, "observations", "model_id").await,
        "v32 migration must add observations.model_id"
    );
    assert!(
        read_schema_version(&store).await >= 32,
        "schema_version must be at least 32 after the v31→v32 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// #4373 column-count parity between fresh-create and ALTER-migrate routes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_and_migrated_column_parity() {
    // Migration path: v31 DB → v32.
    let up_dir = TempDir::new().unwrap();
    create_v31_database(&up_dir.path().join("unimatrix.db")).await;
    let up_store = open_store(&up_dir).await;
    let up_cols = column_names(&up_store, "observations").await;
    up_store.close().await.unwrap();

    // Fresh-create path: brand-new DB at CURRENT_SCHEMA_VERSION.
    let fresh_dir = TempDir::new().unwrap();
    let fresh_store = open_store(&fresh_dir).await;
    let fresh_cols = column_names(&fresh_store, "observations").await;
    fresh_store.close().await.unwrap();

    assert_eq!(
        fresh_cols, up_cols,
        "fresh-create and migration routes must produce identical observations columns (order + names)"
    );
    // Both routes carry the two new columns.
    assert!(fresh_cols.iter().any(|c| c == "source_domain"));
    assert!(fresh_cols.iter().any(|c| c == "model_id"));
}

// ---------------------------------------------------------------------------
// Idempotence (R-04.4): re-run adds no duplicate column, no error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v31_to_v32_migration_idempotent() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v31_database(&db_path).await;

    // First open applies v31→v32.
    let store = open_store(&dir).await;
    assert!(has_column(&store, "observations", "source_domain").await);
    store.close().await.unwrap();

    // Re-open at v32: migration short-circuits (version match); the pragma pre-check
    // guards ADD COLUMN if reached. Must not error, must not duplicate columns.
    let store = open_store(&dir).await;
    let cols = column_names(&store, "observations").await;
    assert_eq!(
        cols.iter().filter(|c| *c == "source_domain").count(),
        1,
        "source_domain must appear exactly once after re-open"
    );
    assert_eq!(
        cols.iter().filter(|c| *c == "model_id").count(),
        1,
        "model_id must appear exactly once after re-open"
    );
    assert!(
        read_schema_version(&store).await >= 32,
        "re-open stays >= v32"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Data-intact (#378): pre-existing observations rows survive the migration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_from_populated_v31_data_intact() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v31_database(&db_path).await;

    let store = open_store(&dir).await;

    // The pre-existing observations row must survive; the new columns are NULL for it.
    let row = sqlx::query(
        "SELECT hook, topic_source, source_domain, model_id FROM observations \
         WHERE session_id = 'pre-existing-session'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch surviving observations row");

    assert_eq!(row.try_get::<String, _>(0).unwrap(), "PreToolUse");
    assert_eq!(row.try_get::<String, _>(1).unwrap(), "declared");
    assert!(
        row.try_get::<Option<String>, _>(2).unwrap().is_none(),
        "legacy row must have NULL source_domain (read-derived fallback, R-05)"
    );
    assert!(
        row.try_get::<Option<String>, _>(3).unwrap().is_none(),
        "legacy row must have NULL model_id"
    );

    store.close().await.unwrap();
}
