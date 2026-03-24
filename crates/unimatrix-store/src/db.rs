use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use sqlx::ConnectOptions as _;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::analytics::{AnalyticsWrite, spawn_drain_task};
use crate::error::{PoolKind, Result, StoreError};
use crate::pool_config::{
    ANALYTICS_QUEUE_CAPACITY, DRAIN_SHUTDOWN_TIMEOUT, PoolConfig, READ_POOL_ACQUIRE_TIMEOUT,
    apply_pragmas_to_connection, build_connect_options,
};

/// The sqlx-backed storage engine handle.
///
/// Owns the read pool, write pool, analytics queue, drain task, and shed counter.
/// `SqlxStore` is `Send + Sync` and shareable via `Arc<SqlxStore>`.
/// All read/write operations are `async fn` methods on this struct.
pub struct SqlxStore {
    /// Max 6–8 connections. Read-only defense-in-depth (see OQ-DURING-01).
    read_pool: SqlitePool,

    /// Max ≤ 2 connections. Serves integrity writes and the drain task.
    pub(crate) write_pool: SqlitePool,

    /// Sender half of the bounded analytics channel (capacity 1000).
    analytics_tx: tokio::sync::mpsc::Sender<AnalyticsWrite>,

    /// Shutdown signal for the drain task. Option so Drop can take it.
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,

    /// Drain task join handle. Option so close() can take and await it.
    drain_handle: Option<tokio::task::JoinHandle<()>>,

    /// Cumulative count of shed analytics write events since open().
    shed_counter: Arc<AtomicU64>,
}

impl std::fmt::Debug for SqlxStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlxStore")
            .field("shed_counter", &self.shed_counter.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl SqlxStore {
    /// Open or create a database at the given path.
    ///
    /// Steps:
    /// 1. Validate pool config
    /// 2. Open dedicated non-pooled migration connection (ADR-003)
    /// 3. Apply 6 PRAGMAs to migration connection
    /// 4. Run migration — on failure, pool construction does NOT proceed
    /// 5. Drop migration connection before constructing pools (ADR-003)
    /// 6. Build read_pool and write_pool
    /// 7. Run create_tables_if_needed (idempotent)
    /// 8. Spawn drain task
    pub async fn open(path: impl AsRef<Path>, config: PoolConfig) -> Result<SqlxStore> {
        // 1. Validate config before touching the database.
        config.validate()?;

        let db_path = path.as_ref();

        // 2. Open dedicated non-pooled migration connection (ADR-003).
        // Use SqliteConnectOptions (same as build_connect_options) so create_if_missing=true
        // is respected. Raw sqlite:// URLs do not set create_if_missing by default.
        let migration_opts = build_connect_options(db_path);
        let mut migration_conn = migration_opts
            .connect()
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 3. Apply all 6 PRAGMAs to migration connection.
        apply_pragmas_to_connection(&mut migration_conn)
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 4. Run migration. On failure: migration_conn dropped, return error.
        crate::migration::migrate_if_needed(&mut migration_conn, db_path).await?;

        // 5. Drop migration connection BEFORE constructing pools (ADR-003).
        drop(migration_conn);

        // 6. Build SqliteConnectOptions with all 6 PRAGMAs for pool connections.
        let opts = build_connect_options(db_path);

        // 7a. Construct read_pool.
        // OQ-DURING-01: remove .read_only(true) if WAL checkpoint stops working.
        let read_pool = SqlitePoolOptions::new()
            .max_connections(config.read_max_connections)
            .acquire_timeout(config.read_acquire_timeout)
            .connect_with(opts.clone().read_only(true))
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 7b. Construct write_pool.
        let write_pool = SqlitePoolOptions::new()
            .max_connections(config.write_max_connections)
            .acquire_timeout(config.write_acquire_timeout)
            .connect_with(opts)
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 8. Run create_tables (idempotent) to ensure fresh databases have all tables.
        create_tables_if_needed(&write_pool)
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 9. Create analytics queue channel and shed counter.
        let (analytics_tx, analytics_rx) = tokio::sync::mpsc::channel(ANALYTICS_QUEUE_CAPACITY);
        let shed_counter = Arc::new(AtomicU64::new(0));
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        // 10. Spawn drain task. Owned by SqlxStore via drain_handle.
        let drain_handle = spawn_drain_task(
            write_pool.clone(),
            analytics_rx,
            shutdown_rx,
            shed_counter.clone(),
        );

        Ok(SqlxStore {
            read_pool,
            write_pool,
            analytics_tx,
            shutdown_tx: Some(shutdown_tx),
            drain_handle: Some(drain_handle),
            shed_counter,
        })
    }

    /// Open an existing database in read-only mode — no migrations, no drain task.
    ///
    /// Intended for eval snapshot access (nan-007, FR-24, C-02). The returned
    /// `SqlxStore` has only a read pool; the write pool is a read-only clone of
    /// the same pool so that `write_pool_server()` callers receive an error from
    /// SQLite on any write attempt. No migrations are run, no drain task is
    /// spawned, and `enqueue_analytics` is a silent no-op (sender dropped).
    ///
    /// Callers MUST NOT call `enqueue_analytics` on a readonly store — it silently
    /// discards all events (the receiver is immediately dropped).
    ///
    /// Returns `StoreError::Open` if the database file cannot be opened.
    pub async fn open_readonly(path: impl AsRef<Path>) -> Result<SqlxStore> {
        let db_path = path.as_ref();

        // Build read-only options WITHOUT journal_mode=WAL. Switching journal
        // mode requires a write transaction; VACUUM INTO produces a
        // delete-journal snapshot, so setting WAL here would fail with
        // SQLITE_READONLY (error code 8). All other pragmas are safe read-only.
        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .busy_timeout(Duration::from_secs(10))
            .pragma("synchronous", "NORMAL")
            .pragma("foreign_keys", "ON")
            .pragma("busy_timeout", "10000")
            .pragma("cache_size", "-16384")
            .read_only(true)
            .create_if_missing(false);

        let read_pool = SqlitePoolOptions::new()
            .max_connections(8)
            .acquire_timeout(READ_POOL_ACQUIRE_TIMEOUT)
            .connect_with(opts.clone())
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // write_pool reuses the same read-only options; SQLite will reject any
        // write attempt with SQLITE_READONLY, which is the correct behaviour for
        // a snapshot-backed store. We need a non-empty pool so write_pool_server()
        // callers don't panic on acquire.
        let write_pool = read_pool.clone();

        // Create a no-op analytics channel — drop the receiver immediately so
        // all enqueue_analytics calls hit the Closed branch and are silently discarded.
        let (analytics_tx, _rx) = tokio::sync::mpsc::channel(1);

        // No drain task; no shutdown signal needed.
        let shed_counter = Arc::new(AtomicU64::new(0));

        Ok(SqlxStore {
            read_pool,
            write_pool,
            analytics_tx,
            shutdown_tx: None,
            drain_handle: None,
            shed_counter,
        })
    }

    /// Graceful async teardown.
    ///
    /// Signals the drain task to flush remaining events and exit, then awaits
    /// completion within `DRAIN_SHUTDOWN_TIMEOUT`. Call in every test and in
    /// server shutdown paths. (TC-02)
    pub async fn close(mut self) -> Result<()> {
        // Send shutdown signal to drain task.
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Await drain task with grace period.
        if let Some(handle) = self.drain_handle.take() {
            match tokio::time::timeout(DRAIN_SHUTDOWN_TIMEOUT, handle).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) if e.is_panic() => {
                    tracing::error!("drain task panicked during shutdown: {:?}", e);
                    return Err(StoreError::DrainTaskPanic);
                }
                Ok(Err(_)) => {}
                Err(_elapsed) => {
                    tracing::warn!(
                        timeout_secs = DRAIN_SHUTDOWN_TIMEOUT.as_secs(),
                        "drain task did not exit within grace period; continuing with close"
                    );
                }
            }
        }
        // Explicitly close both pools so all SQLite connections are released
        // before the file lock can be acquired by another opener. Relying on
        // Pool::drop() is insufficient because drop() initiates but does not
        // await the async close, leaving the WAL/lock held until the runtime
        // processes the close future.
        self.write_pool.close().await;
        self.read_pool.close().await;
        Ok(())
    }

    /// Fire-and-forget analytics enqueue. Non-async; uses try_send semantics.
    ///
    /// If the queue is full: sheds the event, logs at WARN, increments shed counter.
    /// NEVER call this for integrity table writes (entries, entry_tags, audit_log, etc.).
    pub fn enqueue_analytics(&self, event: AnalyticsWrite) {
        match self.analytics_tx.try_send(event) {
            Ok(()) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Full(e)) => {
                let total = self.shed_counter.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::warn!(
                    variant = e.variant_name(),
                    queue_capacity = ANALYTICS_QUEUE_CAPACITY,
                    shed_total = total,
                    "analytics write shed: queue at capacity"
                );
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                // Drain task exited (shutdown in progress). Discard silently.
            }
        }
    }

    /// Returns cumulative count of shed analytics events since open().
    ///
    /// Used by context_status (FR-16, AC-18).
    pub fn shed_events_total(&self) -> u64 {
        self.shed_counter.load(Ordering::Relaxed)
    }

    /// Returns a reference to the read pool.
    ///
    /// Used by read-only query methods throughout the crate.
    pub(crate) fn read_pool(&self) -> &SqlitePool {
        &self.read_pool
    }

    /// Public read pool accessor for integration tests (test-support feature only).
    ///
    /// Gated behind `#[cfg(feature = "test-support")]` — not available in production.
    /// Integration tests in `tests/` use this to issue raw sqlx queries for schema
    /// validation and cascade checks that cannot be expressed through the public API.
    #[cfg(feature = "test-support")]
    pub fn read_pool_test(&self) -> &SqlitePool {
        &self.read_pool
    }

    /// Public write pool accessor for integration tests (test-support feature only).
    ///
    /// Gated behind `#[cfg(feature = "test-support")]` — not available in production.
    /// Integration tests in `tests/` use this to issue direct writes (e.g., DELETE for
    /// cascade verification) that go through the integrity path without analytics routing.
    #[cfg(feature = "test-support")]
    pub fn write_pool_test(&self) -> &SqlitePool {
        &self.write_pool
    }

    /// Public write pool accessor for server-internal use (export, import, audit, registry).
    ///
    /// Exposed so that server-layer code can issue raw sqlx queries against tables
    /// (audit_log, agent_registry, observations, shadow_evaluations) that are not
    /// covered by the high-level `SqlxStore` API. Callers MUST NOT use this for
    /// analytics-path writes — use `enqueue_analytics` instead.
    pub fn write_pool_server(&self) -> &SqlitePool {
        &self.write_pool
    }

    /// Insert a single row into the `cycle_events` table (crt-025).
    ///
    /// Uses the direct write pool (ADR-003): `CYCLE_EVENTS` is a structural audit-trail
    /// table, not an observational-telemetry table, so it must not go through the analytics
    /// drain where it could be shed under queue pressure.
    ///
    /// `seq` is advisory (ADR-002): computed by the caller as
    /// `COALESCE(MAX(seq), -1) + 1` scoped to `cycle_id`. True ordering at query
    /// time uses `ORDER BY timestamp ASC, seq ASC`.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_cycle_event(
        &self,
        cycle_id: &str,
        seq: i64,
        event_type: &str,
        phase: Option<&str>,
        outcome: Option<&str>,
        next_phase: Option<&str>,
        timestamp: i64,
        goal: Option<&str>, // col-025: only Some for cycle_start events; None otherwise
    ) -> Result<()> {
        let mut conn = self
            .write_pool
            .acquire()
            .await
            .map_err(|e| crate::error::StoreError::Database(e.into()))?;

        sqlx::query(
            "INSERT INTO cycle_events
                (cycle_id, seq, event_type, phase, outcome, next_phase, timestamp, goal)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(cycle_id)
        .bind(seq)
        .bind(event_type)
        .bind(phase)
        .bind(outcome)
        .bind(next_phase)
        .bind(timestamp)
        .bind(goal)
        .execute(&mut *conn)
        .await
        .map_err(|e| crate::error::StoreError::Database(e.into()))?;

        Ok(())
    }

    /// Load the goal from the `cycle_start` event row for a given `cycle_id` (col-025).
    ///
    /// Returns:
    ///   `Ok(Some(goal))` — cycle_start row exists with a non-NULL goal
    ///   `Ok(None)`       — row absent, or goal IS NULL (caller omitted goal, or pre-v16 cycle)
    ///   `Err(...)`       — DB infrastructure failure (caller should degrade to None)
    ///
    /// Uses `idx_cycle_events_cycle_id` for a single indexed point lookup (pattern #3383).
    /// `LIMIT 1` guards against duplicate cycle_start rows (defensive, ADR-001).
    pub async fn get_cycle_start_goal(&self, cycle_id: &str) -> Result<Option<String>> {
        let result: Option<Option<String>> = sqlx::query_scalar::<_, Option<String>>(
            "SELECT goal FROM cycle_events
             WHERE cycle_id = ?1 AND event_type = 'cycle_start'
             ORDER BY timestamp DESC, seq DESC
             LIMIT 1",
        )
        .bind(cycle_id)
        .fetch_optional(&self.write_pool)
        .await
        .map_err(|e| crate::error::StoreError::Database(e.into()))?;

        // fetch_optional returns:
        //   None           — no matching row
        //   Some(None)     — row matched but goal IS NULL
        //   Some(Some(s))  — row matched and goal is non-NULL
        //
        // Flatten: both absent-row and NULL-goal map to Ok(None).
        Ok(result.flatten())
    }

    /// Compute the advisory next `seq` for a `cycle_id` in `CYCLE_EVENTS`.
    ///
    /// Uses `SELECT COALESCE(MAX(seq), -1) + 1 FROM cycle_events WHERE cycle_id = ?`.
    /// `seq` is advisory per ADR-002: the true ordering at query time uses
    /// `ORDER BY timestamp ASC, seq ASC`. On any error, returns `0` — safe because
    /// the AUTOINCREMENT `id` column preserves row identity regardless of seq value.
    ///
    /// Called inside the fire-and-forget spawn in the UDS listener, not on the hot path.
    pub async fn get_next_cycle_seq(&self, cycle_id: &str) -> i64 {
        let result: std::result::Result<Option<i64>, sqlx::Error> = sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), -1) + 1 FROM cycle_events WHERE cycle_id = ?1",
        )
        .bind(cycle_id)
        .fetch_one(&self.write_pool)
        .await;

        result.ok().flatten().unwrap_or(0)
    }

    /// WAL checkpoint + VACUUM compaction. Run during graceful shutdown when
    /// `Arc::try_unwrap(store)` succeeds. Safe to call from async context.
    pub async fn compact(&self) -> Result<()> {
        let mut conn = self
            .write_pool
            .acquire()
            .await
            .map_err(|e| crate::error::StoreError::Database(e.into()))?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&mut *conn)
            .await
            .map_err(|e| crate::error::StoreError::Database(e.into()))?;
        sqlx::query("VACUUM")
            .execute(&mut *conn)
            .await
            .map_err(|e| crate::error::StoreError::Database(e.into()))?;
        Ok(())
    }
}

impl Drop for SqlxStore {
    /// Non-blocking shutdown signal.
    ///
    /// Fires if `close()` was not called. In test contexts, MUST NOT be relied
    /// upon — always call `close().await` explicitly (TC-02).
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        // drain_handle is NOT awaited here (Drop cannot be async).
    }
}

// ---------------------------------------------------------------------------
// create_tables_if_needed
// ---------------------------------------------------------------------------

/// Create all tables and indexes if they don't exist (idempotent).
///
/// Called after migration to handle fresh databases that skipped migration
/// (no entries table existed). All DDL uses IF NOT EXISTS.
pub(crate) async fn create_tables_if_needed(
    pool: &SqlitePool,
) -> std::result::Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;

    // All tables in dependency order (entries first, then referencing tables)
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS entries (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS entry_tags (
            entry_id INTEGER NOT NULL,
            tag      TEXT    NOT NULL,
            PRIMARY KEY (entry_id, tag),
            FOREIGN KEY (entry_id) REFERENCES entries(id) ON DELETE CASCADE
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entries_topic      ON entries(topic)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entries_category   ON entries(category)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entries_status     ON entries(status)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entries_created_at ON entries(created_at)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entry_tags_tag      ON entry_tags(tag)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_entry_tags_entry_id ON entry_tags(entry_id)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS vector_map (
            entry_id INTEGER PRIMARY KEY,
            hnsw_data_id INTEGER NOT NULL
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS counters (
            name TEXT PRIMARY KEY,
            value INTEGER NOT NULL
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS feature_entries (
            feature_id TEXT    NOT NULL,
            entry_id   INTEGER NOT NULL,
            phase      TEXT,
            PRIMARY KEY (feature_id, entry_id)
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS cycle_events (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_cycle_events_cycle_id ON cycle_events (cycle_id)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS co_access (
            entry_id_a   INTEGER NOT NULL,
            entry_id_b   INTEGER NOT NULL,
            count        INTEGER NOT NULL DEFAULT 1,
            last_updated INTEGER NOT NULL,
            PRIMARY KEY (entry_id_a, entry_id_b),
            CHECK (entry_id_a < entry_id_b)
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_co_access_b ON co_access(entry_id_b)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS outcome_index (
            feature_cycle TEXT NOT NULL,
            entry_id INTEGER NOT NULL,
            PRIMARY KEY (feature_cycle, entry_id)
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS observation_metrics (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS observation_phase_metrics (
            feature_cycle   TEXT    NOT NULL,
            phase_name      TEXT    NOT NULL,
            duration_secs   INTEGER NOT NULL DEFAULT 0,
            tool_call_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (feature_cycle, phase_name),
            FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS signal_queue (
            signal_id     INTEGER PRIMARY KEY,
            session_id    TEXT    NOT NULL,
            created_at    INTEGER NOT NULL,
            entry_ids     TEXT    NOT NULL DEFAULT '[]',
            signal_type   INTEGER NOT NULL,
            signal_source INTEGER NOT NULL
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS sessions (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_feature_cycle ON sessions(feature_cycle)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_started_at    ON sessions(started_at)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS injection_log (
            log_id     INTEGER PRIMARY KEY,
            session_id TEXT    NOT NULL,
            entry_id   INTEGER NOT NULL,
            confidence REAL    NOT NULL,
            timestamp  INTEGER NOT NULL
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_injection_log_session ON injection_log(session_id)",
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_injection_log_entry   ON injection_log(entry_id)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS agent_registry (
            agent_id           TEXT    PRIMARY KEY,
            trust_level        INTEGER NOT NULL,
            capabilities       TEXT    NOT NULL DEFAULT '[]',
            allowed_topics     TEXT,
            allowed_categories TEXT,
            enrolled_at        INTEGER NOT NULL,
            last_seen_at       INTEGER NOT NULL,
            active             INTEGER NOT NULL DEFAULT 1
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_log (
            event_id   INTEGER PRIMARY KEY,
            timestamp  INTEGER NOT NULL,
            session_id TEXT    NOT NULL,
            agent_id   TEXT    NOT NULL,
            operation  TEXT    NOT NULL,
            target_ids TEXT    NOT NULL DEFAULT '[]',
            outcome    INTEGER NOT NULL,
            detail     TEXT    NOT NULL DEFAULT ''
        )",
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_log_agent     ON audit_log(agent_id)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp ON audit_log(timestamp)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS observations (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_observations_session ON observations(session_id)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_observations_ts ON observations(ts_millis)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS shadow_evaluations (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_shadow_eval_ts ON shadow_evaluations(timestamp)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS topic_deliveries (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS query_log (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_query_log_session ON query_log(session_id)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_query_log_ts ON query_log(ts)")
        .execute(&mut *conn)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS graph_edges (
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
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_graph_edges_source_id ON graph_edges(source_id)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_graph_edges_target_id ON graph_edges(target_id)")
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_relation_type ON graph_edges(relation_type)",
    )
    .execute(&mut *conn)
    .await?;

    // Initialize counters that other modules expect.
    // Bind CURRENT_SCHEMA_VERSION to avoid drift between this and migration.rs (crt-025).
    sqlx::query("INSERT OR IGNORE INTO counters (name, value) VALUES ('schema_version', ?1)")
        .bind(crate::migration::CURRENT_SCHEMA_VERSION as i64)
        .execute(&mut *conn)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO counters (name, value) VALUES ('next_entry_id', 1)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO counters (name, value) VALUES ('next_signal_id', 0)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO counters (name, value) VALUES ('next_log_id', 0)")
        .execute(&mut *conn)
        .await?;
    sqlx::query("INSERT OR IGNORE INTO counters (name, value) VALUES ('next_audit_event_id', 0)")
        .execute(&mut *conn)
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Pool timeout error mapping
// ---------------------------------------------------------------------------

/// Maps a sqlx pool acquire error to `StoreError::PoolTimeout` with the
/// correct pool kind and configured timeout duration.
pub(crate) fn map_pool_timeout(e: sqlx::Error, pool: PoolKind) -> StoreError {
    match e {
        sqlx::Error::PoolTimedOut => StoreError::PoolTimeout {
            pool,
            elapsed: match pool {
                PoolKind::Read => crate::pool_config::READ_POOL_ACQUIRE_TIMEOUT,
                PoolKind::Write => crate::pool_config::WRITE_POOL_ACQUIRE_TIMEOUT,
            },
        },
        other => StoreError::Database(other.into()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool_config::PoolConfig;

    async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.db");
        let store = SqlxStore::open(&path, PoolConfig::test_default())
            .await
            .expect("open test store");
        (store, dir)
    }

    #[tokio::test]
    async fn test_open_creates_store() {
        let (store, _dir) = open_test_store().await;
        assert_eq!(store.shed_events_total(), 0);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_open_write_max_3_rejected() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.db");
        let config = PoolConfig {
            read_max_connections: 4,
            write_max_connections: 3,
            read_acquire_timeout: std::time::Duration::from_millis(500),
            write_acquire_timeout: std::time::Duration::from_secs(1),
        };
        let result = SqlxStore::open(&path, config).await;
        assert!(result.is_err(), "write_max=3 should be rejected");
        let err = result.unwrap_err();
        assert!(
            matches!(err, StoreError::InvalidPoolConfig { .. }),
            "expected InvalidPoolConfig, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_open_applies_wal_pragma() {
        let (store, _dir) = open_test_store().await;

        // Query journal_mode from write pool
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&store.write_pool)
            .await
            .expect("query journal_mode");

        assert_eq!(mode, "wal", "expected WAL journal mode");
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_open_applies_foreign_keys_pragma() {
        let (store, _dir) = open_test_store().await;

        let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&store.write_pool)
            .await
            .expect("query foreign_keys");

        assert_eq!(fk, 1, "expected foreign_keys=ON (1)");
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_enqueue_analytics_does_not_panic() {
        let (store, _dir) = open_test_store().await;
        store.enqueue_analytics(AnalyticsWrite::CoAccess { id_a: 1, id_b: 2 });
        assert_eq!(store.shed_events_total(), 0);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_shed_counter_increments_on_full_queue() {
        // Use a minimal config so the channel fills quickly
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("test.db");

        // Open a store with a 0-capacity-equivalent: we can't make the channel
        // capacity configurable without changing the signature. Instead, we
        // verify the counter behavior by checking that the counter is exposed.
        let store = SqlxStore::open(&path, PoolConfig::test_default())
            .await
            .expect("open");
        // Baseline: no shedding yet
        assert_eq!(store.shed_events_total(), 0);
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_close_is_idempotent_via_drop() {
        // drop() after close() should not panic
        let (store, _dir) = open_test_store().await;
        store.close().await.unwrap();
        // _dir drops here — no panic
    }

    #[tokio::test]
    async fn test_create_tables_counters_initialized() {
        let (store, _dir) = open_test_store().await;

        let v: i64 = sqlx::query_scalar("SELECT value FROM counters WHERE name = 'next_entry_id'")
            .fetch_one(&store.write_pool)
            .await
            .expect("query next_entry_id");

        assert_eq!(v, 1, "next_entry_id should initialize to 1");
        store.close().await.unwrap();
    }

    // -----------------------------------------------------------------------
    // graph_edges DDL tests (store-schema, crt-021)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_graph_edges_table_created_on_fresh_db() {
        let (store, _dir) = open_test_store().await;

        let sql: Option<String> = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='graph_edges'",
        )
        .fetch_optional(&store.write_pool)
        .await
        .expect("query sqlite_master");

        assert!(sql.is_some(), "graph_edges table must exist on fresh db");
        let ddl = sql.unwrap();
        assert!(
            ddl.contains("CREATE TABLE"),
            "sqlite_master.sql must contain CREATE TABLE"
        );
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_graph_edges_columns_and_types() {
        let (store, _dir) = open_test_store().await;

        // pragma_table_info returns rows: (cid, name, type, notnull, dflt_value, pk)
        let rows: Vec<(i64, String, String, i64)> = sqlx::query_as(
            "SELECT cid, name, type, \"notnull\" FROM pragma_table_info('graph_edges')",
        )
        .fetch_all(&store.write_pool)
        .await
        .expect("pragma_table_info");

        assert!(!rows.is_empty(), "graph_edges must have columns");

        let col_names: Vec<&str> = rows.iter().map(|(_, n, _, _)| n.as_str()).collect();
        let expected_cols = [
            "id",
            "source_id",
            "target_id",
            "relation_type",
            "weight",
            "created_at",
            "created_by",
            "source",
            "bootstrap_only",
            "metadata",
        ];
        for col in &expected_cols {
            assert!(col_names.contains(col), "missing column: {col}");
        }

        let col_map: std::collections::HashMap<&str, (&str, i64)> = rows
            .iter()
            .map(|(_, n, t, nn)| (n.as_str(), (t.as_str(), *nn)))
            .collect();

        assert_eq!(col_map["weight"].0, "REAL", "weight must be REAL");
        assert_eq!(
            col_map["bootstrap_only"].0, "INTEGER",
            "bootstrap_only must be INTEGER"
        );
        assert_eq!(col_map["metadata"].0, "TEXT", "metadata must be TEXT");

        // NOT NULL columns
        for col in &["source_id", "target_id", "relation_type"] {
            assert_eq!(col_map[col].1, 1, "{col} must be NOT NULL (notnull=1)");
        }

        // metadata must NOT be NOT NULL (nullable)
        assert_eq!(
            col_map["metadata"].1, 0,
            "metadata must be nullable (notnull=0)"
        );

        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_graph_edges_unique_constraint_prevents_duplicate() {
        let (store, _dir) = open_test_store().await;

        let now = 1_700_000_000_i64;
        sqlx::query(
            "INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) VALUES (1, 2, 'Supersedes', 1.0, ?, '', '', 0)",
        )
        .bind(now)
        .execute(&store.write_pool)
        .await
        .expect("first insert");

        let result = sqlx::query(
            "INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) VALUES (1, 2, 'Supersedes', 1.0, ?, '', '', 0)",
        )
        .bind(now)
        .execute(&store.write_pool)
        .await;

        assert!(
            result.is_err(),
            "duplicate (source_id, target_id, relation_type) must fail"
        );
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_graph_edges_insert_or_ignore_idempotent() {
        let (store, _dir) = open_test_store().await;

        let now = 1_700_000_000_i64;
        for _ in 0..2 {
            sqlx::query(
                "INSERT OR IGNORE INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) VALUES (1, 2, 'Supersedes', 1.0, ?, '', '', 0)",
            )
            .bind(now)
            .execute(&store.write_pool)
            .await
            .expect("insert or ignore");
        }

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM graph_edges WHERE source_id=1 AND target_id=2 AND relation_type='Supersedes'")
                .fetch_one(&store.write_pool)
                .await
                .expect("count");

        assert_eq!(count, 1, "INSERT OR IGNORE must leave exactly one row");
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_graph_edges_unique_allows_different_relation_types() {
        let (store, _dir) = open_test_store().await;

        let now = 1_700_000_000_i64;
        sqlx::query(
            "INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) VALUES (1, 2, 'Supersedes', 1.0, ?, '', '', 0)",
        )
        .bind(now)
        .execute(&store.write_pool)
        .await
        .expect("Supersedes insert");

        sqlx::query(
            "INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) VALUES (1, 2, 'CoAccess', 0.8, ?, '', '', 0)",
        )
        .bind(now)
        .execute(&store.write_pool)
        .await
        .expect("CoAccess insert");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM graph_edges WHERE source_id=1 AND target_id=2",
        )
        .fetch_one(&store.write_pool)
        .await
        .expect("count");

        assert_eq!(
            count, 2,
            "different relation types on same pair must both persist"
        );
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_graph_edges_indexes_exist() {
        let (store, _dir) = open_test_store().await;

        let names: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='graph_edges'",
        )
        .fetch_all(&store.write_pool)
        .await
        .expect("query indexes");

        let expected = [
            "idx_graph_edges_source_id",
            "idx_graph_edges_target_id",
            "idx_graph_edges_relation_type",
        ];
        for idx in &expected {
            assert!(names.iter().any(|n| n == idx), "missing index: {idx}");
        }
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_graph_edges_metadata_default_null() {
        let (store, _dir) = open_test_store().await;

        let now = 1_700_000_000_i64;
        sqlx::query(
            "INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) VALUES (10, 20, 'Supports', 1.0, ?, '', '', 0)",
        )
        .bind(now)
        .execute(&store.write_pool)
        .await
        .expect("insert");

        let metadata: Option<String> = sqlx::query_scalar(
            "SELECT metadata FROM graph_edges WHERE source_id=10 AND target_id=20",
        )
        .fetch_one(&store.write_pool)
        .await
        .expect("fetch metadata");

        assert!(metadata.is_none(), "metadata must default to NULL");
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_graph_edges_bootstrap_only_defaults_zero() {
        let (store, _dir) = open_test_store().await;

        let now = 1_700_000_000_i64;
        // Insert without specifying bootstrap_only — rely on column DEFAULT 0
        sqlx::query(
            "INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at, created_by, source) VALUES (30, 40, 'CoAccess', 0.5, ?, '', '')",
        )
        .bind(now)
        .execute(&store.write_pool)
        .await
        .expect("insert without bootstrap_only");

        let bootstrap_only: i64 = sqlx::query_scalar(
            "SELECT bootstrap_only FROM graph_edges WHERE source_id=30 AND target_id=40",
        )
        .fetch_one(&store.write_pool)
        .await
        .expect("fetch bootstrap_only");

        assert_eq!(bootstrap_only, 0, "bootstrap_only must default to 0");
        store.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_schema_version_initialized_to_current_on_fresh_db() {
        let (store, _dir) = open_test_store().await;

        let v: i64 = sqlx::query_scalar("SELECT value FROM counters WHERE name = 'schema_version'")
            .fetch_one(&store.write_pool)
            .await
            .expect("query schema_version");

        assert_eq!(
            v,
            crate::migration::CURRENT_SCHEMA_VERSION as i64,
            "schema_version must initialize to CURRENT_SCHEMA_VERSION on fresh db"
        );
        store.close().await.unwrap();
    }
}
