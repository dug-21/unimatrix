//! Integration tests for the v27→v28 schema migration (vnc-030, ADR-005).
//!
//! Adds `observations.topic_source TEXT NULL` — the F6 (#682) retirement-gate
//! evidence base. Pragma-guarded idempotent ALTER (pattern #4092/C-07); version
//! stamp at the end of `run_main_migrations` in one transaction. No backfill:
//! pre-vnc-030 rows stay NULL-source by design (SR-04/R-20).
//!
//! Covers (test-plan/topic-source-migration.md):
//!   migration_fresh_db_adds_topic_source_column   — fresh DB → column present, v28
//!   migration_already_migrated_db_is_noop         — re-run is a pragma-guarded no-op
//!   migration_pre_migration_v27_to_v28            — v27 DB → column added, stamped 28
//!   migration_leaves_existing_rows_null           — no backfill; existing rows NULL
//!   current_schema_version_is_28_unique           — constant is 28
//!
//! Pattern: build a v27-shaped database (v26 DDL + the four context_graph indexes,
//! observations WITHOUT topic_source, schema_version=27). Open with the current
//! SqlxStore to trigger v27→v28. Assert `topic_source` exists, is TEXT/nullable,
//! version is 28, and pre-existing rows remain NULL-source.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V27 database builder
// ---------------------------------------------------------------------------

/// Create a v27-shaped database at the given path.
///
/// The v27 DDL = v26 DDL + the four context_graph indexes added by the v27
/// migration. The `observations` table has its v26 shape (10 columns) WITHOUT
/// `topic_source` — that column is added by the v28 migration under test.
async fn create_v27_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v27 setup conn");

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
        // v27 observations: 10 columns, WITHOUT topic_source (added by v28).
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
        // The four context_graph indexes added by the v27 migration (present at v27).
        "CREATE INDEX idx_entries_supersedes ON entries(supersedes)",
        "CREATE INDEX idx_entries_superseded_by ON entries(superseded_by)",
        "CREATE INDEX idx_graph_edges_source_type ON graph_edges(source_id, relation_type)",
        "CREATE INDEX idx_graph_edges_target_type ON graph_edges(target_id, relation_type)",
        // Append-only triggers from v24→v25 migration.
        "CREATE TRIGGER audit_log_no_update
         BEFORE UPDATE ON audit_log
         BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not permitted'); END",
        "CREATE TRIGGER audit_log_no_delete
         BEFORE DELETE ON audit_log
         BEGIN SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not permitted'); END",
        // NOTE: observations.topic_source is intentionally absent — added by v28.
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index/trigger");
    }

    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 27)",
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

    // Seed pre-migration observation rows (no topic_source column yet) so the
    // no-backfill guarantee can be asserted post-migration.
    sqlx::query(
        "INSERT INTO observations
             (session_id, ts_millis, hook, tool, topic_signal, phase)
         VALUES
             ('s1', 100, 'PostToolUse', 'Edit', 'rust', 'design'),
             ('s2', 200, 'PostToolUse', 'Bash', NULL, NULL)",
    )
    .execute(&mut conn)
    .await
    .expect("seed observations");
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

async fn topic_source_column_count(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'topic_source'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma_table_info topic_source")
}

// ---------------------------------------------------------------------------
// current_schema_version_is_28_unique (R-11)
// ---------------------------------------------------------------------------

#[test]
#[allow(clippy::assertions_on_constants)] // version constant is compile-time; assertion guards the bump
fn test_current_schema_version_is_28() {
    const {
        assert!(
            unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 28,
            "CURRENT_SCHEMA_VERSION must be >= 28 after vnc-030"
        )
    };
}

// ---------------------------------------------------------------------------
// migration_fresh_db_adds_topic_source_column
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_has_topic_source_column() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    assert!(
        read_schema_version(&store).await >= 28,
        "schema_version must be >= 28 on a fresh database"
    );

    // Column present exactly once, TEXT, nullable.
    assert_eq!(
        topic_source_column_count(&store).await,
        1,
        "fresh database (v28 schema) must have observations.topic_source"
    );

    let (col_type, notnull): (String, i64) = sqlx::query_as(
        "SELECT type, \"notnull\" FROM pragma_table_info('observations') WHERE name = 'topic_source'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma type/notnull");
    assert_eq!(col_type, "TEXT", "topic_source must be TEXT");
    assert_eq!(notnull, 0, "topic_source must be nullable (NULL allowed)");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// migration_pre_migration_v27_to_v28
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v27_to_v28_adds_topic_source() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v27_database(&db_path).await;

    // Pre-condition: column absent before migration.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut pre_conn = opts.connect().await.expect("pre-migration check conn");
        let pre: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'topic_source'",
        )
        .fetch_one(&mut pre_conn)
        .await
        .expect("pre-migration topic_source check");
        assert_eq!(
            pre, 0,
            "pre-condition: observations.topic_source must NOT exist at v27"
        );
    }

    // Trigger v27→v28 by opening with SqlxStore.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    assert_eq!(
        topic_source_column_count(&store).await,
        1,
        "observations.topic_source must exist after v27→v28 migration"
    );
    assert!(
        read_schema_version(&store).await >= 28,
        "schema_version must be >= 28 after v27→v28 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// migration_already_migrated_db_is_noop (idempotency via pragma guard)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v28_migration_is_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v27_database(&db_path).await;

    // First open: triggers v27→v28.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("first open");
    let v1 = read_schema_version(&store).await;
    let c1 = topic_source_column_count(&store).await;
    store.close().await.unwrap();

    // Second open: already at v28 — the `current_version < 28` block is skipped,
    // and even if entered the pragma guard makes the ALTER a no-op (no duplicate
    // column, no error).
    let store2 = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("second open — idempotency check");
    let v2 = read_schema_version(&store2).await;
    let c2 = topic_source_column_count(&store2).await;
    store2.close().await.unwrap();

    assert!(
        v1 >= 28,
        "schema_version must be >= 28 after first open, got {v1}"
    );
    assert_eq!(
        v1, v2,
        "schema_version must not change on idempotent re-open"
    );
    assert_eq!(
        c1, 1,
        "topic_source must exist exactly once after first open"
    );
    assert_eq!(
        c2, 1,
        "topic_source must still exist exactly once on idempotent re-open (no duplicate)"
    );
}

// ---------------------------------------------------------------------------
// migration_leaves_existing_rows_null (no backfill, R-20)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_leaves_existing_rows_null() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v27_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    // The two seeded pre-migration rows are preserved...
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM observations")
        .fetch_one(store.read_pool_test())
        .await
        .expect("count observations");
    assert_eq!(total, 2, "pre-migration observation rows must be preserved");

    // ...and every existing row's topic_source is NULL (no backfill by design,
    // ADR-005 §3 / R-20: F6 distribution windows on post-migration rows only).
    let null_sources: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM observations WHERE topic_source IS NULL")
            .fetch_one(store.read_pool_test())
            .await
            .expect("count null topic_source");
    assert_eq!(
        null_sources, 2,
        "all pre-migration rows must have NULL topic_source (no backfill)"
    );

    store.close().await.unwrap();
}
