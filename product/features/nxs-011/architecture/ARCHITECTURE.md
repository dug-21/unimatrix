# nxs-011: sqlx Migration — Architecture

## System Overview

nxs-011 replaces the `rusqlite 0.34` + `Mutex<Connection>` storage layer with `sqlx` and a
dual-pool architecture. It eliminates 101 `spawn_blocking` call sites across the server crate,
retires the `AsyncEntryStore` bridge, and makes the entire storage layer async-native by
exploiting SQLite WAL's true concurrent read capability.

The migration is a transport layer change only. Schema version stays at 12. No new tables or
columns are introduced. The public behavioral contract of every store method is preserved —
only the mechanism (sync mutex → async pool) and call convention (blocking → `.await`) change.

This feature is a prerequisite for all Wave 1 features (NLI, graph edges, confidence weight
updates) because each of those adds analytics write patterns that would compound the existing
`spawn_blocking` debt if built on top of the current layer.

### Position in the Unimatrix System

```
MCP Server (unimatrix-server)
  └── tools.rs / background.rs / server.rs     [101 spawn_blocking sites → zero]
        └── SqlxStore (unimatrix-store)          [replaces Store{Mutex<Connection>}]
              ├── read_pool: SqlitePool          [6-8 connections, WAL concurrent reads]
              ├── write_pool: SqlitePool         [≤2 connections, integrity writes + drain]
              └── AnalyticsQueue                 [bounded mpsc, drain task, shed counter]

unimatrix-core
  └── traits.rs EntryStore                      [18 sync methods → 18 async fn (RPITIT)]
  └── async_wrappers.rs AsyncEntryStore         [retired entirely]

unimatrix-observe
  └── dead_knowledge.rs                         [lock_conn() → async sqlx query]
```

---

## Component Breakdown

### 1. SqlxStore (unimatrix-store/src/db.rs)

The concrete store struct. Replaces `Store { conn: Mutex<Connection> }`.

**Responsibilities:**
- Owns both connection pools and their lifecycle
- Provides all async store methods (read, integrity write, analytics enqueue)
- Owns the drain task, its shutdown channel, and the shed counter
- Exposes `shed_counter()` for `context_status` observability (SR-08)

**Fields:**
```rust
pub struct SqlxStore {
    read_pool:    SqlitePool,
    write_pool:   SqlitePool,
    analytics_tx: mpsc::Sender<AnalyticsWrite>,
    shutdown_tx:  Option<oneshot::Sender<()>>,    // Option so Drop can take it
    drain_handle: Option<JoinHandle<()>>,          // Option so close() can await it
    shed_counter: Arc<AtomicU64>,
}
```

`shutdown_tx` and `drain_handle` are `Option` so that `Drop` can take ownership to send the
signal and `close()` can await the handle. Using `Option<oneshot::Sender>` is idiomatic for
this pattern and avoids a `Mutex<Option<...>>`.

**Construction:** `Store::open(path: impl AsRef<Path>, config: PoolConfig) -> Result<SqlxStore>`
(async). Sequence documented in FR-08 / ADR-003.

**Teardown:**
- `impl Drop for SqlxStore`: attempts to send shutdown signal via `shutdown_tx.take()`. Does
  not block. If the tokio runtime is still live, the drain task receives the signal and
  completes its final batch. In test contexts where Drop fires outside a tokio context,
  the signal send succeeds on the channel but the drain task continues to completion on the
  runtime that spawned it.
- `async fn close(mut self)`: sends shutdown signal, awaits `drain_handle` with a 5s
  timeout. This is the correct teardown path for all tests and server shutdown. See ADR-003.

---

### 2. PoolConfig (unimatrix-store/src/pool_config.rs)

Configuration struct passed to `Store::open()`. Encodes all pool parameters.

```rust
pub struct PoolConfig {
    pub read_max_connections:  u32,     // 1–8; recommended 6–8
    pub write_max_connections: u32,     // 1–2; > 2 rejected at startup (AC-09)
    pub read_acquire_timeout:  Duration,
    pub write_acquire_timeout: Duration,
}

impl PoolConfig {
    pub fn default() -> Self { /* read=8, write=2, timeouts per ADR-001 */ }
    pub fn test_default() -> Self { /* read=2, write=1, shorter timeouts */ }
}
```

Validation at `Store::open()`: if `write_max_connections > 2`, return
`Err(StoreError::InvalidPoolConfig)` before any connection is opened.

**PRAGMAs** are applied via `SqliteConnectOptions::pragma()` per-connection on pool
construction, not in a separate step. This ensures every connection in the pool — including
connections lazily opened after pool construction — has the correct configuration.

```rust
fn build_connect_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL")
        .pragma("wal_autocheckpoint", "1000")
        .pragma("foreign_keys", "ON")
        .pragma("busy_timeout", "5000")
        .pragma("cache_size", "-16384")
        .create_if_missing(true)
}
```

---

### 3. AnalyticsWrite Enum + AnalyticsQueue (unimatrix-store/src/analytics.rs)

**AnalyticsWrite enum:**

```rust
#[non_exhaustive]
pub enum AnalyticsWrite {
    CoAccess { id_a: u64, id_b: u64 },
    SessionUpdate {
        session_id: String,
        feature_cycle: Option<String>,
        agent_role: Option<String>,
        started_at: i64,
        ended_at: Option<i64>,
        status: i64,
        compaction_count: i64,
        outcome: Option<String>,
        total_injections: i64,
        keywords: Option<String>,
    },
    InjectionLog {
        session_id: String,
        entry_id: u64,
        confidence: f64,
        timestamp: i64,
    },
    QueryLog {
        session_id: String,
        query_text: String,
        ts: i64,
        result_count: i64,
        result_entry_ids: Option<String>,
        similarity_scores: Option<String>,
        retrieval_mode: Option<String>,
        source: String,
    },
    SignalQueue {
        session_id: String,
        created_at: i64,
        entry_ids: String,
        signal_type: i64,
        signal_source: i64,
    },
    Observation {
        session_id: String,
        ts_millis: i64,
        hook: String,
        tool: Option<String>,
        input: Option<String>,
        response_size: Option<i64>,
        response_snippet: Option<String>,
        topic_signal: Option<String>,
    },
    ObservationMetric {
        feature_cycle: String,
        // full ObservationMetrics field set from observation_metrics table
        computed_at: i64,
        total_tool_calls: i64,
        total_duration_secs: i64,
        session_count: i64,
        search_miss_rate: f64,
        // ... remaining fields per observation_metrics schema
    },
    ShadowEvaluation {
        timestamp: i64,
        rule_name: String,
        rule_category: String,
        neural_category: String,
        neural_confidence: f64,
        convention_score: f64,
        rule_accepted: i64,
        digest: Option<Vec<u8>>,
    },
    FeatureEntry { feature_id: String, entry_id: u64 },
    TopicDelivery {
        topic: String,
        created_at: i64,
        completed_at: Option<i64>,
        status: String,
        github_issue: Option<i64>,
        total_sessions: i64,
        total_tool_calls: i64,
        total_duration_secs: i64,
        phases_completed: Option<String>,
    },
    OutcomeIndex { feature_cycle: String, entry_id: u64 },
    // Wave 1 additions: GraphEdge (W1-1), ConfidenceWeightUpdate (W3-1) — not yet defined.
    // #[non_exhaustive] ensures adding those variants does not break the drain task's
    // match in dependent crates.
}
```

The `#[non_exhaustive]` attribute (SR-06, C-08) ensures that adding Wave 1 variants
(`GraphEdge`, `ConfidenceWeightUpdate`) does not break the exhaustiveness check in the
drain task's match arm in any crate that imports this enum. The drain task's match inside
`unimatrix-store` itself is exhaustive because the drain task is defined in the same crate
and can be updated when new variants are added. External crates using a `match` on
`AnalyticsWrite` must include a `_ => {}` catch-all arm.

**Drain task loop** (in `analytics.rs`, spawned in `Store::open()`):

```rust
async fn run_drain_task(
    mut rx: mpsc::Receiver<AnalyticsWrite>,
    mut shutdown: oneshot::Receiver<()>,
    write_pool: SqlitePool,
) {
    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => {
                // Drain all remaining items, commit, exit.
                drain_remaining(&mut rx, &write_pool).await;
                return;
            }
            Some(first) = rx.recv() => {
                let mut batch = Vec::with_capacity(50);
                batch.push(first);
                // Collect up to 49 more without blocking.
                while batch.len() < 50 {
                    match rx.try_recv() {
                        Ok(e) => batch.push(e),
                        Err(_) => break,
                    }
                }
                // If still under 50, wait up to 500ms for more.
                if batch.len() < 50 {
                    let _ = tokio::time::timeout(
                        DRAIN_FLUSH_INTERVAL,
                        fill_batch(&mut batch, &mut rx),
                    ).await;
                }
                commit_batch(batch, &write_pool).await;
            }
        }
    }
}
```

Constants in `analytics.rs`:
```rust
pub(crate) const DRAIN_BATCH_SIZE: usize = 50;
pub(crate) const DRAIN_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
```

**Shed path** in `SqlxStore::enqueue_analytics()`:
```rust
pub fn enqueue_analytics(&self, event: AnalyticsWrite) {
    match self.analytics_tx.try_send(event) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(e)) => {
            let count = self.shed_counter.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::warn!(
                variant = e.variant_name(),
                queue_len = ANALYTICS_QUEUE_CAPACITY,
                capacity = ANALYTICS_QUEUE_CAPACITY,
                shed_total = count,
                "analytics write shed"
            );
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            // Drain task exited (shutdown in progress); discard silently.
        }
    }
}
```

`variant_name()` is a helper method on `AnalyticsWrite` that returns the variant name as
a `&'static str` without consuming the value. Because `AnalyticsWrite` owns the data for
the WARN log path, `TrySendError::Full(e)` provides access.

---

### 4. EntryStore Trait Migration (unimatrix-core/src/traits.rs)

All 18 methods become `async fn` using RPITIT (Rust 1.89). See ADR-005.

```rust
pub trait EntryStore: Send + Sync {
    async fn insert(&self, entry: NewEntry) -> Result<u64, CoreError>;
    async fn update(&self, entry: EntryRecord) -> Result<(), CoreError>;
    async fn update_status(&self, id: u64, status: Status) -> Result<(), CoreError>;
    async fn delete(&self, id: u64) -> Result<(), CoreError>;
    async fn get(&self, id: u64) -> Result<EntryRecord, CoreError>;
    async fn exists(&self, id: u64) -> Result<bool, CoreError>;
    async fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError>;
    async fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>, CoreError>;
    async fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>, CoreError>;
    async fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>, CoreError>;
    async fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>, CoreError>;
    async fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>, CoreError>;
    async fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<(), CoreError>;
    async fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>, CoreError>;
    async fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>, CoreError>;
    async fn read_counter(&self, name: &str) -> Result<u64, CoreError>;
    async fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>;
    // shed_events_total is NOT on EntryStore; it is a SqlxStore-specific method
    // accessed via the concrete type, not the trait. Callers in context_status
    // have access to the concrete Arc<SqlxStore>.
}
```

**`where Self: Sized` bound:** Not needed. RPITIT async traits in Rust 1.89 do not require
`where Self: Sized` on individual methods. The trait itself becomes non-object-safe (which
is correct and expected). The `where Self: Sized` bound is only necessary if you need to
selectively opt methods out of `dyn` dispatch — since the entire trait is non-object-safe,
no such bound is needed on any method.

**Object-safety tests removed.** Replaced with impl-completeness tests:

```rust
// unimatrix-core/tests/impl_completeness.rs
fn assert_entry_store_impl<S: EntryStore + Send + Sync>(_: &S) {}

#[tokio::test]
async fn sqlx_store_implements_entry_store() {
    let store = SqlxStore::open(temp_db_path(), PoolConfig::test_default()).await.unwrap();
    assert_entry_store_impl(&store);
    store.close().await;
}
```

The old `dyn EntryStore` compile tests in `unimatrix-core/src/traits.rs` are deleted.

---

### 5. Migration Connection Architecture

See ADR-003 for the full decision rationale. The sequence in `Store::open()`:

```rust
pub async fn open(path: impl AsRef<Path>, config: PoolConfig) -> Result<SqlxStore> {
    // 1. Validate config first — fail fast before touching the database.
    if config.write_max_connections > 2 {
        return Err(StoreError::InvalidPoolConfig {
            reason: format!(
                "write_pool max_connections {} exceeds hard cap of 2",
                config.write_max_connections
            ),
        });
    }

    let db_path = path.as_ref();

    // 2. Open dedicated migration connection (non-pooled).
    let mut migration_conn = SqliteConnection::connect(
        db_path.to_str().expect("valid UTF-8 path")
    ).await.map_err(|e| StoreError::Open(e.into()))?;

    // 3. Apply PRAGMAs on migration connection (same set as pools).
    apply_pragmas_connection(&mut migration_conn).await
        .map_err(|e| StoreError::Open(e.into()))?;

    // 4. Run migration. On failure: migration_conn is dropped, return error.
    //    Pool construction does NOT proceed.
    migrate_if_needed(&mut migration_conn, db_path).await
        .map_err(|e| StoreError::Migration { source: e.into() })?;

    // 5. Drop migration connection explicitly before pool construction.
    drop(migration_conn);

    // 6. Construct pools.
    let opts = build_connect_options(db_path);
    let read_pool = SqlitePoolOptions::new()
        .max_connections(config.read_max_connections)
        .acquire_timeout(config.read_acquire_timeout)
        .connect_with(opts.clone().read_only(true))
        .await
        .map_err(|e| StoreError::Open(e.into()))?;

    let write_pool = SqlitePoolOptions::new()
        .max_connections(config.write_max_connections)
        .acquire_timeout(config.write_acquire_timeout)
        .connect_with(opts)
        .await
        .map_err(|e| StoreError::Open(e.into()))?;

    // 7. Create analytics queue and shed counter.
    let (analytics_tx, analytics_rx) = mpsc::channel(ANALYTICS_QUEUE_CAPACITY);
    let shed_counter = Arc::new(AtomicU64::new(0));
    let (shutdown_tx, shutdown_rx) = oneshot::channel();

    // 8. Spawn drain task.
    let drain_handle = tokio::spawn(run_drain_task(
        analytics_rx,
        shutdown_rx,
        write_pool.clone(),
    ));

    Ok(SqlxStore {
        read_pool,
        write_pool,
        analytics_tx,
        shutdown_tx: Some(shutdown_tx),
        drain_handle: Some(drain_handle),
        shed_counter,
    })
}
```

Note: `read_only(true)` on the read pool's connect options prevents accidental writes
through the read pool at the SQLite level. This is defense-in-depth — the code architecture
already routes all writes through `write_pool`, but the SQLite-level read-only flag
provides a hard barrier.

---

### 6. SqliteWriteTransaction Retirement

See ADR-002 for the full decision. Summary:

The 5 call sites that use `begin_write()` / `txn.guard` are rewritten to use
`write_pool.begin().await?` directly. No typed wrapper is introduced. The `txn.rs` file
is deleted.

**Pattern at each call site:**
```rust
// Before (sync, MutexGuard lifetime escapes):
let txn = store.begin_write()?;
txn.guard.execute("...", [])?;
txn.commit()?;

// After (async, sqlx transaction):
let mut txn = store.write_pool.begin().await?;
sqlx::query("...").execute(&mut *txn).await?;
txn.commit().await?;
// Rollback on Drop is handled by sqlx::Transaction's Drop impl.
```

Where a call site previously accessed `txn.guard` for multiple SQL operations within one
transaction, the replacement uses `sqlx::Transaction<'_, Sqlite>` passed as `&mut *txn`
to each `sqlx::query!()` call. This preserves atomicity without a wrapper type.

---

### 7. unimatrix-observe Migration

`dead_knowledge.rs` currently calls `store.lock_conn()` and uses `rusqlite::params!`.
See ADR-006 for the decision to convert `ExtractionRule::evaluate()` to `async fn` across
all 5 extraction rules.

**Scope summary (ADR-006):**
- 5 extraction rules affected (not 21 — the 21 detection rules use `DetectionRule::detect()`
  which is a separate trait that never touches the store and is unaffected)
- Only `dead_knowledge.rs` has real logic to rewrite; the other 4 gain `async` on the
  method signature only

**Migration approach:**
1. `ExtractionRule::evaluate()` becomes `async fn` across the trait and all 5 implementations
   (ADR-006, Option A). The `store` parameter type changes from `&Store` to `&SqlxStore`.

2. Dynamic dispatch for `Vec<Box<dyn ExtractionRule>>` resolves the object-safety concern
   via an explicit enum over the 5 concrete rule types (preferred) or `async_trait` macro.
   This is a delivery-level implementation decision documented in code comments.

3. `query_accessed_active_entries` in `dead_knowledge.rs` is rewritten as an async sqlx
   query on `read_pool`:
   ```rust
   async fn query_accessed_active_entries(
       store: &SqlxStore,
   ) -> Result<Vec<(u64, String, u32)>, String> {
       sqlx::query!(
           "SELECT id, title, access_count FROM entries
            WHERE status = ?1 AND access_count > 0",
           Status::Active as i64
       )
       .fetch_all(&store.read_pool)
       .await
       .map(|rows| rows.into_iter().map(|r| (r.id as u64, r.title, r.access_count as u32)).collect())
       .map_err(|e| e.to_string())
   }
   ```

4. The `spawn_blocking` wrapper around `run_extraction_rules` in `background.rs` is
   removed. The call site becomes a direct `.await` (consistent with eliminating
   `spawn_blocking` debt — nxs-011's primary goal).

5. `unimatrix-observe/Cargo.toml`: remove `rusqlite` dependency (direct and transitive).

6. Tests for `DeadKnowledgeRule` are rewritten using `#[tokio::test]` and `SqlxStore`.

7. The `observe` crate migrates in the same delivery wave as the store crate (C-09, SR-07).

---

### 8. sqlx-data.json Placement

See ADR-004. Single workspace-level file at the repository root, generated by
`cargo sqlx prepare --workspace`. This covers all crates in the workspace.

---

### 9. AsyncEntryStore Retirement

`unimatrix-core/src/async_wrappers.rs` contains three structs:
- `AsyncEntryStore<T>` — 18 `spawn_blocking`-wrapped methods. **Deleted entirely.**
- `AsyncVectorStore<T>` — 8 methods wrapping HNSW. **Untouched** (C-06).
- `AsyncEmbedService<T>` — 3 methods wrapping ONNX. **Untouched** (C-06).

After deletion of `AsyncEntryStore`, `async_wrappers.rs` remains in place containing only
`AsyncVectorStore` and `AsyncEmbedService`. The file is not renamed or moved.

---

## Component Interactions

```
Store::open() ──────────────────────────────────────────────────────┐
  1. Validate PoolConfig                                              │
  2. Open SqliteConnection (non-pooled) → migrate_if_needed()        │
  3. Drop migration conn                                              │
  4. Build read_pool (SqlitePoolOptions)                              │
  5. Build write_pool (SqlitePoolOptions)                             │
  6. Create mpsc channel (capacity 1000)                             │
  7. Spawn drain_task ──► run_drain_task(rx, shutdown_rx, write_pool)│
  8. Return SqlxStore                                                 │
                                                                      │
MCP Tool Call (hot path):                                            │
  tool_handler → store.write_entry(e).await                         │
    → write_pool.acquire() → sqlx::query!() → release               │
  tool_handler → store.enqueue_analytics(event)  [non-async]        │
    → analytics_tx.try_send(event)                                   │
      → Ok: enqueued                                                  │
      → Full: shed_counter++, WARN log                               │
                                                                      │
Drain Task Loop:                                                      │
  select!(shutdown_rx | rx.recv())                                   │
    → collect batch ≤50 events (non-blocking try_recv)               │
    → wait ≤500ms for more if batch < 50                             │
    → write_pool.begin() → execute batch → commit()                  │
    → ERROR log on commit failure (data loss acceptable for analytics)│
                                                                      │
Store::close() / Drop:                                               │
  shutdown_tx.send(())                                               │
  drain_task: finish current batch → drain remaining → commit → exit │
  close(): await drain_handle (5s timeout)                           │
```

---

## Technology Decisions

| Decision | Choice | ADR |
|----------|--------|-----|
| Pool acquire timeouts | read=2s, write=5s | ADR-001 |
| SqliteWriteTransaction replacement | Direct `pool.begin().await?` at call sites | ADR-002 |
| Migration connection sequencing | Dedicated non-pooled connection before pool construction | ADR-003 |
| sqlx-data.json placement | Workspace-level single file | ADR-004 |
| Native async fn in EntryStore trait | RPITIT (Rust 1.89), no async_trait | ADR-005 |
| ExtractionRule::evaluate() async conversion | Option A: convert all 5 extraction rules to async fn; remove spawn_blocking call site | ADR-006 |

---

## Integration Points

The server crate (`unimatrix-server`) constructs `SqlxStore` and holds it as `Arc<SqlxStore>`.
The construction point is in `server.rs` (the server startup function):

```rust
// Before:
let store = Arc::new(Store::open(db_path)?);
let async_store = AsyncEntryStore::new(Arc::clone(&store));

// After:
let store = Arc::new(
    SqlxStore::open(db_path, PoolConfig::default()).await?
);
// async_store is gone; all callers use store.method().await directly.
```

`background.rs` is updated to accept `Arc<SqlxStore>` and call store methods directly
without `spawn_blocking`. `AsyncEntryStore` parameter is removed from all function
signatures that previously required it.

The `shed_events_total` field in `context_status` output is populated by calling
`store.shed_events_total()` on the concrete `Arc<SqlxStore>` — this is not on the
`EntryStore` trait because `VectorStore` and `EmbedService` implementations are
unaffected, and the shed counter is a `SqlxStore`-specific operational metric.

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|----------------|--------|
| `Store::open` | `async fn open(path: impl AsRef<Path>, config: PoolConfig) -> Result<SqlxStore>` | `unimatrix-store/src/db.rs` |
| `Store::close` | `async fn close(mut self)` | `unimatrix-store/src/db.rs` |
| `SqlxStore::enqueue_analytics` | `fn enqueue_analytics(&self, event: AnalyticsWrite)` (non-async) | `unimatrix-store/src/db.rs` |
| `SqlxStore::shed_events_total` | `fn shed_events_total(&self) -> u64` | `unimatrix-store/src/db.rs` |
| `PoolConfig::default` | `fn default() -> PoolConfig` | `unimatrix-store/src/pool_config.rs` |
| `PoolConfig::test_default` | `fn test_default() -> PoolConfig` | `unimatrix-store/src/pool_config.rs` |
| `AnalyticsWrite` enum | `#[non_exhaustive] pub enum AnalyticsWrite { ... }` | `unimatrix-store/src/analytics.rs` |
| `EntryStore` trait (async) | 18 `async fn` methods | `unimatrix-core/src/traits.rs` |
| `StoreError::InvalidPoolConfig` | `InvalidPoolConfig { reason: String }` | `unimatrix-store/src/error.rs` |
| `StoreError::PoolTimeout` | `PoolTimeout { pool: PoolKind, elapsed: Duration }` | `unimatrix-store/src/error.rs` |
| `StoreError::Migration` | `Migration { source: Box<dyn Error + Send + Sync> }` | `unimatrix-store/src/error.rs` |
| `StoreError::DrainTaskPanic` | `DrainTaskPanic` | `unimatrix-store/src/error.rs` |
| `PoolKind` enum | `pub enum PoolKind { Read, Write }` | `unimatrix-store/src/error.rs` |
| `ANALYTICS_QUEUE_CAPACITY` | `pub const ANALYTICS_QUEUE_CAPACITY: usize = 1000` | `unimatrix-store/src/analytics.rs` |
| `DRAIN_BATCH_SIZE` | `pub(crate) const DRAIN_BATCH_SIZE: usize = 50` | `unimatrix-store/src/analytics.rs` |
| `DRAIN_FLUSH_INTERVAL` | `pub(crate) const DRAIN_FLUSH_INTERVAL: Duration = Duration::from_millis(500)` | `unimatrix-store/src/analytics.rs` |
| `READ_POOL_ACQUIRE_TIMEOUT` | `pub const READ_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(2)` | `unimatrix-store/src/pool_config.rs` |
| `WRITE_POOL_ACQUIRE_TIMEOUT` | `pub const WRITE_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5)` | `unimatrix-store/src/pool_config.rs` |
| `migrate_if_needed` (adapted) | `pub(crate) async fn migrate_if_needed(conn: &mut SqliteConnection, db_path: &Path) -> Result<()>` | `unimatrix-store/src/migration.rs` |
| `ExtractionRule::evaluate` (observe) | `async fn evaluate(&self, observations: &[ObservationRecord], store: &SqlxStore) -> Vec<ProposedEntry>` — ADR-006 | `unimatrix-observe/src/extraction/mod.rs` |
| `run_extraction_rules` (observe) | `async fn run_extraction_rules(observations: &[ObservationRecord], store: &SqlxStore, rules: &[...]) -> Vec<ProposedEntry>` | `unimatrix-observe/src/extraction/mod.rs` |

---

## Source File Map

The following new files are introduced:

| File | Content |
|------|---------|
| `unimatrix-store/src/pool_config.rs` | `PoolConfig`, `READ_POOL_ACQUIRE_TIMEOUT`, `WRITE_POOL_ACQUIRE_TIMEOUT` |
| `unimatrix-store/src/analytics.rs` | `AnalyticsWrite` enum, drain task, `DRAIN_BATCH_SIZE`, `DRAIN_FLUSH_INTERVAL`, `ANALYTICS_QUEUE_CAPACITY` |

The following files are rewritten:

| File | Change |
|------|--------|
| `unimatrix-store/src/db.rs` | `Store` → `SqlxStore`, async `open()`, `close()`, `enqueue_analytics()`, `shed_events_total()` |
| `unimatrix-store/src/error.rs` | Add `InvalidPoolConfig`, `PoolTimeout`, `Migration`, `DrainTaskPanic`, `PoolKind`; remove `Sqlite(rusqlite::Error)` |
| `unimatrix-store/src/migration.rs` | `migrate_if_needed` adapts to `&mut SqliteConnection`, removes `rusqlite` dependency |
| `unimatrix-core/src/traits.rs` | `EntryStore` methods become `async fn`; object-safety tests removed |
| `unimatrix-core/src/async_wrappers.rs` | `AsyncEntryStore` deleted; `AsyncVectorStore` and `AsyncEmbedService` untouched |
| `unimatrix-observe/src/extraction/dead_knowledge.rs` | `lock_conn()` + `rusqlite::params!` → async sqlx query on `read_pool` |

The following files are deleted:

| File | Reason |
|------|--------|
| `unimatrix-store/src/txn.rs` | `SqliteWriteTransaction<'a>` retired (ADR-002) |

---

## Open Questions

1. **`ExtractionRule` trait async signature** — RESOLVED. See ADR-006. The trait converts
   to `async fn evaluate()` via Option A (RPITIT). Scope is 5 extraction rules, not 21
   (the 21 detection rules use `DetectionRule::detect()` which is unaffected). The
   `spawn_blocking` wrapper around `run_extraction_rules` in `background.rs` is removed.

2. **Drain task shutdown timeout configurability** — OQ-05 asks whether the 5s grace period
   should be configurable via `PoolConfig`. The architecture defines it as a constant
   (`DRAIN_SHUTDOWN_TIMEOUT = 5s`) for now. Test contexts that need shorter timeouts can use
   `PoolConfig::test_default()` which may set this lower. If runtime configurability is needed,
   add `drain_shutdown_timeout: Duration` to `PoolConfig` in a follow-on. Not a blocker.

3. **`write_pool.read_only(false)` vs default** — sqlx's `SqliteConnectOptions` defaults to
   read-write mode. The `read_only(true)` on `read_pool` is defense-in-depth but requires
   verification that sqlx's SQLite driver supports this correctly on all connection lifecycle
   events. If `read_only` mode causes issues (e.g., prevents WAL checkpoint), it can be
   removed — the routing architecture already prevents writes through `read_pool` at the code
   level.
