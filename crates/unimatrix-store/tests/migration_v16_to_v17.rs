//! Integration tests for the v16→v17 schema migration (col-028).
//!
//! Covers: AC-13 (CURRENT_SCHEMA_VERSION = 17), AC-14 (query_log.phase column added),
//! AC-15 (idempotency), AC-17 (phase round-trip SR-01 guard), AC-18 (pre-existing rows
//! get phase=None), AC-19 (all six T-V17-* tests), pattern #1264 (pragma_table_info guard).
//!
//! Pattern: create a v16-shaped database, open with current SqlxStore to trigger
//! migration, assert schema state and data round-trips.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;
use unimatrix_store::query_log::QueryLogRecord;
use unimatrix_store::test_helpers::open_test_store;

// ---------------------------------------------------------------------------
// V16 database builder
// ---------------------------------------------------------------------------

/// Create a v16-shaped database at the given path.
///
/// Contains all tables present at v16: all v15 tables + `cycle_events.goal` column.
/// The `query_log` table has NO `phase` column — that is what v16→v17 adds.
/// schema_version = 16.
async fn create_v16_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v16 setup conn");

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
        // query_log WITHOUT phase column — this is the v16 shape.
        // v16→v17 adds the phase column.
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

    // Seed counters at v16.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 16)",
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

async fn phase_column_exists(store: &SqlxStore) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check query_log.phase column");
    count > 0
}

// ---------------------------------------------------------------------------
// Unit test: CURRENT_SCHEMA_VERSION constant = 17 (AC-13)
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_17() {
    assert_eq!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION,
        17,
        "CURRENT_SCHEMA_VERSION must be 17"
    );
}

// ---------------------------------------------------------------------------
// T-V17-01: Fresh database creates schema v17 directly (AC-14, AC-13)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v17() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: empty path — no prior DB.
    // Act: SqlxStore::open calls create_tables_if_needed() for fresh DBs.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    // Assert: schema_version == 17
    assert_eq!(
        read_schema_version(&store).await,
        17,
        "fresh database must be at schema v17"
    );

    // Assert: phase column present (fresh schema has full DDL including phase)
    assert!(
        phase_column_exists(&store).await,
        "fresh database must have query_log.phase column"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V17-02: v16→v17 migration adds phase column (AC-14)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v16_to_v17_migration_adds_phase_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v16 database — query_log exists, phase column absent.
    create_v16_database(&db_path).await;

    // Act: open triggers v16→v17 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store after v16→v17 migration");

    // Assert: phase column now exists.
    assert!(
        phase_column_exists(&store).await,
        "query_log.phase column must exist after v16→v17 migration (AC-14)"
    );

    // Assert: schema_version == 17.
    assert_eq!(read_schema_version(&store).await, 17);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V17-03: idx_query_log_phase index present after migration (AC-14)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v16_to_v17_migration_creates_phase_index() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v16_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Check index exists via sqlite_master
    let index_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type='index' AND name='idx_query_log_phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check idx_query_log_phase");

    assert_eq!(
        index_exists, 1,
        "idx_query_log_phase must be created by v16→v17 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V17-04: Idempotency — running migration twice succeeds (AC-15)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v16_to_v17_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v16_database(&db_path).await;

    // Run 1: applies v16→v17 migration.
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("first open");
        assert!(phase_column_exists(&store).await);
        assert_eq!(read_schema_version(&store).await, 17);
        store.close().await.unwrap();
    }

    // Run 2: must be a no-op — no errors, no duplicate column.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open must succeed (idempotency)");

    assert_eq!(read_schema_version(&store).await, 17);

    // Exactly one phase column (pragma_table_info guard prevents duplicate ALTER)
    let col_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count phase columns");
    assert_eq!(
        col_count, 1,
        "exactly one phase column after idempotent run"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V17-05: Pre-existing rows have phase=None after migration (AC-18)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v16_pre_existing_query_log_rows_have_null_phase() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v16 database with a pre-seeded query_log row (v16 columns only).
    create_v16_database(&db_path).await;
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        // Insert with 8 columns (no phase — this is the v16 schema)
        sqlx::query(
            "INSERT INTO query_log \
             (session_id, query_text, ts, result_count, \
              result_entry_ids, similarity_scores, retrieval_mode, source) \
             VALUES ('pre-migration-session', 'test query', 1700000000, 0, \
                     NULL, NULL, 'semantic', 'mcp')",
        )
        .execute(&mut conn)
        .await
        .expect("insert pre-existing row");
    }

    // Act: open triggers v16→v17 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: read the row back using the updated scan function.
    let rows = store
        .scan_query_log_by_session("pre-migration-session")
        .await
        .expect("scan_query_log_by_session must not error");

    assert_eq!(rows.len(), 1, "exactly one pre-existing row");
    assert!(
        rows[0].phase.is_none(),
        "pre-existing query_log row must have phase = None after migration (no backfill, AC-18)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// T-V17-06: schema_version counter = 17 after migration (AC-13, AC-19)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_schema_version_is_17_after_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v16_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open migrated store");

    // Assert: counters table carries schema_version = 17.
    assert_eq!(read_schema_version(&store).await, 17);

    // Assert: Rust const agrees.
    assert_eq!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION, 17);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// AC-17: query_log.phase round-trip — SR-01 atomic-unit guard
// ---------------------------------------------------------------------------
//
// This is the primary runtime guard against positional column index drift
// across the four atomic sites: analytics.rs INSERT (?9), both scan_query_log_*
// SELECTs (tenth column), and row_to_query_log (index 9).
//
// If any of the four sites diverges:
// - INSERT missing phase bind: rows[0].phase = None even though Some("design") written.
// - SELECT missing phase column: row_to_query_log panics or returns wrong type at index 9.
// - row_to_query_log reading index 8 (source): phase reads back as "mcp" instead of "design".
//
// Pattern: flush (close+reopen) drains the analytics channel so rows are committed
// before the scan reads. This matches the sqlite_parity.rs `flush` pattern (#3004).

#[tokio::test]
async fn test_query_log_phase_round_trip_some() {
    // Arrange: fresh v17 store
    let dir = TempDir::new().expect("temp dir");
    let store = open_test_store(&dir).await;

    // Act: enqueue a query_log row via insert_query_log with phase = Some("design")
    let record = QueryLogRecord::new(
        "session-rt-some".to_string(),
        "round trip phase some".to_string(),
        &[1_u64, 2, 3],
        &[0.9_f64, 0.8, 0.7],
        "semantic",
        "mcp",
        Some("design".to_string()), // col-028: phase — NEW parameter
    );
    store.insert_query_log(&record);

    // Flush: close + reopen drains the analytics channel (sqlite_parity flush pattern).
    store.close().await.expect("close");
    let store = open_test_store(&dir).await;

    // Assert: read back via scan_query_log_by_session
    let rows = store
        .scan_query_log_by_session("session-rt-some")
        .await
        .expect("scan_query_log_by_session must not error (AC-17)");

    assert_eq!(rows.len(), 1, "exactly one row in AC-17 round-trip test");
    assert_eq!(
        rows[0].phase,
        Some("design".to_string()),
        "AC-17 SR-01 guard: phase must round-trip — written as Some('design'), read back as \
         Some('design'). Mismatch indicates positional drift in INSERT, SELECT, or \
         row_to_query_log (col-028, ADR-007)."
    );

    store.close().await.expect("close");
}

#[tokio::test]
async fn test_query_log_phase_round_trip_none() {
    // phase=None must deserialize as None (not empty string, not panic) — AC-17 NULL path.
    let dir = TempDir::new().expect("temp dir");
    let store = open_test_store(&dir).await;

    let record = QueryLogRecord::new(
        "session-rt-none".to_string(),
        "round trip phase none".to_string(),
        &[4_u64, 5],
        &[0.85_f64, 0.75],
        "strict",
        "mcp",
        None, // col-028: phase = NULL
    );
    store.insert_query_log(&record);

    store.close().await.expect("close");
    let store = open_test_store(&dir).await;

    let rows = store
        .scan_query_log_by_session("session-rt-none")
        .await
        .expect("scan_query_log_by_session with phase=None must not error");

    assert_eq!(rows.len(), 1, "exactly one row in AC-17 None round-trip");
    assert!(
        rows[0].phase.is_none(),
        "AC-17 SR-01 guard: phase=None must round-trip as None (NULL → Option<String>::None). \
         Got: {:?}",
        rows[0].phase
    );

    store.close().await.expect("close");
}

#[tokio::test]
async fn test_query_log_phase_round_trip_non_trivial_value() {
    // EC-06: phase containing a slash must round-trip correctly.
    // Verifies that SQLx parameterized binding handles non-trivial strings.
    let dir = TempDir::new().expect("temp dir");
    let store = open_test_store(&dir).await;

    let record = QueryLogRecord::new(
        "session-rt-slash".to_string(),
        "round trip non-trivial phase".to_string(),
        &[],
        &[],
        "flexible",
        "mcp",
        Some("design/v2".to_string()), // EC-06: contains slash
    );
    store.insert_query_log(&record);

    store.close().await.expect("close");
    let store = open_test_store(&dir).await;

    let rows = store
        .scan_query_log_by_session("session-rt-slash")
        .await
        .expect("scan must not error");

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].phase,
        Some("design/v2".to_string()),
        "EC-06: phase 'design/v2' must round-trip exactly (parameterized binding, no injection)"
    );

    store.close().await.expect("close");
}
