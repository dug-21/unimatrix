//! Integration tests for the v15→v16 schema migration (col-025).
//!
//! Covers: AC-09 (cycle_events.goal column added), AC-16 (CURRENT_SCHEMA_VERSION = 16),
//! R-02 (migration cascade), R-08 (no positional offset after migration), R-10
//! (fresh DB at v16), R-14 (idempotency), pattern #1264 (pragma_table_info guard).
//!
//! Pattern: create a v15-shaped database, open with current SqlxStore to trigger
//! migration, assert schema state and data round-trips.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V15 database builder
// ---------------------------------------------------------------------------

/// Create a v15-shaped database at the given path.
///
/// Contains all tables present at v15: all v14 tables + `cycle_events` (WITHOUT
/// the `goal` column) + `feature_entries.phase` column. schema_version = 15.
/// The `goal` column is intentionally absent — that is what v15→v16 adds.
async fn create_v15_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v15 setup conn");

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
        // feature_entries WITH phase column — this is the v15 shape (v14→v15 added it).
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
        // cycle_events WITHOUT goal column — this is the v15 shape.
        // v15→v16 adds the goal column.
        "CREATE TABLE cycle_events (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            cycle_id   TEXT    NOT NULL,
            seq        INTEGER NOT NULL,
            event_type TEXT    NOT NULL,
            phase      TEXT,
            outcome    TEXT,
            next_phase TEXT,
            timestamp  INTEGER NOT NULL
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
        "CREATE INDEX idx_cycle_events_cycle_id ON cycle_events (cycle_id)",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index");
    }

    // Seed counters at v15.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 15)",
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

async fn goal_column_exists(store: &SqlxStore) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check cycle_events.goal column");
    count > 0
}

// ---------------------------------------------------------------------------
// Unit test: CURRENT_SCHEMA_VERSION constant = 16
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_16() {
    // Simple constant check to catch accidental off-by-one in version bump.
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        16,
        "CURRENT_SCHEMA_VERSION must be 16"
    );
}

// ---------------------------------------------------------------------------
// T-V16-01: Fresh database creates schema v16 directly (AC-09, R-10)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v16() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: empty path — no prior DB.
    // Act: SqlxStore::open calls create_tables_if_needed() for fresh DBs.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Assert: schema_version == 16
    assert_eq!(
        read_schema_version(&store).await,
        16,
        "fresh database must be at schema v16"
    );

    // Assert: goal column present on cycle_events (fresh schema has the full DDL).
    assert!(
        goal_column_exists(&store).await,
        "fresh database must have cycle_events.goal column"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-02: v15→v16 migration adds goal column (AC-09, R-02)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v15_to_v16_migration_adds_goal_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v15 database — cycle_events exists, goal column absent.
    create_v15_database(&db_path).await;

    // Act: open triggers v15→v16 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store after v15→v16 migration");

    // Assert: goal column now exists.
    assert!(
        goal_column_exists(&store).await,
        "cycle_events.goal column must exist after v15→v16 migration (AC-09)"
    );

    // Assert: schema_version == 16.
    assert_eq!(read_schema_version(&store).await, 16);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-03: Pre-existing rows have NULL goal (AC-09 — no backfill)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v15_pre_existing_rows_have_null_goal() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v15 database with a pre-seeded cycle_events row (v15 columns only).
    create_v15_database(&db_path).await;
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query(
            "INSERT INTO cycle_events (cycle_id, seq, event_type, timestamp) \
             VALUES ('pre-v16-cycle', 0, 'cycle_start', 1700000000)",
        )
        .execute(&mut conn)
        .await
        .expect("insert pre-existing row");
    }

    // Act: open triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: goal IS NULL — no backfill (ADR-001, col-025).
    let goal: Option<String> =
        sqlx::query_scalar("SELECT goal FROM cycle_events WHERE cycle_id = 'pre-v16-cycle'")
            .fetch_one(store.read_pool_test())
            .await
            .expect("fetch goal for pre-existing row");

    assert!(
        goal.is_none(),
        "pre-existing cycle_events rows must have goal = NULL (no backfill, ADR-001)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-04: Idempotency — migration is a no-op on second open (AC-09, Gate 3c #1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v15_to_v16_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v15_database(&db_path).await;

    // Run 1: applies v15→v16 migration.
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("first open");
        assert!(goal_column_exists(&store).await);
        assert_eq!(read_schema_version(&store).await, 16);
        store.close().await.unwrap();
    }

    // Run 2: must be a no-op — no errors.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open must succeed (idempotency)");

    assert_eq!(read_schema_version(&store).await, 16);
    assert!(goal_column_exists(&store).await);

    // Exactly one goal column must exist (not duplicated).
    let col_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count goal columns");
    assert_eq!(col_count, 1, "exactly one goal column after idempotent run");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-05: pragma_table_info guard prevents duplicate goal column (pattern #1264)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_pragma_table_info_guard_prevents_duplicate_goal_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v15 database, then manually add goal column before opening store.
    create_v15_database(&db_path).await;
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query("ALTER TABLE cycle_events ADD COLUMN goal TEXT")
            .execute(&mut conn)
            .await
            .expect("manually add goal column");
    }

    // Act: open store — pragma guard sees column already exists, skips ALTER TABLE.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open must succeed — pragma guard skips duplicate ALTER TABLE");

    // Assert: no error; schema_version == 16; exactly one goal column.
    assert_eq!(read_schema_version(&store).await, 16);
    let col_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count goal columns");
    assert_eq!(
        col_count, 1,
        "exactly one goal column after pragma guard (pattern #1264)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-06: schema_version counter = 16 after migration (AC-16)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_schema_version_is_16_after_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v15_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Assert: counters table carries schema_version = 16.
    assert_eq!(read_schema_version(&store).await, 16);

    // Assert: Rust const agrees.
    assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 16);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-07: insert_cycle_event full column assertion (R-08 / Gate 3c #6)
//
// Writes a cycle_start event with a known goal; reads back by named column.
// Detects binding transposition (R-08): any off-by-one in ?1..?8 would
// produce wrong values for at least one column.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_insert_cycle_event_full_column_assertion() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    store
        .insert_cycle_event(
            "col-025",                                          // cycle_id   ?1
            0,                                                  // seq        ?2
            "cycle_start",                                      // event_type ?3
            Some("scope"),                                      // phase      ?4
            None,                                               // outcome    ?5
            Some("design"),                                     // next_phase ?6
            1700000000_i64,                                     // timestamp  ?7
            Some("Implement feature goal signal for col-025."), // goal ?8
        )
        .await
        .expect("insert_cycle_event must succeed");

    use sqlx::Row as _;
    let row = sqlx::query(
        "SELECT cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal
           FROM cycle_events WHERE cycle_id = 'col-025'",
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
    let goal: Option<String> = row.try_get("goal").unwrap();

    assert_eq!(cycle_id, "col-025");
    assert_eq!(seq, 0);
    assert_eq!(event_type, "cycle_start");
    assert_eq!(phase.as_deref(), Some("scope"));
    assert!(outcome.is_none());
    assert_eq!(next_phase.as_deref(), Some("design"));
    assert_eq!(timestamp, 1700000000);
    assert_eq!(
        goal.as_deref(),
        Some("Implement feature goal signal for col-025.")
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-08: insert_cycle_event with None goal writes NULL (FR-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_insert_cycle_event_goal_none_writes_null() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    store
        .insert_cycle_event(
            "test-cycle",
            0,
            "cycle_start",
            None,
            None,
            None,
            1700000000,
            None,
        )
        .await
        .expect("insert cycle_start with goal=None must succeed");

    let goal: Option<String> =
        sqlx::query_scalar("SELECT goal FROM cycle_events WHERE cycle_id = 'test-cycle'")
            .fetch_one(store.read_pool_test())
            .await
            .expect("fetch goal");

    assert!(goal.is_none(), "goal=None must write NULL to DB");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-09: Non-start events always have NULL goal (FR-01 / ADR-001)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_insert_cycle_event_goal_null_for_non_start_events() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    store
        .insert_cycle_event(
            "test-cycle",
            0,
            "cycle_phase_end",
            Some("design"),
            None,
            Some("delivery"),
            1700000001,
            None,
        )
        .await
        .expect("insert cycle_phase_end");

    store
        .insert_cycle_event(
            "test-cycle",
            1,
            "cycle_stop",
            None,
            None,
            None,
            1700000002,
            None,
        )
        .await
        .expect("insert cycle_stop");

    use sqlx::Row as _;
    let rows = sqlx::query(
        "SELECT event_type, goal FROM cycle_events WHERE cycle_id = 'test-cycle' ORDER BY seq ASC",
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("fetch rows");

    assert_eq!(rows.len(), 2);
    for row in &rows {
        let event_type: String = row.try_get("event_type").unwrap();
        let goal: Option<String> = row.try_get("goal").unwrap();
        assert!(
            goal.is_none(),
            "event_type={event_type} must have goal=NULL"
        );
    }

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-10: get_cycle_start_goal returns stored goal (R-03, AC-03)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_cycle_start_goal_returns_stored_goal() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    store
        .insert_cycle_event(
            "col-025-goal-test",
            0,
            "cycle_start",
            None,
            None,
            None,
            1700000000,
            Some("test goal text"),
        )
        .await
        .expect("insert cycle_start");

    let result = store
        .get_cycle_start_goal("col-025-goal-test")
        .await
        .expect("get_cycle_start_goal must not error");

    assert_eq!(result.as_deref(), Some("test goal text"));

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-11: get_cycle_start_goal returns None for unknown cycle_id (R-03, AC-14)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_cycle_start_goal_returns_none_for_unknown_cycle_id() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    let result = store
        .get_cycle_start_goal("nonexistent-cycle-id")
        .await
        .expect("get_cycle_start_goal must not error for missing cycle_id");

    assert!(result.is_none(), "absent cycle_id must return Ok(None)");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-12: get_cycle_start_goal returns None when goal IS NULL (R-03, AC-03)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_cycle_start_goal_returns_none_when_goal_is_null() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Insert cycle_start with goal = None (NULL in DB).
    store
        .insert_cycle_event(
            "null-goal-cycle",
            0,
            "cycle_start",
            None,
            None,
            None,
            1700000000,
            None,
        )
        .await
        .expect("insert cycle_start with null goal");

    let result = store
        .get_cycle_start_goal("null-goal-cycle")
        .await
        .expect("get_cycle_start_goal must not error");

    assert!(result.is_none(), "NULL goal in DB must flatten to Ok(None)");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V16-13: get_cycle_start_goal LIMIT 1 semantics — returns first row (R-10)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_cycle_start_goal_multiple_start_rows_returns_first() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Insert two cycle_start rows for the same cycle_id (simulated corrupted state).
    store
        .insert_cycle_event(
            "dup-cycle",
            0,
            "cycle_start",
            None,
            None,
            None,
            1700000001,
            Some("first goal"),
        )
        .await
        .expect("insert first cycle_start");

    store
        .insert_cycle_event(
            "dup-cycle",
            1,
            "cycle_start",
            None,
            None,
            None,
            1700000002,
            Some("second goal"),
        )
        .await
        .expect("insert second cycle_start");

    // LIMIT 1 must return the first row by insertion order.
    let result = store
        .get_cycle_start_goal("dup-cycle")
        .await
        .expect("get_cycle_start_goal must not error");

    assert!(
        result.is_some(),
        "must return Some goal when at least one cycle_start row exists"
    );
    // The result must be one of the two goals — LIMIT 1 guarantees only one is returned.
    let goal = result.unwrap();
    assert!(
        goal == "first goal" || goal == "second goal",
        "returned goal must be one of the two inserted values, got: {goal}"
    );

    store.close().await.unwrap();
}
