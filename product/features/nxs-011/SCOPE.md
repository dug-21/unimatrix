# nxs-011: sqlx Migration — Connection Pools + Async-Native Storage

## Problem Statement

The storage layer is built on `rusqlite 0.34` with a single `Mutex<Connection>` shared
across all callers. Every database operation — read or write, hot-path or analytics
background work — must serialize through this one lock. The consequences are measured
and documented in Unimatrix entries #731, #735, #771, and #1759:

- **Reads block on writes and vice versa**, even though SQLite WAL mode permits true
  concurrent readers. The `Mutex<Connection>` prevents the runtime from exploiting
  this for free.
- **Every DB operation on the async runtime requires `spawn_blocking`**, offloading
  synchronous lock acquisition to the tokio blocking pool. The server crate has
  approximately 101 `spawn_blocking` call sites. Background processing in
  `background.rs` (2,374 lines) and the MCP tools layer in `tools.rs` (2,626 lines)
  are saturated with these wrappers.
- **`AsyncEntryStore` bridge exists solely to adapt sync DB calls for async callers.**
  It wraps 18 synchronous `EntryStore` methods in `spawn_blocking`, adding latency
  and allocation overhead on every MCP tool invocation. It is structural debt, not
  a design feature.
- **`SqliteWriteTransaction<'a>` carries a `MutexGuard<'a, Connection>` lifetime.**
  This lifetime escapes the transaction boundary in 5 call sites in the server crate,
  making the type non-async-safe and blocking future architectural evolution.
- **`lock_conn()` is called 213 times** across the codebase (96 times in the server
  crate alone). These are direct mutex acquisitions without timeout or backpressure —
  they block indefinitely under write saturation.
- **No backend abstraction.** Every query is coupled to `rusqlite`-specific types
  (`rusqlite::params!`, `named_params!`, `OptionalExtension`, `query_map`, etc.) —
  312 rusqlite-specific API call sites across store and server crates combined.
  Migration to PostgreSQL for centralized deployment requires a full rewrite.

The cost compounds with every feature built before this migration: each new W1/W2
feature adds more `spawn_blocking` sites that must later be unwound.

## Goals

1. Replace `rusqlite` + `Mutex<Connection>` in `unimatrix-store` with `sqlx` +
   a dual-pool architecture (`read_pool` with 6-8 connections, `write_pool` capped
   at 2 connections).
2. Introduce an async analytics write queue: bounded channel (capacity 1000),
   drain at 50 events or 500ms, shed-under-load policy for analytics writes only,
   integrity writes never dropped.
3. Make all `Store` methods async-native — remove all `spawn_blocking(|| store.X())`
   call sites in the server, engine, and observe crates.
4. Retire the `AsyncEntryStore` bridge wrapper in `unimatrix-core/src/async_wrappers.rs`.
5. Enable `sqlx` compile-time query checking: `sqlx::query!()` macros with
   `SQLX_OFFLINE=true` and a committed `sqlx-data.json` schema cache.
6. Preserve existing `migration.rs` logic — execute it through sqlx connections.
   Migration to sqlx's built-in migration runner is out of scope.
7. Establish backend abstraction: application code identical for SQLite and PostgreSQL
   backends; switching requires only a connection string change plus pragma adjustments.

## Non-Goals

- **PostgreSQL switch**: This feature does not migrate the production database to
  PostgreSQL. It positions the code for that migration; it does not perform it.
- **sqlx built-in migration runner**: The existing `migration.rs` schema upgrade logic
  is preserved and adapted to run through sqlx connections. Replacing it with
  `sqlx::migrate!()` macros is a follow-on task.
- **Rayon thread pool**: CPU-bound ML inference (NLI in W1-2, GNN in W3-1) runs on
  a dedicated rayon pool bridged via oneshot channel. That architecture is independent
  of the database layer and comes in with NLI (W1-2), not here.
- **Database file split**: A two-file (knowledge.db + analytics.db) architecture was
  considered and rejected (product vision Decision 4). This feature uses a single file
  with two pool handles.
- **New schema changes**: No table additions, column changes, or index modifications.
  Schema version stays at 12; this is a transport layer change only.
- **VectorStore or EmbedService async migration**: The `AsyncVectorStore` and
  `AsyncEmbedService` wrappers in `async_wrappers.rs` wrap HNSW and ONNX operations
  respectively. HNSW is memory-only and CPU-bound; ONNX is CPU-bound ML inference.
  Neither belongs in a connection pool. They may be retained, adapted, or addressed
  as separate work — not in scope here.
- **`unimatrix-learn` spawn_blocking**: The learn crate has 1 `spawn_blocking` call
  site wrapping CPU-bound neural model training, not DB access. Out of scope.
- **`unimatrix-adapt` crate changes**: Has no rusqlite dependency.

## Background Research

### Current Architecture

**Storage handle** (`crates/unimatrix-store/src/db.rs`):
```
pub struct Store {
    pub(crate) conn: Mutex<Connection>,
}
```
`lock_conn()` returns a `MutexGuard<'_, Connection>`. 213 call sites across all crates.

**Transaction wrapper** (`txn.rs`): `SqliteWriteTransaction<'a>` holds a
`MutexGuard<'a, Connection>`. The `'a` lifetime and the guard carve the connection
out of the mutex for the transaction duration. This design is incompatible with async
— you cannot hold a `MutexGuard` across an `.await` point. Used in 5 call sites in
the server crate (audit.rs, server.rs ×3, store_correct.rs, store_ops.rs).

**`AsyncEntryStore` bridge** (`crates/unimatrix-core/src/async_wrappers.rs`):
18-method struct wrapping every `EntryStore` method in `spawn_blocking`. Also contains
`AsyncVectorStore` (8 methods) and `AsyncEmbedService` (3 methods) — the latter two
are not DB-related and require separate disposition.

**Downstream rusqlite exposure**: The store crate re-exports rusqlite via
`pub use rusqlite` in `lib.rs`. The server crate uses `unimatrix_store::rusqlite`
directly — 20+ call sites in `background.rs`, `export.rs`, `registry.rs`, `audit.rs`,
`contradiction.rs`, `tools.rs`, `server.rs`, `embed_reconstruct.rs`, `listener.rs`,
`import/inserters.rs`, and several services. All of these will need to migrate from
rusqlite types to sqlx equivalents.

**`observe` crate direct rusqlite use**: `dead_knowledge.rs` calls `store.lock_conn()`
and uses `rusqlite::params!` directly. This crate (and its callers in the server)
must be migrated.

**Migration system** (`migration.rs`, 983 lines): Runs at `Store::open()` time via
`migrate_if_needed()`. Operates directly on a `rusqlite::Connection`. Must be
preserved and adapted to operate on a sqlx connection or a dedicated migration
connection handle.

**Test surface**: 103 unit tests in store crate source, 85 in store integration tests,
1,406 in server crate source, 39 in server integration tests. The server test surface
is the migration long tail — nearly all tests call synchronous store APIs today and
will need async test bodies.

**PRAGMAs**: 6 PRAGMAs set at open time (`journal_mode=WAL`, `synchronous=NORMAL`,
`wal_autocheckpoint=1000`, `foreign_keys=ON`, `busy_timeout=5000`,
`cache_size=-16384`). sqlx SQLite pools expose `SqliteConnectOptions` for setting
PRAGMAs per connection — this is the migration path.

**Analytics write candidates** (per product vision W0-1):
- Queue writes: `co_access`, `sessions`, `injection_log`, `query_log`, `signal_queue`,
  `observations`, `observation_metrics`, `shadow_evaluations`, `feature_entries`,
  `topic_deliveries`, `outcome_index`
- Integrity writes (bypass queue, never dropped): `entries`, `entry_tags`, `audit_log`,
  `agent_registry`, `vector_map`, `counters`

**Unimatrix knowledge findings**:
- Entry #731: Batched fire-and-forget DB writes pattern — co-access and access recording
  already batch within `spawn_blocking`. The analytics queue formalizes this pattern.
- Entry #771: Blocking `store.lock_conn()` on tokio async runtime causes intermittent
  hangs — confirms the urgency of removing sync DB acquisition from the hot path.
- Entry #1759: Extraction tick batch size controls mutex hold duration — the analytics
  queue drain batch size (≤50) serves the same contention-limiting purpose.

### Codebase Scale Assessment

| Component | Lines | Scope | Primary Work |
|---|---|---|---|
| `unimatrix-store/src/` | 7,554 | Full rewrite | rusqlite → sqlx throughout |
| `unimatrix-server/src/` | 47,784 | Targeted edits | spawn_blocking removal (101 sites), rusqlite→sqlx (44 direct usages in services) |
| `unimatrix-core/src/async_wrappers.rs` | ~317 | Retire `AsyncEntryStore` | Keep `AsyncVectorStore`, `AsyncEmbedService` or address separately |
| `unimatrix-observe/src/extraction/dead_knowledge.rs` | ~160 | 1 call site | `lock_conn()` → async sqlx query |
| `migration.rs` | 983 | Adapt (preserve logic) | rusqlite::Connection → sqlx connection handle |

## Proposed Approach

**Phase 1 — Store crate rewrite**: Replace `rusqlite` with `sqlx` in `Cargo.toml`.
Introduce `SqlxStore` holding `read_pool: SqlitePool` and `write_pool: SqlitePool`.
Convert all methods in `write.rs`, `read.rs`, `sessions.rs`, `injection_log.rs`,
`query_log.rs`, `signal.rs`, `topic_deliveries.rs`, `counters.rs`, `metrics.rs` to
`async fn`. Adapt `migration.rs` to use a dedicated sqlx connection for schema
migration. Configure all 6 PRAGMAs via `SqliteConnectOptions` on pool construction.
Remove `pub use rusqlite` from `lib.rs`.

**Phase 2 — Analytics write queue**: Add `AnalyticsQueue` struct (bounded
`tokio::sync::mpsc::channel`, capacity 1000). Define `AnalyticsWrite` enum covering
all analytics table variants. Spawn a drain task on the `write_pool` that batches
≤50 events or flushes every 500ms. Shed policy: drop + log when channel is full.
Integrity writes go directly to `write_pool`, bypassing the queue.

**Phase 3 — Server crate migration**: Remove all `spawn_blocking(|| store.X())`
call sites. Remove `AsyncEntryStore` from `unimatrix-core` and all import sites in
the server. Convert service methods that called through `AsyncEntryStore` to use
`store.X().await` directly. Migrate all `unimatrix_store::rusqlite::*` imports to
sqlx equivalents. Retire `SqliteWriteTransaction` (or replace with `sqlx::Transaction`
where explicit transaction control is needed).

**Phase 4 — sqlx offline mode**: Run `cargo sqlx prepare` against the schema to
generate `sqlx-data.json`. Commit to repository root. Add `SQLX_OFFLINE=true` to
CI build steps in `.github/workflows/release.yml`. Add developer instructions to
project README for regenerating the cache after schema changes.

**Key rationale for dual-pool over alternatives**:
- `Mutex<SqliteConnection>` (single async connection): removes spawn_blocking but
  preserves serialization; concurrent reads still block. Rejected.
- Single `SqlitePool`: allows concurrent reads but complicates write serialization;
  SQLite WAL with >2 concurrent writers risks contention. Rejected.
- Dual pool: read_pool exploits WAL concurrent reads; write_pool serializes writes
  with connection cap of 2. Correct.

## Acceptance Criteria

- AC-01: `unimatrix-store` Cargo.toml no longer depends on `rusqlite`; depends on
  `sqlx` with the `sqlite` and `runtime-tokio` features.
- AC-02: `Store::open()` (or equivalent) constructs a `read_pool` (`max_connections`
  6–8) and a `write_pool` (`max_connections` 2) using `SqliteConnectOptions` with
  all 6 PRAGMAs applied per connection.
- AC-03: All `Store` methods that perform DB operations are `async fn`; none call
  `std::sync::Mutex::lock()` or any blocking synchronization primitive.
- AC-04: `AsyncEntryStore` in `unimatrix-core/src/async_wrappers.rs` is removed;
  no call sites remain in the server or observe crates.
- AC-05: All `spawn_blocking(|| store.X())` call sites in the server crate are
  removed (baseline: 101 call sites at scope time).
- AC-06: `AnalyticsQueue` is implemented with a bounded channel (capacity 1000),
  drain batch size ≤50, drain interval 500ms, shed-under-load for analytics writes,
  and bypass for integrity writes.
- AC-07: Analytics writes (co_access, sessions, injection_log, query_log,
  signal_queue, observations, observation_metrics, shadow_evaluations, feature_entries,
  topic_deliveries, outcome_index) are routed through `AnalyticsQueue`.
- AC-08: Integrity writes (entries, entry_tags, audit_log, agent_registry, vector_map,
  counters) bypass the analytics queue and go directly through `write_pool`; these
  writes are never dropped under any load condition.
- AC-09: `write_pool` max_connections is enforced at ≤ 2; a configuration value
  above this limit is rejected at startup with a structured error.
- AC-10: Pool `acquire_timeout` is configured on both pools and returns a structured
  `StoreError` variant (not a panic or indefinite block) when a connection cannot be
  acquired within the timeout.
- AC-11: Existing schema migration logic (`migration.rs`) is preserved and executes
  correctly through the new sqlx connection infrastructure; all 16 migration
  integration tests pass.
- AC-12: `sqlx-data.json` is generated and committed; CI enforces `SQLX_OFFLINE=true`.
- AC-13: `pub use rusqlite` is removed from `unimatrix-store/src/lib.rs`; no
  downstream crate references `unimatrix_store::rusqlite`.
- AC-14: All existing passing tests continue to pass; no net reduction in test count.
- AC-15: The analytics write queue shed event is observable — dropped analytics
  writes are logged at WARN level with a count and the queue capacity.
- AC-16: `SqliteWriteTransaction<'a>` (which holds a `MutexGuard`) is retired; any
  sites requiring explicit transaction control use `sqlx::Transaction` instead.

## Constraints

**Hard technical constraints:**

1. **SQLite WAL write concurrency limit**: SQLite WAL mode supports one writer at a
   time. `write_pool max_connections` must be capped at 2; values above 2 add latency
   without throughput gain and risk WAL contention. This is a hard cap enforced at
   startup (AC-09).

2. **sqlx `query!()` macros require compile-time schema**: `sqlx::query!()` macros
   are checked at compile time against a live database or a cached `sqlx-data.json`.
   `SQLX_OFFLINE=true` must be set in CI or builds fail without a live `DATABASE_URL`.
   Regenerating `sqlx-data.json` after every schema change is a developer discipline
   requirement.

3. **`async fn` in traits requires RPITIT or `async_trait`**: Rust 1.75+ supports
   return-position `impl Trait` in traits. The workspace is pinned to `rust-version =
   "1.89"` so native async traits are available. The `EntryStore` trait in
   `unimatrix-core/src/traits.rs` (18 methods) must be rewritten with `async fn`
   signatures. All downstream implementations (the `StoreAdapter` in `unimatrix-core`
   and the server's direct usage) must update accordingly.

4. **Migration system runs at open time**: `migrate_if_needed()` currently takes
   `&Store` and a `&Path`. With an async store, migration must either run
   synchronously against a dedicated non-pooled connection before pool construction,
   or the `Store::open()` equivalent becomes `async`. The latter is strongly
   preferred (sqlx provides `sqlx::sqlite::SqliteConnection::connect()` for
   pre-pool migration runs).

5. **`SqliteWriteTransaction<'a>` lifetime incompatibility**: The current transaction
   type holds a `MutexGuard<'a, Connection>`, which cannot cross `.await` points.
   All 5 call sites (server.rs ×3, store_correct.rs, store_ops.rs, audit.rs) must
   be rewritten using `sqlx::Transaction` or converted to non-transactional sqlx
   queries with explicit BEGIN/COMMIT.

6. **`unimatrix-observe` crate has direct rusqlite dependency via `store.lock_conn()`**:
   `dead_knowledge.rs` calls `lock_conn()` and uses `rusqlite::params!`. The observe
   crate must be updated alongside the store crate; it cannot be deferred.

7. **Test infrastructure is synchronous today**: 1,406 server crate unit tests and
   39 integration tests were written against the sync `Store` API. Converting to
   `#[tokio::test]` is mechanical but voluminous. Test helpers in
   `unimatrix-store/src/test_helpers.rs` must be rewritten as async. This is the
   "long tail" called out in the product vision (1.5–2 week estimate).

8. **`rusqlite` is re-exported from the store crate**: `pub use rusqlite` in
   `lib.rs` means downstream crates use `unimatrix_store::rusqlite::*` types
   directly. All 20+ server-crate import sites must be migrated to sqlx equivalents
   when `pub use rusqlite` is removed.

9. **`product/PRODUCT-VISION.md` commit belongs in this feature's commit history**:
   The product vision was updated for this feature; the commit that modified it
   should be referenced in the nxs-011 commit sequence.

## Open Questions

1. **`EntryStore` trait signature**: Should the trait methods become `async fn`
   natively (Rust 1.75+ RPITIT), or use `async_trait` for object safety? The current
   trait has object-safety tests (`dyn EntryStore` compile checks). `async fn` in
   traits is not object-safe without boxing. Decision needed before spec phase.

2. **Analytics queue drain ownership**: Should the `AnalyticsQueue` drain task be
   owned by `Store` (started in `open()`) or by the server's background task
   orchestration (`background.rs`)? Owning it in `Store` is cleaner but requires
   Store to hold a `tokio::runtime::Handle`. Owning it in the server preserves the
   current separation of concerns.

3. **Migration connection handle**: Should `migrate_if_needed` operate on a raw
   `sqlx::SqliteConnection` (non-pooled, opened synchronously-then-closed before
   pool construction), or should it operate on a `&SqlitePool` connection from the
   write pool? The former avoids pool construction before migration completes; the
   latter reuses pool infrastructure.

4. **`SqliteWriteTransaction` replacement API**: The 5 call sites that use
   `begin_write()` and `txn.guard` (raw connection access within a transaction)
   need a replacement. Does the spec define a typed transaction wrapper over
   `sqlx::Transaction<'_, Sqlite>`, or does each call site use
   `pool.begin().await?` directly?

5. **`AsyncVectorStore` and `AsyncEmbedService` disposition**: These wrappers in
   `async_wrappers.rs` wrap CPU-bound (not DB) operations. They are not blocking DB
   calls and are not addressed by this feature. Should they be retained as-is, moved
   to a separate module, or scheduled for a follow-on task? Needs explicit decision
   to avoid scope creep during implementation.

6. **`SQLX_OFFLINE` enforcement mechanism**: The current CI workflow is
   `.github/workflows/release.yml`. Should `SQLX_OFFLINE=true` be added to the
   release workflow, a new CI check workflow, or both? And is the `sqlx-data.json`
   committed at the workspace root or per-crate? (`cargo sqlx prepare --workspace`
   produces a workspace-level file.)

7. **Pool acquire timeout values**: The product vision says to configure
   `acquire_timeout` but does not specify values. What are the target timeouts for
   `read_pool` and `write_pool`? (Suggested: read_pool 2s, write_pool 5s — but this
   requires architect sign-off before it becomes an AC.)

## Tracking

https://github.com/dug-21/unimatrix/issues/298
