//! Integration tests for the v10->v11 schema migration (nxs-010).
//!
//! These tests create v10-shaped databases with controlled session data,
//! then open them with the current Store code to trigger migration.

use rusqlite::Connection;
use tempfile::TempDir;
use unimatrix_store::Store;

/// Create a v10 database at the given path with the full table set.
/// Returns nothing; the database is ready for session seeding.
fn create_v10_database(path: &std::path::Path) {
    let conn = Connection::open(path).expect("open db");

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;",
    )
    .expect("pragmas");

    // Create all tables that exist at v10 (minimal set needed for migration)
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
        CREATE INDEX idx_shadow_eval_ts ON shadow_evaluations(timestamp);",
    )
    .expect("create v10 tables");

    // Set schema_version = 10
    conn.execute_batch(
        "INSERT INTO counters (name, value) VALUES ('schema_version', 10);
         INSERT INTO counters (name, value) VALUES ('next_entry_id', 1);
         INSERT INTO counters (name, value) VALUES ('next_signal_id', 0);
         INSERT INTO counters (name, value) VALUES ('next_log_id', 0);
         INSERT INTO counters (name, value) VALUES ('next_audit_event_id', 0);",
    )
    .expect("seed counters");
}

/// Insert a session row into a v10 database.
fn insert_session(
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

/// Count rows in topic_deliveries.
fn count_topic_deliveries(store: &Store) -> i64 {
    let conn = store.lock_conn();
    conn.query_row("SELECT COUNT(*) FROM topic_deliveries", [], |row| {
        row.get(0)
    })
    .expect("count topic_deliveries")
}

/// Count rows in query_log.
fn count_query_log(store: &Store) -> i64 {
    let conn = store.lock_conn();
    conn.query_row("SELECT COUNT(*) FROM query_log", [], |row| row.get(0))
        .expect("count query_log")
}

/// Count columns in a table via pragma_table_info.
fn column_count(store: &Store, table: &str) -> i64 {
    let conn = store.lock_conn();
    conn.query_row(
        &format!("SELECT COUNT(*) FROM pragma_table_info('{table}')"),
        [],
        |row| row.get(0),
    )
    .expect("column count")
}

/// Read a topic_delivery row by topic name.
struct TopicDeliveryRow {
    topic: String,
    created_at: i64,
    status: String,
    total_sessions: i64,
    total_tool_calls: i64,
    total_duration_secs: i64,
}

fn read_topic_delivery(store: &Store, topic: &str) -> Option<TopicDeliveryRow> {
    let conn = store.lock_conn();
    conn.query_row(
        "SELECT topic, created_at, status, total_sessions, total_tool_calls, total_duration_secs
         FROM topic_deliveries WHERE topic = ?1",
        rusqlite::params![topic],
        |row| {
            Ok(TopicDeliveryRow {
                topic: row.get(0)?,
                created_at: row.get(1)?,
                status: row.get(2)?,
                total_sessions: row.get(3)?,
                total_tool_calls: row.get(4)?,
                total_duration_secs: row.get(5)?,
            })
        },
    )
    .ok()
}

#[test]
fn test_migration_v10_to_v11_basic() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v10 database with attributed sessions
    create_v10_database(&db_path);
    {
        let conn = Connection::open(&db_path).expect("open");
        insert_session(&conn, "s1", Some("topic-a"), 1000, Some(1100));
        insert_session(&conn, "s2", Some("topic-a"), 2000, Some(2300));
        insert_session(&conn, "s3", Some("topic-b"), 3000, Some(3050));
    }

    // Act: open with current Store code -> triggers v10->v11 migration
    let store = Store::open(&db_path).expect("open store");

    // Assert: tables created with correct column counts
    assert_eq!(column_count(&store, "topic_deliveries"), 9);
    assert_eq!(column_count(&store, "query_log"), 9);

    // Assert: topic-a backfilled correctly
    let ta = read_topic_delivery(&store, "topic-a").expect("topic-a exists");
    assert_eq!(ta.topic, "topic-a");
    assert_eq!(ta.total_sessions, 2);
    assert_eq!(ta.total_duration_secs, 400); // (1100-1000) + (2300-2000)
    assert_eq!(ta.created_at, 1000); // MIN(started_at)
    assert_eq!(ta.status, "completed");
    assert_eq!(ta.total_tool_calls, 0); // not backfilled

    // Assert: topic-b backfilled correctly
    let tb = read_topic_delivery(&store, "topic-b").expect("topic-b exists");
    assert_eq!(tb.total_sessions, 1);
    assert_eq!(tb.total_duration_secs, 50);
    assert_eq!(tb.created_at, 3000);
    assert_eq!(tb.status, "completed");

    // Assert: schema_version bumped to 11
    assert_eq!(read_schema_version(&store), 11);

    // Assert: query_log table is empty (no backfill for query_log)
    assert_eq!(count_query_log(&store), 0);
}

#[test]
fn test_migration_v10_to_v11_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v10 database with 1 attributed session
    create_v10_database(&db_path);
    {
        let conn = Connection::open(&db_path).expect("open");
        insert_session(&conn, "s1", Some("topic-x"), 5000, Some(5500));
    }

    // Act: open store (migration runs), then close and re-open
    {
        let store = Store::open(&db_path).expect("open store");
        assert_eq!(count_topic_deliveries(&store), 1);
        assert_eq!(read_schema_version(&store), 11);
    }

    // Act: re-open on same path
    let store = Store::open(&db_path).expect("re-open store");

    // Assert: no error, no duplicates
    assert_eq!(count_topic_deliveries(&store), 1);
    assert_eq!(read_schema_version(&store), 11);
}

#[test]
fn test_migration_v10_to_v11_empty_sessions() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: create v10 database with sessions table but zero rows
    create_v10_database(&db_path);

    // Act
    let store = Store::open(&db_path).expect("open store");

    // Assert
    assert_eq!(count_topic_deliveries(&store), 0);
    assert_eq!(read_schema_version(&store), 11);
}

#[test]
fn test_migration_v10_to_v11_no_attributed_sessions() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: sessions with NULL or empty feature_cycle
    create_v10_database(&db_path);
    {
        let conn = Connection::open(&db_path).expect("open");
        insert_session(&conn, "s1", None, 1000, Some(1100));
        insert_session(&conn, "s2", Some(""), 2000, Some(2200));
        insert_session(&conn, "s3", None, 3000, Some(3300));
    }

    // Act
    let store = Store::open(&db_path).expect("open store");

    // Assert: all excluded by WHERE clause
    assert_eq!(count_topic_deliveries(&store), 0);
    assert_eq!(read_schema_version(&store), 11);
}

#[test]
fn test_migration_backfill_null_ended_at_mixed() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: 3 sessions for topic-x, one with NULL ended_at
    create_v10_database(&db_path);
    {
        let conn = Connection::open(&db_path).expect("open");
        insert_session(&conn, "s1", Some("topic-x"), 1000, Some(1200)); // 200
        insert_session(&conn, "s2", Some("topic-x"), 2000, Some(2100)); // 100
        insert_session(&conn, "s3", Some("topic-x"), 3000, None); // NULL
    }

    // Act
    let store = Store::open(&db_path).expect("open store");

    // Assert: NULL excluded from SUM but session counted
    let tx = read_topic_delivery(&store, "topic-x").expect("topic-x exists");
    assert_eq!(tx.total_sessions, 3);
    assert_eq!(tx.total_duration_secs, 300); // 200 + 100, NULL excluded
    assert_eq!(tx.created_at, 1000);
}

#[test]
fn test_migration_backfill_all_null_ended_at() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: all sessions with NULL ended_at
    create_v10_database(&db_path);
    {
        let conn = Connection::open(&db_path).expect("open");
        insert_session(&conn, "s1", Some("topic-y"), 1000, None);
        insert_session(&conn, "s2", Some("topic-y"), 2000, None);
    }

    // Act
    let store = Store::open(&db_path).expect("open store");

    // Assert: COALESCE returns 0 when all durations are NULL
    let ty = read_topic_delivery(&store, "topic-y").expect("topic-y exists");
    assert_eq!(ty.total_sessions, 2);
    assert_eq!(ty.total_duration_secs, 0);
}

#[test]
fn test_migration_fresh_database_skips() {
    // Arrange: completely fresh database (no pre-existing tables)
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Act: open fresh -- migration skipped, create_tables handles everything
    let store = Store::open(&db_path).expect("open store");

    // Assert: tables exist (created by create_tables, not migration)
    assert_eq!(column_count(&store, "topic_deliveries"), 9);
    assert_eq!(column_count(&store, "query_log"), 9);
    assert_eq!(count_topic_deliveries(&store), 0);
    assert_eq!(count_query_log(&store), 0);
}

#[test]
fn test_migration_v10_to_v11_partial_rerun() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v10 database with sessions, plus manually-created tables
    // (simulating partial migration where tables exist but version not bumped)
    create_v10_database(&db_path);
    {
        let conn = Connection::open(&db_path).expect("open");

        // Seed sessions
        insert_session(&conn, "s1", Some("topic-p"), 1000, Some(1500));

        // Manually create the tables as if partial migration ran
        conn.execute_batch(
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
            CREATE INDEX idx_query_log_session ON query_log(session_id);
            CREATE INDEX idx_query_log_ts ON query_log(ts);",
        )
        .expect("create partial tables");

        // schema_version is still 10 (not bumped)
    }

    // Act: open store -- migration guard fires because version < 11
    let store = Store::open(&db_path).expect("open store");

    // Assert: CREATE TABLE IF NOT EXISTS succeeds (no error on existing tables)
    // INSERT OR IGNORE backfill creates rows
    assert_eq!(count_topic_deliveries(&store), 1);
    let tp = read_topic_delivery(&store, "topic-p").expect("topic-p exists");
    assert_eq!(tp.total_sessions, 1);
    assert_eq!(tp.total_duration_secs, 500);

    // schema_version updated to 11
    assert_eq!(read_schema_version(&store), 11);
}
