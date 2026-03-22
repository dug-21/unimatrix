//! Integration tests for the v14→v15 schema migration (crt-025).
//!
//! Covers: AC-10 (cycle_events table created), AC-11 (feature_entries.phase column added),
//! R-05 (no positional offset after migration), R-10 (fresh DB at v15), NFR-05 (idempotency),
//! C-05 (no backfill), C-08 (pragma_table_info guard).
//!
//! Pattern: create a v14-shaped database, open with current SqlxStore to trigger
//! migration, assert schema state and data round-trips.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V14 database builder
// ---------------------------------------------------------------------------

/// Create a v14-shaped database at the given path.
///
/// Contains all tables present at v14 (with graph_edges and domain_metrics_json).
/// schema_version = 14.
/// `feature_entries` intentionally lacks a `phase` column.
/// `cycle_events` table is absent — both are what v14→v15 adds.
async fn create_v14_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v14 setup conn");

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
        // feature_entries WITHOUT a phase column — this is what v14→v15 adds.
        "CREATE TABLE feature_entries (
            feature_id TEXT NOT NULL,
            entry_id   INTEGER NOT NULL,
            PRIMARY KEY (feature_id, entry_id)
        )",
        // No cycle_events table — this is the other v14→v15 addition.
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
        // observation_metrics WITH domain_metrics_json — this is v14 shape.
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

    // Seed counters at v14.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 14)",
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

async fn cycle_events_table_exists(store: &SqlxStore) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cycle_events'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check cycle_events table");
    count > 0
}

async fn phase_column_exists_on_feature_entries(store: &SqlxStore) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('feature_entries') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check feature_entries.phase column");
    count > 0
}

// ---------------------------------------------------------------------------
// Unit test: CURRENT_SCHEMA_VERSION constant = 15
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_15() {
    // Simple constant check to catch accidental off-by-one in version bump.
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        15,
        "CURRENT_SCHEMA_VERSION must be 15"
    );
}

// ---------------------------------------------------------------------------
// T-MIG-01: Fresh database creates schema v15 directly (AC-11, R-10)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v15() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: empty path — no prior DB.
    // Act: SqlxStore::open calls create_tables_if_needed() for fresh DBs.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Assert: schema_version == 15
    assert_eq!(
        read_schema_version(&store).await,
        15,
        "fresh database must be at schema v15"
    );

    // Assert: cycle_events table exists
    assert!(
        cycle_events_table_exists(&store).await,
        "fresh database must have cycle_events table"
    );

    // Assert: feature_entries has phase column
    assert!(
        phase_column_exists_on_feature_entries(&store).await,
        "fresh database must have feature_entries.phase column"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-01b: Fresh DB cycle_events table has correct schema (R-10, FR-07.2)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_cycle_events_table_schema() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Verify all expected columns are present via pragma_table_info.
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('cycle_events') ORDER BY cid")
            .fetch_all(store.read_pool_test())
            .await
            .expect("pragma_table_info cycle_events");

    for expected in &[
        "id",
        "cycle_id",
        "seq",
        "event_type",
        "phase",
        "outcome",
        "next_phase",
        "timestamp",
    ] {
        assert!(
            columns.iter().any(|c| c == expected),
            "cycle_events must have column '{expected}'"
        );
    }

    // Verify index exists.
    let idx_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_cycle_events_cycle_id'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check index");
    assert_eq!(idx_count, 1, "idx_cycle_events_cycle_id must exist");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-02: v14→v15 adds cycle_events table (AC-10, R-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v14_to_v15_migration_adds_cycle_events_table() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v14 database without cycle_events table.
    create_v14_database(&db_path).await;

    // Act: open triggers v14→v15 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store after v14→v15 migration");

    // Assert: cycle_events table now exists.
    assert!(
        cycle_events_table_exists(&store).await,
        "cycle_events table must exist after v14→v15 migration (AC-10)"
    );

    // Assert: schema_version == 15.
    assert_eq!(read_schema_version(&store).await, 15);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-02b: v14→v15 adds phase column to feature_entries (AC-10, R-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v14_to_v15_migration_adds_phase_column_to_feature_entries() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v14 database — feature_entries has no phase column.
    create_v14_database(&db_path).await;

    // Act: open triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store after v14→v15 migration");

    // Assert: phase column now exists on feature_entries.
    assert!(
        phase_column_exists_on_feature_entries(&store).await,
        "feature_entries.phase column must exist after v14→v15 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-02c: Pre-existing feature_entries rows have NULL phase (C-05, FR-06.4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v14_pre_existing_rows_have_null_phase() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v14 database with a pre-seeded feature_entries row.
    create_v14_database(&db_path).await;
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query(
            "INSERT INTO feature_entries (feature_id, entry_id) VALUES ('old-feature', 99)",
        )
        .execute(&mut conn)
        .await
        .expect("insert pre-existing row");
    }

    // Act: open triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: phase IS NULL — no backfill (C-05, FR-06.4).
    let phase: Option<String> =
        sqlx::query_scalar("SELECT phase FROM feature_entries WHERE entry_id = 99")
            .fetch_one(store.read_pool_test())
            .await
            .expect("fetch phase for pre-existing row");

    assert!(
        phase.is_none(),
        "pre-existing feature_entries rows must have phase = NULL (C-05, no backfill)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-03: Migration is idempotent (AC-10, R-05, NFR-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v14_to_v15_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v14_database(&db_path).await;

    // Run 1: applies v14→v15 migration.
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("first open");
        assert_eq!(read_schema_version(&store).await, 15);
        assert!(cycle_events_table_exists(&store).await);
        assert!(phase_column_exists_on_feature_entries(&store).await);
        store.close().await.unwrap();
    }

    // Run 2: must be a no-op — no errors.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open must succeed (NFR-05: idempotent)");

    assert_eq!(read_schema_version(&store).await, 15);
    assert!(cycle_events_table_exists(&store).await);
    assert!(phase_column_exists_on_feature_entries(&store).await);

    // Exactly one phase column must exist.
    let col_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('feature_entries') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count phase columns");
    assert_eq!(
        col_count, 1,
        "exactly one phase column after idempotent run"
    );

    // Exactly one cycle_events table must exist.
    let tbl_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cycle_events'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count cycle_events tables");
    assert_eq!(
        tbl_count, 1,
        "exactly one cycle_events table after idempotent run"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-03b: pragma_table_info guard prevents duplicate column (NFR-05, C-08)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pragma_table_info_guard_prevents_duplicate_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v14 database, then manually add the phase column before opening.
    create_v14_database(&db_path).await;
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query("ALTER TABLE feature_entries ADD COLUMN phase TEXT")
            .execute(&mut conn)
            .await
            .expect("manually add phase column");
    }

    // Act: open store — migration sees column already exists and skips ALTER TABLE.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open must succeed — pragma guard skips duplicate ALTER TABLE (C-08)");

    // Assert: no error, schema_version = 15, column exists exactly once.
    assert_eq!(read_schema_version(&store).await, 15);
    let col_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('feature_entries') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count phase columns");
    assert_eq!(
        col_count, 1,
        "exactly one phase column after pragma guard (C-08)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-04: Schema version is 15 after migration (AC-10)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_schema_version_is_15_after_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v14_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Assert: counters table carries schema_version = 15.
    assert_eq!(read_schema_version(&store).await, 15);

    // Assert: Rust const agrees.
    assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 15);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-05: Data round-trip — feature_entries with phase column (R-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v15_feature_entries_round_trip_with_phase() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v14_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Insert a row with the new phase column.
    sqlx::query(
        "INSERT INTO feature_entries (feature_id, entry_id, phase) VALUES ('crt-025', 1, 'scope')",
    )
    .execute(store.write_pool_test())
    .await
    .expect("insert feature_entries row with phase");

    // Select back by named columns — R-05: no positional offset regression.
    use sqlx::Row as _;
    let row =
        sqlx::query("SELECT feature_id, entry_id, phase FROM feature_entries WHERE entry_id = 1")
            .fetch_one(store.read_pool_test())
            .await
            .expect("fetch feature_entries row");

    let feature_id: String = row.try_get("feature_id").unwrap();
    let entry_id: i64 = row.try_get("entry_id").unwrap();
    let phase: Option<String> = row.try_get("phase").unwrap();

    assert_eq!(feature_id, "crt-025");
    assert_eq!(entry_id, 1);
    assert_eq!(phase.as_deref(), Some("scope"));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_v15_feature_entries_null_phase_row() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v14_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Insert a row with NULL phase — backward compatible path.
    sqlx::query(
        "INSERT INTO feature_entries (feature_id, entry_id, phase) VALUES ('crt-025', 2, NULL)",
    )
    .execute(store.write_pool_test())
    .await
    .expect("insert feature_entries row with null phase");

    let phase: Option<String> =
        sqlx::query_scalar("SELECT phase FROM feature_entries WHERE entry_id = 2")
            .fetch_one(store.read_pool_test())
            .await
            .expect("fetch phase");

    assert!(phase.is_none(), "NULL phase must round-trip correctly");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-MIG-05b: Data round-trip — cycle_events via insert_cycle_event (R-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v15_cycle_events_round_trip() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v14_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Insert via SqlxStore::insert_cycle_event.
    store
        .insert_cycle_event(
            "crt-025", // cycle_id
            0,         // seq
            "cycle_start",
            Some("scope"),
            None,
            Some("design"),
            1700000000, // timestamp
        )
        .await
        .expect("insert_cycle_event must succeed");

    // Query back by cycle_id.
    use sqlx::Row as _;
    let row = sqlx::query(
        "SELECT cycle_id, seq, event_type, phase, outcome, next_phase, timestamp
           FROM cycle_events WHERE cycle_id = 'crt-025'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch cycle_events row");

    let cycle_id: String = row.try_get("cycle_id").unwrap();
    let seq: i64 = row.try_get("seq").unwrap();
    let event_type: String = row.try_get("event_type").unwrap();
    let phase: Option<String> = row.try_get("phase").unwrap();
    let outcome: Option<String> = row.try_get("outcome").unwrap();
    let next_phase: Option<String> = row.try_get("next_phase").unwrap();
    let timestamp: i64 = row.try_get("timestamp").unwrap();

    assert_eq!(cycle_id, "crt-025");
    assert_eq!(seq, 0);
    assert_eq!(event_type, "cycle_start");
    assert_eq!(phase.as_deref(), Some("scope"));
    assert!(outcome.is_none());
    assert_eq!(next_phase.as_deref(), Some("design"));
    assert_eq!(timestamp, 1700000000);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_v15_cycle_events_all_nullable_columns_null() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v14_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Insert with all nullable fields as None.
    store
        .insert_cycle_event("test-cycle", 1, "cycle_stop", None, None, None, 1700000001)
        .await
        .expect("insert_cycle_event with nulls must succeed");

    use sqlx::Row as _;
    let row = sqlx::query(
        "SELECT phase, outcome, next_phase FROM cycle_events WHERE cycle_id = 'test-cycle'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch row");

    let phase: Option<String> = row.try_get("phase").unwrap();
    let outcome: Option<String> = row.try_get("outcome").unwrap();
    let next_phase: Option<String> = row.try_get("next_phase").unwrap();

    assert!(phase.is_none(), "phase must be NULL");
    assert!(outcome.is_none(), "outcome must be NULL");
    assert!(next_phase.is_none(), "next_phase must be NULL");

    store.close().await.unwrap();
}
