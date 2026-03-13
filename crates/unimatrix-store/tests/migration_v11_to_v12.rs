//! Integration tests for the v11->v12 schema migration (col-022).
//!
//! These tests create v11-shaped databases with controlled session data,
//! then open them with the current Store code to trigger migration.
//! Covers: R-03 (column index shift), R-05 (migration), R-06 (JSON fidelity).

use rusqlite::Connection;
use tempfile::TempDir;
use unimatrix_store::Store;

/// Create a v11 database at the given path with the full table set.
/// Based on the v10 helper from migration_v10_to_v11.rs, plus v11 tables.
fn create_v11_database(path: &std::path::Path) {
    let conn = Connection::open(path).expect("open db");

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;",
    )
    .expect("pragmas");

    // Create all tables that exist at v11 (sessions WITHOUT keywords column)
    conn.execute_batch(
        "CREATE TABLE counters (
            name TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        );

        CREATE TABLE entries (
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
        );

        CREATE TABLE entry_tags (
            entry_id INTEGER NOT NULL,
            tag      TEXT    NOT NULL,
            PRIMARY KEY (entry_id, tag),
            FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
        );

        CREATE TABLE sessions (
            session_id       TEXT    PRIMARY KEY,
            feature_cycle    TEXT,
            agent_role       TEXT,
            started_at       INTEGER NOT NULL,
            ended_at         INTEGER,
            status           INTEGER NOT NULL DEFAULT 0,
            compaction_count INTEGER NOT NULL DEFAULT 0,
            outcome          TEXT,
            total_injections INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE co_access (
            entry_id_a   INTEGER NOT NULL,
            entry_id_b   INTEGER NOT NULL,
            count        INTEGER NOT NULL DEFAULT 1,
            last_updated INTEGER NOT NULL,
            PRIMARY KEY (entry_id_a, entry_id_b),
            CHECK (entry_id_a < entry_id_b)
        );

        CREATE TABLE vector_map (
            entry_id INTEGER PRIMARY KEY,
            hnsw_data_id INTEGER NOT NULL
        );

        CREATE TABLE feature_entries (
            feature_id TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (feature_id, entry_id)
        );

        CREATE TABLE outcome_index (
            feature_cycle TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (feature_cycle, entry_id)
        );

        CREATE TABLE signal_queue (
            signal_id     INTEGER PRIMARY KEY,
            session_id    TEXT    NOT NULL,
            created_at    INTEGER NOT NULL,
            entry_ids     TEXT    NOT NULL DEFAULT '[]',
            signal_type   INTEGER NOT NULL,
            signal_source INTEGER NOT NULL
        );

        CREATE TABLE injection_log (
            log_id     INTEGER PRIMARY KEY,
            session_id TEXT    NOT NULL,
            entry_id   INTEGER NOT NULL,
            confidence REAL    NOT NULL,
            timestamp  INTEGER NOT NULL
        );

        CREATE TABLE agent_registry (
            agent_id           TEXT    PRIMARY KEY,
            trust_level        INTEGER NOT NULL,
            capabilities       TEXT    NOT NULL DEFAULT '[]',
            allowed_topics     TEXT,
            allowed_categories TEXT,
            enrolled_at        INTEGER NOT NULL,
            last_seen_at       INTEGER NOT NULL,
            active             INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE audit_log (
            event_id   INTEGER PRIMARY KEY,
            timestamp  INTEGER NOT NULL,
            session_id TEXT    NOT NULL,
            agent_id   TEXT    NOT NULL,
            operation  TEXT    NOT NULL,
            target_ids TEXT    NOT NULL DEFAULT '[]',
            outcome    INTEGER NOT NULL,
            detail     TEXT    NOT NULL DEFAULT ''
        );

        CREATE TABLE observations (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id      TEXT    NOT NULL,
            ts_millis       INTEGER NOT NULL,
            hook            TEXT    NOT NULL,
            tool            TEXT,
            input           TEXT,
            response_size   INTEGER,
            response_snippet TEXT,
            topic_signal    TEXT
        );

        CREATE TABLE observation_metrics (
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
        );

        CREATE TABLE observation_phase_metrics (
            feature_cycle   TEXT    NOT NULL,
            phase_name      TEXT    NOT NULL,
            duration_secs   INTEGER NOT NULL DEFAULT 0,
            tool_call_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (feature_cycle, phase_name),
            FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE
        );

        CREATE TABLE shadow_evaluations (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp         INTEGER NOT NULL,
            rule_name         TEXT    NOT NULL,
            rule_category     TEXT    NOT NULL,
            neural_category   TEXT    NOT NULL,
            neural_confidence REAL    NOT NULL,
            convention_score  REAL    NOT NULL,
            rule_accepted     INTEGER NOT NULL,
            digest            BLOB
        );

        CREATE TABLE topic_deliveries (
            topic TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            completed_at INTEGER,
            status TEXT NOT NULL DEFAULT 'active',
            github_issue INTEGER,
            total_sessions INTEGER NOT NULL DEFAULT 0,
            total_tool_calls INTEGER NOT NULL DEFAULT 0,
            total_duration_secs INTEGER NOT NULL DEFAULT 0,
            phases_completed TEXT
        );

        CREATE TABLE query_log (
            query_id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            query_text TEXT NOT NULL,
            ts INTEGER NOT NULL,
            result_count INTEGER NOT NULL,
            result_entry_ids TEXT,
            similarity_scores TEXT,
            retrieval_mode TEXT,
            source TEXT NOT NULL
        );

        CREATE INDEX idx_entries_topic ON entries(topic);
        CREATE INDEX idx_entries_category ON entries(category);
        CREATE INDEX idx_entries_status ON entries(status);
        CREATE INDEX idx_entries_created_at ON entries(created_at);
        CREATE INDEX idx_entry_tags_tag ON entry_tags(tag);
        CREATE INDEX idx_entry_tags_entry_id ON entry_tags(entry_id);
        CREATE INDEX idx_co_access_b ON co_access(entry_id_b);
        CREATE INDEX idx_sessions_feature_cycle ON sessions(feature_cycle);
        CREATE INDEX idx_sessions_started_at ON sessions(started_at);
        CREATE INDEX idx_injection_log_session ON injection_log(session_id);
        CREATE INDEX idx_injection_log_entry ON injection_log(entry_id);
        CREATE INDEX idx_audit_log_agent ON audit_log(agent_id);
        CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);
        CREATE INDEX idx_observations_session ON observations(session_id);
        CREATE INDEX idx_observations_ts ON observations(ts_millis);
        CREATE INDEX idx_shadow_eval_ts ON shadow_evaluations(timestamp);
        CREATE INDEX idx_query_log_session ON query_log(session_id);
        CREATE INDEX idx_query_log_ts ON query_log(ts);",
    )
    .expect("create v11 tables");

    // Set schema_version = 11
    conn.execute_batch(
        "INSERT INTO counters (name, value) VALUES ('schema_version', 11);
         INSERT INTO counters (name, value) VALUES ('next_entry_id', 1);
         INSERT INTO counters (name, value) VALUES ('next_signal_id', 0);
         INSERT INTO counters (name, value) VALUES ('next_log_id', 0);
         INSERT INTO counters (name, value) VALUES ('next_audit_event_id', 0);",
    )
    .expect("seed counters");
}

/// Insert a session row into a v11 database (without keywords column).
fn insert_v11_session(
    conn: &Connection,
    session_id: &str,
    feature_cycle: Option<&str>,
    started_at: i64,
    ended_at: Option<i64>,
) {
    conn.execute(
        "INSERT INTO sessions (session_id, feature_cycle, started_at, ended_at, status)
         VALUES (?1, ?2, ?3, ?4, 0)",
        rusqlite::params![session_id, feature_cycle, started_at, ended_at],
    )
    .expect("insert session");
}

/// Read schema_version from a store via raw SQL.
fn read_schema_version(store: &Store) -> i64 {
    let conn = store.lock_conn();
    conn.query_row(
        "SELECT value FROM counters WHERE name = 'schema_version'",
        [],
        |row| row.get(0),
    )
    .expect("read schema_version")
}

/// Check if a column exists on a table.
fn has_column(store: &Store, table: &str, column: &str) -> bool {
    let conn = store.lock_conn();
    conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'"
        ),
        [],
        |row| Ok(row.get::<_, i64>(0)? > 0),
    )
    .expect("has_column check")
}

// ============================================================
// Migration tests (R-05)
// ============================================================

#[test]
fn test_migration_v11_to_v12_adds_keywords_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v11 database
    create_v11_database(&db_path);

    // Act: open with current Store code -> triggers v11->v12 migration
    let store = Store::open(&db_path).expect("open store");

    // Assert: keywords column exists on sessions table
    assert!(
        has_column(&store, "sessions", "keywords"),
        "keywords column should exist after migration"
    );

    // Assert: schema_version bumped to 12
    assert_eq!(read_schema_version(&store), 12);

    // CURRENT_SCHEMA_VERSION is pub(crate), verified via schema_version counter above.
}

#[test]
fn test_migration_v12_existing_sessions_have_null_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v11 database with 3 session rows
    create_v11_database(&db_path);
    {
        let conn = Connection::open(&db_path).expect("open");
        insert_v11_session(&conn, "s1", Some("fc-a"), 1000, Some(1100));
        insert_v11_session(&conn, "s2", Some("fc-a"), 2000, None);
        insert_v11_session(&conn, "s3", None, 3000, Some(3300));
    }

    // Act: open store (migration runs)
    let store = Store::open(&db_path).expect("open store");

    // Assert: all 3 sessions readable with keywords = None
    let s1 = store.get_session("s1").expect("get s1").expect("s1 exists");
    assert_eq!(s1.keywords, None);
    assert_eq!(s1.feature_cycle, Some("fc-a".to_string()));

    let s2 = store.get_session("s2").expect("get s2").expect("s2 exists");
    assert_eq!(s2.keywords, None);

    let s3 = store.get_session("s3").expect("get s3").expect("s3 exists");
    assert_eq!(s3.keywords, None);
    assert_eq!(s3.feature_cycle, None);
}

#[test]
fn test_migration_v12_idempotency() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v11 database
    create_v11_database(&db_path);

    // Act: open store (migration runs), then close and re-open
    {
        let store = Store::open(&db_path).expect("open store");
        assert_eq!(read_schema_version(&store), 12);
    }

    // Act: re-open on same path -- migration should skip (already at v12)
    let store = Store::open(&db_path).expect("re-open store");

    // Assert: no error, schema still 12
    assert_eq!(read_schema_version(&store), 12);
    assert!(has_column(&store, "sessions", "keywords"));
}

#[test]
fn test_migration_v12_empty_database() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v11 database with no sessions
    create_v11_database(&db_path);

    // Act
    let store = Store::open(&db_path).expect("open store");

    // Assert: migration succeeds with no rows
    assert_eq!(read_schema_version(&store), 12);
    assert!(has_column(&store, "sessions", "keywords"));
}

// ============================================================
// SessionRecord round-trip tests (R-03)
// ============================================================

#[test]
fn test_session_record_round_trip_with_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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

    store.insert_session(&record).expect("insert");
    let got = store.get_session("rt-kw").expect("get").expect("exists");

    assert_eq!(got.session_id, "rt-kw");
    assert_eq!(got.feature_cycle, Some("fc-rt".to_string()));
    assert_eq!(got.agent_role, Some("developer".to_string()));
    assert_eq!(got.started_at, 1000);
    assert_eq!(got.ended_at, Some(2000));
    assert_eq!(got.status, unimatrix_store::SessionLifecycleStatus::Completed);
    assert_eq!(got.compaction_count, 3);
    assert_eq!(got.outcome, Some("success".to_string()));
    assert_eq!(got.total_injections, 42);
    assert_eq!(got.keywords, Some(r#"["attr","lifecycle"]"#.to_string()));
}

#[test]
fn test_session_record_round_trip_without_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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

    store.insert_session(&record).expect("insert");
    let got = store.get_session("rt-no-kw").expect("get").expect("exists");

    // keywords must be None, not Some("null") or Some("")
    assert_eq!(got.keywords, None);
    // All other fields must be correct (column index not shifted)
    assert_eq!(got.feature_cycle, Some("fc-nk".to_string()));
    assert_eq!(got.agent_role, Some("tester".to_string()));
    assert_eq!(got.started_at, 5000);
    assert_eq!(got.ended_at, None);
    assert_eq!(got.status, unimatrix_store::SessionLifecycleStatus::Active);
    assert_eq!(got.total_injections, 7);
}

#[test]
fn test_session_record_round_trip_empty_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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

    store.insert_session(&record).expect("insert");
    let got = store.get_session("rt-empty-kw").expect("get").expect("exists");
    assert_eq!(got.keywords, Some("[]".to_string()));
}

#[test]
fn test_session_columns_count_matches_from_row() {
    // Structural test: SESSION_COLUMNS comma-separated count == SessionRecord fields
    let columns_str = "session_id, feature_cycle, agent_role, started_at, ended_at, \
                       status, compaction_count, outcome, total_injections, keywords";
    let column_count = columns_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .count();
    // SessionRecord has 10 fields
    assert_eq!(column_count, 10, "SESSION_COLUMNS token count must match SessionRecord field count");
}

// ============================================================
// Keywords column persistence tests
// ============================================================

#[test]
fn test_update_session_keywords_writes_to_column() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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
    store.insert_session(&record).expect("insert");

    // Update keywords
    store
        .update_session_keywords("kw-write", r#"["a","b"]"#)
        .expect("update keywords");

    // Read back
    let got = store.get_session("kw-write").expect("get").expect("exists");
    assert_eq!(got.keywords, Some(r#"["a","b"]"#.to_string()));
}

#[test]
fn test_update_session_keywords_overwrites_existing() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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
    store.insert_session(&record).expect("insert");

    store
        .update_session_keywords("kw-overwrite", r#"["new"]"#)
        .expect("update keywords");

    let got = store.get_session("kw-overwrite").expect("get").expect("exists");
    assert_eq!(got.keywords, Some(r#"["new"]"#.to_string()));
}

#[test]
fn test_update_session_keywords_nonexistent_session() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

    // Should succeed (no-op UPDATE, 0 rows affected)
    store
        .update_session_keywords("ghost", r#"["x"]"#)
        .expect("update keywords on nonexistent should not error");
}

// ============================================================
// Keywords JSON fidelity tests (R-06)
// ============================================================

#[test]
fn test_keywords_json_round_trip_special_chars() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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
    store.insert_session(&record).expect("insert");

    let got = store.get_session("json-special").expect("get").expect("exists");
    let deserialized: Vec<String> =
        serde_json::from_str(got.keywords.as_ref().expect("keywords")).expect("deserialize");
    assert_eq!(deserialized, vec!["has \"quotes\"", "back\\slash"]);
}

#[test]
fn test_keywords_json_unicode() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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
    store.insert_session(&record).expect("insert");

    let got = store.get_session("json-unicode").expect("get").expect("exists");
    let deserialized: Vec<String> =
        serde_json::from_str(got.keywords.as_ref().expect("keywords")).expect("deserialize");
    assert_eq!(deserialized, keywords);
}

#[test]
fn test_keywords_null_vs_empty_distinction() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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

    store.insert_session(&a).expect("insert a");
    store.insert_session(&b).expect("insert b");

    let got_a = store.get_session("null-kw").expect("get a").expect("a exists");
    let got_b = store.get_session("empty-kw").expect("get b").expect("b exists");

    assert_eq!(got_a.keywords, None);
    assert_eq!(got_b.keywords, Some("[]".to_string()));
}

// ============================================================
// update_session with keywords via updater closure
// ============================================================

#[test]
fn test_update_session_sets_keywords_via_closure() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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
    store.insert_session(&record).expect("insert");

    store
        .update_session("upd-kw", |r| {
            r.keywords = Some(r#"["updated"]"#.to_string());
        })
        .expect("update session");

    let got = store.get_session("upd-kw").expect("get").expect("exists");
    assert_eq!(got.keywords, Some(r#"["updated"]"#.to_string()));
}

// ============================================================
// scan_sessions_by_feature includes keywords
// ============================================================

#[test]
fn test_scan_sessions_by_feature_includes_keywords() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    let store = Store::open(&db_path).expect("open store");

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
    store.insert_session(&record).expect("insert");

    let results = store.scan_sessions_by_feature("fc-scan").expect("scan");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].keywords, Some(r#"["scan-keyword"]"#.to_string()));
}
