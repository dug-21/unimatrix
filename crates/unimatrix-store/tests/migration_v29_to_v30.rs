//! Integration tests for the v29→v30 schema migration (crt-055, ADR-001 #5051).
//!
//! Adds 16 durable per-cycle aggregate columns to `cycle_review_index`:
//!   15 INTEGER NOT NULL DEFAULT 0 metric columns + `signal_class_counts_json`
//!   TEXT NOT NULL DEFAULT '{}'. Every metric column is INTEGER (no REAL/float),
//!   so the push_bind(f64) non-finite footgun (#4529/#4533) is designed out;
//!   `context_reload_pct` stores basis points (0–10000). The pragma_table_info
//!   pre-check per column makes each ALTER idempotent, so re-run is a no-op.
//!   crt-055 owns the SUMMARY_SCHEMA_VERSION 4→5 bump separately.
//!
//! Covers (test-plan/cycle_review_index_schema.md):
//!   test_migration_v29_to_v30_adds_v5_columns — v29 DB → all 16 columns, v30
//!   test_migration_v29_to_v30_idempotent      — re-run is a no-op, >= v30
//!   test_v5_columns_match_contract            — name/type/notnull/default per column
//!   test_context_reload_pct_is_integer_not_real — AC-20 column-type guard
//!   test_no_token_named_column                — AC-10 bytes-not-tokens guard
//!   test_current_schema_version_is_at_least_30 — constant bumped
//!
//! Pattern mirrors migration_v28_to_v29.rs: build a minimal v29-shaped database
//! (entries gates the migration; counters(schema_version=29); cycle_review_index
//! present at its pre-v5 v24 shape so the v29→v30 ALTERs are observable).

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::Row;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

/// The 16 v5 columns crt-055 adds, in fresh-create / ALTER order.
const V5_COLUMNS: [&str; 16] = [
    "phase_count",
    "phase_transition_count",
    "phase_rework_count",
    "phase_unclosed_count",
    "phase_total_duration_secs",
    "rework_session_count",
    "total_session_count",
    "knowledge_reuse_served_count",
    "transcript_bytes_total",
    "transcript_delta_count",
    "transcript_error_count",
    "transcript_refusal_count",
    "signal_class_counts_json",
    "compaction_count",
    "compaction_reread_count",
    "context_reload_pct",
];

// ---------------------------------------------------------------------------
// V29 database builder (minimal — only what gates + survives the v30 block)
// ---------------------------------------------------------------------------

/// Create a minimal v29-shaped database at the given path.
///
/// `entries` presence gates the migration; `counters(schema_version=29)` drives
/// the v29→v30 block under test. `cycle_review_index` is created at its pre-v5
/// (v24) shape — WITHOUT the 16 v5 columns — so the v29→v30 ALTERs are observable.
async fn create_v29_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v29 setup conn");

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
        // cycle_review_index at the pre-v5 (v24) shape — the 16 v5 columns are
        // intentionally ABSENT so the v29→v30 ALTER block adds them observably.
        "CREATE TABLE cycle_review_index (
            feature_cycle         TEXT    PRIMARY KEY,
            schema_version        INTEGER NOT NULL,
            computed_at           INTEGER NOT NULL,
            raw_signals_available INTEGER NOT NULL DEFAULT 1,
            summary_json          TEXT    NOT NULL,
            corrections_total     INTEGER NOT NULL DEFAULT 0,
            corrections_agent     INTEGER NOT NULL DEFAULT 0,
            corrections_human     INTEGER NOT NULL DEFAULT 0,
            corrections_system    INTEGER NOT NULL DEFAULT 0,
            deprecations_total    INTEGER NOT NULL DEFAULT 0,
            orphan_deprecations   INTEGER NOT NULL DEFAULT 0,
            first_computed_at     INTEGER NOT NULL DEFAULT 0
        )",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table");
    }

    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 29)",
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

async fn column_present(store: &SqlxStore, col: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index') WHERE name = ?1",
    )
    .bind(col)
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma_table_info lookup")
        > 0
}

/// (type, notnull, dflt_value) for a single column of cycle_review_index.
async fn column_contract(store: &SqlxStore, col: &str) -> (String, i64, Option<String>) {
    let row = sqlx::query(
        "SELECT type, \"notnull\", dflt_value \
         FROM pragma_table_info('cycle_review_index') WHERE name = ?1",
    )
    .bind(col)
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma_table_info row");
    (
        row.try_get::<String, _>(0).unwrap(),
        row.try_get::<i64, _>(1).unwrap(),
        row.try_get::<Option<String>, _>(2).unwrap(),
    )
}

async fn open_store(dir: &TempDir) -> SqlxStore {
    let db_path = dir.path().join("unimatrix.db");
    SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store")
}

// ---------------------------------------------------------------------------
// test_current_schema_version_is_at_least_30
// ---------------------------------------------------------------------------

#[test]
#[allow(clippy::assertions_on_constants)] // version constant is compile-time; assertion guards the bump
fn test_current_schema_version_is_at_least_30() {
    assert!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 30,
        "CURRENT_SCHEMA_VERSION must be >= 30 after crt-055, got {}",
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION
    );
}

// ---------------------------------------------------------------------------
// test_migration_v29_to_v30_adds_v5_columns (AC-02)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v29_to_v30_adds_v5_columns() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");

    // Arrange: a v29 DB without the v5 columns.
    create_v29_database(&db_path).await;

    // Act: open with the current binary → triggers v29→v30.
    let store = open_store(&dir).await;

    // Assert: all 16 v5 columns present and version stamped >= 30.
    for col in V5_COLUMNS {
        assert!(
            column_present(&store, col).await,
            "v30 migration must add column {col}"
        );
    }
    assert!(
        read_schema_version(&store).await >= 30,
        "schema_version must be stamped >= 30 after the v29→v30 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// test_migration_v29_to_v30_idempotent (AC-03, R-10)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v29_to_v30_idempotent() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");

    create_v29_database(&db_path).await;

    // First open applies v29→v30.
    let store = open_store(&dir).await;
    for col in V5_COLUMNS {
        assert!(column_present(&store, col).await);
    }
    store.close().await.unwrap();

    // Re-open: at v30 the migration is a no-op (pragma-guarded ALTERs skip and the
    // version match short-circuits). Must not error; no duplicate columns.
    let store = open_store(&dir).await;
    for col in V5_COLUMNS {
        assert!(
            column_present(&store, col).await,
            "column {col} must persist across re-open"
        );
        // Exactly one column of each name (pragma returns one row per column).
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index') WHERE name = ?1",
        )
        .bind(col)
        .fetch_one(store.read_pool_test())
        .await
        .expect("count");
        assert_eq!(count, 1, "column {col} must not be duplicated on re-run");
    }
    assert!(
        read_schema_version(&store).await >= 30,
        "re-open must remain at >= v30"
    );
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// test_v5_columns_match_contract (AC-02)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v5_columns_match_contract() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v29_database(&db_path).await;
    let store = open_store(&dir).await;

    for col in V5_COLUMNS {
        let (ty, notnull, dflt) = column_contract(&store, col).await;
        assert_eq!(notnull, 1, "{col} must be NOT NULL");
        if col == "signal_class_counts_json" {
            assert_eq!(ty, "TEXT", "signal_class_counts_json must be TEXT");
            assert_eq!(
                dflt.as_deref(),
                Some("'{}'"),
                "signal_class_counts_json must DEFAULT '{{}}'"
            );
        } else {
            assert_eq!(
                ty, "INTEGER",
                "{col} must be INTEGER (no REAL/float column)"
            );
            assert_eq!(dflt.as_deref(), Some("0"), "{col} must DEFAULT 0");
        }
    }

    // Structural leak gate: no content field added by the v5 migration.
    assert!(
        !column_present(&store, "content").await,
        "cycle_review_index must NOT carry a content column (structural leak gate)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// test_context_reload_pct_is_integer_not_real (AC-20)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_context_reload_pct_is_integer_not_real() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v29_database(&db_path).await;
    let store = open_store(&dir).await;

    let (ty, _, _) = column_contract(&store, "context_reload_pct").await;
    assert_eq!(
        ty, "INTEGER",
        "context_reload_pct must be INTEGER (basis points), never REAL (ADR-005)"
    );

    // No REAL/float metric column anywhere in the v5 set.
    for col in V5_COLUMNS {
        if col == "signal_class_counts_json" {
            continue;
        }
        let (ty, _, _) = column_contract(&store, col).await;
        assert_ne!(
            ty, "REAL",
            "{col} must not be REAL — every metric column is INTEGER"
        );
    }

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// test_no_token_named_column (AC-10, R-13) — throughput unit is bytes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_no_token_named_column() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("unimatrix.db");
    create_v29_database(&db_path).await;
    let store = open_store(&dir).await;

    let names: Vec<String> =
        sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('cycle_review_index')")
            .fetch_all(store.read_pool_test())
            .await
            .expect("pragma names");

    assert!(
        !names.iter().any(|n| n.to_lowercase().contains("token")),
        "cycle_review_index must NOT carry any token-named column (bytes, not tokens)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// test_fresh_create_and_upgrade_agree (#4153 three-path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_create_and_upgrade_agree() {
    // Upgrade path: v29 DB → v30.
    let up_dir = TempDir::new().unwrap();
    create_v29_database(&up_dir.path().join("unimatrix.db")).await;
    let up_store = open_store(&up_dir).await;
    let mut up_cols: Vec<String> =
        sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('cycle_review_index')")
            .fetch_all(up_store.read_pool_test())
            .await
            .expect("upgrade pragma names");
    up_cols.sort();
    up_store.close().await.unwrap();

    // Fresh-create path: brand-new DB created at CURRENT_SCHEMA_VERSION.
    let fresh_dir = TempDir::new().unwrap();
    let fresh_store = open_store(&fresh_dir).await;
    let mut fresh_cols: Vec<String> =
        sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('cycle_review_index')")
            .fetch_all(fresh_store.read_pool_test())
            .await
            .expect("fresh pragma names");
    fresh_cols.sort();
    fresh_store.close().await.unwrap();

    assert_eq!(
        fresh_cols, up_cols,
        "fresh-create and upgrade paths must produce the same cycle_review_index columns"
    );
}
