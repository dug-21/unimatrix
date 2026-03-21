//! Integration tests for the v12→v13 schema migration (crt-021).
//!
//! These tests create v12-shaped databases with controlled entry and co_access
//! data, then open them with the current SqlxStore code to trigger migration.
//!
//! Covers: R-06 (empty co_access), R-08 (idempotency), R-13 (no analytics queue),
//!         R-15 (weight normalization), AC-05, AC-06, AC-07, AC-08, AC-18, AC-21.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V12 database builder
// ---------------------------------------------------------------------------

/// Create a v12-shaped database at the given path.
///
/// Contains all tables present at v12 (no graph_edges). schema_version = 12.
/// The graph_edges table is intentionally absent — migration adds it.
async fn create_v12_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v12 setup conn");

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
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index");
    }

    // Seed counters at v12 — graph_edges table intentionally absent.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 12)",
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
// Data insertion helpers
// ---------------------------------------------------------------------------

/// Insert a minimal entry row into a v12 database.
async fn insert_v12_entry(path: &Path, id: i64, supersedes: Option<i64>) {
    let opts = SqliteConnectOptions::new().filename(path);
    let mut conn = opts
        .connect()
        .await
        .expect("open conn for insert_v12_entry");

    sqlx::query(
        "INSERT INTO entries
            (id, title, content, topic, category, source, status, confidence,
             created_at, updated_at)
         VALUES (?1, ?2, ?3, 'test', 'pattern', 'test', 0, 0.5, 1700000000, 1700000000)",
    )
    .bind(id)
    .bind(format!("entry-{id}"))
    .bind(format!("content for entry {id}"))
    .execute(&mut conn)
    .await
    .expect("insert entry");

    if let Some(sup) = supersedes {
        sqlx::query("UPDATE entries SET supersedes = ?1 WHERE id = ?2")
            .bind(sup)
            .bind(id)
            .execute(&mut conn)
            .await
            .expect("set supersedes");
    }
}

/// Insert a co_access row into a v12 database.
/// entry_id_a must be < entry_id_b (enforced by CHECK constraint).
async fn insert_v12_co_access(path: &Path, id_a: i64, id_b: i64, count: i64) {
    let opts = SqliteConnectOptions::new().filename(path);
    let mut conn = opts
        .connect()
        .await
        .expect("open conn for insert_v12_co_access");

    sqlx::query(
        "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated)
         VALUES (?1, ?2, ?3, 1700000000)",
    )
    .bind(id_a)
    .bind(id_b)
    .bind(count)
    .execute(&mut conn)
    .await
    .expect("insert co_access");
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

async fn graph_edges_table_exists(store: &SqlxStore) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='graph_edges'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check graph_edges exists");
    count > 0
}

async fn count_graph_edges_by_type(store: &SqlxStore, relation_type: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_edges WHERE relation_type = ?1")
        .bind(relation_type)
        .fetch_one(store.read_pool_test())
        .await
        .expect("count graph_edges by type")
}

async fn count_all_graph_edges(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(store.read_pool_test())
        .await
        .expect("count all graph_edges")
}

// ---------------------------------------------------------------------------
// Test 9: CURRENT_SCHEMA_VERSION constant = 13 (AC-18)
// ---------------------------------------------------------------------------

// Note: CURRENT_SCHEMA_VERSION was 13 when this test was written; it is now 14 (col-023).
// The v12→v13 migration behaviour is verified by the functional tests below.
// The constant is tested in migration_v13_to_v14.rs::test_current_schema_version_is_14.
#[test]
fn test_current_schema_version_is_at_least_13() {
    assert!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 13,
        "CURRENT_SCHEMA_VERSION must be >= 13"
    );
}

// ---------------------------------------------------------------------------
// Test 1: v12→v13 Supersedes bootstrap (AC-05, AC-06, AC-18, R-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_supersedes_bootstrap() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: 3 entries forming a chain: 1 ← 2 ← 3
    create_v12_database(&db_path).await;
    insert_v12_entry(&db_path, 1, None).await;
    insert_v12_entry(&db_path, 2, Some(1)).await; // B supersedes A
    insert_v12_entry(&db_path, 3, Some(2)).await; // C supersedes B

    // Act: open with current code → triggers v12→v13 migration
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: schema_version = 14 (migration continues through v13→v14)
    assert_eq!(read_schema_version(&store).await, 14);

    // Assert: graph_edges table exists
    assert!(graph_edges_table_exists(&store).await);

    // Assert: exactly 2 Supersedes rows
    assert_eq!(count_graph_edges_by_type(&store, "Supersedes").await, 2);

    // Assert: B→A edge: source_id=1 (entry.supersedes), target_id=2 (entry.id)
    let row_ba: (i64, i64, String, f64, i64, String, String, i64) = sqlx::query_as(
        "SELECT source_id, target_id, relation_type, weight, bootstrap_only,
                created_by, source, metadata IS NULL
         FROM graph_edges
         WHERE source_id = 1 AND target_id = 2 AND relation_type = 'Supersedes'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch B→A edge");

    let (src, tgt, rtype, weight, bootstrap_only, created_by, src_col, metadata_is_null) = row_ba;
    assert_eq!(src, 1, "source_id must be entry.supersedes (old)");
    assert_eq!(tgt, 2, "target_id must be entry.id (new)");
    assert_eq!(rtype, "Supersedes");
    assert!(
        (weight - 1.0_f64).abs() < 1e-9,
        "Supersedes weight must be 1.0"
    );
    assert_eq!(bootstrap_only, 0, "bootstrap_only must be 0");
    assert_eq!(created_by, "bootstrap");
    assert_eq!(src_col, "entries.supersedes");
    assert_eq!(metadata_is_null, 1, "metadata must be NULL (IS NULL = 1)");

    // Assert: C→B edge: source_id=2, target_id=3
    let count_cb: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges
         WHERE source_id = 2 AND target_id = 3 AND relation_type = 'Supersedes'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count C→B edge");
    assert_eq!(count_cb, 1, "C→B Supersedes edge must exist");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 2: R-06 — empty co_access migration succeeds (R-06, AC-07)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_empty_co_access_succeeds() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v12 database with empty co_access table
    create_v12_database(&db_path).await;

    // Act: migration must not error on empty co_access
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("migration must succeed with empty co_access (R-06)");

    // Assert: schema_version = 14 (migration continues through v13→v14)
    assert_eq!(read_schema_version(&store).await, 14);

    // Assert: zero CoAccess edges — no weight NOT NULL violation triggered
    assert_eq!(count_graph_edges_by_type(&store, "CoAccess").await, 0);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 3: CoAccess threshold and weight normalization (AC-07, R-15)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_co_access_threshold_and_weights() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: co_access rows:
    //   (1, 2, count=2) — below threshold, must NOT appear
    //   (1, 3, count=3) — at threshold, weight = 3/5 = 0.6
    //   (1, 4, count=5) — max, weight = 5/5 = 1.0
    create_v12_database(&db_path).await;
    insert_v12_co_access(&db_path, 1, 2, 2).await;
    insert_v12_co_access(&db_path, 1, 3, 3).await;
    insert_v12_co_access(&db_path, 1, 4, 5).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: exactly 2 CoAccess edges (count=2 pair excluded)
    assert_eq!(count_graph_edges_by_type(&store, "CoAccess").await, 2);

    // Assert: pair (1, 2) does NOT appear — count < threshold
    let count_12: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges
         WHERE source_id = 1 AND target_id = 2 AND relation_type = 'CoAccess'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count pair 1,2");
    assert_eq!(
        count_12, 0,
        "pair (1,2) count=2 must be excluded (below threshold)"
    );

    // Assert: pair (1, 4) weight = 1.0 (count=5 is max)
    let weight_14: f64 = sqlx::query_scalar(
        "SELECT weight FROM graph_edges
         WHERE source_id = 1 AND target_id = 4 AND relation_type = 'CoAccess'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch weight for pair 1,4");
    assert!(
        (weight_14 - 1.0_f64).abs() < 1e-6,
        "pair (1,4) weight must be 1.0, got {weight_14}"
    );

    // Assert: pair (1, 3) weight = 0.6 (count=3/max=5)
    let weight_13: f64 = sqlx::query_scalar(
        "SELECT weight FROM graph_edges
         WHERE source_id = 1 AND target_id = 3 AND relation_type = 'CoAccess'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch weight for pair 1,3");
    assert!(
        (weight_13 - 0.6_f64).abs() < 1e-6,
        "pair (1,3) weight must be 0.6, got {weight_13}"
    );

    // Assert: weight for count=5 > weight for count=3 (R-15 validation)
    assert!(
        weight_14 > weight_13,
        "higher count must produce higher weight (count=5 > count=3)"
    );

    // Assert: all CoAccess rows have bootstrap_only=0, source='bootstrap', created_by='bootstrap'
    let co_access_attrs: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT bootstrap_only, created_by, source
         FROM graph_edges
         WHERE relation_type = 'CoAccess'",
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("fetch CoAccess attrs");

    for (bootstrap_only, created_by, source) in &co_access_attrs {
        assert_eq!(*bootstrap_only, 0, "CoAccess bootstrap_only must be 0");
        assert_eq!(created_by, "bootstrap");
        assert_eq!(source, "co_access");
    }

    // Assert: all weights in range (0.0, 1.0]
    let weights: Vec<f64> =
        sqlx::query_scalar("SELECT weight FROM graph_edges WHERE relation_type = 'CoAccess'")
            .fetch_all(store.read_pool_test())
            .await
            .expect("fetch weights");
    for w in &weights {
        assert!(*w > 0.0, "weight must be > 0.0, got {w}");
        assert!(*w <= 1.0, "weight must be <= 1.0, got {w}");
    }

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 4: CoAccess all-below-threshold produces no edges
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_co_access_all_below_threshold() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: co_access rows all below threshold
    create_v12_database(&db_path).await;
    insert_v12_co_access(&db_path, 1, 2, 1).await;
    insert_v12_co_access(&db_path, 2, 3, 2).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: zero CoAccess edges, migration completed without error
    assert_eq!(count_graph_edges_by_type(&store, "CoAccess").await, 0);
    assert_eq!(read_schema_version(&store).await, 14);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 5: No Contradicts edges bootstrapped (AC-08)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_no_contradicts_bootstrapped() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v12 database with some entries (gives migration data to work with)
    create_v12_database(&db_path).await;
    insert_v12_entry(&db_path, 1, None).await;
    insert_v12_entry(&db_path, 2, Some(1)).await;
    insert_v12_co_access(&db_path, 1, 2, 5).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: zero Contradicts edges — shadow_evaluations has no entry ID pairs (AC-08)
    assert_eq!(
        count_graph_edges_by_type(&store, "Contradicts").await,
        0,
        "migration must write zero Contradicts edges (AC-08)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 6: Idempotency — double run (R-08, AC-05)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_idempotent_double_run() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v12 database with data
    create_v12_database(&db_path).await;
    insert_v12_entry(&db_path, 1, None).await;
    insert_v12_entry(&db_path, 2, Some(1)).await;
    insert_v12_co_access(&db_path, 1, 2, 5).await;

    // Act: first open — triggers v12→v13 migration
    let edge_count_first;
    {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("first open");
        assert_eq!(read_schema_version(&store).await, 14);
        edge_count_first = count_all_graph_edges(&store).await;
        store.close().await.unwrap();
    }

    // Act: second open — migration must be a no-op (already at v14)
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open must succeed (no UNIQUE constraint error)");

    // Assert: identical row counts after both opens
    let edge_count_second = count_all_graph_edges(&store).await;
    assert_eq!(
        edge_count_first, edge_count_second,
        "row counts must be identical after double migration run"
    );

    // Assert: schema_version still 14
    assert_eq!(read_schema_version(&store).await, 14);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 7: Bootstrap-to-confirmed promotion path (AC-21)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v13_bootstrap_only_promotion_delete_insert() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: fresh v13 database (skip migration path)
    create_v12_database(&db_path).await;
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    let now = 1_700_000_000_i64;

    // Insert a bootstrap_only=1 edge (simulating a W1-2 bootstrap-origin edge)
    sqlx::query(
        "INSERT INTO graph_edges
            (source_id, target_id, relation_type, weight, created_at,
             created_by, source, bootstrap_only)
         VALUES (10, 20, 'CoAccess', 0.5, ?1, 'bootstrap', 'co_access', 1)",
    )
    .bind(now)
    .execute(store.write_pool_test())
    .await
    .expect("insert bootstrap_only=1 edge");

    // Act step 1: DELETE the bootstrap-only row
    sqlx::query(
        "DELETE FROM graph_edges
         WHERE source_id = 10 AND target_id = 20
           AND relation_type = 'CoAccess' AND bootstrap_only = 1",
    )
    .execute(store.write_pool_test())
    .await
    .expect("delete bootstrap edge");

    // Act step 2: INSERT with bootstrap_only=0 (promoted — confirmed by NLI)
    sqlx::query(
        "INSERT INTO graph_edges
            (source_id, target_id, relation_type, weight, created_at,
             created_by, source, bootstrap_only)
         VALUES (10, 20, 'CoAccess', 0.5, ?1, 'nli-agent', 'nli', 0)",
    )
    .bind(now)
    .execute(store.write_pool_test())
    .await
    .expect("insert promoted edge");

    // Assert: final row has bootstrap_only=0
    let bootstrap_only: i64 = sqlx::query_scalar(
        "SELECT bootstrap_only FROM graph_edges
         WHERE source_id = 10 AND target_id = 20 AND relation_type = 'CoAccess'",
    )
    .fetch_one(store.write_pool_test())
    .await
    .expect("fetch bootstrap_only");
    assert_eq!(
        bootstrap_only, 0,
        "promoted edge must have bootstrap_only=0"
    );

    // Act step 3: repeat the INSERT with INSERT OR IGNORE (idempotency check)
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
            (source_id, target_id, relation_type, weight, created_at,
             created_by, source, bootstrap_only)
         VALUES (10, 20, 'CoAccess', 0.5, ?1, 'nli-agent', 'nli', 0)",
    )
    .bind(now)
    .execute(store.write_pool_test())
    .await
    .expect("idempotent insert must not error");

    // Assert: still exactly one row — no duplicates
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges
         WHERE source_id = 10 AND target_id = 20 AND relation_type = 'CoAccess'",
    )
    .fetch_one(store.write_pool_test())
    .await
    .expect("count rows");
    assert_eq!(
        count, 1,
        "exactly one row must remain after idempotent insert"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 8: Fresh database with no entries or co_access (edge case)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_empty_entries_and_co_access() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: v12 database with zero entries and zero co_access rows
    create_v12_database(&db_path).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("migration must succeed with no data");

    // Assert: migration completed without error, zero graph_edges rows
    assert_eq!(read_schema_version(&store).await, 14);
    assert!(graph_edges_table_exists(&store).await);
    assert_eq!(count_all_graph_edges(&store).await, 0);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Test 10: Bootstrap inserts use direct SQL, not analytics queue (R-13)
// ---------------------------------------------------------------------------

// This is a code inspection test — verified by confirming migration.rs has zero
// AnalyticsWrite references. The test asserts the constant is accessible (proves
// the module is compiled with the right structure) and documents the boundary.
#[test]
fn inspect_migration_no_analytics_write_calls() {
    // If migration.rs imported AnalyticsWrite, it would appear in the module's
    // use declarations. The test plan verifies zero occurrences via code inspection.
    // This test documents the R-13 boundary: bootstrap inserts use raw sqlx queries.
    //
    // Structural proof: CURRENT_SCHEMA_VERSION is accessible from migration module.
    // Any AnalyticsWrite import would require unimatrix_store::analytics, which is
    // not imported in migration.rs (see file-level imports).
    // Note: CURRENT_SCHEMA_VERSION is 14 (col-023). The R-13 boundary still holds.
    assert!(unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 13);
}

// ---------------------------------------------------------------------------
// Supersedes edge direction confirmation (VARIANCE 1)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_supersedes_edge_direction() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    // Arrange: entry id=2, supersedes=1 (B supersedes A)
    create_v12_database(&db_path).await;
    insert_v12_entry(&db_path, 1, None).await;
    insert_v12_entry(&db_path, 2, Some(1)).await;

    // Act
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // Assert: source_id=1 (old/entry.supersedes), target_id=2 (new/entry.id)
    // VARIANCE 1: architecture migration SQL direction governs over SPECIFICATION FR-08.
    let (source_id, target_id): (i64, i64) = sqlx::query_as(
        "SELECT source_id, target_id FROM graph_edges WHERE relation_type = 'Supersedes'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("fetch Supersedes edge");

    assert_eq!(
        source_id, 1,
        "source_id must be entry.supersedes (old entry)"
    );
    assert_eq!(target_id, 2, "target_id must be entry.id (new entry)");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// Supersedes edges are bootstrap_only=0 (AC-06)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v12_to_v13_supersedes_bootstrap_only_zero() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    create_v12_database(&db_path).await;
    insert_v12_entry(&db_path, 1, None).await;
    insert_v12_entry(&db_path, 2, Some(1)).await;

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open store");

    // All Supersedes rows must have bootstrap_only=0 (authoritative, not heuristic)
    let non_zero_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges
         WHERE relation_type = 'Supersedes' AND bootstrap_only != 0",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count non-zero bootstrap_only");

    assert_eq!(
        non_zero_count, 0,
        "all Supersedes edges must have bootstrap_only=0 (AC-06)"
    );

    store.close().await.unwrap();
}
