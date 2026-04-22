//! Integration tests for the v23→v24 schema migration (crt-047).
//!
//! Covers:
//!   MIG-V24-U-01 — CURRENT_SCHEMA_VERSION constant is >= 24
//!   MIG-V24-U-02 — Fresh database initializes directly to v24
//!   MIG-V24-U-03 — v23→v24 migration adds all seven columns
//!   MIG-V24-U-04 — Idempotency: re-open v24 database is a no-op
//!   MIG-V24-U-05 — Partial column pre-existence: idempotency (ADR-004)
//!
//! Pattern: create a v23-shaped database programmatically (all DDL from v22→v23 migration,
//! including idx_entry_tags_tag_entry_id, but NOT including the seven new columns on
//! cycle_review_index). Open with current SqlxStore to trigger v23→v24 migration. Assert
//! schema state.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::Row as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V23 database builder
// ---------------------------------------------------------------------------

/// Create a v23-shaped database at the given path.
///
/// The v23 DDL = v22 DDL + idx_entry_tags_tag_entry_id compound index.
/// Does NOT include the seven new curation health columns on cycle_review_index
/// (those are added by the v24 migration).
/// Seeds at least one cycle_review_index row to verify DEFAULT 0 lands on new columns.
async fn create_v23_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v23 setup conn");

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
        "CREATE TABLE entry_tags (
            entry_id INTEGER NOT NULL,
            tag      TEXT    NOT NULL,
            PRIMARY KEY (entry_id, tag),
            FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
        )",
        "CREATE TABLE co_access (
            entry_id_a   INTEGER NOT NULL,
            entry_id_b   INTEGER NOT NULL,
            count        INTEGER NOT NULL DEFAULT 1,
            last_updated INTEGER NOT NULL,
            PRIMARY KEY (entry_id_a, entry_id_b),
            CHECK (entry_id_a < entry_id_b)
        )",
        "CREATE TABLE vector_map (
            entry_id INTEGER PRIMARY KEY,
            hnsw_data_id INTEGER NOT NULL
        )",
        "CREATE TABLE feature_entries (
            feature_id TEXT NOT NULL,
            entry_id   INTEGER NOT NULL,
            phase      TEXT,
            PRIMARY KEY (feature_id, entry_id)
        )",
        "CREATE TABLE outcome_index (
            feature_cycle TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (feature_cycle, entry_id)
        )",
        "CREATE TABLE signal_queue (
            signal_id     INTEGER PRIMARY KEY,
            session_id    TEXT    NOT NULL,
            created_at    INTEGER NOT NULL,
            entry_ids     TEXT    NOT NULL DEFAULT '[]',
            signal_type   INTEGER NOT NULL,
            signal_source INTEGER NOT NULL
        )",
        "CREATE TABLE sessions (
            session_id       TEXT    PRIMARY KEY,
            feature_cycle    TEXT,
            agent_role       TEXT,
            started_at       INTEGER NOT NULL,
            ended_at         INTEGER,
            status           INTEGER NOT NULL DEFAULT 0,
            compaction_count INTEGER NOT NULL DEFAULT 0,
            outcome          TEXT,
            total_injections INTEGER NOT NULL DEFAULT 0,
            keywords         TEXT
        )",
        "CREATE TABLE injection_log (
            log_id     INTEGER PRIMARY KEY,
            session_id TEXT    NOT NULL,
            entry_id   INTEGER NOT NULL,
            confidence REAL    NOT NULL,
            timestamp  INTEGER NOT NULL
        )",
        "CREATE TABLE agent_registry (
            agent_id           TEXT    PRIMARY KEY,
            trust_level        INTEGER NOT NULL,
            capabilities       TEXT    NOT NULL DEFAULT '[]',
            allowed_topics     TEXT,
            allowed_categories TEXT,
            enrolled_at        INTEGER NOT NULL,
            last_seen_at       INTEGER NOT NULL,
            active             INTEGER NOT NULL DEFAULT 1
        )",
        "CREATE TABLE audit_log (
            event_id   INTEGER PRIMARY KEY,
            timestamp  INTEGER NOT NULL,
            session_id TEXT    NOT NULL,
            agent_id   TEXT    NOT NULL,
            operation  TEXT    NOT NULL,
            target_ids TEXT    NOT NULL DEFAULT '[]',
            outcome    INTEGER NOT NULL,
            detail     TEXT    NOT NULL DEFAULT ''
        )",
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
            phase            TEXT
        )",
        "CREATE TABLE observation_metrics (
            feature_cycle                      TEXT    PRIMARY KEY,
            computed_at                        INTEGER NOT NULL DEFAULT 0,
            total_tool_calls                   INTEGER NOT NULL DEFAULT 0,
            total_duration_secs                INTEGER NOT NULL DEFAULT 0,
            session_count                      INTEGER NOT NULL DEFAULT 0,
            search_miss_rate                   REAL    NOT NULL DEFAULT 0.0,
            edit_bloat_total_kb                REAL    NOT NULL DEFAULT 0.0,
            edit_bloat_ratio                   REAL    NOT NULL DEFAULT 0.0,
            permission_friction_events         INTEGER NOT NULL DEFAULT 0,
            bash_for_search_count              INTEGER NOT NULL DEFAULT 0,
            cold_restart_events                INTEGER NOT NULL DEFAULT 0,
            coordinator_respawn_count          INTEGER NOT NULL DEFAULT 0,
            parallel_call_rate                 REAL    NOT NULL DEFAULT 0.0,
            context_load_before_first_write_kb REAL    NOT NULL DEFAULT 0.0,
            total_context_loaded_kb            REAL    NOT NULL DEFAULT 0.0,
            post_completion_work_pct           REAL    NOT NULL DEFAULT 0.0,
            follow_up_issues_created           INTEGER NOT NULL DEFAULT 0,
            knowledge_entries_stored           INTEGER NOT NULL DEFAULT 0,
            sleep_workaround_count             INTEGER NOT NULL DEFAULT 0,
            agent_hotspot_count                INTEGER NOT NULL DEFAULT 0,
            friction_hotspot_count             INTEGER NOT NULL DEFAULT 0,
            session_hotspot_count              INTEGER NOT NULL DEFAULT 0,
            scope_hotspot_count                INTEGER NOT NULL DEFAULT 0,
            domain_metrics_json                TEXT    NULL
        )",
        "CREATE TABLE observation_phase_metrics (
            feature_cycle   TEXT    NOT NULL,
            phase_name      TEXT    NOT NULL,
            duration_secs   INTEGER NOT NULL DEFAULT 0,
            tool_call_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (feature_cycle, phase_name),
            FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE
        )",
        "CREATE TABLE shadow_evaluations (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp         INTEGER NOT NULL,
            rule_name         TEXT    NOT NULL,
            rule_category     TEXT    NOT NULL,
            neural_category   TEXT    NOT NULL,
            neural_confidence REAL    NOT NULL,
            convention_score  REAL    NOT NULL,
            rule_accepted     INTEGER NOT NULL,
            digest            BLOB
        )",
        "CREATE TABLE topic_deliveries (
            topic TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            completed_at INTEGER,
            status TEXT NOT NULL DEFAULT 'active',
            github_issue INTEGER,
            total_sessions INTEGER NOT NULL DEFAULT 0,
            total_tool_calls INTEGER NOT NULL DEFAULT 0,
            total_duration_secs INTEGER NOT NULL DEFAULT 0,
            phases_completed TEXT
        )",
        "CREATE TABLE query_log (
            query_id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            query_text TEXT NOT NULL,
            ts INTEGER NOT NULL,
            result_count INTEGER NOT NULL,
            result_entry_ids TEXT,
            similarity_scores TEXT,
            retrieval_mode TEXT,
            source TEXT NOT NULL,
            phase TEXT
        )",
        "CREATE TABLE graph_edges (
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id      INTEGER NOT NULL,
            target_id      INTEGER NOT NULL,
            relation_type  TEXT    NOT NULL,
            weight         REAL    NOT NULL DEFAULT 1.0,
            created_at     INTEGER NOT NULL,
            created_by     TEXT    NOT NULL DEFAULT '',
            source         TEXT    NOT NULL DEFAULT '',
            bootstrap_only INTEGER NOT NULL DEFAULT 0,
            metadata       TEXT    DEFAULT NULL,
            UNIQUE(source_id, target_id, relation_type)
        )",
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
        // v23 cycle_review_index: 5 columns only — new columns absent intentionally
        "CREATE TABLE cycle_review_index (
            feature_cycle         TEXT    PRIMARY KEY,
            schema_version        INTEGER NOT NULL,
            computed_at           INTEGER NOT NULL,
            raw_signals_available INTEGER NOT NULL DEFAULT 1,
            summary_json          TEXT    NOT NULL
        )",
        // goal_clusters added at v22
        "CREATE TABLE goal_clusters (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            feature_cycle   TEXT    NOT NULL UNIQUE,
            goal_embedding  BLOB    NOT NULL,
            phase           TEXT,
            entry_ids_json  TEXT    NOT NULL,
            outcome         TEXT,
            created_at      INTEGER NOT NULL
        )",
        "CREATE INDEX idx_entries_topic ON entries(topic)",
        "CREATE INDEX idx_entries_category ON entries(category)",
        "CREATE INDEX idx_entries_status ON entries(status)",
        "CREATE INDEX idx_entries_created_at ON entries(created_at)",
        "CREATE INDEX idx_entry_tags_tag ON entry_tags(tag)",
        "CREATE INDEX idx_entry_tags_entry_id ON entry_tags(entry_id)",
        // v23 compound index is present
        "CREATE INDEX idx_entry_tags_tag_entry_id ON entry_tags(tag, entry_id)",
        "CREATE INDEX idx_co_access_b ON co_access(entry_id_b)",
        "CREATE INDEX idx_sessions_feature_cycle ON sessions(feature_cycle)",
        "CREATE INDEX idx_sessions_started_at ON sessions(started_at)",
        "CREATE INDEX idx_injection_log_session ON injection_log(session_id)",
        "CREATE INDEX idx_injection_log_entry ON injection_log(entry_id)",
        "CREATE INDEX idx_audit_log_agent ON audit_log(agent_id)",
        "CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp)",
        "CREATE INDEX idx_observations_session ON observations(session_id)",
        "CREATE INDEX idx_observations_ts ON observations(ts_millis)",
        "CREATE INDEX idx_observations_topic_phase ON observations (topic_signal, phase)",
        "CREATE INDEX idx_shadow_eval_ts ON shadow_evaluations(timestamp)",
        "CREATE INDEX idx_query_log_session ON query_log(session_id)",
        "CREATE INDEX idx_query_log_ts ON query_log(ts)",
        "CREATE INDEX idx_query_log_phase ON query_log(phase)",
        "CREATE INDEX idx_graph_edges_source_id ON graph_edges(source_id)",
        "CREATE INDEX idx_graph_edges_target_id ON graph_edges(target_id)",
        "CREATE INDEX idx_graph_edges_relation_type ON graph_edges(relation_type)",
        "CREATE INDEX idx_cycle_events_cycle_id ON cycle_events (cycle_id)",
        "CREATE INDEX idx_goal_clusters_created_at ON goal_clusters(created_at DESC)",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index");
    }

    // Seed counters at v23.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 23)",
        "INSERT INTO counters (name, value) VALUES ('next_entry_id', 1)",
        "INSERT INTO counters (name, value) VALUES ('next_signal_id', 0)",
        "INSERT INTO counters (name, value) VALUES ('next_log_id', 0)",
        "INSERT INTO counters (name, value) VALUES ('next_audit_event_id', 0)",
    ] {
        sqlx::query(seed)
            .execute(&mut conn)
            .await
            .expect("seed counters");
    }

    // Seed a pre-existing cycle_review_index row (5-column shape).
    // After migration, this row must have DEFAULT 0 for all seven new columns.
    sqlx::query(
        "INSERT INTO cycle_review_index
             (feature_cycle, schema_version, computed_at, raw_signals_available, summary_json)
         VALUES
             ('pre-existing-cycle', 1, 1700000000, 1, '{\"test\":true}')",
    )
    .execute(&mut conn)
    .await
    .expect("seed cycle_review_index");
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

// ---------------------------------------------------------------------------
// MIG-V24-U-01: CURRENT_SCHEMA_VERSION constant is >= 24
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_at_least_24() {
    assert!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 24,
        "CURRENT_SCHEMA_VERSION must be >= 24 after crt-047, got {}",
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION
    );
}

// ---------------------------------------------------------------------------
// MIG-V24-U-02: Fresh database initializes directly to v24
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v24() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    assert!(
        read_schema_version(&store).await >= 24,
        "schema_version must be >= 24 on fresh db"
    );

    // All seven new columns must be present on a fresh database.
    for col in &[
        "corrections_total",
        "corrections_agent",
        "corrections_human",
        "corrections_system",
        "deprecations_total",
        "orphan_deprecations",
        "first_computed_at",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index') WHERE name = ?1",
        )
        .bind(col)
        .fetch_one(store.read_pool_test())
        .await
        .expect("pragma_table_info");
        assert_eq!(count, 1, "column {col} must exist on fresh db");
    }

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V24-U-03: v23→v24 migration adds all seven columns (AC-14, AC-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v23_to_v24_migration_adds_all_seven_columns() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v23_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after v23→v24 migration");

    assert!(
        read_schema_version(&store).await >= 24,
        "schema_version must be >= 24 after v23→v24 migration"
    );

    // Each of the seven new columns must be present.
    for col in &[
        "corrections_total",
        "corrections_agent",
        "corrections_human",
        "corrections_system",
        "deprecations_total",
        "orphan_deprecations",
        "first_computed_at",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index') WHERE name = ?1",
        )
        .bind(col)
        .fetch_one(store.read_pool_test())
        .await
        .expect("pragma_table_info");
        assert_eq!(count, 1, "column {col} must exist after v23→v24 migration");
    }

    // The pre-existing row must have DEFAULT 0 for all seven new columns.
    let row = sqlx::query(
        "SELECT corrections_total, corrections_agent, corrections_human,
                corrections_system, deprecations_total, orphan_deprecations,
                first_computed_at
         FROM cycle_review_index WHERE feature_cycle = 'pre-existing-cycle'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pre-existing row");

    for col_idx in 0usize..7 {
        assert_eq!(
            row.get::<i64, _>(col_idx),
            0,
            "column index {col_idx} must be 0 (DEFAULT 0) for pre-existing row"
        );
    }

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V24-U-04: Idempotency — re-open v24 database is a no-op
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v24_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v23_database(&db_path).await;

    // First open triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("first open");
    assert!(read_schema_version(&store).await >= 24);
    store.close().await.unwrap();

    // Second open must be a no-op.
    let store2 = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("second open must not error");
    assert!(
        read_schema_version(&store2).await >= 24,
        "schema_version must remain >= 24 on re-open"
    );
    store2.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V24-U-05: Partial column pre-existence — idempotency (R-03, ADR-004)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v24_migration_idempotent_when_some_columns_pre_exist() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v23_database(&db_path).await;

    // Manually add three of seven columns before migration runs,
    // simulating a crashed mid-migration state.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts
            .connect()
            .await
            .expect("raw conn for partial migration");

        sqlx::query(
            "ALTER TABLE cycle_review_index \
             ADD COLUMN corrections_total INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&mut conn)
        .await
        .expect("partial add corrections_total");

        sqlx::query(
            "ALTER TABLE cycle_review_index \
             ADD COLUMN corrections_agent INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&mut conn)
        .await
        .expect("partial add corrections_agent");

        sqlx::query(
            "ALTER TABLE cycle_review_index \
             ADD COLUMN corrections_human INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&mut conn)
        .await
        .expect("partial add corrections_human");

        // schema_version counter intentionally stays at 23 (crash before bump).
    }

    // Opening the store must complete the migration without error.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after partial migration must succeed");

    // All seven columns must be present.
    for col in &[
        "corrections_total",
        "corrections_agent",
        "corrections_human",
        "corrections_system",
        "deprecations_total",
        "orphan_deprecations",
        "first_computed_at",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('cycle_review_index') WHERE name = ?1",
        )
        .bind(col)
        .fetch_one(store.read_pool_test())
        .await
        .expect("pragma_table_info");
        assert_eq!(
            count, 1,
            "column {col} must exist after partial-recovery migration"
        );
    }

    assert!(
        read_schema_version(&store).await >= 24,
        "schema_version must be >= 24 after partial-recovery migration"
    );

    store.close().await.unwrap();
}
