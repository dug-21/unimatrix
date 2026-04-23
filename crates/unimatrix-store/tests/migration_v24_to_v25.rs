//! Integration tests for the v24→v25 schema migration (vnc-014 / ASS-050).
//!
//! Covers:
//!   MIG-V25-U-01 — CURRENT_SCHEMA_VERSION constant is >= 25
//!   MIG-V25-U-02 — Fresh database initializes directly to v25 (12 columns, triggers)
//!   MIG-V25-U-03 — v24→v25 migration adds all four columns with correct defaults
//!   MIG-V25-U-04 — Idempotency: re-open v25 database is a no-op
//!   MIG-V25-U-05 — Partial column pre-existence: idempotency after partial crash
//!   MIG-V25-U-06 — Row count and schema version unchanged after migration (AC-09)
//!   MIG-V25-U-07 — Fresh-DB schema identical to migrated-DB schema (R-11 parity)
//!   MIG-V25-U-08 — Append-only triggers fire on DELETE and UPDATE
//!   MIG-V25-U-09 — Both triggers present in sqlite_master after migration
//!   MIG-V25-U-10 — v24 database with zero rows migrates cleanly

#![cfg(feature = "test-support")]

use std::path::Path;

use sqlx::ConnectOptions as _;
use sqlx::Row as _;
use sqlx::sqlite::SqliteConnectOptions;
use tempfile::TempDir;
use unimatrix_store::SqlxStore;
use unimatrix_store::pool_config::PoolConfig;

// ---------------------------------------------------------------------------
// V24 database builder
// ---------------------------------------------------------------------------

/// Create a v24-shaped database at the given path.
///
/// The v24 DDL has audit_log with 8 columns only (no new columns, no triggers).
/// Optionally seeds rows into audit_log and entries to verify no data loss (AC-09).
async fn create_v24_database(path: &Path, seed_rows: bool) {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);

    let mut conn = opts.connect().await.expect("open v24 setup conn");

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
        // v24 audit_log: 8 columns only — no new fields, no triggers
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
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id       TEXT    NOT NULL,
            ts_millis        INTEGER NOT NULL,
            hook             TEXT    NOT NULL,
            tool             TEXT,
            input            TEXT,
            response_size    INTEGER,
            response_snippet TEXT,
            topic_signal     TEXT,
            phase            TEXT
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
            id             INTEGER PRIMARY KEY AUTOINCREMENT,
            cycle_id       TEXT    NOT NULL,
            seq            INTEGER NOT NULL,
            event_type     TEXT    NOT NULL,
            phase          TEXT,
            outcome        TEXT,
            next_phase     TEXT,
            timestamp      INTEGER NOT NULL,
            goal           TEXT,
            goal_embedding BLOB
        )",
        "CREATE TABLE cycle_review_index (
            feature_cycle         TEXT    PRIMARY KEY,
            schema_version        INTEGER NOT NULL,
            computed_at           INTEGER NOT NULL,
            raw_signals_available INTEGER NOT NULL DEFAULT 1,
            summary_json          TEXT    NOT NULL,
            corrections_total     INTEGER NOT NULL DEFAULT 0,
            corrections_agent     INTEGER NOT NULL DEFAULT 0,
            corrections_human     INTEGER NOT NULL DEFAULT 0,
            corrections_system    INTEGER NOT NULL DEFAULT 0,
            deprecations_total    INTEGER NOT NULL DEFAULT 0,
            orphan_deprecations   INTEGER NOT NULL DEFAULT 0,
            first_computed_at     INTEGER NOT NULL DEFAULT 0
        )",
        "CREATE TABLE goal_clusters (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            feature_cycle   TEXT    NOT NULL UNIQUE,
            goal_embedding  BLOB    NOT NULL,
            phase           TEXT,
            entry_ids_json  TEXT    NOT NULL,
            outcome         TEXT,
            created_at      INTEGER NOT NULL
        )",
        "CREATE INDEX idx_entries_topic ON entries(topic)",
        "CREATE INDEX idx_entries_category ON entries(category)",
        "CREATE INDEX idx_entries_status ON entries(status)",
        "CREATE INDEX idx_entries_created_at ON entries(created_at)",
        "CREATE INDEX idx_entry_tags_tag ON entry_tags(tag)",
        "CREATE INDEX idx_entry_tags_entry_id ON entry_tags(entry_id)",
        "CREATE INDEX idx_entry_tags_tag_entry_id ON entry_tags(tag, entry_id)",
        "CREATE INDEX idx_co_access_b ON co_access(entry_id_b)",
        "CREATE INDEX idx_sessions_feature_cycle ON sessions(feature_cycle)",
        "CREATE INDEX idx_sessions_started_at ON sessions(started_at)",
        "CREATE INDEX idx_injection_log_session ON injection_log(session_id)",
        "CREATE INDEX idx_injection_log_entry ON injection_log(entry_id)",
        "CREATE INDEX idx_audit_log_agent ON audit_log(agent_id)",
        "CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp)",
        "CREATE INDEX idx_observations_session ON observations(session_id)",
        "CREATE INDEX idx_observations_ts ON observations(ts_millis)",
        "CREATE INDEX idx_observations_topic_phase ON observations (topic_signal, phase)",
        "CREATE INDEX idx_shadow_eval_ts ON shadow_evaluations(timestamp)",
        "CREATE INDEX idx_query_log_session ON query_log(session_id)",
        "CREATE INDEX idx_query_log_ts ON query_log(ts)",
        "CREATE INDEX idx_query_log_phase ON query_log(phase)",
        "CREATE INDEX idx_graph_edges_source_id ON graph_edges(source_id)",
        "CREATE INDEX idx_graph_edges_target_id ON graph_edges(target_id)",
        "CREATE INDEX idx_graph_edges_relation_type ON graph_edges(relation_type)",
        "CREATE INDEX idx_cycle_events_cycle_id ON cycle_events (cycle_id)",
        "CREATE INDEX idx_goal_clusters_created_at ON goal_clusters(created_at DESC)",
    ] {
        sqlx::query(ddl)
            .execute(&mut conn)
            .await
            .expect("create table/index");
    }

    // Seed counters at v24.
    for seed in &[
        "INSERT INTO counters (name, value) VALUES ('schema_version', 24)",
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

    if seed_rows {
        // Seed audit_log rows (8-column shape) to verify no data loss after migration.
        for i in 1i64..=5 {
            sqlx::query(
                "INSERT INTO audit_log
                     (event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail)
                 VALUES (?1, ?2, 'sess-1', 'agent-1', 'context_store', '[]', 0, 'seeded row')",
            )
            .bind(i)
            .bind(1_700_000_000_i64 + i)
            .execute(&mut conn)
            .await
            .expect("seed audit_log row");
        }

        // Seed entries rows to verify broader data preservation.
        for i in 1i64..=3 {
            sqlx::query(
                "INSERT INTO entries
                     (id, title, content, topic, category, source, status, confidence,
                      created_at, updated_at)
                 VALUES (?1, 'title', 'content', 'test', 'convention', 'test', 0, 0.5,
                         1700000000, 1700000000)",
            )
            .bind(i)
            .execute(&mut conn)
            .await
            .expect("seed entries row");
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn read_schema_version(store: &SqlxStore) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'schema_version'")
        .fetch_one(store.read_pool_test())
        .await
        .expect("read schema_version")
}

// ---------------------------------------------------------------------------
// MIG-V25-U-01: CURRENT_SCHEMA_VERSION constant is >= 25
// ---------------------------------------------------------------------------

#[test]
fn test_current_schema_version_is_at_least_25() {
    assert!(
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION >= 25,
        "CURRENT_SCHEMA_VERSION must be >= 25 after vnc-014, got {}",
        unimatrix_store::migration::CURRENT_SCHEMA_VERSION
    );
}

// ---------------------------------------------------------------------------
// MIG-V25-U-02: Fresh database initializes directly to v25 — 12 columns, triggers present
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_fresh_db_creates_schema_v25() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open fresh store");

    assert_eq!(read_schema_version(&store).await, 25);

    // All four new columns must be present on a fresh database.
    for col in &[
        "credential_type",
        "capability_used",
        "agent_attribution",
        "metadata",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('audit_log') WHERE name = ?1",
        )
        .bind(col)
        .fetch_one(store.read_pool_test())
        .await
        .expect("pragma_table_info");
        assert_eq!(count, 1, "column {col} must exist on fresh db");
    }

    // Both append-only triggers must be present.
    let trigger_names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='audit_log' ORDER BY name",
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("query triggers");

    assert!(
        trigger_names.contains(&"audit_log_no_delete".to_string()),
        "audit_log_no_delete must exist; found: {:?}",
        trigger_names
    );
    assert!(
        trigger_names.contains(&"audit_log_no_update".to_string()),
        "audit_log_no_update must exist; found: {:?}",
        trigger_names
    );

    // Column count must be exactly 12.
    let col_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('audit_log')")
        .fetch_one(store.read_pool_test())
        .await
        .expect("column count");
    assert_eq!(col_count, 12, "audit_log must have 12 columns on fresh db");

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V25-U-03: v24→v25 migration adds all four columns with correct defaults
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v24_to_v25_migration_adds_all_four_columns() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v24_database(&db_path, true).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after v24→v25 migration");

    assert_eq!(read_schema_version(&store).await, 25);

    // 12 columns total.
    let col_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('audit_log')")
        .fetch_one(store.read_pool_test())
        .await
        .expect("column count");
    assert_eq!(
        col_count, 12,
        "audit_log must have 12 columns after migration"
    );

    // Verify each new column's type, NOT NULL constraint, and DEFAULT value.
    let rows = sqlx::query(
        "SELECT name, type, \"notnull\", dflt_value \
         FROM pragma_table_info('audit_log') ORDER BY cid",
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("pragma_table_info");

    let col_map: std::collections::HashMap<String, (String, i64, Option<String>)> = rows
        .iter()
        .map(|r| {
            (
                r.try_get::<String, _>(0).unwrap(),
                (
                    r.try_get::<String, _>(1).unwrap(),
                    r.try_get::<i64, _>(2).unwrap(),
                    r.try_get::<Option<String>, _>(3).unwrap(),
                ),
            )
        })
        .collect();

    // credential_type: TEXT NOT NULL DEFAULT 'none'
    let (_, notnull, dflt) = col_map.get("credential_type").expect("credential_type");
    assert_eq!(*notnull, 1, "credential_type must be NOT NULL");
    assert_eq!(
        dflt.as_deref(),
        Some("'none'"),
        "credential_type DEFAULT must be 'none'"
    );

    // capability_used: TEXT NOT NULL DEFAULT ''
    let (_, notnull, dflt) = col_map.get("capability_used").expect("capability_used");
    assert_eq!(*notnull, 1, "capability_used must be NOT NULL");
    assert_eq!(
        dflt.as_deref(),
        Some("''"),
        "capability_used DEFAULT must be ''"
    );

    // agent_attribution: TEXT NOT NULL DEFAULT ''
    let (_, notnull, dflt) = col_map.get("agent_attribution").expect("agent_attribution");
    assert_eq!(*notnull, 1, "agent_attribution must be NOT NULL");
    assert_eq!(
        dflt.as_deref(),
        Some("''"),
        "agent_attribution DEFAULT must be ''"
    );

    // metadata: TEXT NOT NULL DEFAULT '{}'
    let (_, notnull, dflt) = col_map.get("metadata").expect("metadata");
    assert_eq!(*notnull, 1, "metadata must be NOT NULL");
    assert_eq!(
        dflt.as_deref(),
        Some("'{}'"),
        "metadata DEFAULT must be '{{}}'"
    );

    // Verify seeded rows have correct default values for new columns.
    let row = sqlx::query(
        "SELECT credential_type, capability_used, agent_attribution, metadata \
         FROM audit_log WHERE event_id = 1",
    )
    .fetch_one(store.read_pool_test())
    .await
    .expect("seeded row");

    assert_eq!(
        row.get::<String, _>(0),
        "none",
        "credential_type must default to 'none'"
    );
    assert_eq!(
        row.get::<String, _>(1),
        "",
        "capability_used must default to ''"
    );
    assert_eq!(
        row.get::<String, _>(2),
        "",
        "agent_attribution must default to ''"
    );
    assert_eq!(
        row.get::<String, _>(3),
        "{}",
        "metadata must default to '{{}}'"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V25-U-04: Idempotency — re-open v25 database is a no-op
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_migration_idempotent() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v24_database(&db_path, false).await;

    // First open triggers migration.
    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("first open");
    assert_eq!(read_schema_version(&store).await, 25);
    store.close().await.unwrap();

    // Second open must be a no-op.
    let store2 = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("second open must not error");
    assert_eq!(
        read_schema_version(&store2).await,
        25,
        "schema_version must remain 25 on re-open"
    );

    let col_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('audit_log')")
        .fetch_one(store2.read_pool_test())
        .await
        .expect("column count");
    assert_eq!(col_count, 12, "column count must remain 12 after re-open");

    store2.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V25-U-05: Partial column pre-existence — idempotency after partial crash
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_migration_idempotent_one_column_pre_exists() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v24_database(&db_path, false).await;

    // Manually add one column — simulates crash after first ALTER.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("raw conn");
        sqlx::query(
            "ALTER TABLE audit_log ADD COLUMN credential_type TEXT NOT NULL DEFAULT 'none'",
        )
        .execute(&mut conn)
        .await
        .expect("partial add credential_type");
        // schema_version stays at 24 — simulates crash before version bump
    }

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after partial migration must succeed");

    // All four columns must be present without a "duplicate column name" error.
    let col_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('audit_log')")
        .fetch_one(store.read_pool_test())
        .await
        .expect("column count");
    assert_eq!(col_count, 12, "12 columns after partial-recovery migration");
    assert_eq!(read_schema_version(&store).await, 25);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_v25_migration_idempotent_all_columns_pre_exist() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v24_database(&db_path, false).await;

    // Manually add all four columns — simulates crash after all ALTERs but before version bump.
    {
        let opts = SqliteConnectOptions::new().filename(&db_path);
        let mut conn = opts.connect().await.expect("raw conn");
        for sql in &[
            "ALTER TABLE audit_log ADD COLUMN credential_type TEXT NOT NULL DEFAULT 'none'",
            "ALTER TABLE audit_log ADD COLUMN capability_used TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE audit_log ADD COLUMN agent_attribution TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE audit_log ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}'",
        ] {
            sqlx::query(sql)
                .execute(&mut conn)
                .await
                .expect("partial add");
        }
        // schema_version stays at 24
    }

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after all-columns-pre-exist must succeed");

    let col_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('audit_log')")
        .fetch_one(store.read_pool_test())
        .await
        .expect("column count");
    assert_eq!(col_count, 12, "12 columns after all-pre-exist recovery");
    assert_eq!(read_schema_version(&store).await, 25);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V25-U-06: Row count unchanged after migration (AC-09)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_migration_row_count_unchanged() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v24_database(&db_path, true).await; // 5 audit_log rows, 3 entries rows

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after migration");

    let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(store.read_pool_test())
        .await
        .expect("audit_log count");
    assert_eq!(audit_count, 5, "audit_log must have 5 rows (unchanged)");

    let entries_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
        .fetch_one(store.read_pool_test())
        .await
        .expect("entries count");
    assert_eq!(entries_count, 3, "entries must have 3 rows (unchanged)");

    assert_eq!(read_schema_version(&store).await, 25);

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V25-U-07: Fresh-DB schema parity with migrated-DB schema (R-11)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_fresh_db_parity_with_migrated_db() {
    // Fresh database A: via create_tables_if_needed
    let dir_a = TempDir::new().expect("temp dir A");
    let store_a = SqlxStore::open(&dir_a.path().join("a.db"), PoolConfig::test_default())
        .await
        .expect("open fresh db A");

    // Migrated database B: v24 → v25
    let dir_b = TempDir::new().expect("temp dir B");
    let db_path_b = dir_b.path().join("b.db");
    create_v24_database(&db_path_b, false).await;
    let store_b = SqlxStore::open(&db_path_b, PoolConfig::test_default())
        .await
        .expect("open migrated db B");

    // Compare pragma_table_info for audit_log: both must have identical column definitions.
    let rows_a = sqlx::query(
        "SELECT name, type, \"notnull\", dflt_value \
         FROM pragma_table_info('audit_log') ORDER BY cid",
    )
    .fetch_all(store_a.read_pool_test())
    .await
    .expect("pragma_table_info A");

    let rows_b = sqlx::query(
        "SELECT name, type, \"notnull\", dflt_value \
         FROM pragma_table_info('audit_log') ORDER BY cid",
    )
    .fetch_all(store_b.read_pool_test())
    .await
    .expect("pragma_table_info B");

    assert_eq!(
        rows_a.len(),
        rows_b.len(),
        "both databases must have the same column count"
    );

    for (i, (ra, rb)) in rows_a.iter().zip(rows_b.iter()).enumerate() {
        let name_a = ra.try_get::<String, _>(0).unwrap();
        let name_b = rb.try_get::<String, _>(0).unwrap();
        let type_a = ra.try_get::<String, _>(1).unwrap();
        let type_b = rb.try_get::<String, _>(1).unwrap();
        let nn_a = ra.try_get::<i64, _>(2).unwrap();
        let nn_b = rb.try_get::<i64, _>(2).unwrap();
        let dflt_a = ra.try_get::<Option<String>, _>(3).unwrap();
        let dflt_b = rb.try_get::<Option<String>, _>(3).unwrap();

        assert_eq!(
            name_a, name_b,
            "column {i} name mismatch: {name_a} vs {name_b}"
        );
        assert_eq!(type_a, type_b, "column {i} ({name_a}) type mismatch");
        assert_eq!(nn_a, nn_b, "column {i} ({name_a}) notnull mismatch");
        assert_eq!(dflt_a, dflt_b, "column {i} ({name_a}) default mismatch");
    }

    // Both must have both triggers.
    for store in &[&store_a, &store_b] {
        let triggers: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='audit_log' ORDER BY name",
        )
        .fetch_all(store.read_pool_test())
        .await
        .expect("triggers");
        assert!(
            triggers.contains(&"audit_log_no_delete".to_string()),
            "both DBs must have audit_log_no_delete"
        );
        assert!(
            triggers.contains(&"audit_log_no_update".to_string()),
            "both DBs must have audit_log_no_update"
        );
    }

    store_a.close().await.unwrap();
    store_b.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V25-U-08: Append-only triggers fire on DELETE and UPDATE (R-01, AC-05b, SEC-04)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_append_only_triggers_fire_on_delete() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open store");

    // Insert one row to attempt to delete.
    sqlx::query(
        "INSERT INTO audit_log
             (event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail)
         VALUES (1, 1700000001, 'sess-1', 'agent-1', 'test_op', '[]', 0, '')",
    )
    .execute(store.write_pool_test())
    .await
    .expect("insert row");

    let delete_result = sqlx::query("DELETE FROM audit_log WHERE event_id = 1")
        .execute(store.write_pool_test())
        .await;

    assert!(
        delete_result.is_err(),
        "DELETE must fail due to append-only trigger"
    );
    let err_msg = delete_result.unwrap_err().to_string();
    assert!(
        err_msg.contains("audit_log is append-only: DELETE not permitted"),
        "DELETE error must contain the trigger message, got: {err_msg}"
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_v25_append_only_triggers_fire_on_update() {
    let dir = TempDir::new().expect("temp dir");
    let store = SqlxStore::open(&dir.path().join("test.db"), PoolConfig::test_default())
        .await
        .expect("open store");

    // Insert one row to attempt to update.
    sqlx::query(
        "INSERT INTO audit_log
             (event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail)
         VALUES (1, 1700000001, 'sess-1', 'agent-1', 'test_op', '[]', 0, '')",
    )
    .execute(store.write_pool_test())
    .await
    .expect("insert row");

    let update_result = sqlx::query("UPDATE audit_log SET detail = 'tampered' WHERE event_id = 1")
        .execute(store.write_pool_test())
        .await;

    assert!(
        update_result.is_err(),
        "UPDATE must fail due to append-only trigger"
    );
    let err_msg = update_result.unwrap_err().to_string();
    assert!(
        err_msg.contains("audit_log is append-only: UPDATE not permitted"),
        "UPDATE error must contain the trigger message, got: {err_msg}"
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V25-U-09: Triggers present in sqlite_master after migration (SEC-04)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_triggers_in_sqlite_master_after_migration() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v24_database(&db_path, false).await;

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("open after v24→v25 migration");

    let trigger_names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='audit_log' ORDER BY name",
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("query triggers");

    assert!(
        trigger_names.contains(&"audit_log_no_delete".to_string()),
        "audit_log_no_delete must exist in sqlite_master after migration; found: {:?}",
        trigger_names
    );
    assert!(
        trigger_names.contains(&"audit_log_no_update".to_string()),
        "audit_log_no_update must exist in sqlite_master after migration; found: {:?}",
        trigger_names
    );

    store.close().await.unwrap();
}

// ---------------------------------------------------------------------------
// MIG-V25-U-10: v24 database with zero rows — migration succeeds cleanly (EC-07)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_v25_migration_empty_audit_log_succeeds() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("test.db");
    create_v24_database(&db_path, false).await; // no rows seeded

    let store = SqlxStore::open(&db_path, PoolConfig::test_default())
        .await
        .expect("migration on empty audit_log must succeed");

    let col_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('audit_log')")
        .fetch_one(store.read_pool_test())
        .await
        .expect("column count");
    assert_eq!(col_count, 12, "12 columns even when audit_log was empty");

    let trigger_names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='audit_log' ORDER BY name",
    )
    .fetch_all(store.read_pool_test())
    .await
    .expect("triggers");
    assert_eq!(trigger_names.len(), 2, "both triggers must exist");

    assert_eq!(read_schema_version(&store).await, 25);

    store.close().await.unwrap();
}
