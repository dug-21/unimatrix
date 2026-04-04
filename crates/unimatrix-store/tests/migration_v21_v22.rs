//! Integration tests for the v21→v22 schema migration (crt-046).
//!
//! Covers:
//!   MIG-V22-U-01 — CURRENT_SCHEMA_VERSION constant is >= 22
//!   MIG-V22-U-02 — Fresh database initializes directly to v22
//!   MIG-V22-U-03 — v21→v22 migration creates goal_clusters table with 7 columns
//!   MIG-V22-U-04 — v21→v22 migration creates idx_goal_clusters_created_at index
//!   MIG-V22-U-05 — Idempotency: re-open v22 database is a no-op
//!
//! Pattern: create a v21-shaped database programmatically (all tables from v20→v21 migration,
//! plus goal_embedding BLOB on cycle_events + phase TEXT on observations).
//! Open with current SqlxStore to trigger v21→v22 migration. Assert schema state.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V21 database builder
// ---------------------------------------------------------------------------

/// Create a v21-shaped database at the given path.
///
/// The v21 DDL = v20 DDL + goal_embedding BLOB on cycle_events + phase TEXT on observations
/// + composite index idx_observations_topic_phase. No goal_clusters table.
async fn create_v21_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v21 setup conn");

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
        // observations at v21: includes phase column
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
        // cycle_events at v21: includes goal_embedding BLOB
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
        "CREATE TABLE cycle_review_index (
            feature_cycle         TEXT    PRIMARY KEY,
            schema_version        INTEGER NOT NULL,
            computed_at           INTEGER NOT NULL,
            raw_signals_available INTEGER NOT NULL DEFAULT 1,
            summary_json          TEXT    NOT NULL
        )",
        "CREATE INDEX idx_entries_topic ON entries(topic)",
        "CREATE INDEX idx_entries_category ON entries(category)",
        "CREATE INDEX idx_entries_status ON entries(status)",
        "CREATE INDEX idx_entries_created_at ON entries(created_at)",
        "CREATE INDEX idx_entry_tags_tag ON entry_tags(tag)",
        "CREATE INDEX idx_entry_tags_entry_id ON entry_tags(entry_id)",
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
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index");
    }

    // Seed counters at v21.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 21)",
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
// MIG-V22-U-01: CURRENT_SCHEMA_VERSION constant is >= 22
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_at_least_22() {
    assert!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 22,
        "CURRENT_SCHEMA_VERSION must be >= 22 after crt-046, got {}",
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION
    );
}

// ---------------------------------------------------------------------------
// MIG-V22-U-02: Fresh database initializes directly to v22
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v22() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    assert_eq!(read_schema_version(&store).await, 22);

    // goal_clusters table must be present.
    let has_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='goal_clusters'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("sqlite_master goal_clusters");
    assert_eq!(has_table, 1, "goal_clusters must be present on fresh db");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V22-U-03: v21→v22 migration creates goal_clusters with 7 columns (AC-12, R-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v21_to_v22_migration_creates_goal_clusters() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v21_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after v21→v22 migration");

    // Assert schema_version == 22.
    assert_eq!(read_schema_version(&store).await, 22);

    // Assert goal_clusters table exists.
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='goal_clusters'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("sqlite_master goal_clusters");
    assert_eq!(
        table_count, 1,
        "goal_clusters table must exist after v21→v22 migration"
    );

    // Assert exactly 7 columns.
    let col_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('goal_clusters')")
            .fetch_one(store.read_pool_test())
            .await
            .expect("pragma_table_info goal_clusters");
    assert_eq!(col_count, 7, "goal_clusters must have exactly 7 columns");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V22-U-04: v21→v22 migration creates idx_goal_clusters_created_at
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v21_to_v22_migration_creates_index() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v21_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after v21→v22 migration");

    let index_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='index' AND name='idx_goal_clusters_created_at'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("sqlite_master idx_goal_clusters_created_at");
    assert_eq!(
        index_count, 1,
        "idx_goal_clusters_created_at must exist after migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V22-U-05: Idempotency — re-open v22 database (R-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v22_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v21_database(&db_path).await;

    // First open triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("first open");
    assert_eq!(read_schema_version(&store).await, 22);
    store.close().await.unwrap();

    // Second open must be a no-op.
    let store2 = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("second open must not error");
    assert_eq!(
        read_schema_version(&store2).await,
        22,
        "schema_version must remain 22 on re-open"
    );
    store2.close().await.unwrap();
}
