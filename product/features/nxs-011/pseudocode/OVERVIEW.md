# nxs-011 Pseudocode Overview
## sqlx Migration — Connection Pools + Async-Native Storage

---

## Component Dependency Graph

```
pool-config          (no deps — foundational constants and struct)
    |
migration            (depends on pool-config: uses shared PRAGMA helper)
    |
sqlx-store           (depends on pool-config + migration: constructs both)
    |
analytics-queue      (depends on sqlx-store: defines AnalyticsWrite, spawned by open())
    |
entry-store-trait    (depends on sqlx-store: SqlxStore must exist before trait is made async)
    |
async-wrappers       (depends on entry-store-trait: AsyncEntryStore deleted after trait is async)
    |
server-migration     (depends on sqlx-store + async-wrappers: all call sites rewritten)
    |
observe-migration    (depends on sqlx-store: store type changes; must land same wave as store)
    |
ci-offline           (depends on server-migration + observe-migration: queries finalized)
```

---

## Recommended Wave Structure for Stage 3b

### Wave 1 — Foundation (no cross-crate deps)

| Component | File(s) | Rationale |
|-----------|---------|-----------|
| pool-config | `unimatrix-store/src/pool_config.rs` (new) | No deps; defines constants used by everything else |
| migration | `unimatrix-store/src/migration.rs` (rewrite) | Adapts to sqlx connection; must exist before SqlxStore::open() |

### Wave 2 — Store Core

| Component | File(s) | Rationale |
|-----------|---------|-----------|
| analytics-queue | `unimatrix-store/src/analytics.rs` (new) | AnalyticsWrite enum + drain task; needed by SqlxStore fields |
| sqlx-store | `unimatrix-store/src/db.rs` + `error.rs` + `write.rs` + `read.rs` + `sessions.rs` + `injection_log.rs` + `query_log.rs` + `signal.rs` + `topic_deliveries.rs` + `counters.rs` + `metrics.rs` + `lib.rs` + `test_helpers.rs` + `Cargo.toml` | Core store rewrite; delete `txn.rs` |

### Wave 3 — Trait + Bridge Retirement

| Component | File(s) | Rationale |
|-----------|---------|-----------|
| entry-store-trait | `unimatrix-core/src/traits.rs` + `unimatrix-core/tests/impl_completeness.rs` | Make EntryStore async; requires SqlxStore to exist |
| async-wrappers | `unimatrix-core/src/async_wrappers.rs` | Delete AsyncEntryStore; retain AsyncVectorStore + AsyncEmbedService |

### Wave 4 — Consumers (must land atomically — compile break otherwise)

| Component | File(s) | Rationale |
|-----------|---------|-----------|
| server-migration | `unimatrix-server/src/` (multiple files) | Remove 101 spawn_blocking; rewrite 5 begin_write sites |
| observe-migration | `unimatrix-observe/src/extraction/` (6 files) | C-09: same wave as store; fails to compile once pub use rusqlite removed |

### Wave 5 — CI Infrastructure

| Component | File(s) | Rationale |
|-----------|---------|-----------|
| ci-offline | `sqlx-data.json` (new at repo root) + `.github/workflows/release.yml` | Requires all sqlx::query!() call sites to be finalized |

---

## Shared Types and Their Owning Crates

| Type | Owning Crate | File | Consumers |
|------|-------------|------|-----------|
| `SqlxStore` | `unimatrix-store` | `src/db.rs` | `unimatrix-server`, `unimatrix-observe` |
| `PoolConfig` | `unimatrix-store` | `src/pool_config.rs` | `unimatrix-server` (server startup), tests |
| `AnalyticsWrite` | `unimatrix-store` | `src/analytics.rs` | `unimatrix-server` (enqueue calls) |
| `StoreError` (new variants) | `unimatrix-store` | `src/error.rs` | `unimatrix-server`, `unimatrix-core` |
| `PoolKind` | `unimatrix-store` | `src/error.rs` | `unimatrix-server` (error display) |
| `EntryStore` (async trait) | `unimatrix-core` | `src/traits.rs` | `unimatrix-server`, `unimatrix-store` (impl) |
| `READ_POOL_ACQUIRE_TIMEOUT` | `unimatrix-store` | `src/pool_config.rs` | tests, doc |
| `WRITE_POOL_ACQUIRE_TIMEOUT` | `unimatrix-store` | `src/pool_config.rs` | tests, doc |
| `ANALYTICS_QUEUE_CAPACITY` | `unimatrix-store` | `src/analytics.rs` | tests |
| `DRAIN_BATCH_SIZE` | `unimatrix-store` | `src/analytics.rs` (pub(crate)) | drain task |
| `DRAIN_FLUSH_INTERVAL` | `unimatrix-store` | `src/analytics.rs` (pub(crate)) | drain task |

---

## Critical Integration Seams

### Seam 1: Server → SqlxStore construction
- Before: `Arc::new(Store::open(db_path)?)` + `AsyncEntryStore::new(Arc::clone(&store))`
- After: `Arc::new(SqlxStore::open(db_path, PoolConfig::default()).await?)`
- File: `unimatrix-server/src/server.rs`

### Seam 2: Server → analytics queue
- Before: `spawn_blocking(|| store.record_access(ids))` for co_access writes
- After: `store.enqueue_analytics(AnalyticsWrite::CoAccess { id_a, id_b })` (non-async, try_send)
- All analytics write methods are synchronous calls on SqlxStore that enqueue internally

### Seam 3: Server → write_pool transactions (5 ADR-002 call sites)
- Before: `let txn = store.begin_write()?; txn.guard.execute(...)?; txn.commit()?`
- After: `let mut txn = store.write_pool.begin().await?; sqlx::query!("...").execute(&mut *txn).await?; txn.commit().await?`
- Files audited: `server.rs` (×3 production begin_write), `store_correct.rs` (×1), `store_ops.rs` (×1)
- Note: `audit.rs` contains `write_in_txn(&SqliteWriteTransaction)` as a helper called FROM server.rs transactions. This becomes an async helper accepting `&mut sqlx::Transaction<'_, Sqlite>`. The 4 begin_write calls in audit.rs tests are test-only, not production call sites.

### Seam 4: observe → SqlxStore read_pool
- Before: `store.lock_conn()` + `rusqlite::params!` in `dead_knowledge.rs`
- After: `sqlx::query!("SELECT ...").fetch_all(&store.read_pool).await`
- File: `unimatrix-observe/src/extraction/dead_knowledge.rs`

### Seam 5: EntryStore trait → callers
- Before: `async_store: AsyncEntryStore<Arc<Store>>` held by server; methods via `spawn_blocking`
- After: `store: Arc<SqlxStore>` held by server; methods via `.await` directly on the async trait impl

---

## OQ-BLOCK-02 Resolution: SqliteWriteTransaction Call Site Audit

The architecture documents "5 call sites" but the brief lists 6 entries (server.rs ×3, store_correct.rs, store_ops.rs, audit.rs). After auditing `crates/unimatrix-server/src/`:

**Production call sites requiring begin_write → write_pool.begin().await rewrite: 5**

| File | Line | Nature |
|------|------|--------|
| `server.rs` | ~430 | Entry insert + vector map + audit in one txn |
| `server.rs` | ~591 | Correction entry insert + original deprecation + audit in one txn |
| `server.rs` | ~1034 | (third production use — confirm by reading full file) |
| `services/store_correct.rs` | ~88 | Correction chain update |
| `services/store_ops.rs` | ~191 | Multi-table atomic operation |

**audit.rs: NOT a standalone call site.** `infra/audit.rs` defines `write_in_txn(&SqliteWriteTransaction)` as a helper that is called by the server.rs transactions. It becomes `async fn write_in_txn(txn: &mut sqlx::Transaction<'_, Sqlite>, event: AuditEvent)`. The 4 `begin_write().unwrap()` calls in audit.rs are in `#[cfg(test)]` test functions — they become `#[tokio::test]` bodies using `write_pool.begin().await`.

**Confirmed production count: 5 begin_write call sites. Architecture document is correct.**

---

## Files Created vs Deleted vs Rewritten

### New Files
- `crates/unimatrix-store/src/pool_config.rs`
- `crates/unimatrix-store/src/analytics.rs`
- `sqlx-data.json` (workspace root)
- `crates/unimatrix-core/tests/impl_completeness.rs`

### Deleted Files
- `crates/unimatrix-store/src/txn.rs` (SqliteWriteTransaction retired, ADR-002)

### Rewritten Files (partial list — see component pseudocode for details)
- `crates/unimatrix-store/src/db.rs` — SqlxStore replaces Store
- `crates/unimatrix-store/src/error.rs` — new error variants
- `crates/unimatrix-store/src/migration.rs` — async fn, sqlx connection
- `crates/unimatrix-store/src/write.rs`, `read.rs`, `sessions.rs`, etc. — async methods
- `crates/unimatrix-core/src/traits.rs` — async EntryStore (RPITIT)
- `crates/unimatrix-core/src/async_wrappers.rs` — AsyncEntryStore deleted
- `crates/unimatrix-server/src/server.rs`, `background.rs`, `tools.rs`, etc.
- `crates/unimatrix-observe/src/extraction/` (5 extraction rule files + mod.rs)
- `.github/workflows/release.yml`

---

## OQ-DURING Items (for implementers)

- **OQ-DURING-01** (read_only pool + WAL checkpoint): If `SqliteConnectOptions.read_only(true)` prevents WAL auto-checkpoint from running, remove it from the read_pool options. The routing architecture already prevents writes through read_pool at code level; the flag is defense-in-depth only. See R-12 (Low severity). Safe to remove without ADR revision.
- **OQ-DURING-02** (drain shutdown timeout): 5s grace period is a constant `DRAIN_SHUTDOWN_TIMEOUT`. If test execution reveals the constant is too long, it can be added to `PoolConfig` as a field. Current spec: constant only.
- **OQ-DURING-03** (AnalyticsWrite field completeness): Verified against schema v12 DDL from `db.rs`. See analytics-queue.md for the reconciled field sets. The `ObservationMetric` variant has 23 fields from the `observation_metrics` table DDL.
