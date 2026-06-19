//! Integration tests for the v25→v26 schema migration (bugfix-587).
//!
//! Covers:
//!   MIG-V26-U-01 — CURRENT_SCHEMA_VERSION constant is >= 26
//!   MIG-V26-U-02 — Fresh database initializes directly to v26+
//!   MIG-V26-U-03 — v25→v26 migration removes phantom next_audit_event_id counter
//!   MIG-V26-U-04 — v25→v26 migration creates next_audit_id counter seeded from max event_id
//!   MIG-V26-U-05 — Idempotency: re-open v26 database is a no-op
//!
//! Pattern: create a v25-shaped database (audit_log with 12 columns + append-only triggers
//! but with `next_audit_event_id` counter seeded at 0 instead of `next_audit_id`).
//! Open with current SqlxStore to trigger v25→v26 migration. Assert schema state.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V25 database builder
// ---------------------------------------------------------------------------

/// Create a v25-shaped database at the given path.
///
/// The v25 DDL = v24 DDL + four audit_log attribution columns + append-only triggers.
/// The v25 counters include the phantom `next_audit_event_id` row (the bug fixed by bugfix-587).
/// Does NOT include `next_audit_id` (added by the v26 migration).
async fn create_v25_database(path: &Path, seed_audit_rows: bool) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v25 setup conn");

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
        // v25 audit_log: 12 columns (4 new attribution columns added by v24→v25 migration)
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
        // Append-only triggers (added by v24→v25 migration)
        "CREATE TRIGGER audit_log_no_update
         BEFORE UPDATE ON audit_log
         BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END",
        "CREATE TRIGGER audit_log_no_delete
         BEFORE DELETE ON audit_log
         BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index/trigger");
    }

    // Seed counters at v25 — NOTE: includes phantom `next_audit_event_id` (the bug).
    // Does NOT include `next_audit_id` (added by v26 migration).
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 25)",
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

    if seed_audit_rows {
        // Seed audit_log rows to verify that next_audit_id is seeded from max(event_id).
        for i in 1i64..=3 {
            sqlx::query(
                "INSERT INTO audit_log
                     (event_id, timestamp, session_id, agent_id, operation, target_ids,
                      outcome, detail, credential_type, capability_used, agent_attribution, metadata)
                 VALUES (?1, ?2, 'sess-1', 'agent-1', 'context_store', '[]', 0, 'seeded',
                         'none', '', '', '{}')",
            )
            .bind(i * 10) // event_id 10, 20, 30 — max is 30
            .bind(1_700_000_000_i64 + i)
            .execute(&mut conn)
            .await
            .expect("seed audit_log row");
        }
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
// MIG-V26-U-01: CURRENT_SCHEMA_VERSION constant is >= 26
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_at_least_26() {
    const {
        assert!(
            unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 26,
            "CURRENT_SCHEMA_VERSION must be >= 26 after bugfix-587"
        )
    };
}

// ---------------------------------------------------------------------------
// MIG-V26-U-02: Fresh database initializes directly to v26+
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v26() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    assert!(
        read_schema_version(&store).await >= 26,
        "schema_version must be >= 26 on a fresh database"
    );

    // next_audit_id must be present (not next_audit_event_id).
    let has_audit_id: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM counters WHERE name = 'next_audit_id'")
            .fetch_one(store.read_pool_test())
            .await
            .expect("check next_audit_id exists");
    assert!(
        has_audit_id,
        "next_audit_id must be present on a fresh database"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V26-U-03: v25→v26 migration removes phantom next_audit_event_id counter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_to_v26_removes_phantom_counter() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v25_database(&db_path, false).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    // The phantom `next_audit_event_id` counter must be removed.
    let has_phantom: bool =
        sqlx::query_scalar("SELECT COUNT(*) > 0 FROM counters WHERE name = 'next_audit_event_id'")
            .fetch_one(store.read_pool_test())
            .await
            .expect("check phantom counter");
    assert!(
        !has_phantom,
        "phantom next_audit_event_id counter must be removed by v25→v26 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V26-U-04: v25→v26 migration creates next_audit_id seeded from max(event_id)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_to_v26_seeds_next_audit_id_from_max_event_id() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v25_database(&db_path, true).await; // seeds 3 rows with event_id 10, 20, 30

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    let next_audit_id: i64 =
        sqlx::query_scalar("SELECT value FROM counters WHERE name = 'next_audit_id'")
            .fetch_one(store.read_pool_test())
            .await
            .expect("read next_audit_id");

    // next_audit_id must be seeded from max(event_id) = 30.
    assert_eq!(
        next_audit_id, 30,
        "next_audit_id must be seeded from max(event_id)=30 after v25→v26 migration, got {next_audit_id}"
    );

    assert!(
        read_schema_version(&store).await >= 26,
        "schema_version must be >= 26 after v25→v26 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V26-U-05: Idempotency — re-open v26+ database is a no-op
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v26_migration_is_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v25_database(&db_path, false).await;

    // First open: triggers v25→v26 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("first open");
    let v1 = read_schema_version(&store).await;
    store.close().await.unwrap();

    // Second open: already at v26+, migration block must be a no-op.
    let store2 = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("second open");
    let v2 = read_schema_version(&store2).await;
    store2.close().await.unwrap();

    assert!(
        v1 >= 26,
        "schema_version must be >= 26 after first open, got {v1}"
    );
    assert_eq!(
        v1, v2,
        "schema_version must not change on idempotent re-open"
    );
}
