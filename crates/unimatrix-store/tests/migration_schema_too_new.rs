//! Integration tests for SchemaTooNew rejection (GH #650).
//!
//! Covers:
//!   SCHEMA-TOO-NEW-01 — open() rejects a database with schema_version > CURRENT_SCHEMA_VERSION
//!   SCHEMA-TOO-NEW-02 — error contains correct database_version and binary_version
//!   SCHEMA-TOO-NEW-03 — open() succeeds when schema_version == CURRENT_SCHEMA_VERSION (no regression)

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::migration::CURRENT_SCHEMA_VERSION;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// Helper: create a minimal database at a specific schema_version
// ---------------------------------------------------------------------------

/// Create a minimal database with the `entries` and `counters` tables and the
/// given `schema_version`. This is enough to trigger the version check in
/// `migrate_if_needed()` without requiring the full DDL of all tables.
async fn create_database_at_version(path: &Path, schema_version: u64) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open setup conn");

    sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&mut conn)
        .await
        .expect("wal");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut conn)
        .await
        .expect("fk");

    // Minimal tables: counters + entries (entries must exist for migration to
    // read schema_version rather than returning Ok(()) for fresh databases).
    sqlx::query(
        "CREATE TABLE counters (
            name TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        )",
    )
    .execute(&mut conn)
    .await
    .expect("create counters");

    sqlx::query(
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
    )
    .execute(&mut conn)
    .await
    .expect("create entries");

    sqlx::query("INSERT INTO counters (name, value) VALUES ('schema_version', ?1)")
        .bind(schema_version as i64)
        .execute(&mut conn)
        .await
        .expect("seed schema_version");
}

// ---------------------------------------------------------------------------
// SCHEMA-TOO-NEW-01: open() rejects future schema version
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_open_rejects_schema_too_new() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let future_version = CURRENT_SCHEMA_VERSION + 1;
    create_database_at_version(&db_path, future_version).await;

    let result = SqlxStore::open(&db_path, PoolConfig::test_default()).await;

    assert!(
        result.is_err(),
        "open() must reject a database with schema_version > CURRENT_SCHEMA_VERSION"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("newer than this binary supports"),
        "error message must describe the problem: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// SCHEMA-TOO-NEW-02: error contains correct version numbers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_schema_too_new_error_contains_versions() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let future_version = CURRENT_SCHEMA_VERSION + 5;
    create_database_at_version(&db_path, future_version).await;

    let result = SqlxStore::open(&db_path, PoolConfig::test_default()).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();

    let db_v_str = future_version.to_string();
    let bin_v_str = CURRENT_SCHEMA_VERSION.to_string();
    assert!(
        err_msg.contains(&db_v_str),
        "error must contain database version {db_v_str}: {err_msg}"
    );
    assert!(
        err_msg.contains(&bin_v_str),
        "error must contain binary version {bin_v_str}: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// SCHEMA-TOO-NEW-03: open() succeeds when schema_version == CURRENT (no regression)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_open_succeeds_at_current_schema_version() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // A fresh database opens at CURRENT_SCHEMA_VERSION.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open fresh store must succeed");

    let version: i64 =
        sqlx::query_scalar("SELECT value FROM counters WHERE name = 'schema_version'")
            .fetch_one(store.read_pool_test())
            .await
            .expect("read schema_version");

    assert_eq!(
        version, CURRENT_SCHEMA_VERSION as i64,
        "fresh database must be at CURRENT_SCHEMA_VERSION"
    );

    store.close().await.unwrap();
}
