//! Integration tests for the v17→v18 schema migration (crt-033).
//!
//! Covers: MIG-U-01 (CURRENT_SCHEMA_VERSION = 18), MIG-U-02 (fresh DB creates v18),
//! MIG-U-03 (v17→v18 creates cycle_review_index), MIG-U-04 (all 5 columns present),
//! MIG-U-05 (pre-existing data survives), MIG-U-06 (idempotency), MIG-U-07
//! (test_current_schema_version_is_at_least_17 in migration_v16_to_v17.rs verified
//! separately via grep).
//!
//! Pattern: create a v17-shaped database, open with current SqlxStore to trigger
//! migration, assert schema state and data round-trips.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V17 database builder
// ---------------------------------------------------------------------------

/// Create a v17-shaped database at the given path.
///
/// Contains all tables present at v17: all v16 tables + `query_log.phase` column.
/// The `cycle_review_index` table must NOT exist — that is what v17→v18 adds.
/// schema_version = 17.
async fn create_v17_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v17 setup conn");

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
        // feature_entries WITH phase column — v15 added it.
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
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id      TEXT    NOT NULL,
            ts_millis       INTEGER NOT NULL,
            hook            TEXT    NOT NULL,
            tool            TEXT,
            input           TEXT,
            response_size   INTEGER,
            response_snippet TEXT,
            topic_signal    TEXT
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
        // query_log WITH phase column — v16→v17 added it. This is the v17 shape.
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
        // cycle_events WITH goal column — v15→v16 added it.
        "CREATE TABLE cycle_events (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            cycle_id   TEXT    NOT NULL,
            seq        INTEGER NOT NULL,
            event_type TEXT    NOT NULL,
            phase      TEXT,
            outcome    TEXT,
            next_phase TEXT,
            timestamp  INTEGER NOT NULL,
            goal       TEXT
        )",
        // NOTE: cycle_review_index intentionally absent — this is the v17 shape.
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

    // Seed counters at v17.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 17)",
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
// Post-migration assertion helpers
// ---------------------------------------------------------------------------

async fn read_schema_version(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'schema_version'")
        .fetch_one(store.read_pool_test())
        .await
        .expect("read schema_version")
}

async fn cycle_review_index_exists(store: &SqlxStore) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cycle_review_index'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check cycle_review_index table");
    count > 0
}

// ---------------------------------------------------------------------------
// MIG-U-01: CURRENT_SCHEMA_VERSION constant == 18 (AC-01, R-01)
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_18() {
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        18,
        "CURRENT_SCHEMA_VERSION must be 18"
    );
}

// ---------------------------------------------------------------------------
// MIG-U-02: Fresh database creates schema v18 (AC-01, R-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v18() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: empty path — no prior DB.
    // Act: SqlxStore::open calls create_tables_if_needed() for fresh DBs.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Assert: schema_version == 18
    assert_eq!(
        read_schema_version(&store).await,
        18,
        "fresh database must be at schema v18"
    );

    // Assert: cycle_review_index table present (fresh schema has full DDL)
    assert!(
        cycle_review_index_exists(&store).await,
        "fresh database must have cycle_review_index table"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-U-03: v17→v18 migration creates cycle_review_index table (AC-02, AC-13, R-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v17_to_v18_migration_creates_table() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v17 database — cycle_review_index table absent.
    create_v17_database(&db_path).await;

    // Assert pre-condition: cycle_review_index not yet in DB.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("pre-check conn");
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cycle_review_index'",
        )
        .fetch_one(&mut conn)
        .await
        .expect("pre-check");
        assert_eq!(count, 0, "cycle_review_index must not exist in v17 shape");
    }

    // Act: open triggers v17→v18 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store after v17→v18 migration");

    // Assert: cycle_review_index now exists.
    assert!(
        cycle_review_index_exists(&store).await,
        "cycle_review_index table must exist after v17→v18 migration (AC-02)"
    );

    // Assert: schema_version == 18.
    assert_eq!(
        read_schema_version(&store).await,
        18,
        "schema_version must be 18 after v17→v18 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-U-04: All five columns present after migration (AC-02, R-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v17_to_v18_migration_table_has_five_columns() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v17_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store after migration");

    // Assert: exactly 5 columns in cycle_review_index.
    let col_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('cycle_review_index')")
            .fetch_one(store.read_pool_test())
            .await
            .expect("pragma_table_info count");

    assert_eq!(
        col_count, 5,
        "cycle_review_index must have exactly 5 columns after v17→v18 migration"
    );

    // Assert all five column names.
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('cycle_review_index') ORDER BY cid")
            .fetch_all(store.read_pool_test())
            .await
            .expect("pragma_table_info names");

    assert!(
        columns.contains(&"feature_cycle".to_string()),
        "cycle_review_index must have feature_cycle column"
    );
    assert!(
        columns.contains(&"schema_version".to_string()),
        "cycle_review_index must have schema_version column"
    );
    assert!(
        columns.contains(&"computed_at".to_string()),
        "cycle_review_index must have computed_at column"
    );
    assert!(
        columns.contains(&"raw_signals_available".to_string()),
        "cycle_review_index must have raw_signals_available column"
    );
    assert!(
        columns.contains(&"summary_json".to_string()),
        "cycle_review_index must have summary_json column"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-U-05: Pre-existing data survives migration (AC-02, NFR-04, R-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v17_to_v18_migration_preserves_existing_data() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v17 database with a pre-seeded entries row.
    create_v17_database(&db_path).await;
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query(
            "INSERT INTO entries \
             (id, title, content, topic, category, source, status, confidence, \
              created_at, updated_at) \
             VALUES (1, 'test-title', 'test-content', 'test-topic', 'convention', \
                     'test', 0, 0.5, 1700000000, 1700000000)",
        )
        .execute(&mut conn)
        .await
        .expect("insert pre-existing entry");
    }

    // Act: SqlxStore::open triggers v17→v18 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: schema_version == 18 (confirming migration ran).
    assert_eq!(
        read_schema_version(&store).await,
        18,
        "schema_version must be 18 after migration"
    );

    // Assert: pre-existing entry is still readable (no data loss).
    let entry = store
        .get(1)
        .await
        .expect("entry must still be readable after migration");
    assert_eq!(
        entry.title, "test-title",
        "pre-existing entry title must survive migration"
    );

    // Assert: cycle_review_index exists with no rows (no spurious inserts).
    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cycle_review_index")
        .fetch_one(store.read_pool_test())
        .await
        .expect("count cycle_review_index rows");
    assert_eq!(
        row_count, 0,
        "cycle_review_index must be empty after migration (no backfill)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-U-06: Idempotency — running migration twice succeeds (NFR-06, R-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v17_to_v18_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v17_database(&db_path).await;

    // Run 1: applies v17→v18 migration.
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("first open");
        assert!(cycle_review_index_exists(&store).await);
        assert_eq!(read_schema_version(&store).await, 18);
        store.close().await.unwrap();
    }

    // Run 2: must be a no-op — CREATE TABLE IF NOT EXISTS swallows no-op.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open must succeed (idempotency)");

    assert_eq!(read_schema_version(&store).await, 18);

    // Exactly one cycle_review_index table (not two).
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cycle_review_index'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count cycle_review_index tables");
    assert_eq!(
        table_count, 1,
        "exactly one cycle_review_index table after idempotent run (NFR-06)"
    );

    store.close().await.unwrap();
}
