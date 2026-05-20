//! Integration tests for the v26→v27 schema migration (vnc-018).
//!
//! Covers:
//!   MIG-V27-U-01 — CURRENT_SCHEMA_VERSION constant is >= 27
//!   MIG-V27-U-02 — Fresh database initializes directly to v27 with all 4 indexes present
//!   MIG-V27-U-03 — v26→v27 migration creates all 4 required indexes (AC-19)
//!   MIG-V27-U-04 — Idempotency: re-open v27 database is a no-op
//!   MIG-V27-U-05 — schema_version updated to 27 after migration
//!
//! Pattern: create a v26-shaped database (all v25 tables + next_audit_id counter +
//! schema_version=26, but WITHOUT the four new context_graph indexes).
//! Open with current SqlxStore to trigger v26→v27 migration. Assert all 4 indexes exist.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V26 database builder
// ---------------------------------------------------------------------------

/// Create a v26-shaped database at the given path.
///
/// The v26 DDL = v25 DDL (audit_log 12 columns + triggers + next_audit_id counter).
/// Does NOT include the four new context_graph indexes (added by v27 migration):
///   - idx_entries_supersedes
///   - idx_entries_superseded_by
///   - idx_graph_edges_source_type
///   - idx_graph_edges_target_type
async fn create_v26_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v26 setup conn");

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
        // v26 audit_log: 12 columns (full schema from v24→v25 migration)
        "CREATE TABLE audit_log (
            event_id          INTEGER PRIMARY KEY,
            timestamp         INTEGER NOT NULL,
            session_id        TEXT    NOT NULL,
            agent_id          TEXT    NOT NULL,
            operation         TEXT    NOT NULL,
            target_ids        TEXT    NOT NULL DEFAULT '[]',
            outcome           INTEGER NOT NULL,
            detail            TEXT    NOT NULL DEFAULT '',
            credential_type   TEXT    NOT NULL DEFAULT 'none',
            capability_used   TEXT    NOT NULL DEFAULT '',
            agent_attribution TEXT    NOT NULL DEFAULT '',
            metadata          TEXT    NOT NULL DEFAULT '{}'
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
        "CREATE INDEX idx_entry_tags_tag_entry_id ON entry_tags(tag, entry_id)",
        "CREATE INDEX idx_co_access_b ON co_access(entry_id_b)",
        "CREATE INDEX idx_sessions_feature_cycle ON sessions(feature_cycle)",
        "CREATE INDEX idx_sessions_started_at ON sessions(started_at)",
        "CREATE INDEX idx_injection_log_session ON injection_log(session_id)",
        "CREATE INDEX idx_injection_log_entry ON injection_log(entry_id)",
        "CREATE INDEX idx_audit_log_agent ON audit_log(agent_id)",
        "CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp)",
        "CREATE INDEX idx_audit_log_session ON audit_log(session_id)",
        "CREATE INDEX idx_audit_log_cred ON audit_log(credential_type)",
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
        // Append-only triggers from v24→v25 migration
        "CREATE TRIGGER audit_log_no_update
         BEFORE UPDATE ON audit_log
         BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END",
        "CREATE TRIGGER audit_log_no_delete
         BEFORE DELETE ON audit_log
         BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END",
        // NOTE: idx_entries_supersedes, idx_entries_superseded_by,
        //       idx_graph_edges_source_type, idx_graph_edges_target_type
        // are intentionally absent — they are added by the v27 migration.
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index/trigger");
    }

    // Seed counters at v26 (with correct next_audit_id, not the phantom).
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 26)",
        "INSERT INTO counters (name, value) VALUES ('next_entry_id', 1)",
        "INSERT INTO counters (name, value) VALUES ('next_signal_id', 0)",
        "INSERT INTO counters (name, value) VALUES ('next_log_id', 0)",
        "INSERT INTO counters (name, value) VALUES ('next_audit_id', 0)",
    ] {
        sqlx::query(seed)
            .execute(&mut conn)
            .await
            .expect("seed counters");
    }

    // Seed a few entries and graph_edges rows so the migration runs on real data.
    sqlx::query(
        "INSERT INTO entries
             (id, title, content, topic, category, source, status, confidence,
              created_at, updated_at, supersedes, superseded_by)
         VALUES
             (1, 'Entry A', 'content a', 'rust', 'pattern', 'agent', 0, 0.5, 0, 0, NULL, NULL),
             (2, 'Entry B', 'content b', 'rust', 'pattern', 'agent', 0, 0.5, 0, 0, 1, NULL),
             (3, 'Entry C', 'content c', 'rust', 'convention', 'agent', 0, 0.5, 0, 0, NULL, NULL)",
    )
    .execute(&mut conn)
    .await
    .expect("seed entries");

    sqlx::query(
        "INSERT INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
         VALUES
             (1, 2, 'Supersedes', 1.0, 0, 'bootstrap', 'entries.supersedes', 0),
             (2, 3, 'Informs', 0.8, 0, 'agent', 'context_edge', 0)",
    )
    .execute(&mut conn)
    .await
    .expect("seed graph_edges");
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

const V27_INDEX_NAMES: [&str; 4] = [
    "idx_entries_supersedes",
    "idx_entries_superseded_by",
    "idx_graph_edges_source_type",
    "idx_graph_edges_target_type",
];

// ---------------------------------------------------------------------------
// MIG-V27-U-01: CURRENT_SCHEMA_VERSION constant is >= 27
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_at_least_27() {
    assert!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 27,
        "CURRENT_SCHEMA_VERSION must be >= 27 after vnc-018, got {}",
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION
    );
}

// ---------------------------------------------------------------------------
// MIG-V27-U-02: Fresh database initializes directly to v27 with all 4 indexes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v27() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    assert!(
        read_schema_version(&store).await >= 27,
        "schema_version must be >= 27 on a fresh database"
    );

    // All 4 new indexes must be present on a fresh database (via create_tables_if_needed).
    for index_name in V27_INDEX_NAMES {
        let exists: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='index' AND name=?",
        )
        .bind(index_name)
        .fetch_one(store.read_pool_test())
        .await
        .expect("sqlite_master index check");
        assert!(
            exists,
            "Index '{index_name}' must exist on a fresh database (v27 schema)"
        );
    }

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V27-U-03: v26→v27 migration creates all 4 required indexes (AC-19)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v26_to_v27_creates_four_indexes() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v26_database(&db_path).await;

    // Verify that the four indexes are absent before migration.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut pre_conn = opts.connect().await.expect("pre-migration check conn");
        for index_name in V27_INDEX_NAMES {
            let exists: bool = sqlx::query_scalar(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='index' AND name=?",
            )
            .bind(index_name)
            .fetch_one(&mut pre_conn)
            .await
            .expect("pre-migration index check");
            assert!(
                !exists,
                "Pre-condition: index '{index_name}' must NOT exist before v27 migration"
            );
        }
    }

    // Trigger the v26→v27 migration by opening with SqlxStore.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    // Assert: all four indexes now exist in sqlite_master.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN (?,?,?,?)",
    )
    .bind("idx_entries_supersedes")
    .bind("idx_entries_superseded_by")
    .bind("idx_graph_edges_source_type")
    .bind("idx_graph_edges_target_type")
    .fetch_one(store.read_pool_test())
    .await
    .expect("count v27 indexes");

    assert_eq!(
        count, 4,
        "All four v27 indexes must be present after migration; found {count}"
    );

    // Assert each index individually for clearer failure messages.
    for index_name in V27_INDEX_NAMES {
        let exists: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='index' AND name=?",
        )
        .bind(index_name)
        .fetch_one(store.read_pool_test())
        .await
        .expect("sqlite_master index check");
        assert!(
            exists,
            "Index '{index_name}' must exist after v26→v27 migration"
        );
    }

    // Assert schema_version was updated to >= 27.
    let version = read_schema_version(&store).await;
    assert!(
        version >= 27,
        "schema_version must be >= 27 after v26→v27 migration, got {version}"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V27-U-04: Idempotency — re-open v27+ database is a no-op
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v27_migration_is_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v26_database(&db_path).await;

    // First open: triggers v26→v27 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("first open");
    let v1 = read_schema_version(&store).await;
    store.close().await.unwrap();

    // Second open: already at v27+, migration block must be skipped.
    let store2 = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("second open — idempotency check");
    let v2 = read_schema_version(&store2).await;

    // All four indexes still present — no duplicates, no errors.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN (?,?,?,?)",
    )
    .bind("idx_entries_supersedes")
    .bind("idx_entries_superseded_by")
    .bind("idx_graph_edges_source_type")
    .bind("idx_graph_edges_target_type")
    .fetch_one(store2.read_pool_test())
    .await
    .expect("count v27 indexes on re-open");

    store2.close().await.unwrap();

    assert!(
        v1 >= 27,
        "schema_version must be >= 27 after first open, got {v1}"
    );
    assert_eq!(
        v1, v2,
        "schema_version must not change on idempotent re-open"
    );
    assert_eq!(
        count, 4,
        "All four indexes must still be present on idempotent re-open; found {count}"
    );
}

// ---------------------------------------------------------------------------
// MIG-V27-U-05: schema_version updated to 27 after migration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v26_to_v27_schema_version_updated() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v26_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    let version = read_schema_version(&store).await;
    assert_eq!(
        version, 27,
        "schema_version must be exactly 27 after v26→v27 migration, got {version}"
    );

    store.close().await.unwrap();
}
