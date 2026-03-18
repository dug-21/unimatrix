# nxs-011: sqlx Migration — Connection Pools + Async-Native Storage
## SPECIFICATION

**Feature ID**: nxs-011
**Phase**: Nexus
**Status**: In Specification

---

## Objective

Replace the `rusqlite 0.34` + `Mutex<Connection>` storage layer in `unimatrix-store` with
`sqlx` and a dual-pool architecture (`read_pool` with 6–8 connections, `write_pool` capped
at 2). Introduce an async analytics write queue that decouples background analytics volume
from MCP hot-path integrity writes. Remove all `spawn_blocking` DB call sites across the
server, engine, and observe crates, retiring the `AsyncEntryStore` bridge wrapper and making
the entire storage layer async-native.

---

## Ubiquitous Language

| Term | Definition |
|------|-----------|
| **integrity write** | A write to a table whose loss is never acceptable under any load condition: `entries`, `entry_tags`, `audit_log`, `agent_registry`, `vector_map`, `counters`. These bypass the analytics queue and go directly through `write_pool`. |
| **analytics write** | A write to a background telemetry or derived-data table where bounded loss under extreme load is acceptable: `co_access`, `sessions`, `injection_log`, `query_log`, `signal_queue`, `observations`, `observation_metrics`, `shadow_evaluations`, `feature_entries`, `topic_deliveries`, `outcome_index`. |
| **drain task** | A long-lived `tokio::spawn` task, started in `Store::open()`, that reads batches from the analytics queue channel and commits them to `write_pool`. It owns the receiver half of the bounded channel. |
| **shed event** | The act of dropping an `AnalyticsWrite` variant from the queue when the bounded channel is at capacity. Every shed event increments a cumulative counter and emits a WARN log. Only analytics writes may be shed; integrity writes cannot. |
| **pool saturation** | The state in which all connections in a pool are in use and callers must wait for an `acquire_timeout` to elapse before receiving a `StoreError::PoolTimeout`. |
| **migration connection** | A dedicated non-pooled `sqlx::SqliteConnection` opened before pool construction, used exclusively by `migrate_if_needed()`, and closed before the first pool connection is acquired. |
| **SqlxStore** | The concrete struct that replaces the current `Store { conn: Mutex<Connection> }`. Holds `read_pool`, `write_pool`, the analytics queue sender, the drain task shutdown handle, and the cumulative shed counter. |
| **AnalyticsQueue** | The bounded `tokio::sync::mpsc` channel (capacity 1000) plus the drain task. The sender half is held by `SqlxStore`; the receiver half is owned exclusively by the drain task. |
| **RPITIT** | Return-position `impl Trait` in traits — the Rust 1.75+ stable mechanism used to write `async fn` in trait definitions without `async_trait`. The workspace is pinned to `rust-version = "1.89"`, so RPITIT is available. |
| **impl-completeness test** | A compile-time test that asserts a concrete type fully satisfies a trait by constructing a `fn requires_trait(s: &impl EntryStore)` function. Replaces the previous object-safety tests (`dyn EntryStore` compile checks) which are invalid for RPITIT async traits. |

---

## Functional Requirements

### FR-01: Dual Pool Construction

**What**: `Store::open()` (async) constructs two independent `SqlitePool` instances against
the same database file.

**Input**: Database file path, `PoolConfig` (see Domain Models).

**Output**: A `SqlxStore` instance with `read_pool` (max 6–8 connections) and `write_pool`
(max ≤ 2 connections), both configured with all 6 PRAGMAs via `SqliteConnectOptions`.

**PRAGMAs applied per connection** (via `SqliteConnectOptions::pragma`):
- `journal_mode = WAL`
- `synchronous = NORMAL`
- `wal_autocheckpoint = 1000`
- `foreign_keys = ON`
- `busy_timeout = 5000`
- `cache_size = -16384`

**Error conditions**:
- `write_pool max_connections > 2` → `StoreError::InvalidPoolConfig` at startup; process must not proceed.
- Database file does not exist and cannot be created → `StoreError::Open`.
- PRAGMA application failure → `StoreError::Open` with cause.

**Constraint**: Pool construction must not occur before `migrate_if_needed()` completes on
the migration connection. See FR-08.

---

### FR-02: Pool Acquire Timeout

**What**: Both pools have a configured `acquire_timeout`. When a connection cannot be
acquired within the timeout, the caller receives a structured error; the runtime must
never block indefinitely on pool saturation.

**Timeout values**: Defined by architect ADR; this spec records the requirement. Until
the ADR is filed, the implementation must use named constants (`READ_POOL_ACQUIRE_TIMEOUT`,
`WRITE_POOL_ACQUIRE_TIMEOUT`) with documented default values (suggested: 2s read, 5s write)
rather than inline literals, so the ADR can update a single location.

**Error condition**: Timeout → `StoreError::PoolTimeout { pool: PoolKind, elapsed: Duration }`.
No panic. No indefinite block.

---

### FR-03: Analytics Write Queue

**What**: `SqlxStore` holds the sender half of a bounded `tokio::sync::mpsc::channel`
(capacity 1000) typed as `mpsc::Sender<AnalyticsWrite>`. The drain task holds the
receiver half.

**Send path**:
- Caller constructs an `AnalyticsWrite` variant and calls a non-async helper (e.g., `store.enqueue_analytics(event)`).
- If the channel has capacity, the event is enqueued and the caller returns immediately (fire-and-forget).
- If the channel is at capacity (i.e., `try_send` returns `Err(TrySendError::Full)`): the event is dropped, the shed counter is incremented atomically, and a WARN log is emitted containing the dropped variant name and current channel capacity.

**Integrity writes are never enqueued**: All writes to integrity tables use
`store.write_pool` directly, bypassing this path entirely.

---

### FR-04: Drain Task Lifecycle

**What**: The drain task is started by `Store::open()` via `tokio::spawn`. It runs for
the lifetime of the `SqlxStore`.

**Drain loop**:
1. Receive up to 50 `AnalyticsWrite` events from the channel (non-blocking collection after first event).
2. If fewer than 50 events are available, wait up to 500ms for additional events before committing the partial batch.
3. Open a transaction on `write_pool`, execute all events in the batch, commit.
4. On commit failure: log at ERROR level, do not retry (data loss is acceptable for analytics; retrying risks double-writes).
5. Return to step 1.

**Shutdown path**:
- `Store::open()` returns a `oneshot::Sender<()>` shutdown signal stored in `SqlxStore`.
- When the signal is sent (via `Store::close()` or `Drop`), the drain task: finishes the current batch if one is in progress, drains any remaining channel items into one final batch, commits, then exits.
- `Store::close()` awaits the drain task join handle. It must not return until the drain task has exited.
- If the drain task does not exit within a configurable grace period (default 5s), `Store::close()` logs a WARNING and returns; it does not panic.

**Test teardown**: All test helpers that open a `Store` must call `Store::close().await`
(or its equivalent) before the test exits. The drain task must not outlive the test's
tokio runtime. See Constraints §Test Infrastructure.

---

### FR-05: Async Store API — Read Operations

**What**: All read methods on `SqlxStore` (previously backed by `lock_conn()`) are
rewritten as `async fn` using `read_pool`.

**Affected source files**: `read.rs`, `sessions.rs` (read paths), `query_log.rs` (read paths),
`counters.rs` (read paths), `metrics.rs`, `export.rs`.

**Requirements**:
- No call to `std::sync::Mutex::lock()`.
- No `spawn_blocking` wrapper.
- Concurrent callers must be able to acquire `read_pool` connections simultaneously (WAL mode).
- Return type `Result<T, StoreError>` where `StoreError::PoolTimeout` is a first-class variant.

---

### FR-06: Async Store API — Integrity Write Operations

**What**: All write methods targeting integrity tables are rewritten as `async fn` using
`write_pool` directly.

**Integrity tables**: `entries`, `entry_tags`, `audit_log`, `agent_registry`, `vector_map`,
`counters`.

**Affected source files**: `write.rs`, `counters.rs` (write paths), `sessions.rs`
(integrity paths if any), `audit.rs`.

**Requirements**:
- Callers use `.await`; no `spawn_blocking`.
- Under write saturation, the caller receives `StoreError::PoolTimeout` (FR-02), not an indefinite block.
- These writes must never be silently dropped.

---

### FR-07: Async Store API — Analytics Write Operations

**What**: All write methods targeting analytics tables are converted to enqueue an
`AnalyticsWrite` variant via FR-03. The methods return immediately (fire-and-forget);
they do not await a pool connection.

**Analytics tables**: `co_access`, `sessions`, `injection_log`, `query_log`,
`signal_queue`, `observations`, `observation_metrics`, `shadow_evaluations`,
`feature_entries`, `topic_deliveries`, `outcome_index`.

**Affected source files**: `write.rs`, `sessions.rs`, `injection_log.rs`, `query_log.rs`,
`signal.rs`, `topic_deliveries.rs`, `metrics.rs`.

**Requirements**:
- Method body calls `store.enqueue_analytics(event)` (non-async, try_send semantics).
- No pool acquisition in the calling method.
- Shed policy (FR-03) applies if channel is full.

---

### FR-08: Migration System Adaptation

**What**: `migrate_if_needed()` in `migration.rs` is adapted to operate on a dedicated
non-pooled `sqlx::SqliteConnection` rather than `rusqlite::Connection`.

**Sequence**:
1. `Store::open()` opens a migration connection via `sqlx::SqliteConnection::connect()`.
2. Calls `migrate_if_needed(&mut migration_conn, db_path)`.
3. On success: drops the migration connection.
4. On failure: returns `StoreError::Migration` and does not proceed to pool construction.
5. Pool construction (FR-01) begins only after step 3.

**Preservation requirement**: All existing migration logic (12 schema version transitions,
v0→v12) must be preserved verbatim in terms of SQL executed. The adaptation replaces
only the connection type and execution API, not the SQL or version-check logic.

**Regression harness** (see AC-17): A migration integration test must apply each of the
12 version transitions in sequence, starting from a schema-less database, and assert the
final schema version is 12.

---

### FR-09: EntryStore Trait Migration to Async

**What**: The `EntryStore` trait in `unimatrix-core/src/traits.rs` (18 methods) is
rewritten with native `async fn` signatures using RPITIT (Rust 1.89).

**Requirements**:
- All 18 methods become `async fn`.
- The trait is no longer object-safe (RPITIT async traits cannot be used as `dyn EntryStore`).
  This is expected and correct.
- Any existing `dyn EntryStore` usage in tests or production code must be replaced
  with generic bounds `T: EntryStore` or concrete type references.
- Object-safety compile tests (`dyn EntryStore` assertions) are replaced with
  impl-completeness tests (see AC-20, Domain Models §ImplCompletenessTest).
- `SqlxStore` is the sole production implementor. `StoreAdapter` in `unimatrix-core`
  is updated to use the new async trait.

---

### FR-10: AsyncEntryStore Retirement

**What**: The `AsyncEntryStore` struct in `unimatrix-core/src/async_wrappers.rs` is
deleted. All 18 `spawn_blocking`-wrapped methods are removed.

**Requirements**:
- No import of `AsyncEntryStore` anywhere in the server, observe, or core crates after this feature.
- Call sites that previously called `async_store.method().await` are updated to call
  `store.method().await` directly on `SqlxStore`.
- `AsyncVectorStore` and `AsyncEmbedService` in the same file are untouched
  (explicitly out of scope; see NOT IN SCOPE).

---

### FR-11: spawn_blocking Removal — Server Crate

**What**: All 101 `spawn_blocking(|| store.X())` call sites in the server crate are
removed. Each site is replaced by a direct `.await` call on the async store method.

**Affected files** (non-exhaustive, architect to confirm full list):
`background.rs`, `tools.rs`, `audit.rs`, `server.rs`, `store_correct.rs`, `store_ops.rs`,
`embed_reconstruct.rs`, `listener.rs`, `registry.rs`, `contradiction.rs`, `export.rs`,
`import/inserters.rs`.

**Requirements**:
- Zero `spawn_blocking(|| store` patterns in the server crate post-migration (verifiable by grep).
- Service methods previously wrapped in `spawn_blocking` become `async fn` and use `.await`.
- Test bodies for affected methods become `async` (see Constraints §Test Infrastructure).

---

### FR-12: SqliteWriteTransaction Retirement

**What**: `SqliteWriteTransaction<'a>` in `txn.rs` (which holds a `MutexGuard<'a, Connection>`)
is removed. The type is incompatible with async and cannot cross `.await` points.

**Replacement**: The 5 call sites that use `begin_write()` are rewritten using one of:
- `write_pool.begin().await?` for `sqlx::Transaction<'_, Sqlite>` where explicit
  transaction control is required, or
- Individual `async fn` calls that execute atomically via the pool's connection lifecycle,
  where transaction boundaries were implicit.

**Call sites** (per SCOPE.md):
- `server.rs` ×3
- `store_correct.rs`
- `store_ops.rs`
- `audit.rs`

**Architect decision point**: Whether to define a typed wrapper over
`sqlx::Transaction<'_, Sqlite>` (for ergonomics) or use `pool.begin().await` directly
at each call site. The spec does not mandate either; it mandates that `SqliteWriteTransaction<'a>`
is gone and no `MutexGuard` lifetime escapes any function boundary.

---

### FR-13: rusqlite Re-export Removal

**What**: `pub use rusqlite` is removed from `unimatrix-store/src/lib.rs`. All downstream
usages of `unimatrix_store::rusqlite::*` are migrated to sqlx equivalents.

**Affected import sites** (per SCOPE.md, 20+ in server crate):
`background.rs`, `export.rs`, `registry.rs`, `audit.rs`, `contradiction.rs`, `tools.rs`,
`server.rs`, `embed_reconstruct.rs`, `listener.rs`, `import/inserters.rs`.

**Requirements**:
- `rusqlite` does not appear in `unimatrix-store/Cargo.toml` as a direct dependency.
- No `use unimatrix_store::rusqlite` or `unimatrix_store::rusqlite::*` anywhere in the
  codebase after this feature.
- `unimatrix-observe/src/extraction/dead_knowledge.rs` uses `store.lock_conn()` and
  `rusqlite::params!` directly — this must be migrated in the same delivery wave (SR-07).

---

### FR-14: unimatrix-observe Migration

**What**: `dead_knowledge.rs` in `unimatrix-observe` calls `store.lock_conn()` and
references `rusqlite::params!`. This crate must be migrated alongside the store crate.

**Requirements**:
- `dead_knowledge.rs` call site replaced with async sqlx query.
- `unimatrix-observe/Cargo.toml` removes `rusqlite` dependency (direct or transitive via store).
- The observe crate compiles without `pub use rusqlite` in the store.

---

### FR-15: sqlx Compile-Time Query Checking

**What**: All SQL in `unimatrix-store` is written using `sqlx::query!()` macros.
A `sqlx-data.json` schema cache is generated and committed to the repository root.

**Requirements**:
- `cargo sqlx prepare --workspace` is run after any schema change and the resulting
  `sqlx-data.json` is committed before the PR is merged.
- `SQLX_OFFLINE=true` is set in the CI build environment (`.github/workflows/release.yml`
  and any additional CI workflow that runs `cargo build` or `cargo test`).
- A CI step explicitly fails with a human-readable message if `sqlx-data.json` is absent
  or stale (not just a cryptic macro expansion error).
- Developer instructions for regenerating `sqlx-data.json` are added to the project README
  or contributing guide.

---

### FR-16: Shed Counter Observability

**What**: The cumulative count of shed analytics write events is exposed in the
`context_status` MCP tool output.

**Requirements**:
- `SqlxStore` maintains an `AtomicU64` shed counter, incremented on every shed event.
- `context_status` response includes a `shed_events_total` field (or equivalent) under
  a storage health section.
- The counter is monotonically increasing; it is not reset between requests.
- When no events have been shed, the field is present and has value 0.

---

### FR-17: AnalyticsWrite Enum Extensibility

**What**: The `AnalyticsWrite` enum is defined as `#[non_exhaustive]` to allow Wave 1
features (NLI graph edges, confidence weight updates) to add variants without breaking
the drain task's match exhaustiveness in dependent crates.

**Requirements**:
- `AnalyticsWrite` is marked `#[non_exhaustive]`.
- The drain task's match on `AnalyticsWrite` variants includes a catch-all arm that logs
  at DEBUG level and skips unrecognised variants.
- Adding a new variant in a future wave does not require modifying the drain task match
  in `unimatrix-store` unless the new variant requires new SQL.

---

## Non-Functional Requirements

### NF-01: write_pool Connection Cap

`write_pool` `max_connections` must be ≤ 2. A configuration value above this limit is
rejected at `Store::open()` with `StoreError::InvalidPoolConfig` before any connection
is opened. This is a hard startup gate, not a warning.

**Rationale**: SQLite WAL mode serialises writers at the WAL append point. More than 2
concurrent write connections adds latency without throughput gain and risks WAL contention.

---

### NF-02: Pool Acquire Timeout — Structured Error

When either pool's acquire timeout elapses, callers receive `StoreError::PoolTimeout`.
The runtime must not block indefinitely. Panics on pool exhaustion are forbidden.
Timeout values are defined by architect ADR (see FR-02 for naming convention).

---

### NF-03: Analytics Queue Capacity and Shed Policy

- Channel capacity: 1000 events.
- Shed policy: drop the event, increment `AtomicU64` shed counter, emit WARN log.
- Log message must include: the `AnalyticsWrite` variant name, current queue length, and
  the capacity limit.
- Integrity writes (FR-06) are never subject to this policy; they do not use the queue.

---

### NF-04: Drain Batch Parameters

- Batch size: ≤ 50 events per transaction.
- Flush interval: ≤ 500ms (a partial batch is committed after 500ms of inactivity even
  if fewer than 50 events are available).
- These values are compile-time constants (`DRAIN_BATCH_SIZE`, `DRAIN_FLUSH_INTERVAL`)
  defined in a single location in the store crate.

---

### NF-05: Shed Counter Observability

The cumulative shed counter must be readable from `context_status` output without
restarting the server. See FR-16. The counter value must reflect all shed events since
the `Store` was opened, not just events since the last `context_status` call.

---

### NF-06: Test Count Preservation

All tests passing before this feature must continue to pass after. There must be no net
reduction in test count. The pre-migration baseline is:

| Suite | Count (scope time) |
|-------|-------------------|
| `unimatrix-store` unit | 103 |
| `unimatrix-store` integration | 85 |
| `unimatrix-server` unit | 1,406 |
| `unimatrix-server` integration | 39 |
| Migration integration | 16 |
| **Total** | **1,649** |

Tests converted from `#[test]` to `#[tokio::test]` count as preserved (same test, async
body). New tests added for AC-17 through AC-20 are additive.

---

### NF-07: CI Build with SQLX_OFFLINE

`SQLX_OFFLINE=true` is enforced in all CI build and test steps. CI must fail with a
human-readable error (not a cryptic macro expansion failure) if `sqlx-data.json` is
absent, malformed, or does not cover a query used in the build. See FR-15.

---

## Acceptance Criteria

All AC-IDs from SCOPE.md are preserved. Four additional criteria are added (AC-17–AC-20)
per spawn prompt instructions and risk assessment recommendations.

| ID | Criterion | Verification Method |
|----|-----------|-------------------|
| **AC-01** | `unimatrix-store` Cargo.toml depends on `sqlx` (features: `sqlite`, `runtime-tokio`) and does not depend on `rusqlite`. | `cargo metadata` + `grep -r rusqlite unimatrix-store/Cargo.toml` |
| **AC-02** | `Store::open()` constructs `read_pool` (max 6–8) and `write_pool` (max 2) via `SqliteConnectOptions` with all 6 PRAGMAs applied per connection. | Unit test: open store, query `PRAGMA journal_mode` and `PRAGMA foreign_keys` from each pool, assert values. |
| **AC-03** | All `Store` methods that perform DB operations are `async fn`; none call `std::sync::Mutex::lock()` or any blocking synchronisation primitive. | `grep -r "Mutex::lock\|lock_conn\|spawn_blocking" crates/unimatrix-store/src/` returns zero matches. |
| **AC-04** | `AsyncEntryStore` in `unimatrix-core/src/async_wrappers.rs` is removed; no call sites remain in the server or observe crates. | `grep -r "AsyncEntryStore" crates/` returns zero matches. |
| **AC-05** | All `spawn_blocking(|| store.` call sites in the server crate are removed (baseline: 101). | `grep -rn "spawn_blocking.*store\." crates/unimatrix-server/src/` returns zero matches. |
| **AC-06** | `AnalyticsQueue` is implemented with a bounded channel (capacity 1000), drain batch ≤ 50, drain interval 500ms, shed-under-load for analytics writes, and bypass for integrity writes. | Unit test: fill queue to capacity, verify 1001st event is shed and counter increments; integration test: drain task commits ≤ 50 events per batch. |
| **AC-07** | Analytics writes (`co_access`, `sessions`, `injection_log`, `query_log`, `signal_queue`, `observations`, `observation_metrics`, `shadow_evaluations`, `feature_entries`, `topic_deliveries`, `outcome_index`) route through `AnalyticsQueue`. | Code review: each affected write method calls `enqueue_analytics`; integration test: write to each analytics table and verify drain task commits it. |
| **AC-08** | Integrity writes (`entries`, `entry_tags`, `audit_log`, `agent_registry`, `vector_map`, `counters`) bypass the analytics queue and go directly through `write_pool`; these writes are never dropped under any load condition. | Test: exhaust analytics queue, verify integrity write still succeeds without pool contention or data loss. |
| **AC-09** | `write_pool max_connections` > 2 is rejected at startup with `StoreError::InvalidPoolConfig`. | Unit test: construct `PoolConfig { write_max: 3 }`, assert `Store::open()` returns `Err(StoreError::InvalidPoolConfig)`. |
| **AC-10** | Pool `acquire_timeout` is configured on both pools; timeout returns `StoreError::PoolTimeout` (not panic or indefinite block). | Integration test: saturate write pool with long-running transactions, assert a new write caller receives `StoreError::PoolTimeout` within the configured timeout. |
| **AC-11** | Existing schema migration logic (`migration.rs`) executes correctly through the sqlx connection infrastructure; all 16 migration integration tests pass. | `cargo test -p unimatrix-store --test migration` passes. |
| **AC-12** | `sqlx-data.json` is generated and committed; CI enforces `SQLX_OFFLINE=true`. | CI log shows `SQLX_OFFLINE=true`; file exists at workspace root; `cargo build --offline` succeeds. |
| **AC-13** | `pub use rusqlite` is removed from `unimatrix-store/src/lib.rs`; no downstream crate references `unimatrix_store::rusqlite`. | `grep -r "unimatrix_store::rusqlite" crates/` returns zero matches. |
| **AC-14** | All existing passing tests continue to pass; no net reduction in test count (baseline: 1,649 total). | `cargo test --workspace` passes; test count ≥ baseline. |
| **AC-15** | Shed events are logged at WARN level with dropped variant name and queue capacity. | Unit test: induce a shed event, assert WARN log contains variant name and capacity. |
| **AC-16** | `SqliteWriteTransaction<'a>` is retired; no `MutexGuard` lifetime escapes any function boundary in the codebase. | `grep -r "SqliteWriteTransaction\|MutexGuard" crates/` returns zero matches in production code. |
| **AC-17** | Migration regression harness covers all 12 schema version transitions (v0→v12) via the adapted `migration.rs` on a sqlx connection. | Integration test: open an empty database, run `migrate_if_needed`, assert each intermediate schema version is reached in sequence and final version is 12. At least one test per version transition. |
| **AC-18** | Shed counter increments are visible in `context_status` health output; the field `shed_events_total` is present and reflects cumulative shed events since store open. | Integration test: induce N shed events, call `context_status`, assert `shed_events_total == N`. |
| **AC-19** | `Store::close()` (or equivalent) awaits drain task completion before returning; no test exits with a live drain task holding a `write_pool` connection. | Integration test: enqueue events, call `Store::close()`, assert all events are committed and join handle has exited; assert pool connection count returns to 0. |
| **AC-20** | Impl-completeness tests replace object-safety tests for `SqlxStore` + `EntryStore`; `dyn EntryStore` compile tests are removed. | Test file contains `fn assert_impl<S: EntryStore>(_: &S) {}` called with `SqlxStore`; no `dyn EntryStore` trait object construction in test suite. |

**Total acceptance criteria: 20**

---

## Domain Models

### SqlxStore

```
SqlxStore {
    read_pool:        SqlitePool,          // max 6–8 connections, read-only queries
    write_pool:       SqlitePool,          // max ≤ 2 connections, integrity writes + drain task
    analytics_tx:     mpsc::Sender<AnalyticsWrite>,  // sender half of analytics queue
    shutdown_tx:      oneshot::Sender<()>, // signals drain task to flush and exit
    drain_handle:     JoinHandle<()>,      // held for Store::close() to await
    shed_counter:     Arc<AtomicU64>,      // cumulative shed events, shared with drain task for observability
}
```

**Invariants**:
- `write_pool.max_connections` is always ≤ 2. Violated only if `PoolConfig` validation is bypassed (which is forbidden).
- `drain_handle` is always Some until `Store::close()` is called.
- `shed_counter` is monotonically non-decreasing.
- `analytics_tx` is the only sender half; the drain task holds the sole receiver. No other component holds a cloned sender except via `SqlxStore` methods.

---

### AnalyticsWrite

An `#[non_exhaustive]` enum with one variant per analytics write operation. Exhaustive
list at spec time (Wave 1 additions come via new variants, not changes to existing ones):

```
#[non_exhaustive]
enum AnalyticsWrite {
    CoAccess        { id_a: u64, id_b: u64 },
    SessionUpdate   { session_id: String, /* fields per sessions table */ },
    InjectionLog    { /* fields per injection_log table */ },
    QueryLog        { /* fields per query_log table */ },
    SignalQueue     { /* fields per signal_queue table */ },
    Observation     { session_id: String, hook: String, /* fields per observations table */ },
    ObservationMetric { /* fields per observation_metrics table */ },
    ShadowEvaluation  { /* fields per shadow_evaluations table */ },
    FeatureEntry    { /* fields per feature_entries table */ },
    TopicDelivery   { /* fields per topic_deliveries table */ },
    OutcomeIndex    { feature_cycle: String, entry_id: u64 },
}
```

**Architect's responsibility**: Define the exact field sets per variant to match the
current table schemas. The spec defines the variant names and their mapping to tables.

---

### AnalyticsQueue

```
AnalyticsQueue {
    tx:           mpsc::Sender<AnalyticsWrite>,     // capacity 1000
    rx:           mpsc::Receiver<AnalyticsWrite>,   // owned by drain task only
    shed_counter: Arc<AtomicU64>,
    drain_task:   JoinHandle<()>,
}
```

**Drain task loop** (pseudocode):

```
loop {
    select! {
        _ = shutdown_rx => {
            drain_remaining(); commit(); return;
        }
        event = rx.recv() => {
            batch.push(event);
            // collect up to 49 more without blocking
            while batch.len() < 50 {
                match rx.try_recv() {
                    Ok(e)   => batch.push(e),
                    Err(_)  => break,
                }
            }
            // wait up to 500ms for more if batch is small
            if batch.len() < 50 {
                timeout(500ms, fill_batch(&mut batch, &rx)).await;
            }
            commit_batch(batch, &write_pool).await;
        }
    }
}
```

---

### Pool Configuration

```
PoolConfig {
    read_max_connections:  u32,  // valid range: 1–8; recommended: 6–8
    write_max_connections: u32,  // valid range: 1–2; > 2 rejected at startup
    read_acquire_timeout:  Duration,   // from architect ADR
    write_acquire_timeout: Duration,   // from architect ADR
    pragmas:               PragmaSet,  // all 6 PRAGMAs, applied per connection
}
```

**PragmaSet** (invariant: all 6 must be present):
```
PragmaSet {
    journal_mode:        "WAL",
    synchronous:         "NORMAL",
    wal_autocheckpoint:  1000,
    foreign_keys:        true,
    busy_timeout:        5000,   // ms
    cache_size:          -16384, // pages (negative = kibibytes)
}
```

---

### StoreError Taxonomy

New variants added by this feature (existing variants preserved):

```
StoreError {
    // existing variants ...

    // new in nxs-011:
    InvalidPoolConfig { reason: String },
    // Raised when write_pool max_connections > 2, or other pool config is invalid.

    PoolTimeout { pool: PoolKind, elapsed: Duration },
    // Raised when acquire_timeout elapses on either pool.

    Migration { source: Box<dyn Error + Send + Sync> },
    // Raised when migrate_if_needed() fails on the migration connection.
    // Distinguishable from general StoreError::Database to allow targeted recovery.

    DrainTaskPanic,
    // Raised by Store::close() if the drain task's JoinHandle resolves with a panic.
}

enum PoolKind { Read, Write }
```

---

### ImplCompletenessTest

Replaces previous `dyn EntryStore` object-safety tests (which are invalid for RPITIT
async traits). Pattern:

```rust
// In tests/impl_completeness.rs:
fn assert_entry_store_impl<S: EntryStore>(_: &S) {}

#[tokio::test]
async fn sqlx_store_implements_entry_store() {
    let store = SqlxStore::open(test_db_path(), PoolConfig::test_default()).await.unwrap();
    assert_entry_store_impl(&store);
}
```

This test compiles only if `SqlxStore` provides a concrete implementation of every
`EntryStore` method. It does not test runtime behaviour; it is a compile-time correctness
gate.

---

## User Workflows

### Workflow 1: MCP Tool Invocation (Hot Path)

1. MCP client calls a tool (e.g., `context_store`).
2. Tool handler calls `store.write_entry(entry).await`.
3. `write_entry` acquires a connection from `write_pool` (integrity write path).
4. If pool is saturated: `StoreError::PoolTimeout` is returned to the tool handler within
   the configured timeout. Tool handler returns an MCP error to the client.
5. On success: write completes, connection is released to pool.
6. If the tool also triggers analytics (e.g., `co_access` update), the handler calls
   `store.enqueue_analytics(AnalyticsWrite::CoAccess { .. })` — non-blocking, fire-and-forget.

### Workflow 2: Drain Task Batch Commit

1. Drain task receives first event from channel.
2. Collects up to 49 more events (non-blocking `try_recv` loop).
3. If < 50 events collected, waits up to 500ms for more.
4. Opens a transaction on `write_pool`.
5. Executes all batch events as individual SQL statements within the transaction.
6. Commits. On failure: logs ERROR, discards batch (analytics writes; loss is acceptable).
7. Loop continues.

### Workflow 3: Store Open (Server Startup)

1. `Store::open(db_path, config)` is called (async).
2. Opens migration connection (non-pooled `SqliteConnection`).
3. Runs `migrate_if_needed()` — applies any pending schema transitions.
4. Drops migration connection.
5. Constructs `read_pool` with `SqliteConnectOptions` + PRAGMAs.
6. Constructs `write_pool` with `SqliteConnectOptions` + PRAGMAs; rejects if `max_connections > 2`.
7. Creates bounded mpsc channel (capacity 1000) + `AtomicU64` shed counter.
8. Spawns drain task; stores `JoinHandle` and `oneshot::Sender` in `SqlxStore`.
9. Returns `SqlxStore`.

### Workflow 4: Store Close (Server Shutdown / Test Teardown)

1. `Store::close()` is called (async).
2. Sends shutdown signal via `oneshot::Sender`.
3. Drain task receives signal, finishes current batch, drains remaining channel, commits, exits.
4. `Store::close()` awaits `drain_handle` with a 5s grace period.
5. If grace period elapses without task exit: logs WARNING, returns.
6. Pools are dropped; all connections are returned to SQLite and file locks are released.

### Workflow 5: Shed Event

1. MCP tool handler calls `store.enqueue_analytics(event)`.
2. `try_send` on the bounded channel returns `Err(TrySendError::Full)`.
3. `shed_counter.fetch_add(1, Ordering::Relaxed)` is called.
4. WARN log is emitted: `"analytics write shed: variant={variant}, queue_len=1000, capacity=1000"`.
5. Caller returns normally (no error propagated to the tool handler).

---

## Constraints

### Hard Technical Constraints

**C-01: SQLite WAL Write Serialisation**
SQLite WAL mode supports one writer at a time at the WAL append point. `write_pool max_connections`
must be ≤ 2. This is a startup-time hard cap (AC-09), not a recommendation.

**C-02: async fn in Traits Requires Rust 1.89+**
The workspace is pinned to `rust-version = "1.89"`. Native RPITIT async traits are used.
`async_trait` crate must not be introduced. `dyn EntryStore` is not supported by RPITIT
async traits and must not appear in production code.

**C-03: Migration Must Precede Pool Construction**
`migrate_if_needed()` must complete on a dedicated non-pooled connection before either
pool is constructed. A pool constructed against an un-migrated schema produces undefined
behaviour (AC-11, FR-08).

**C-04: Integrity Writes Never Shed**
The analytics queue shed policy must never apply to integrity tables. Any code path that
enqueues an integrity write is a critical defect. This constraint is validated by AC-08.

**C-05: No spawn_blocking in Store Crate**
The store crate must contain zero `spawn_blocking` call sites after this feature. The
purpose of the migration is to make DB operations async-native; re-introducing
`spawn_blocking` anywhere in the store defeats this. Verifiable by grep in CI.

**C-06: AsyncVectorStore and AsyncEmbedService Untouched**
`async_wrappers.rs` non-DB wrappers (`AsyncVectorStore`, `AsyncEmbedService`) must not
be modified, removed, or relocated in this feature. Any disposition of those wrappers
requires a separate scope approval. This prevents scope creep and protects against
unintended regressions in the HNSW and ONNX paths.

**C-07: No Schema Changes**
Schema version remains at 12. No new tables, columns, or indexes are added. No SQL DDL
changes. This is a transport layer migration only.

**C-08: AnalyticsWrite Enum is Non-Exhaustive**
`AnalyticsWrite` must be `#[non_exhaustive]` (SR-06). Wave 1 additions must not require
modification of the drain task's match statement in `unimatrix-store`. See FR-17.

**C-09: unimatrix-observe Migrates in the Same Wave**
`dead_knowledge.rs` uses `store.lock_conn()` and `rusqlite::params!`. It must migrate
in the same delivery wave as the store crate — it cannot be deferred (SR-07). The
observe crate must compile without the `pub use rusqlite` re-export.

**C-10: sqlx-data.json Regeneration Discipline**
`sqlx-data.json` must be regenerated via `cargo sqlx prepare --workspace` after every
schema change and committed before the PR merges. A stale cache silently disables
compile-time SQL validation. This is a developer discipline constraint enforced via CI
(AC-12, NF-07).

---

### Test Infrastructure Constraints

**TC-01: Async Test Bodies**
All tests that exercise `SqlxStore` methods must use `#[tokio::test]`. The existing
`test_helpers.rs` must be rewritten as async helpers. Synchronous `Store` construction
in tests is not supported after this migration.

**TC-02: Store::close() in Every Test**
Every test that opens a `Store` must call `Store::close().await` (or use a drop guard
that calls it) before the test function exits. A drain task outliving the test's tokio
runtime silently corrupts isolation between tests and may cause spurious panics.

**TC-03: No Shared Store Across Tests**
Tests must not share a `SqlxStore` instance across test cases. Each `#[tokio::test]`
function that needs a store must construct its own via `SqlxStore::open(temp_db_path(), ...).await`.

**TC-04: Migration Test Isolation**
Migration regression tests (AC-17) must use a freshly created temporary database for
each test case. They must not share state with the main store test fixtures.

**TC-05: Drain Task Observability in Tests**
Tests that verify analytics writes must call `Store::close().await` before asserting
committed data. The drain task batches and commits asynchronously; asserting immediately
after `enqueue_analytics` without closing risks a race condition.

---

### CI Constraints

**CI-01: SQLX_OFFLINE=true in All CI Build Steps**
Set as an environment variable in `.github/workflows/release.yml` for all `cargo build`
and `cargo test` steps. Must not be opt-in or conditional.

**CI-02: sqlx-data.json Present and Current**
CI must fail with a structured error (not a cryptic macro expansion failure) if
`sqlx-data.json` is absent or does not cover a query. This requires either:
- `cargo sqlx check` as a pre-build step, or
- The `SQLX_OFFLINE=true` macro expansion to produce a `compile_error!` with a human-readable message.

**CI-03: No rusqlite in Store Crate**
CI must include a check (grep or `cargo deny`) that rejects any re-introduction of
`rusqlite` as a direct dependency in `unimatrix-store` or `unimatrix-server`.

---

## Dependencies

| Dependency | Version / Notes | Scope |
|-----------|----------------|-------|
| `sqlx` | ≥ 0.8 with features `sqlite`, `runtime-tokio`, `macros` | `unimatrix-store` (replaces `rusqlite`) |
| `tokio` | existing workspace version, features: `sync`, `time`, `rt` | `unimatrix-store` (drain task, mpsc, oneshot) |
| `rusqlite` | **removed** from `unimatrix-store` and `unimatrix-server` | — |
| `sqlx-cli` | dev tool only, not a Cargo dep | Developer workflow (`cargo sqlx prepare`) |
| Existing `unimatrix-core` traits | `EntryStore` trait rewritten with RPITIT async | `unimatrix-core` |
| Existing `unimatrix-observe` | `dead_knowledge.rs` migrated | `unimatrix-observe` |

**No new crates are introduced** beyond `sqlx`. The `async_trait` crate must not be
introduced (C-02).

---

## NOT In Scope

The following are explicitly excluded to prevent scope creep. Any implementation touching
these areas requires a separate scope approval:

1. **PostgreSQL migration**: This feature positions the code for PostgreSQL; it does not
   migrate the production database.
2. **sqlx built-in migration runner** (`sqlx::migrate!()`): `migration.rs` logic is
   adapted to use sqlx connections, not replaced with the sqlx migration framework.
3. **AsyncVectorStore and AsyncEmbedService**: CPU-bound HNSW and ONNX wrappers are
   untouched. See C-06.
4. **unimatrix-learn spawn_blocking**: The learn crate's single `spawn_blocking` for
   CPU-bound neural training is out of scope.
5. **unimatrix-adapt crate**: Has no rusqlite dependency; no changes.
6. **Schema changes**: No new tables, columns, or indexes. Schema version stays at 12.
7. **Rayon thread pool**: CPU-bound ML inference pool comes in with NLI (W1-2).
8. **Database file split**: Rejected in product vision Decision 4. This feature uses a
   single file with two pool handles.
9. **HTTP transport**: UDS-only for this feature; HTTP is a W2-2 concern.
10. **Two-pool transaction spanning**: No cross-pool transactions. Read pool is
    read-only; write pool handles all mutations. Queries that require reading then writing
    are sequenced (read from read_pool, write to write_pool), not transactional across pools.

---

## Open Questions for Architect

**OQ-01 — Pool acquire timeout values**
`read_pool` and `write_pool` acquire timeout values are not specified in SCOPE.md. The
suggested defaults (read: 2s, write: 5s) require architect sign-off before they become
a testable acceptance criterion. An ADR is expected before AC-10 can be fully verified.
*(Corresponds to SCOPE.md Q7.)*

**OQ-02 — SqliteWriteTransaction replacement API shape**
The 5 call sites that use `begin_write()` and raw guard access within a transaction need
a replacement. Options: (a) a typed wrapper over `sqlx::Transaction<'_, Sqlite>` for
ergonomics, or (b) direct `pool.begin().await?` at each call site. The spec mandates the
`SqliteWriteTransaction` is gone; the architect decides the replacement shape.
*(Corresponds to SCOPE.md Q4.)*

**OQ-03 — AnalyticsWrite variant field sets**
This spec defines variant names and table mappings. The exact field sets per variant must
match the current table schemas. The architect should cross-reference `migration.rs`
schema version 12 DDL to confirm all fields are covered.

**OQ-04 — sqlx-data.json placement**
`cargo sqlx prepare --workspace` produces a workspace-level `sqlx-data.json`. The SCOPE.md
assumption is that a single file covers all crates. If per-crate files are required
(e.g., due to crate-level `DATABASE_URL` differences), the CI workflow is more complex.
Architect to confirm.
*(Corresponds to SCOPE.md §Assumptions, Constraint 2.)*

**OQ-05 — Drain task grace period configurability**
The spec sets a 5s default grace period for drain task shutdown. Should this be
configurable via `PoolConfig` or a separate `ShutdownConfig`? Particularly relevant for
test contexts where the default may be too long.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for async storage pool connection sqlx spawn_blocking — found entries #735 (spawn_blocking pool saturation), #731 (batched fire-and-forget DB writes pattern), #1367 (spawn_blocking_with_timeout for MCP handlers), #1758 (extract spawn_blocking body into named sync helper), #1915 (ADR-005 accept loop task pattern).
- Queried: /uni-query-patterns for analytics write queue drain task background writes shed policy — found entry #2057 (store-owned background task requires explicit shutdown protocol before spec — directly relevant to FR-04 and AC-19), #1560 (Arc<RwLock<T>> background-tick state cache pattern).

Entry #2057 confirmed the drain task shutdown protocol requirement (SR-09) and directly
shaped FR-04 (drain task lifecycle), AC-19, and the test infrastructure constraints TC-02
and TC-05. Entry #731 confirmed the analytics queue formalises an already-established
pattern of batched fire-and-forget writes.
