//! Integration tests for the v20→v21 schema migration (crt-043).
//!
//! Covers:
//!   MIG-V21-U-01 — CURRENT_SCHEMA_VERSION constant is 21
//!   MIG-V21-U-02 — Fresh database initializes directly to v21
//!   MIG-V21-U-03 — v20→v21 migration adds both columns
//!   MIG-V21-U-04 — Partial-apply recovery (goal_embedding pre-exists)
//!   MIG-V21-U-05 — Idempotency: re-open v21 database is a no-op
//!   MIG-V21-U-06 — Composite index idx_observations_topic_phase present after migration
//!   STORE-U-01   — update_cycle_start_goal_embedding: non-existent cycle_id → Ok
//!   STORE-U-02   — update_cycle_start_goal_embedding: writes blob to cycle_start row
//!
//! Pattern: create a v20-shaped database programmatically (v20 DDL = v19 DDL verbatim;
//! v19→v20 migration adds no new columns, only inserts back-fill data into graph_edges).
//! Open with current SqlxStore to trigger v20→v21 migration. Assert schema state.

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V20 database builder
// ---------------------------------------------------------------------------

/// Create a v20-shaped database at the given path.
///
/// The v20 DDL is identical to the v19 DDL — the v19→v20 migration (crt-044) adds no
/// new columns, only back-fills bidirectional edges into graph_edges. The cycle_events
/// table at v20 has columns: `id, cycle_id, seq, event_type, phase, outcome, next_phase,
/// timestamp, goal` — no `goal_embedding`. The observations table at v20 has columns:
/// `id, session_id, ts_millis, hook, tool, input, response_size, response_snippet,
/// topic_signal` — no `phase`.
async fn create_v20_database(path: &Path) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v20 setup conn");

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
        // observations at v20: no `phase` column
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
        // cycle_events at v20: no `goal_embedding` column
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

    // Seed counters at v20.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 20)",
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

// ---------------------------------------------------------------------------
// MIG-V21-U-01: CURRENT_SCHEMA_VERSION constant is 21
// ---------------------------------------------------------------------------

/// Verify CURRENT_SCHEMA_VERSION constant is at least 21.
/// Non-async: no fixture required.
/// Uses >= so this test remains valid after future version bumps.
#[test]
fn test_current_schema_version_is_at_least_21() {
    assert!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 21,
        "CURRENT_SCHEMA_VERSION must be >= 21, got {}",
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION
    );
}

// ---------------------------------------------------------------------------
// MIG-V21-U-02: Fresh database creates schema v21
// ---------------------------------------------------------------------------

/// A freshly-created database (no migration path) must initialize directly to v21
/// via create_tables_if_needed, and both new columns must be present.
#[tokio::test]
async fn test_fresh_db_creates_schema_v21() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    assert!(read_schema_version(&store).await >= 21);

    // goal_embedding present on cycle_events
    let has_goal_embedding: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal_embedding'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma cycle_events goal_embedding");
    assert_eq!(
        has_goal_embedding, 1,
        "goal_embedding must be present on fresh cycle_events"
    );

    // phase present on observations
    let has_phase: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma observations phase");
    assert_eq!(has_phase, 1, "phase must be present on fresh observations");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V21-U-03: v20→v21 migration — both columns present (R-05, AC-01, AC-07)
// ---------------------------------------------------------------------------

/// Open a real v20 fixture via Store::open(). Assert both columns present and
/// schema_version = 21. This is the mandatory AC-01/AC-07 test (R-05, FR-M-04).
#[tokio::test]
async fn test_v20_to_v21_both_columns_present() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v20_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after v20→v21 migration");

    // Assert schema_version = 21
    assert!(read_schema_version(&store).await >= 21);

    // Assert goal_embedding BLOB on cycle_events
    let has_goal_embedding: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal_embedding'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma cycle_events");
    assert_eq!(
        has_goal_embedding, 1,
        "goal_embedding column must be present on cycle_events"
    );

    // Assert phase TEXT on observations
    let has_phase: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma observations");
    assert_eq!(has_phase, 1, "phase column must be present on observations");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V21-U-04: Partial-apply recovery — pre-existing goal_embedding column
// ---------------------------------------------------------------------------

/// Simulate a partially-applied previous migration attempt: goal_embedding was added
/// manually but schema_version was not bumped. The v21 block must detect this via the
/// pragma_table_info pre-check and skip the first ALTER, then add the second.
#[tokio::test]
async fn test_v20_to_v21_partial_apply_recovery() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v20_database(&db_path).await;

    // Simulate partial application: add goal_embedding manually, leave schema_version at 20.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("setup conn");
        sqlx::query("ALTER TABLE cycle_events ADD COLUMN goal_embedding BLOB")
            .execute(&mut conn)
            .await
            .expect("pre-add goal_embedding");
    }

    // Act: open triggers v20→v21 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open with partial state");

    // Assert: no error; both columns present; schema_version = 21.
    assert!(read_schema_version(&store).await >= 21);

    let has_goal_embedding: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('cycle_events') WHERE name = 'goal_embedding'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma goal_embedding");
    assert_eq!(
        has_goal_embedding, 1,
        "goal_embedding must be present (pre-existing)"
    );

    let has_phase: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('observations') WHERE name = 'phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("pragma phase");
    assert_eq!(has_phase, 1, "phase must be present (added by migration)");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V21-U-05: Idempotency — re-open v21 database (R-06, AC-11)
// ---------------------------------------------------------------------------

/// Opening an already-v21 database a second time must be a no-op: no error,
/// schema_version remains 21.
#[tokio::test]
async fn test_v21_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v20_database(&db_path).await;

    // First open triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("first open");
    assert!(read_schema_version(&store).await >= 21);
    store.close().await.unwrap();

    // Second open must be a no-op — version should not change.
    let store2 = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("second open must not error");
    assert!(
        read_schema_version(&store2).await >= 21,
        "schema_version must remain >= 21 on re-open"
    );
    store2.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V21-U-06: Composite index present after migration (R-13, FR-C-07)
// ---------------------------------------------------------------------------

/// After v20→v21 migration, the composite index idx_observations_topic_phase
/// must be present on the observations table.
#[tokio::test]
async fn test_v21_composite_index_present() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v20_database(&db_path).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    let index_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type = 'index'
           AND tbl_name = 'observations'
           AND name = 'idx_observations_topic_phase'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("check composite index");

    assert_eq!(
        index_count, 1,
        "idx_observations_topic_phase must be present after v20→v21 migration"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// STORE-U-01: update_cycle_start_goal_embedding — non-existent cycle_id → Ok
// ---------------------------------------------------------------------------

/// Calling update_cycle_start_goal_embedding with a cycle_id that has no rows must
/// return Ok(()) — zero rows affected is not an error.
#[tokio::test]
async fn test_update_goal_embedding_nonexistent_cycle_id() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open");

    let bytes = unimatrix_store::encode_goal_embedding(vec![1.0_f32, 2.0_f32])
        .expect("encode must succeed for valid Vec<f32>");

    let result = store
        .update_cycle_start_goal_embedding("nonexistent-cycle-id", bytes)
        .await;

    assert!(
        result.is_ok(),
        "zero rows affected must return Ok, not Err: {:?}",
        result
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// STORE-U-02: update_cycle_start_goal_embedding — writes blob to cycle_start row
// ---------------------------------------------------------------------------

/// Insert a cycle_start row, write an embedding blob, read it back via raw SQL,
/// and assert it decodes to the original Vec<f32>.
#[tokio::test]
async fn test_update_goal_embedding_writes_blob() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open");

    let cycle_id = "test-cycle-abc-123";
    let original_vec: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();

    // Insert a cycle_start row via the store method.
    store
        .insert_cycle_event(
            cycle_id,
            0,
            "cycle_start",
            None,
            None,
            None,
            1_000_000,
            None,
        )
        .await
        .expect("insert cycle_start");

    // Encode and write the embedding.
    let bytes =
        unimatrix_store::encode_goal_embedding(original_vec.clone()).expect("encode must succeed");
    store
        .update_cycle_start_goal_embedding(cycle_id, bytes)
        .await
        .expect("update must succeed");

    // Read back the raw blob via the write pool.
    let raw_blob: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT goal_embedding FROM cycle_events
         WHERE cycle_id = ? AND event_type = 'cycle_start'",
    )
    .bind(cycle_id)
    .fetch_one(store.write_pool_test())
    .await
    .expect("read back goal_embedding");

    assert!(
        raw_blob.is_some(),
        "goal_embedding must be non-NULL after update"
    );

    let decoded = unimatrix_store::decode_goal_embedding(raw_blob.as_deref().unwrap())
        .expect("decode must succeed for bytes written by encode");

    assert_eq!(
        original_vec, decoded,
        "decoded Vec<f32> must match the original"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// STORE-U-03: phase stored and readable
// ---------------------------------------------------------------------------

/// Insert an observation row with phase = "pseudocode" via raw SQL and read it
/// back to confirm the value round-trips through the DB unchanged.
#[tokio::test]
async fn test_phase_stored_and_readable() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    let session_id = "store-u-03-session";
    let ts_millis: i64 = 1_700_000_000_000;

    sqlx::query(
        "INSERT INTO observations
         (session_id, ts_millis, hook, tool, input, response_size, response_snippet,
          topic_signal, phase)
         VALUES (?1, ?2, ?3, NULL, NULL, NULL, NULL, NULL, ?4)",
    )
    .bind(session_id)
    .bind(ts_millis)
    .bind("pre_tool_use")
    .bind("pseudocode")
    .execute(store.write_pool_test())
    .await
    .expect("insert observation with phase");

    let phase: Option<String> =
        sqlx::query_scalar("SELECT phase FROM observations WHERE session_id = ?1")
            .bind(session_id)
            .fetch_one(store.read_pool_test())
            .await
            .expect("read phase");

    assert_eq!(
        phase.as_deref(),
        Some("pseudocode"),
        "phase must round-trip through the DB unchanged"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// STORE-U-04: phase None stored as NULL
// ---------------------------------------------------------------------------

/// Insert an observation row with phase omitted (NULL) and confirm the DB
/// stores NULL, not an empty string or default value.
#[tokio::test]
async fn test_phase_none_stored_as_null() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    let session_id = "store-u-04-session";
    let ts_millis: i64 = 1_700_000_001_000;

    sqlx::query(
        "INSERT INTO observations
         (session_id, ts_millis, hook, tool, input, response_size, response_snippet,
          topic_signal, phase)
         VALUES (?1, ?2, ?3, NULL, NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(session_id)
    .bind(ts_millis)
    .bind("pre_tool_use")
    .execute(store.write_pool_test())
    .await
    .expect("insert observation with NULL phase");

    let phase: Option<String> =
        sqlx::query_scalar("SELECT phase FROM observations WHERE session_id = ?1")
            .bind(session_id)
            .fetch_one(store.read_pool_test())
            .await
            .expect("read phase");

    assert!(
        phase.is_none(),
        "phase must be NULL in the DB when None was stored, got: {:?}",
        phase
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// STORE-U-05: phase persists across v20→v21 migration
// ---------------------------------------------------------------------------

/// Verify that:
///   1. Existing v20 rows have NULL phase after migration (no back-fill).
///   2. New rows inserted after migration correctly store and return `phase`.
#[tokio::test]
async fn test_phase_persists_across_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v20_database(&db_path).await;

    // Insert a row directly via raw SQL while still in v20 shape (no phase column).
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("v20 setup conn");
        sqlx::query(
            "INSERT INTO observations
             (session_id, ts_millis, hook, tool, input, response_size, response_snippet,
              topic_signal)
             VALUES ('pre-migration-session', 1700000002000, 'pre_tool_use',
                     NULL, NULL, NULL, NULL, NULL)",
        )
        .execute(&mut conn)
        .await
        .expect("insert v20 row without phase column");
    }

    // Open via SqlxStore — triggers v20→v21 migration.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    assert!(read_schema_version(&store).await >= 21);

    // Pre-migration row must have NULL phase (migration does not back-fill).
    let pre_phase: Option<String> = sqlx::query_scalar(
        "SELECT phase FROM observations WHERE session_id = 'pre-migration-session'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("read pre-migration phase");

    assert!(
        pre_phase.is_none(),
        "v20 row must have NULL phase after migration — no back-fill expected, got: {:?}",
        pre_phase
    );

    // Post-migration insert must correctly store a non-NULL phase.
    sqlx::query(
        "INSERT INTO observations
         (session_id, ts_millis, hook, tool, input, response_size, response_snippet,
          topic_signal, phase)
         VALUES (?1, ?2, ?3, NULL, NULL, NULL, NULL, NULL, ?4)",
    )
    .bind("post-migration-session")
    .bind(1_700_000_003_000_i64)
    .bind("post_tool_use")
    .bind("delivery")
    .execute(store.write_pool_test())
    .await
    .expect("insert post-migration row with phase");

    let post_phase: Option<String> =
        sqlx::query_scalar("SELECT phase FROM observations WHERE session_id = ?1")
            .bind("post-migration-session")
            .fetch_one(store.read_pool_test())
            .await
            .expect("read post-migration phase");

    assert_eq!(
        post_phase.as_deref(),
        Some("delivery"),
        "post-migration row must store and return the correct phase"
    );

    store.close().await.unwrap();
}
