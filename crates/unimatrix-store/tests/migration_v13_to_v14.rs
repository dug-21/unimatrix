//! Integration tests for the v13→v14 schema migration (col-023).
//!
//! Covers: AC-09 (domain_metrics_json column added), R-05 (no positional offset
//! after migration), R-12 (named-column rollback safety), FM-05 (idempotency).
//!
//! Pattern: create a v13-shaped database, open with current SqlxStore to trigger
//! migration, assert schema state and data round-trips.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V13 database builder
// ---------------------------------------------------------------------------

/// Create a v13-shaped database at the given path.
///
/// Contains all tables present at v13 (with graph_edges). schema_version = 13.
/// The observation_metrics table intentionally lacks `domain_metrics_json` —
/// that column is what the v13→v14 migration adds.
async fn create_v13_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v13 setup conn");

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
            entry_id INTEGER NOT NULL,
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
        // observation_metrics WITHOUT domain_metrics_json — this is v13 shape.
        // Migration adds that column.
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
            scope_hotspot_count                INTEGER NOT NULL DEFAULT 0
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
            source TEXT NOT NULL
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
        "CREATE INDEX idx_graph_edges_source_id ON graph_edges(source_id)",
        "CREATE INDEX idx_graph_edges_target_id ON graph_edges(target_id)",
        "CREATE INDEX idx_graph_edges_relation_type ON graph_edges(relation_type)",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index");
    }

    // Seed counters at v13.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 13)",
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

async fn domain_metrics_json_column_exists(store: &SqlxStore) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('observation_metrics') WHERE name = 'domain_metrics_json'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check domain_metrics_json column");
    count > 0
}

// ---------------------------------------------------------------------------
// T-MIG-08: CURRENT_SCHEMA_VERSION constant = 14
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_14() {
    // Simple constant check to catch accidental off-by-one in version bump.
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        14,
        "CURRENT_SCHEMA_VERSION must be 14"
    );
}

// ---------------------------------------------------------------------------
// T-MIG-01: Fresh database creates schema v14 directly (AC-09)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v14() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: open a fresh database — no prior schema exists.
    // Act: SqlxStore::open calls create_tables_if_needed() for fresh DBs.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Assert: schema_version == 14
    assert_eq!(read_schema_version(&store).await, 14);

    // Assert: domain_metrics_json column exists in observation_metrics
    assert!(
        domain_metrics_json_column_exists(&store).await,
        "fresh database must have domain_metrics_json column (AC-09)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-02: v13 → v14 migration adds domain_metrics_json column (AC-09, R-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v13_to_v14_migration_adds_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v13 database without domain_metrics_json column.
    create_v13_database(&db_path).await;

    // Act: open with current code → triggers v13→v14 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store after v13→v14 migration");

    // Assert: schema_version == 14
    assert_eq!(read_schema_version(&store).await, 14);

    // Assert: column now exists
    assert!(
        domain_metrics_json_column_exists(&store).await,
        "domain_metrics_json column must exist after v13→v14 migration (AC-09)"
    );

    // Assert: CURRENT_SCHEMA_VERSION Rust const is 14
    assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 14);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-03: Round-trip — write and read back all 21 original fields after migration (R-05)
//
// R-05: guards against positional column indexing regression. All 21 original
// fields must read back at their original positions after domain_metrics_json
// is appended as column 22. Named-column queries prevent offset errors.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v14_migration_round_trip_all_original_fields() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v13_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Insert a row using all 21 named original columns (named bindings, not positional).
    sqlx::query(
        "INSERT INTO observation_metrics (
            feature_cycle, computed_at,
            total_tool_calls, total_duration_secs, session_count,
            search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
            permission_friction_events, bash_for_search_count, cold_restart_events,
            coordinator_respawn_count, parallel_call_rate, context_load_before_first_write_kb,
            total_context_loaded_kb, post_completion_work_pct, follow_up_issues_created,
            knowledge_entries_stored, sleep_workaround_count, agent_hotspot_count,
            friction_hotspot_count, session_hotspot_count, scope_hotspot_count
        ) VALUES (
            'round-trip-fc', 1700000000,
            42, 3600, 3,
            0.25, 10.5, 0.3,
            5, 2, 1,
            0, 0.8, 128.0,
            512.0, 0.05, 1,
            7, 0, 1,
            2, 1, 0
        )",
    )
    .execute(store.write_pool_test())
    .await
    .expect("insert round-trip row");

    // SELECT all 21 original columns individually by name — R-05: no positional offset regression.
    // sqlx tuple FromRow is limited to 16 elements; use scalar queries per named column instead.
    use sqlx::Row as _;

    let row = sqlx::query(
        "SELECT total_tool_calls, total_duration_secs, session_count,
                search_miss_rate, edit_bloat_total_kb, edit_bloat_ratio,
                permission_friction_events, bash_for_search_count, cold_restart_events,
                coordinator_respawn_count, parallel_call_rate, context_load_before_first_write_kb,
                total_context_loaded_kb, post_completion_work_pct, follow_up_issues_created,
                knowledge_entries_stored, sleep_workaround_count, agent_hotspot_count,
                friction_hotspot_count, session_hotspot_count, scope_hotspot_count
         FROM observation_metrics WHERE feature_cycle = 'round-trip-fc'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch round-trip row");

    // Each field is verified by name to confirm no positional offset (R-05).
    let total_tool_calls: i64 = row.try_get("total_tool_calls").unwrap();
    let total_duration_secs: i64 = row.try_get("total_duration_secs").unwrap();
    let session_count: i64 = row.try_get("session_count").unwrap();
    let search_miss_rate: f64 = row.try_get("search_miss_rate").unwrap();
    let edit_bloat_total_kb: f64 = row.try_get("edit_bloat_total_kb").unwrap();
    let edit_bloat_ratio: f64 = row.try_get("edit_bloat_ratio").unwrap();
    let permission_friction_events: i64 = row.try_get("permission_friction_events").unwrap();
    let bash_for_search_count: i64 = row.try_get("bash_for_search_count").unwrap();
    let cold_restart_events: i64 = row.try_get("cold_restart_events").unwrap();
    let coordinator_respawn_count: i64 = row.try_get("coordinator_respawn_count").unwrap();
    let parallel_call_rate: f64 = row.try_get("parallel_call_rate").unwrap();
    let context_load_before_first_write_kb: f64 =
        row.try_get("context_load_before_first_write_kb").unwrap();
    let total_context_loaded_kb: f64 = row.try_get("total_context_loaded_kb").unwrap();
    let post_completion_work_pct: f64 = row.try_get("post_completion_work_pct").unwrap();
    let follow_up_issues_created: i64 = row.try_get("follow_up_issues_created").unwrap();
    let knowledge_entries_stored: i64 = row.try_get("knowledge_entries_stored").unwrap();
    let sleep_workaround_count: i64 = row.try_get("sleep_workaround_count").unwrap();
    let agent_hotspot_count: i64 = row.try_get("agent_hotspot_count").unwrap();
    let friction_hotspot_count: i64 = row.try_get("friction_hotspot_count").unwrap();
    let session_hotspot_count: i64 = row.try_get("session_hotspot_count").unwrap();
    let scope_hotspot_count: i64 = row.try_get("scope_hotspot_count").unwrap();

    assert_eq!(total_tool_calls, 42, "total_tool_calls R-05");
    assert_eq!(total_duration_secs, 3600, "total_duration_secs R-05");
    assert_eq!(session_count, 3, "session_count R-05");
    assert!(
        (search_miss_rate - 0.25).abs() < 1e-9,
        "search_miss_rate R-05"
    );
    assert!(
        (edit_bloat_total_kb - 10.5).abs() < 1e-9,
        "edit_bloat_total_kb R-05"
    );
    assert!(
        (edit_bloat_ratio - 0.3).abs() < 1e-9,
        "edit_bloat_ratio R-05"
    );
    assert_eq!(
        permission_friction_events, 5,
        "permission_friction_events R-05"
    );
    assert_eq!(bash_for_search_count, 2, "bash_for_search_count R-05");
    assert_eq!(cold_restart_events, 1, "cold_restart_events R-05");
    assert_eq!(
        coordinator_respawn_count, 0,
        "coordinator_respawn_count R-05"
    );
    assert!(
        (parallel_call_rate - 0.8).abs() < 1e-9,
        "parallel_call_rate R-05"
    );
    assert!(
        (context_load_before_first_write_kb - 128.0).abs() < 1e-9,
        "context_load_before_first_write_kb R-05"
    );
    assert!(
        (total_context_loaded_kb - 512.0).abs() < 1e-9,
        "total_context_loaded_kb R-05"
    );
    assert!(
        (post_completion_work_pct - 0.05).abs() < 1e-9,
        "post_completion_work_pct R-05"
    );
    assert_eq!(follow_up_issues_created, 1, "follow_up_issues_created R-05");
    assert_eq!(knowledge_entries_stored, 7, "knowledge_entries_stored R-05");
    assert_eq!(sleep_workaround_count, 0, "sleep_workaround_count R-05");
    assert_eq!(agent_hotspot_count, 1, "agent_hotspot_count R-05");
    assert_eq!(friction_hotspot_count, 2, "friction_hotspot_count R-05");
    assert_eq!(session_hotspot_count, 1, "session_hotspot_count R-05");
    assert_eq!(scope_hotspot_count, 0, "scope_hotspot_count R-05");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-04: v13 row reads back NULL for domain_metrics_json (AC-09, FR-05.4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v13_row_reads_null_domain_metrics_json() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v13_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Insert a row that omits domain_metrics_json — simulates a row written by a v13 binary.
    sqlx::query(
        "INSERT INTO observation_metrics (
            feature_cycle, computed_at, total_tool_calls, scope_hotspot_count
         ) VALUES ('v13-row', 1700000000, 10, 0)",
    )
    .execute(store.write_pool_test())
    .await
    .expect("insert v13-style row");

    // Assert: domain_metrics_json is NULL for this row (AC-09).
    let domain_json: Option<String> = sqlx::query_scalar(
        "SELECT domain_metrics_json FROM observation_metrics WHERE feature_cycle = 'v13-row'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch domain_metrics_json");

    assert!(
        domain_json.is_none(),
        "v13-style row must read back NULL for domain_metrics_json (AC-09, FR-05.4)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-05: Schema version assertion post-migration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_schema_version_is_14_after_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v13_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Assert: counters table carries schema_version = 14.
    assert_eq!(read_schema_version(&store).await, 14);

    // Assert: Rust const agrees.
    assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 14);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-06: Migration is idempotent — running twice does not error (FM-05)
//
// FM-05: handles the case where the v14 migration was partially applied before
// a crash. The ALTER TABLE ADD COLUMN is guarded by a pragma_table_info pre-check
// so a second call is a no-op rather than an error.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v13_to_v14_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v13_database(&db_path).await;

    // First run — applies v13→v14 migration.
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("first open");
        assert_eq!(read_schema_version(&store).await, 14);
        assert!(domain_metrics_json_column_exists(&store).await);
        store.close().await.unwrap();
    }

    // Second run — must be a no-op; no error from duplicate ALTER TABLE.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open must succeed (FM-05: idempotent migration)");

    assert_eq!(read_schema_version(&store).await, 14);
    assert!(domain_metrics_json_column_exists(&store).await);

    // Only one domain_metrics_json column must exist.
    let col_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('observation_metrics') WHERE name = 'domain_metrics_json'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count domain_metrics_json columns");
    assert_eq!(
        col_count, 1,
        "exactly one domain_metrics_json column must exist after idempotent run"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-07: Rollback safety — v14 schema read by reduced struct (R-12)
//
// R-12: a downgraded binary using named-column queries must not be affected by
// the extra column. SQLite named-column queries are NOT affected by additional
// columns; only positional indexing would be broken.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v14_schema_named_column_readback_with_reduced_struct() {
    // R-12 rollback documentation:
    // When a v14 database is opened by a v13 binary, the v13 binary's SELECT
    // queries use named columns (e.g., SELECT total_tool_calls, ... FROM
    // observation_metrics WHERE ...). SQLite returns only the requested columns.
    // The extra domain_metrics_json column is invisible to the v13 binary.
    // Downgrade risk: only positional indexing (SELECT * then row.get(N)) would
    // be affected — named-column queries are unaffected. All reads in this
    // codebase use named columns, so downgrade is safe for existing rows.

    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v13_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Insert a row that includes domain_metrics_json (v14 binary writing).
    sqlx::query(
        "INSERT INTO observation_metrics (
            feature_cycle, computed_at, total_tool_calls, total_duration_secs,
            scope_hotspot_count, domain_metrics_json
         ) VALUES ('rollback-test', 1700000001, 99, 7200, 3, '{\"k\":1.0}')",
    )
    .execute(store.write_pool_test())
    .await
    .expect("insert v14 row with domain_metrics_json");

    // Simulate a v13 binary: SELECT using only the 21 original named columns.
    // Assert: correct values, no error, no panic (R-12).
    let (total_tool_calls, total_duration_secs, scope_hotspot_count): (i64, i64, i64) =
        sqlx::query_as(
            "SELECT total_tool_calls, total_duration_secs, scope_hotspot_count
             FROM observation_metrics WHERE feature_cycle = 'rollback-test'",
        )
        .fetch_one(store.read_pool_test())
        .await
        .expect("named-column read must succeed on v14 schema (R-12)");

    assert_eq!(
        total_tool_calls, 99,
        "total_tool_calls must be correct (R-12)"
    );
    assert_eq!(
        total_duration_secs, 7200,
        "total_duration_secs must be correct (R-12)"
    );
    assert_eq!(
        scope_hotspot_count, 3,
        "scope_hotspot_count must be correct (R-12)"
    );

    store.close().await.unwrap();
}
