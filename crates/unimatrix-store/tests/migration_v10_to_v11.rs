//! Integration tests for the v10->v11 schema migration (nxs-010).
//!
//! These tests create v10-shaped databases with controlled session data,
//! then open them with the current SqlxStore code to trigger migration.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V10 database setup helpers
// ---------------------------------------------------------------------------

/// Create a v10 database at the given path with the full table set.
async fn create_v10_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open migration setup conn");

    sqlx::query("PRAGMA journal_mode = WAL;")
        .execute(&mut conn)
        .await
        .expect("journal_mode pragma");

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&mut conn)
        .await
        .expect("foreign_keys pragma");

    // Create all tables that exist at v10 (minimal set needed for migration)
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
        "CREATE TABLE sessions (
            session_id       TEXT    PRIMARY KEY,
            feature_cycle    TEXT,
            agent_role       TEXT,
            started_at       INTEGER NOT NULL,
            ended_at         INTEGER,
            status           INTEGER NOT NULL DEFAULT 0,
            compaction_count INTEGER NOT NULL DEFAULT 0,
            outcome          TEXT,
            total_injections INTEGER NOT NULL DEFAULT 0
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
    ] {
        sqlx::query(ddl).execute(&mut conn).await.expect("create table/index");
    }

    // Set schema_version = 10
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 10)",
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

/// Insert a session row into the v10 database.
async fn insert_session(
    path: &Path,
    session_id: &str,
    feature_cycle: Option<&str>,
    started_at: i64,
    ended_at: Option<i64>,
) {
    let opts = SqliteConnectOptions::new().filename(path);
    let mut conn = opts.connect().await.expect("open conn for insert_session");

    sqlx::query(
        "INSERT INTO sessions (session_id, feature_cycle, started_at, ended_at, status)
         VALUES (?1, ?2, ?3, ?4, 0)",
    )
    .bind(session_id)
    .bind(feature_cycle)
    .bind(started_at)
    .bind(ended_at)
    .execute(&mut conn)
    .await
    .expect("insert session");
}

// ---------------------------------------------------------------------------
// Post-migration read helpers using read_pool_test()
// ---------------------------------------------------------------------------

async fn read_schema_version(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'schema_version'")
        .fetch_one(store.read_pool_test())
        .await
        .expect("read schema_version")
}

async fn count_topic_deliveries(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM topic_deliveries")
        .fetch_one(store.read_pool_test())
        .await
        .expect("count topic_deliveries")
}

async fn count_query_log(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM query_log")
        .fetch_one(store.read_pool_test())
        .await
        .expect("count query_log")
}

async fn column_count(store: &SqlxStore, table: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{table}')");
    sqlx::query_scalar::<_, i64>(&sql)
        .fetch_one(store.read_pool_test())
        .await
        .expect("column count")
}

struct TopicDeliveryRow {
    topic: String,
    created_at: i64,
    status: String,
    total_sessions: i64,
    total_tool_calls: i64,
    total_duration_secs: i64,
}

async fn read_topic_delivery(store: &SqlxStore, topic: &str) -> Option<TopicDeliveryRow> {
    use sqlx::Row as _;
    let row = sqlx::query(
        "SELECT topic, created_at, status, total_sessions, total_tool_calls, total_duration_secs
         FROM topic_deliveries WHERE topic = ?1",
    )
    .bind(topic)
    .fetch_optional(store.read_pool_test())
    .await
    .expect("read topic_delivery");

    row.map(|r| TopicDeliveryRow {
        topic: r.try_get("topic").unwrap(),
        created_at: r.try_get("created_at").unwrap(),
        status: r.try_get("status").unwrap(),
        total_sessions: r.try_get("total_sessions").unwrap(),
        total_tool_calls: r.try_get("total_tool_calls").unwrap(),
        total_duration_secs: r.try_get("total_duration_secs").unwrap(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v10_to_v11_basic() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v10 database with attributed sessions
    create_v10_database(&db_path).await;
    insert_session(&db_path, "s1", Some("topic-a"), 1000, Some(1100)).await;
    insert_session(&db_path, "s2", Some("topic-a"), 2000, Some(2300)).await;
    insert_session(&db_path, "s3", Some("topic-b"), 3000, Some(3050)).await;

    // Act: open with current SqlxStore code -> triggers v10->v11 migration
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: tables created with correct column counts
    assert_eq!(column_count(&store, "topic_deliveries").await, 9);
    assert_eq!(column_count(&store, "query_log").await, 9);

    // Assert: topic-a backfilled correctly
    let ta = read_topic_delivery(&store, "topic-a")
        .await
        .expect("topic-a exists");
    assert_eq!(ta.topic, "topic-a");
    assert_eq!(ta.total_sessions, 2);
    assert_eq!(ta.total_duration_secs, 400); // (1100-1000) + (2300-2000)
    assert_eq!(ta.created_at, 1000); // MIN(started_at)
    assert_eq!(ta.status, "completed");
    assert_eq!(ta.total_tool_calls, 0); // not backfilled

    // Assert: topic-b backfilled correctly
    let tb = read_topic_delivery(&store, "topic-b")
        .await
        .expect("topic-b exists");
    assert_eq!(tb.total_sessions, 1);
    assert_eq!(tb.total_duration_secs, 50);
    assert_eq!(tb.created_at, 3000);
    assert_eq!(tb.status, "completed");

    // Assert: schema_version bumped to 14 (migration runs all the way through v13→v14)
    assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)

    // Assert: query_log table is empty (no backfill for query_log)
    assert_eq!(count_query_log(&store).await, 0);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_v10_to_v11_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v10 database with 1 attributed session
    create_v10_database(&db_path).await;
    insert_session(&db_path, "s1", Some("topic-x"), 5000, Some(5500)).await;

    // Act: open store (migration runs), then close and re-open
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("open store");
        assert_eq!(count_topic_deliveries(&store).await, 1);
        assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)
        store.close().await.unwrap();
    }

    // Act: re-open on same path
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("re-open store");

    // Assert: no error, no duplicates
    assert_eq!(count_topic_deliveries(&store).await, 1);
    assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_v10_to_v11_empty_sessions() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v10 database with sessions table but zero rows
    create_v10_database(&db_path).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert
    assert_eq!(count_topic_deliveries(&store).await, 0);
    assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_v10_to_v11_no_attributed_sessions() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: sessions with NULL or empty feature_cycle
    create_v10_database(&db_path).await;
    insert_session(&db_path, "s1", None, 1000, Some(1100)).await;
    insert_session(&db_path, "s2", Some(""), 2000, Some(2200)).await;
    insert_session(&db_path, "s3", None, 3000, Some(3300)).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: all excluded by WHERE clause
    assert_eq!(count_topic_deliveries(&store).await, 0);
    assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_backfill_null_ended_at_mixed() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: 3 sessions for topic-x, one with NULL ended_at
    create_v10_database(&db_path).await;
    insert_session(&db_path, "s1", Some("topic-x"), 1000, Some(1200)).await; // 200
    insert_session(&db_path, "s2", Some("topic-x"), 2000, Some(2100)).await; // 100
    insert_session(&db_path, "s3", Some("topic-x"), 3000, None).await; // NULL

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: NULL excluded from SUM but session counted
    let tx = read_topic_delivery(&store, "topic-x")
        .await
        .expect("topic-x exists");
    assert_eq!(tx.total_sessions, 3);
    assert_eq!(tx.total_duration_secs, 300); // 200 + 100, NULL excluded
    assert_eq!(tx.created_at, 1000);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_backfill_all_null_ended_at() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: all sessions with NULL ended_at
    create_v10_database(&db_path).await;
    insert_session(&db_path, "s1", Some("topic-y"), 1000, None).await;
    insert_session(&db_path, "s2", Some("topic-y"), 2000, None).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: COALESCE returns 0 when all durations are NULL
    let ty = read_topic_delivery(&store, "topic-y")
        .await
        .expect("topic-y exists");
    assert_eq!(ty.total_sessions, 2);
    assert_eq!(ty.total_duration_secs, 0);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_fresh_database_skips() {
    // Arrange: completely fresh database (no pre-existing tables)
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Act: open fresh -- migration skipped, create_tables handles everything
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: tables exist (created by create_tables, not migration)
    assert_eq!(column_count(&store, "topic_deliveries").await, 9);
    assert_eq!(column_count(&store, "query_log").await, 9);
    assert_eq!(count_topic_deliveries(&store).await, 0);
    assert_eq!(count_query_log(&store).await, 0);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_v10_to_v11_partial_rerun() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v10 database with sessions, plus manually-created tables
    // (simulating partial migration where tables exist but version not bumped)
    create_v10_database(&db_path).await;
    insert_session(&db_path, "s1", Some("topic-p"), 1000, Some(1500)).await;

    // Manually create tables as if partial migration ran
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("open conn for partial tables");

        for ddl in &[
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
            "CREATE INDEX idx_query_log_session ON query_log(session_id)",
            "CREATE INDEX idx_query_log_ts ON query_log(ts)",
        ] {
            sqlx::query(ddl)
                .execute(&mut conn)
                .await
                .expect("create partial table");
        }
        // schema_version is still 10 (not bumped)
    }

    // Act: open store -- migration guard fires because version < 11
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: CREATE TABLE IF NOT EXISTS succeeds (no error on existing tables)
    // INSERT OR IGNORE backfill creates rows
    assert_eq!(count_topic_deliveries(&store).await, 1);
    let tp = read_topic_delivery(&store, "topic-p")
        .await
        .expect("topic-p exists");
    assert_eq!(tp.total_sessions, 1);
    assert_eq!(tp.total_duration_secs, 500);

    // schema_version updated to 14 (migration runs all the way through v13→v14)
    assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)

    store.close().await.unwrap();
}
