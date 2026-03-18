# Component: SqlxStore (db.rs)
## File: `crates/unimatrix-store/src/db.rs` (rewrite)

---

## Purpose

`SqlxStore` is the concrete storage struct that replaces `Store { conn: Mutex<Connection> }`.
It owns both pools, the analytics queue, the drain task, and the shed counter. All store
methods become `async fn`. This is the central component of the migration — all other
components either provide inputs to this struct or consume it.

This file also drives the rewrite of `write.rs`, `read.rs`, `sessions.rs`, `injection_log.rs`,
`query_log.rs`, `signal.rs`, `topic_deliveries.rs`, `counters.rs`, and `metrics.rs` since
all those files implement methods that are called on `SqlxStore`. The pseudocode for those
method modules is included here.

---

## Data Structure

```rust
pub struct SqlxStore {
    /// Max 6–8 connections. Applied with .read_only(true) via SqliteConnectOptions
    /// (defense-in-depth — see OQ-DURING-01 if this causes WAL checkpoint issues).
    read_pool:    SqlitePool,

    /// Max ≤ 2 connections. Serves integrity writes and the drain task.
    write_pool:   SqlitePool,

    /// Sender half of the bounded analytics channel (capacity 1000).
    /// Held by SqlxStore; clone not exposed outside.
    analytics_tx: mpsc::Sender<AnalyticsWrite>,

    /// Shutdown signal for the drain task. Option so Drop can take it without Clone.
    shutdown_tx:  Option<oneshot::Sender<()>>,

    /// Drain task join handle. Option so close() can take and await it.
    drain_handle: Option<JoinHandle<()>>,

    /// Cumulative count of shed analytics write events since open().
    /// Monotonically increasing. Exposed via shed_events_total().
    shed_counter: Arc<AtomicU64>,
}
```

The `write_pool` field must be accessible from within the same crate for the 5 call sites
that use `pool.begin().await` directly (ADR-002). Use `pub(crate) write_pool` or expose
a `pub(crate) fn write_pool(&self) -> &SqlitePool` accessor.

---

## Construction: `SqlxStore::open`

```rust
impl SqlxStore {
    pub async fn open(
        path: impl AsRef<Path>,
        config: PoolConfig,
    ) -> Result<SqlxStore, StoreError> {
        // 1. Validate config before touching the database.
        config.validate()?;  // Returns StoreError::InvalidPoolConfig if write_max > 2

        let db_path = path.as_ref();

        // 2. Open dedicated non-pooled migration connection (ADR-003).
        let migration_url = format!(
            "sqlite://{}",
            db_path.to_str().ok_or_else(|| StoreError::InvalidPoolConfig {
                reason: "database path is not valid UTF-8".to_string(),
            })?
        );
        let mut migration_conn = sqlx::SqliteConnection::connect(&migration_url)
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 3. Apply all 6 PRAGMAs to migration connection.
        apply_pragmas_to_connection(&mut migration_conn)
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 4. Run migration. On failure: migration_conn dropped, return error.
        //    Pool construction does NOT proceed.
        migrate_if_needed(&mut migration_conn, db_path)
            .await
            .map_err(|e| StoreError::Migration { source: e.into() })?;

        // 5. Drop migration connection BEFORE constructing pools (ADR-003).
        drop(migration_conn);

        // 6. Build SqliteConnectOptions with all 6 PRAGMAs for pool connections.
        let opts = build_connect_options(db_path);

        // 7. Construct read_pool.
        // OQ-DURING-01: if read_only(true) prevents WAL checkpoint, remove it.
        let read_pool = SqlitePoolOptions::new()
            .max_connections(config.read_max_connections)
            .acquire_timeout(config.read_acquire_timeout)
            .connect_with(opts.clone().read_only(true))
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 8. Construct write_pool.
        let write_pool = SqlitePoolOptions::new()
            .max_connections(config.write_max_connections)
            .acquire_timeout(config.write_acquire_timeout)
            .connect_with(opts)
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 9. Run create_tables (idempotent) to ensure fresh databases have all tables.
        //    Fresh databases skipped migration (no entries table) but still need tables.
        create_tables_if_needed(&write_pool)
            .await
            .map_err(|e| StoreError::Open(e.into()))?;

        // 10. Create analytics queue channel and shed counter.
        let (analytics_tx, analytics_rx) = mpsc::channel(ANALYTICS_QUEUE_CAPACITY);
        let shed_counter = Arc::new(AtomicU64::new(0));
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // 11. Spawn drain task. Owned by SqlxStore via drain_handle.
        let drain_pool = write_pool.clone();
        let drain_handle = tokio::spawn(run_drain_task(analytics_rx, shutdown_rx, drain_pool));

        Ok(SqlxStore {
            read_pool,
            write_pool,
            analytics_tx,
            shutdown_tx: Some(shutdown_tx),
            drain_handle: Some(drain_handle),
            shed_counter,
        })
    }
}
```

Note on `create_tables_if_needed`: This is the existing `create_tables()` function adapted
to accept `&SqlitePool` and execute via a write_pool connection. It is idempotent (all
CREATE TABLE IF NOT EXISTS). It runs after migration to handle fresh databases.

---

## Teardown: `close` and `Drop`

```rust
impl SqlxStore {
    /// Graceful async teardown. Signals drain task, awaits exit with timeout.
    /// Call in every test and in server shutdown. (TC-02)
    pub async fn close(mut self) {
        // Send shutdown signal to drain task.
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Await drain task with grace period.
        if let Some(handle) = self.drain_handle.take() {
            match tokio::time::timeout(DRAIN_SHUTDOWN_TIMEOUT, handle).await {
                Ok(Ok(())) => {}  // Clean exit
                Ok(Err(e)) if e.is_panic() => {
                    tracing::error!("drain task panicked during shutdown: {:?}", e);
                    // DrainTaskPanic is logged but close() returns normally —
                    // the caller cannot do much about a panic.
                }
                Ok(Err(_)) => {}  // JoinError (cancelled or other)
                Err(_elapsed) => {
                    tracing::warn!(
                        timeout_secs = DRAIN_SHUTDOWN_TIMEOUT.as_secs(),
                        "drain task did not exit within grace period; continuing with close"
                    );
                }
            }
        }

        // Pools are dropped here via SqlxStore's Drop. All connections are released.
        // Explicit close is not needed — SqlitePool implements Drop by closing connections.
    }
}

impl Drop for SqlxStore {
    /// Non-blocking shutdown signal. Fires if close() was not called.
    /// In test contexts, MUST NOT be relied upon — always call close().await explicitly.
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            // try_send would fail if drain task already exited; that's fine.
            let _ = tx.send(());
        }
        // drain_handle is NOT awaited here (Drop cannot be async).
        // If the runtime is still live, the drain task receives the signal and completes.
        // If the runtime has already stopped, the task is cancelled — analytics data in flight
        // may be lost. This is why TC-02 mandates Store::close().await in every test.
    }
}
```

---

## Analytics Queue Methods

```rust
impl SqlxStore {
    /// Fire-and-forget analytics enqueue. Non-async; uses try_send semantics.
    /// If queue is full: shed event logged, shed counter incremented. No error propagated.
    /// NEVER call this for integrity table writes (entries, entry_tags, audit_log, etc.).
    pub fn enqueue_analytics(&self, event: AnalyticsWrite) {
        match self.analytics_tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(e)) => {
                let total = self.shed_counter.fetch_add(1, Ordering::Relaxed) + 1;
                tracing::warn!(
                    variant = e.variant_name(),
                    queue_capacity = ANALYTICS_QUEUE_CAPACITY,
                    shed_total = total,
                    "analytics write shed: queue at capacity"
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Drain task exited (shutdown in progress). Discard silently.
            }
        }
    }

    /// Returns cumulative count of shed analytics events since open().
    /// Used by context_status (FR-16, AC-18).
    pub fn shed_events_total(&self) -> u64 {
        self.shed_counter.load(Ordering::Relaxed)
    }
}
```

---

## Write Methods (write.rs, counters.rs, sessions.rs write paths)

All write methods targeting integrity tables use `write_pool` directly. All methods
targeting analytics tables call `enqueue_analytics()`.

### Pattern for integrity write (write.rs):

```rust
impl SqlxStore {
    /// Example: insert a new entry (integrity write via write_pool).
    pub async fn write_entry(&self, entry: NewEntry) -> Result<u64, StoreError> {
        let mut conn = self.write_pool.acquire().await
            .map_err(|e| map_pool_timeout(e, PoolKind::Write))?;

        // Use sqlx::query!() macro for compile-time SQL validation.
        let id: i64 = sqlx::query_scalar!(
            "INSERT INTO entries (title, content, topic, category, source, status,
              confidence, created_at, updated_at, last_accessed_at, access_count,
              supersedes, superseded_by, correction_count, embedding_dim, created_by,
              modified_by, content_hash, previous_hash, version, feature_cycle,
              trust_source, helpful_count, unhelpful_count, pre_quarantine_status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
             RETURNING id",
            entry.title, entry.content, /* ... all fields ... */
        )
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        Ok(id as u64)
    }
}
```

### Pattern for analytics write (sessions.rs, injection_log.rs, etc.):

```rust
    /// Example: log an injection (analytics write via enqueue_analytics).
    pub fn log_injection(&self, session_id: &str, entry_id: u64, confidence: f64) {
        let now = current_unix_seconds();
        self.enqueue_analytics(AnalyticsWrite::InjectionLog {
            session_id: session_id.to_owned(),
            entry_id,
            confidence,
            timestamp: now,
        });
        // Returns () — fire and forget. No await needed.
    }
```

---

## Read Methods (read.rs, counters.rs, query_log.rs read paths)

All read methods use `read_pool`. No `Mutex::lock()`, no `spawn_blocking`.

### Pattern for read (read.rs):

```rust
impl SqlxStore {
    pub async fn get_entry(&self, id: u64) -> Result<EntryRecord, StoreError> {
        let conn = self.read_pool.acquire().await
            .map_err(|e| map_pool_timeout(e, PoolKind::Read))?;

        let row = sqlx::query_as!(
            EntryRow,
            "SELECT id, title, content, topic, category, source, status, confidence,
                    created_at, updated_at, last_accessed_at, access_count,
                    supersedes, superseded_by, correction_count, embedding_dim,
                    created_by, modified_by, content_hash, previous_hash, version,
                    feature_cycle, trust_source, helpful_count, unhelpful_count,
                    pre_quarantine_status
             FROM entries WHERE id = ?1",
            id as i64
        )
        .fetch_optional(&*conn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?
        .ok_or(StoreError::NotFound(id))?;

        Ok(row.into())
    }
}
```

---

## Pool Timeout Error Mapping

```rust
/// Maps sqlx pool timeout error to StoreError::PoolTimeout.
fn map_pool_timeout(e: sqlx::Error, pool: PoolKind) -> StoreError {
    match e {
        sqlx::Error::PoolTimedOut => StoreError::PoolTimeout {
            pool,
            elapsed: match pool {
                PoolKind::Read  => READ_POOL_ACQUIRE_TIMEOUT,
                PoolKind::Write => WRITE_POOL_ACQUIRE_TIMEOUT,
            },
        },
        other => StoreError::Database(other.into()),
    }
}
```

---

## StoreError New Variants (error.rs)

```rust
// Add to existing StoreError enum in crates/unimatrix-store/src/error.rs:

/// write_pool max_connections > 2, or other pool parameter invalid.
InvalidPoolConfig { reason: String },

/// Pool acquire_timeout elapsed before a connection was available.
PoolTimeout { pool: PoolKind, elapsed: Duration },

/// migrate_if_needed() failed. Pool construction did not proceed.
Migration { source: Box<dyn std::error::Error + Send + Sync> },

/// drain_handle.join() resolved with a panic (JoinError::is_panic()).
DrainTaskPanic,

// Remove: Sqlite(rusqlite::Error)
// Keep all other existing variants.
```

```rust
/// Identifies which pool caused a PoolTimeout error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolKind {
    Read,
    Write,
}
```

---

## lib.rs Changes

```rust
// Remove from lib.rs:
pub use rusqlite;                         // Remove this re-export entirely (FR-13)

// Keep (or add) exports:
pub use pool_config::{PoolConfig, READ_POOL_ACQUIRE_TIMEOUT, WRITE_POOL_ACQUIRE_TIMEOUT};
pub use analytics::{AnalyticsWrite, ANALYTICS_QUEUE_CAPACITY};
pub use db::SqlxStore;
pub use error::{StoreError, PoolKind};
```

---

## test_helpers.rs (async rewrite)

```rust
// Replace all sync helpers with async counterparts.

/// Opens a SqlxStore at a temporary path using PoolConfig::test_default().
pub async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)  // caller must call store.close().await before dropping
}

/// Standard teardown for all test stores. TC-02.
pub async fn close_test_store(store: SqlxStore) {
    store.close().await;
}
```

---

## Error Handling Summary

| Error | When | Action |
|-------|------|--------|
| `StoreError::InvalidPoolConfig` | `config.validate()` fails | Propagate to caller; no DB touched |
| `StoreError::Open` | connection or pool construction fails | Propagate; server does not start |
| `StoreError::Migration` | `migrate_if_needed` fails | Propagate; pool construction skipped |
| `StoreError::PoolTimeout` | pool acquire times out | Propagate to MCP tool handler |
| `StoreError::DrainTaskPanic` | drain JoinHandle is_panic | Log error; close() returns normally |
| `StoreError::Database` | sqlx query error on hot path | Propagate to MCP tool handler |

---

## Key Test Scenarios

1. **`test_open_applies_all_6_pragmas`** (AC-02): Open store; query `PRAGMA journal_mode`,
   `PRAGMA foreign_keys` from both pools; assert correct values.

2. **`test_open_write_max_3_rejected`** (AC-09, R-01): `PoolConfig { write_max: 3 }`; assert
   `StoreError::InvalidPoolConfig` before any connection opened.

3. **`test_pool_timeout_write_pool`** (R-01, AC-10): Saturate write pool; assert new writer
   receives `StoreError::PoolTimeout { pool: PoolKind::Write, .. }` within timeout.

4. **`test_close_awaits_drain_exit`** (R-02, AC-19): Enqueue 10 events; call `close().await`;
   assert all 10 rows committed; assert `drain_handle` has exited.

5. **`test_close_grace_period_exceeded`** (R-02): Inject hung drain task; assert `close()`
   returns within `DRAIN_SHUTDOWN_TIMEOUT + margin` and emits WARNING log.

6. **`test_integrity_write_never_shed`** (R-06, AC-08): Fill analytics queue to 1000;
   call `write_entry()`; assert write succeeds.

7. **`test_shed_counter_readable`** (R-04, AC-18): Induce N shed events;
   assert `shed_events_total() == N`.

8. **`test_pragma_per_connection`** (R-11): Open pool with min_connections=0; trigger
   lazy connection creation; query PRAGMAs from the lazily created connection; assert correct.

---

## OQ-DURING Items Affecting This Component

- **OQ-DURING-01** (read_only + WAL checkpoint): If WAL file grows unboundedly during
  integration testing, remove `.read_only(true)` from read pool options. Safe to do without
  ADR revision; routing already prevents writes through read_pool at code level.
- **OQ-DURING-02** (drain shutdown timeout): If test contexts need shorter grace period,
  add `drain_shutdown_timeout: Duration` field to `PoolConfig`. Current: constant only.
