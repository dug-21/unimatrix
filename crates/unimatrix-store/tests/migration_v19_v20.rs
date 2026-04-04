//! Integration tests for the v19→v20 schema migration (crt-044).
//!
//! Covers: MIG-V20-U-01 (CURRENT_SCHEMA_VERSION = 20), MIG-V20-U-02 (fresh DB creates v20),
//! MIG-V20-U-03 (S1 Informs back-fill), MIG-V20-U-04 (S2 Informs back-fill),
//! MIG-V20-U-05 (S8 CoAccess back-fill), MIG-V20-U-06 (S1+S2 count parity),
//! MIG-V20-U-07 (S8 count parity), MIG-V20-U-08 (excluded sources not back-filled),
//! MIG-V20-U-09 (idempotency clean state), MIG-V20-U-10 (idempotency with pre-existing
//! reverse), MIG-V20-U-11 (empty graph_edges no-op).
//!
//! Pattern: create a v19-shaped database (matching post-v18→v19 migration schema),
//! open with current SqlxStore to trigger v19→v20 migration, assert schema state
//! and edge counts.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V19 database builder
// ---------------------------------------------------------------------------

/// Create a v19-shaped database at the given path.
///
/// Schema is identical to v18 (all same tables + cycle_review_index). The only
/// difference from v18 is schema_version = 19. graph_edges has no rows — callers
/// insert rows as needed per test scenario.
async fn create_v19_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v19 setup conn");

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
        // cycle_review_index: added by v17→v18 migration (crt-033). Present in v18 and v19 shape.
        "CREATE TABLE cycle_review_index (
            feature_cycle         TEXT    PRIMARY KEY,
            schema_version        INTEGER NOT NULL,
            computed_at           INTEGER NOT NULL,
            raw_signals_available INTEGER NOT NULL DEFAULT 1,
            summary_json          TEXT    NOT NULL
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
        "CREATE INDEX idx_query_log_phase ON query_log(phase)",
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

    // Seed counters at v19.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 19)",
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
// Post-migration helpers
// ---------------------------------------------------------------------------

async fn read_schema_version(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'schema_version'")
        .fetch_one(store.read_pool_test())
        .await
        .expect("read schema_version")
}

/// Count graph_edges rows matching relation_type and source.
async fn count_graph_edges(store: &SqlxStore, relation_type: &str, source: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM graph_edges WHERE relation_type = ? AND source = ?",
    )
    .bind(relation_type)
    .bind(source)
    .fetch_one(store.read_pool_test())
    .await
    .expect("count_graph_edges")
}

/// Check whether a specific directed edge exists.
async fn edge_exists(
    store: &SqlxStore,
    source_id: i64,
    target_id: i64,
    relation_type: &str,
) -> bool {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges
         WHERE source_id = ? AND target_id = ? AND relation_type = ?",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(relation_type)
    .fetch_one(store.read_pool_test())
    .await
    .expect("edge_exists");
    count > 0
}

/// Fetch (source, bootstrap_only) for a specific directed edge. Returns None if not found.
async fn fetch_edge_source_and_bootstrap(
    store: &SqlxStore,
    source_id: i64,
    target_id: i64,
    relation_type: &str,
) -> Option<(String, i64)> {
    sqlx::query_as::<_, (String, i64)>(
        "SELECT source, bootstrap_only FROM graph_edges
         WHERE source_id = ? AND target_id = ? AND relation_type = ?",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(relation_type)
    .fetch_optional(store.read_pool_test())
    .await
    .expect("fetch_edge_source_and_bootstrap")
}

/// Count all rows in graph_edges.
async fn total_graph_edges_count(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(store.read_pool_test())
        .await
        .expect("total_graph_edges_count")
}

// ---------------------------------------------------------------------------
// MIG-V20-U-01: CURRENT_SCHEMA_VERSION == 20 (AC-06, R-10)
// ---------------------------------------------------------------------------

/// Verify CURRENT_SCHEMA_VERSION constant is at least 20 (updated to 21 by crt-043,
/// then to 22 by crt-046). Uses >= so this test remains valid after future bumps.
/// Non-async: no fixture required.
#[test]
fn test_current_schema_version_is_20() {
    assert!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 20,
        "CURRENT_SCHEMA_VERSION must be >= 20, got {}",
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION
    );
}

// ---------------------------------------------------------------------------
// MIG-V20-U-02: Fresh database creates schema v20 (R-10)
// ---------------------------------------------------------------------------

/// Fresh SqlxStore::open() must land at the current schema version.
/// Updated from v20 to v21 by crt-043, then to v22 by crt-046.
#[tokio::test]
async fn test_fresh_db_creates_schema_v20() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open fresh store");

    assert!(
        read_schema_version(&store).await >= 20,
        "fresh database must be at schema >= v20"
    );

    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(store.read_pool_test())
        .await
        .expect("count graph_edges");
    assert_eq!(row_count, 0, "fresh database graph_edges must be empty");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-03: S1 Informs edge back-filled (AC-09, AC-01, R-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v19_to_v20_back_fills_s1_informs_edge() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;

    // Arrange: one forward-only S1 Informs edge (1→2).
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (1, 2, 'Informs', 0.3, 0, 'tick', 'S1', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert forward S1 Informs edge");

        // Pre-condition: reverse (2→1) must not exist yet.
        let rev: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges
             WHERE source_id = 2 AND target_id = 1 AND relation_type = 'Informs'",
        )
        .fetch_one(&mut conn)
        .await
        .expect("pre-check reverse");
        assert_eq!(rev, 0, "reverse (2→1) must not exist before migration");
    }

    // Act: open triggers v19→v20 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: schema_version is current (v19→v20→v21→v22 migration chain runs in full).
    assert!(read_schema_version(&store).await >= 21);

    // Assert: reverse (2→1) exists.
    assert!(
        edge_exists(&store, 2, 1, "Informs").await,
        "reverse (2→1) S1 Informs edge must be back-filled"
    );

    // Assert: forward (1→2) still exists.
    assert!(
        edge_exists(&store, 1, 2, "Informs").await,
        "forward (1→2) S1 Informs edge must still exist"
    );

    // Assert: back-filled row carries source='S1' and bootstrap_only=0.
    let row = fetch_edge_source_and_bootstrap(&store, 2, 1, "Informs")
        .await
        .expect("back-filled (2→1) row must exist");
    assert_eq!(row.0, "S1", "back-filled row must carry source='S1'");
    assert_eq!(row.1, 0, "back-filled row must have bootstrap_only=0");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-04: S2 Informs edge back-filled (R-01, AC-09)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v19_to_v20_back_fills_s2_informs_edge() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;

    // Arrange: one forward-only S2 Informs edge (3→4).
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (3, 4, 'Informs', 0.5, 0, 'tick', 'S2', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert forward S2 Informs edge");
    }

    // Act: open triggers v19→v20 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: reverse (4→3) exists with source='S2'.
    assert!(
        edge_exists(&store, 4, 3, "Informs").await,
        "reverse (4→3) S2 Informs edge must be back-filled"
    );

    let row = fetch_edge_source_and_bootstrap(&store, 4, 3, "Informs")
        .await
        .expect("back-filled (4→3) row must exist");
    assert_eq!(row.0, "S2", "back-filled row must carry source='S2'");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-05: S8 CoAccess edge back-filled (AC-09, AC-02, R-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v19_to_v20_back_fills_s8_coaccess_edge() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;

    // Arrange: one forward-only S8 CoAccess edge (5→6).
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (5, 6, 'CoAccess', 0.25, 0, 'tick', 'S8', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert forward S8 CoAccess edge");
    }

    // Act: open triggers v19→v20 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: reverse (6→5) exists with source='S8' and bootstrap_only=0.
    assert!(
        edge_exists(&store, 6, 5, "CoAccess").await,
        "reverse (6→5) S8 CoAccess edge must be back-filled"
    );

    let row = fetch_edge_source_and_bootstrap(&store, 6, 5, "CoAccess")
        .await
        .expect("back-filled (6→5) row must exist");
    assert_eq!(row.0, "S8", "back-filled row must carry source='S8'");
    assert_eq!(row.1, 0, "back-filled row must have bootstrap_only=0");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-06: Count parity — S1+S2 Informs (AC-01)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v19_to_v20_s1_s2_count_parity_after_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;

    // Arrange: two forward-only S1 Informs edges and one S2 Informs edge (3 forward total).
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        for (src, tgt, src_field) in &[(1i64, 2i64, "S1"), (3i64, 4i64, "S1"), (5i64, 6i64, "S2")] {
            sqlx::query(
                "INSERT INTO graph_edges
                     (source_id, target_id, relation_type, weight, created_at,
                      created_by, source, bootstrap_only)
                 VALUES (?, ?, 'Informs', 0.4, 0, 'tick', ?, 0)",
            )
            .bind(src)
            .bind(tgt)
            .bind(*src_field)
            .execute(&mut conn)
            .await
            .expect("insert forward Informs edge");
        }
    }

    // Act: open triggers v19→v20 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: 6 total S1/S2 Informs edges (3 forward + 3 reverse).
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges
         WHERE relation_type = 'Informs' AND source IN ('S1', 'S2')",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count S1/S2 Informs");
    assert_eq!(
        total, 6,
        "3 forward + 3 reverse = 6 total S1/S2 Informs edges"
    );

    // Assert: every edge has a reverse partner (count parity).
    let paired: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges g1
         WHERE g1.relation_type = 'Informs'
           AND g1.source IN ('S1', 'S2')
           AND EXISTS (
             SELECT 1 FROM graph_edges g2
             WHERE g2.source_id = g1.target_id
               AND g2.target_id = g1.source_id
               AND g2.relation_type = 'Informs'
           )",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count paired S1/S2 Informs");
    assert!(total > 0);
    assert_eq!(
        total, paired,
        "every S1/S2 Informs edge must have a reverse partner (AC-01)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-07: Count parity — S8 CoAccess (AC-02)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v19_to_v20_s8_count_parity_after_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;

    // Arrange: two forward-only S8 CoAccess edges.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        for (src, tgt) in &[(10i64, 20i64), (30i64, 40i64)] {
            sqlx::query(
                "INSERT INTO graph_edges
                     (source_id, target_id, relation_type, weight, created_at,
                      created_by, source, bootstrap_only)
                 VALUES (?, ?, 'CoAccess', 0.6, 0, 'tick', 'S8', 0)",
            )
            .bind(src)
            .bind(tgt)
            .execute(&mut conn)
            .await
            .expect("insert forward S8 CoAccess edge");
        }
    }

    // Act: open triggers v19→v20 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: 4 total S8 CoAccess edges (2 forward + 2 reverse).
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges
         WHERE relation_type = 'CoAccess' AND source = 'S8'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count S8 CoAccess");
    assert_eq!(
        total, 4,
        "2 forward + 2 reverse = 4 total S8 CoAccess edges"
    );

    // Assert: every edge has a reverse partner.
    let paired: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges g1
         WHERE g1.relation_type = 'CoAccess'
           AND g1.source = 'S8'
           AND EXISTS (
             SELECT 1 FROM graph_edges g2
             WHERE g2.source_id = g1.target_id
               AND g2.target_id = g1.source_id
               AND g2.relation_type = 'CoAccess'
               AND g2.source = 'S8'
           )",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count paired S8 CoAccess");
    assert!(total > 0);
    assert_eq!(
        total, paired,
        "every S8 CoAccess edge must have a reverse partner (AC-02)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-08: Excluded sources not back-filled (R-06, R-07, AC-09)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v19_to_v20_excludes_excluded_sources() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;

    // Arrange: forward edges that must NOT be back-filled.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");

        // nli Informs: intentionally unidirectional (C-04).
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (10, 11, 'Informs', 1.0, 0, 'nli', 'nli', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert nli Informs");

        // cosine_supports Informs: out of scope (C-04).
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (12, 13, 'Informs', 0.8, 0, 'system', 'cosine_supports', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert cosine_supports Informs");

        // co_access CoAccess (both directions already present — already bidirectional since v18→v19).
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (14, 15, 'CoAccess', 0.5, 0, 'tick', 'co_access', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert co_access forward");
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (15, 14, 'CoAccess', 0.5, 0, 'tick', 'co_access', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert co_access reverse");
    }

    // Act: open triggers v19→v20 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("open after migration");

    // Assert: nli reverse must NOT exist (C-04).
    assert!(
        !edge_exists(&store, 11, 10, "Informs").await,
        "nli Informs reverse must NOT be back-filled (C-04)"
    );

    // Assert: cosine_supports reverse must NOT exist (C-04).
    assert!(
        !edge_exists(&store, 13, 12, "Informs").await,
        "cosine_supports Informs reverse must NOT be back-filled (C-04)"
    );

    // Assert: co_access CoAccess count unchanged — still exactly 2 (R-06).
    let ca_count = count_graph_edges(&store, "CoAccess", "co_access").await;
    assert_eq!(
        ca_count, 2,
        "co_access edges must not gain additional rows (R-06)"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-09: Idempotency — clean state (AC-07, R-09)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v19_to_v20_migration_idempotent_clean_state() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;

    // Arrange: two forward-only S1 Informs edges and one S8 CoAccess edge.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (1, 2, 'Informs', 0.4, 0, 'tick', 'S1', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert S1 forward 1");
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (3, 4, 'Informs', 0.4, 0, 'tick', 'S1', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert S1 forward 2");
        sqlx::query(
            "INSERT INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (5, 6, 'CoAccess', 0.3, 0, 'tick', 'S8', 0)",
        )
        .execute(&mut conn)
        .await
        .expect("insert S8 forward");
    }

    // Run 1: applies v19→v20→v21 migration chain in full.
    let count_after_first = {
        let store = SqlxStore::open(&db_path, PoolConfig::default())
            .await
            .expect("first open");
        assert!(read_schema_version(&store).await >= 21);
        let count = total_graph_edges_count(&store).await;
        store.close().await.unwrap();
        count
    };
    // 3 forward + 3 reverse = 6 total.
    assert_eq!(count_after_first, 6, "first open: 3 forward + 3 reverse");

    // Run 2: version guard (current is current) skips already-applied migration blocks;
    // INSERT OR IGNORE + NOT EXISTS provide additional safety. Edge count must be unchanged.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open must succeed");
    let count_after_second = total_graph_edges_count(&store).await;
    assert_eq!(
        count_after_second, count_after_first,
        "second open must not add rows — idempotency guaranteed (AC-07, R-09)"
    );
    assert!(read_schema_version(&store).await >= 21);
    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-10: Idempotency — with pre-existing reverse edge (AC-14, R-09)
// ---------------------------------------------------------------------------

/// Exercises partial-bidirectionality input: some pairs already bidirectional
/// before migration runs. Only the forward-only pair gains a reverse.
#[tokio::test]
async fn test_v19_to_v20_migration_idempotent_with_preexisting_reverse() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;

    // Arrange:
    //   - forward-only S1 Informs edge (1→2) — no reverse yet.
    //   - pre-existing bidirectional S1 Informs pair (3→4) + (4→3).
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        for (src, tgt) in &[(1i64, 2i64), (3i64, 4i64), (4i64, 3i64)] {
            sqlx::query(
                "INSERT INTO graph_edges
                     (source_id, target_id, relation_type, weight, created_at,
                      created_by, source, bootstrap_only)
                 VALUES (?, ?, 'Informs', 0.5, 0, 'tick', 'S1', 0)",
            )
            .bind(src)
            .bind(tgt)
            .execute(&mut conn)
            .await
            .expect("insert edge");
        }
    }

    // Act (first open): triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("first open");

    // Assert: (1→2) pair gained its reverse.
    assert!(
        edge_exists(&store, 2, 1, "Informs").await,
        "reverse (2→1) must be back-filled for forward-only pair"
    );

    // Assert: (3→4) pair still has exactly 2 rows — no duplicate inserted.
    let pairs_34_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_edges
         WHERE ((source_id = 3 AND target_id = 4) OR (source_id = 4 AND target_id = 3))
           AND relation_type = 'Informs'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("count (3,4) pair");
    assert_eq!(
        pairs_34_count, 2,
        "pre-existing bidirectional pair must remain exactly 2 rows (AC-14)"
    );

    // Total: 4 rows (1→2 forward + 2→1 new reverse + 3→4 + 4→3).
    assert_eq!(total_graph_edges_count(&store).await, 4);

    let total_after_first = total_graph_edges_count(&store).await;
    store.close().await.unwrap();

    // Act (second open): must not add any rows.
    let store2 = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("second open");
    assert_eq!(
        total_graph_edges_count(&store2).await,
        total_after_first,
        "second open must not add rows (R-09)"
    );
    store2.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V20-U-11: Empty graph_edges — no-op (edge case)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v19_to_v20_empty_graph_edges_is_noop() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v19_database(&db_path).await;
    // No graph_edges rows inserted — empty table.

    let store = SqlxStore::open(&db_path, PoolConfig::default())
        .await
        .expect("migration on empty graph_edges must not error");

    // v19→v20→...→current migration chain runs.
    assert!(read_schema_version(&store).await >= 21);

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(store.read_pool_test())
        .await
        .expect("count graph_edges");
    assert_eq!(total, 0, "back-fill on empty table must be a no-op");

    store.close().await.unwrap();
}
