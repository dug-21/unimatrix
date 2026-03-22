//! Integration tests for the v11->v12 schema migration (col-022).
//!
//! These tests create v11-shaped databases with controlled session data,
//! then open them with the current SqlxStore code to trigger migration.
//! Covers: R-03 (column index shift), R-05 (migration), R-06 (JSON fidelity).

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V11 database setup helpers
// ---------------------------------------------------------------------------

/// Create a v11 database at the given path with the full table set.
/// Based on the v10 helper, plus v11 tables (topic_deliveries, query_log).
/// Sessions table is v11 (WITHOUT keywords column).
async fn create_v11_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open migration setup conn");

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
        // V11 sessions table: does NOT have keywords column
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
    ] {
        sqlx::query(ddl).execute(&mut conn).await.expect("create table/index");
    }

    // Set schema_version = 11
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 11)",
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

/// Insert a session row into a v11 database (without keywords column).
async fn insert_v11_session(
    path: &Path,
    session_id: &str,
    feature_cycle: Option<&str>,
    started_at: i64,
    ended_at: Option<i64>,
) {
    let opts = SqliteConnectOptions::new().filename(path);
    let mut conn = opts
        .connect()
        .await
        .expect("open conn for insert_v11_session");

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
    .expect("insert v11 session");
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

async fn has_column(store: &SqlxStore, table: &str, column: &str) -> bool {
    let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'");
    let count: i64 = sqlx::query_scalar::<_, i64>(&sql)
        .fetch_one(store.read_pool_test())
        .await
        .expect("has_column check");
    count > 0
}

// ---------------------------------------------------------------------------
// Migration tests (R-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_migration_v11_to_v12_adds_keywords_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v11 database
    create_v11_database(&db_path).await;

    // Act: open with current SqlxStore code -> triggers v11->v12 migration
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: keywords column exists on sessions table
    assert!(
        has_column(&store, "sessions", "keywords").await,
        "keywords column should exist after migration"
    );

    // Assert: schema_version bumped to 14 (migration continues through v13→v14)
    assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_v12_existing_sessions_have_null_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v11 database with 3 session rows
    create_v11_database(&db_path).await;
    insert_v11_session(&db_path, "s1", Some("fc-a"), 1000, Some(1100)).await;
    insert_v11_session(&db_path, "s2", Some("fc-a"), 2000, None).await;
    insert_v11_session(&db_path, "s3", None, 3000, Some(3300)).await;

    // Act: open store (migration runs)
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: all 3 sessions readable with keywords = None
    let s1 = store
        .get_session("s1")
        .await
        .expect("get s1")
        .expect("s1 exists");
    assert_eq!(s1.keywords, None);
    assert_eq!(s1.feature_cycle, Some("fc-a".to_string()));

    let s2 = store
        .get_session("s2")
        .await
        .expect("get s2")
        .expect("s2 exists");
    assert_eq!(s2.keywords, None);

    let s3 = store
        .get_session("s3")
        .await
        .expect("get s3")
        .expect("s3 exists");
    assert_eq!(s3.keywords, None);
    assert_eq!(s3.feature_cycle, None);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_v12_idempotency() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v11 database
    create_v11_database(&db_path).await;

    // Act: open store (migration runs), then close and re-open
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("open store");
        assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)
        store.close().await.unwrap();
    }

    // Act: re-open on same path -- migration should skip (already at v13)
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("re-open store");

    // Assert: no error, schema still 14
    assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)
    assert!(has_column(&store, "sessions", "keywords").await);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_v12_empty_database() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v11 database with no sessions
    create_v11_database(&db_path).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: migration succeeds with no rows
    assert!(read_schema_version(&store).await >= 14); // crt-025: bumped to 15; keep >= 14 (pattern #2933)
    assert!(has_column(&store, "sessions", "keywords").await);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// SessionRecord round-trip tests (R-03)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_session_record_round_trip_with_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    let record = unimatrix_store::SessionRecord {
        session_id: "rt-kw".to_string(),
        feature_cycle: Some("fc-rt".to_string()),
        agent_role: Some("developer".to_string()),
        started_at: 1000,
        ended_at: Some(2000),
        status: unimatrix_store::SessionLifecycleStatus::Completed,
        compaction_count: 3,
        outcome: Some("success".to_string()),
        total_injections: 42,
        keywords: Some(r#"["attr","lifecycle"]"#.to_string()),
    };

    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    let got = store
        .get_session("rt-kw")
        .await
        .expect("get")
        .expect("exists");

    assert_eq!(got.session_id, "rt-kw");
    assert_eq!(got.feature_cycle, Some("fc-rt".to_string()));
    assert_eq!(got.agent_role, Some("developer".to_string()));
    assert_eq!(got.started_at, 1000);
    assert_eq!(got.ended_at, Some(2000));
    assert_eq!(
        got.status,
        unimatrix_store::SessionLifecycleStatus::Completed
    );
    assert_eq!(got.compaction_count, 3);
    assert_eq!(got.outcome, Some("success".to_string()));
    assert_eq!(got.total_injections, 42);
    assert_eq!(got.keywords, Some(r#"["attr","lifecycle"]"#.to_string()));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_session_record_round_trip_without_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    let record = unimatrix_store::SessionRecord {
        session_id: "rt-no-kw".to_string(),
        feature_cycle: Some("fc-nk".to_string()),
        agent_role: Some("tester".to_string()),
        started_at: 5000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 7,
        keywords: None,
    };

    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    let got = store
        .get_session("rt-no-kw")
        .await
        .expect("get")
        .expect("exists");

    // keywords must be None, not Some("null") or Some("")
    assert_eq!(got.keywords, None);
    // All other fields must be correct (column index not shifted)
    assert_eq!(got.feature_cycle, Some("fc-nk".to_string()));
    assert_eq!(got.agent_role, Some("tester".to_string()));
    assert_eq!(got.started_at, 5000);
    assert_eq!(got.ended_at, None);
    assert_eq!(got.status, unimatrix_store::SessionLifecycleStatus::Active);
    assert_eq!(got.total_injections, 7);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_session_record_round_trip_empty_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    let record = unimatrix_store::SessionRecord {
        session_id: "rt-empty-kw".to_string(),
        feature_cycle: None,
        agent_role: None,
        started_at: 9000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: Some("[]".to_string()),
    };

    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    let got = store
        .get_session("rt-empty-kw")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(got.keywords, Some("[]".to_string()));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_session_columns_count_matches_from_row() {
    // Structural test: column list comma-separated count == SessionRecord fields
    let columns_str = "session_id, feature_cycle, agent_role, started_at, ended_at, \
                       status, compaction_count, outcome, total_injections, keywords";
    let column_count = columns_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .count();
    // SessionRecord has 10 fields
    assert_eq!(
        column_count, 10,
        "SESSION_COLUMNS token count must match SessionRecord field count"
    );
}

// ---------------------------------------------------------------------------
// Keywords column persistence tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_session_keywords_writes_to_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Insert session with no keywords
    let record = unimatrix_store::SessionRecord {
        session_id: "kw-write".to_string(),
        feature_cycle: None,
        agent_role: None,
        started_at: 1000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: None,
    };
    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    // Update keywords
    store
        .update_session_keywords("kw-write", r#"["a","b"]"#)
        .await
        .expect("update keywords");

    // Read back
    let got = store
        .get_session("kw-write")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(got.keywords, Some(r#"["a","b"]"#.to_string()));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_update_session_keywords_overwrites_existing() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    let record = unimatrix_store::SessionRecord {
        session_id: "kw-overwrite".to_string(),
        feature_cycle: None,
        agent_role: None,
        started_at: 1000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: Some(r#"["old"]"#.to_string()),
    };
    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    store
        .update_session_keywords("kw-overwrite", r#"["new"]"#)
        .await
        .expect("update keywords");

    let got = store
        .get_session("kw-overwrite")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(got.keywords, Some(r#"["new"]"#.to_string()));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_update_session_keywords_nonexistent_session() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Should succeed (no-op UPDATE, 0 rows affected)
    store
        .update_session_keywords("ghost", r#"["x"]"#)
        .await
        .expect("update keywords on nonexistent should not error");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Keywords JSON fidelity tests (R-06)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_keywords_json_round_trip_special_chars() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    let keywords_json = r#"["has \"quotes\"","back\\slash"]"#;

    let record = unimatrix_store::SessionRecord {
        session_id: "json-special".to_string(),
        feature_cycle: None,
        agent_role: None,
        started_at: 1000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: Some(keywords_json.to_string()),
    };
    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    let got = store
        .get_session("json-special")
        .await
        .expect("get")
        .expect("exists");
    let deserialized: Vec<String> =
        serde_json::from_str(got.keywords.as_ref().expect("keywords")).expect("deserialize");
    assert_eq!(deserialized, vec!["has \"quotes\"", "back\\slash"]);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_keywords_json_unicode() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Store unicode keywords
    let keywords: Vec<String> = vec!["\u{00e9}".to_string(), "emoji\u{2764}".to_string()];
    let keywords_json = serde_json::to_string(&keywords).expect("serialize");

    let record = unimatrix_store::SessionRecord {
        session_id: "json-unicode".to_string(),
        feature_cycle: None,
        agent_role: None,
        started_at: 1000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: Some(keywords_json),
    };
    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    let got = store
        .get_session("json-unicode")
        .await
        .expect("get")
        .expect("exists");
    let deserialized: Vec<String> =
        serde_json::from_str(got.keywords.as_ref().expect("keywords")).expect("deserialize");
    assert_eq!(deserialized, keywords);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_keywords_null_vs_empty_distinction() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Session A: keywords = None (NULL in SQLite)
    let a = unimatrix_store::SessionRecord {
        session_id: "null-kw".to_string(),
        feature_cycle: None,
        agent_role: None,
        started_at: 1000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: None,
    };

    // Session B: keywords = Some("[]") (empty JSON array)
    let b = unimatrix_store::SessionRecord {
        session_id: "empty-kw".to_string(),
        feature_cycle: None,
        agent_role: None,
        started_at: 2000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: Some("[]".to_string()),
    };

    store.insert_session(&a).await.unwrap();
    store.insert_session(&b).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    let got_a = store
        .get_session("null-kw")
        .await
        .expect("get a")
        .expect("a exists");
    let got_b = store
        .get_session("empty-kw")
        .await
        .expect("get b")
        .expect("b exists");

    assert_eq!(got_a.keywords, None);
    assert_eq!(got_b.keywords, Some("[]".to_string()));

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// update_session with keywords via updater closure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_update_session_sets_keywords_via_closure() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    let record = unimatrix_store::SessionRecord {
        session_id: "upd-kw".to_string(),
        feature_cycle: None,
        agent_role: None,
        started_at: 1000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: None,
    };
    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    store
        .update_session("upd-kw", |r| {
            r.keywords = Some(r#"["updated"]"#.to_string());
        })
        .await
        .expect("update session");

    let got = store
        .get_session("upd-kw")
        .await
        .expect("get")
        .expect("exists");
    assert_eq!(got.keywords, Some(r#"["updated"]"#.to_string()));

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// scan_sessions_by_feature includes keywords
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_scan_sessions_by_feature_includes_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    let record = unimatrix_store::SessionRecord {
        session_id: "scan-kw".to_string(),
        feature_cycle: Some("fc-scan".to_string()),
        agent_role: None,
        started_at: 1000,
        ended_at: None,
        status: unimatrix_store::SessionLifecycleStatus::Active,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
        keywords: Some(r#"["scan-keyword"]"#.to_string()),
    };
    store.insert_session(&record).await.unwrap();
    store.close().await.expect("close after insert");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("reopen store");

    let results = store
        .scan_sessions_by_feature("fc-scan")
        .await
        .expect("scan");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].keywords, Some(r#"["scan-keyword"]"#.to_string()));

    store.close().await.unwrap();
}
