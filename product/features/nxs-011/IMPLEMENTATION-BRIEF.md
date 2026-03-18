# nxs-011 Implementation Brief
## sqlx Migration — Connection Pools + Async-Native Storage

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nxs-011/SCOPE.md |
| Scope Risk Assessment | product/features/nxs-011/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nxs-011/architecture/ARCHITECTURE.md |
| Specification | product/features/nxs-011/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/nxs-011/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nxs-011/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/nxs-011/architecture/ADR-001-pool-acquire-timeout.md |
| ADR-002 | product/features/nxs-011/architecture/ADR-002-write-transaction-retirement.md |
| ADR-003 | product/features/nxs-011/architecture/ADR-003-migration-connection-sequencing.md |
| ADR-004 | product/features/nxs-011/architecture/ADR-004-sqlx-data-json-placement.md |
| ADR-005 | product/features/nxs-011/architecture/ADR-005-native-async-trait.md |
| ADR-006 | product/features/nxs-011/architecture/ADR-006-extraction-rule-async-conversion.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| PoolConfig (pool_config.rs) | product/features/nxs-011/pseudocode/pool-config.md | product/features/nxs-011/test-plan/pool-config.md |
| migration.rs adaptation | product/features/nxs-011/pseudocode/migration.md | product/features/nxs-011/test-plan/migration.md |
| AnalyticsQueue + AnalyticsWrite (analytics.rs) | product/features/nxs-011/pseudocode/analytics-queue.md | product/features/nxs-011/test-plan/analytics-queue.md |
| SqlxStore (db.rs + method modules) | product/features/nxs-011/pseudocode/sqlx-store.md | product/features/nxs-011/test-plan/sqlx-store.md |
| EntryStore trait migration (unimatrix-core/traits.rs) | product/features/nxs-011/pseudocode/entry-store-trait.md | product/features/nxs-011/test-plan/entry-store-trait.md |
| AsyncEntryStore retirement (async_wrappers.rs) | product/features/nxs-011/pseudocode/async-wrappers.md | product/features/nxs-011/test-plan/async-wrappers.md |
| Server crate spawn_blocking removal | product/features/nxs-011/pseudocode/server-migration.md | product/features/nxs-011/test-plan/server-migration.md |
| unimatrix-observe migration (dead_knowledge.rs) | product/features/nxs-011/pseudocode/observe-migration.md | product/features/nxs-011/test-plan/observe-migration.md |
| sqlx-data.json + CI enforcement | product/features/nxs-011/pseudocode/ci-offline.md | product/features/nxs-011/test-plan/ci-offline.md |

### Stage 3b Wave Structure (from Stage 3a)

| Wave | Components | Constraint |
|------|-----------|------------|
| Wave 1 | pool-config, migration | No cross-crate deps |
| Wave 2 | analytics-queue, sqlx-store (all method files, error.rs, lib.rs, test_helpers.rs, Cargo.toml, txn.rs deletion) | Depends on Wave 1 |
| Wave 3 | entry-store-trait, async-wrappers | Requires SqlxStore to exist (Wave 2) |
| Wave 4 | server-migration + observe-migration | Must land atomically; compile break without both |
| Wave 5 | ci-offline | All sqlx::query!() sites must be finalized first |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/nxs-011/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/nxs-011/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

### Open Question (surfaced during Stage 3a)

**OQ-NEW-01** (Low): `observation_phase_metrics` table exists in the schema v12 DDL but has no corresponding `AnalyticsWrite` variant and is not listed in the 11 analytics tables in the architecture. Wave 2 delivery agent must audit whether any existing code writes to this table. If a writer exists and uses `spawn_blocking`, an `ObservationPhaseMetric` variant must be added. If no writer exists, no action required.

---

## Goal

Replace the `rusqlite 0.34` + `Mutex<Connection>` storage layer in `unimatrix-store` with `sqlx` and a dual-pool architecture, introduce an async analytics write queue that decouples background analytics volume from MCP hot-path integrity writes, and remove all 101 `spawn_blocking` DB call sites across the server, engine, and observe crates. This migration eliminates structural debt that compounds with every Wave 1 and Wave 2 feature added before it, and positions the codebase for PostgreSQL-backed centralized deployment without per-query rewrites.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Pool acquire timeout values | read_pool: 2s, write_pool: 5s; test defaults 500ms/1s; named public constants | ADR-001 | architecture/ADR-001-pool-acquire-timeout.md |
| SqliteWriteTransaction retirement strategy | Direct `pool.begin().await?` at all 5 call sites; no typed wrapper; txn.rs deleted | ADR-002 | architecture/ADR-002-write-transaction-retirement.md |
| Migration connection sequencing | Dedicated non-pooled `SqliteConnection` opened before pool construction; dropped on success before first pool connection | ADR-003 | architecture/ADR-003-migration-connection-sequencing.md |
| sqlx-data.json placement | Single workspace-level file at repo root; generated by `cargo sqlx prepare --workspace`; CI enforces `SQLX_OFFLINE=true` | ADR-004 | architecture/ADR-004-sqlx-data-json-placement.md |
| async fn in EntryStore trait | Native RPITIT (Rust 1.89); `async_trait` crate not introduced; trait is non-object-safe by design; impl-completeness tests replace dyn compile-tests | ADR-005 | architecture/ADR-005-native-async-trait.md |
| AsyncVectorStore / AsyncEmbedService disposition | Untouched; out of scope; any removal requires separate scope approval (Constraint C-06) | ARCHITECTURE.md C-06 | — |
| AnalyticsWrite enum extensibility | `#[non_exhaustive]`; drain task catch-all arm for unknown variants; Wave 1 additions do not break drain match in dependent crates | SPECIFICATION.md FR-17 / C-08 | — |
| Drain task ownership | Owned by `SqlxStore`; started in `Store::open()`; shutdown via `oneshot::Sender` in `Store::close()` | ARCHITECTURE.md §1, FR-04 | — |

---

## Files to Create

| File | Summary |
|------|---------|
| `crates/unimatrix-store/src/pool_config.rs` | `PoolConfig` struct, `READ_POOL_ACQUIRE_TIMEOUT` (2s), `WRITE_POOL_ACQUIRE_TIMEOUT` (5s), `test_default()` |
| `crates/unimatrix-store/src/analytics.rs` | `AnalyticsWrite` enum, drain task loop, `DRAIN_BATCH_SIZE` (50), `DRAIN_FLUSH_INTERVAL` (500ms), `ANALYTICS_QUEUE_CAPACITY` (1000) |
| `sqlx-data.json` (workspace root) | Compile-time query cache generated by `cargo sqlx prepare --workspace` |
| `crates/unimatrix-core/tests/impl_completeness.rs` | Impl-completeness test replacing dyn EntryStore compile-tests |

## Files to Rewrite

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/db.rs` | `Store{Mutex<Connection>}` → `SqlxStore`; async `open()`, `close()`, `enqueue_analytics()`, `shed_events_total()` |
| `crates/unimatrix-store/src/error.rs` | Add `InvalidPoolConfig`, `PoolTimeout`, `Migration`, `DrainTaskPanic`, `PoolKind`; remove `Sqlite(rusqlite::Error)` |
| `crates/unimatrix-store/src/migration.rs` | `migrate_if_needed` becomes `async fn` accepting `&mut SqliteConnection`; SQL preserved verbatim; rusqlite API → sqlx API |
| `crates/unimatrix-store/src/write.rs` | All write methods become `async fn`; integrity writes via `write_pool`; analytics writes call `enqueue_analytics()` |
| `crates/unimatrix-store/src/read.rs` | All read methods become `async fn` using `read_pool`; no `Mutex::lock()` or `spawn_blocking` |
| `crates/unimatrix-store/src/sessions.rs` | Async read/write split: integrity paths via `write_pool`, analytics paths via `enqueue_analytics()` |
| `crates/unimatrix-store/src/injection_log.rs` | Analytics writes via `enqueue_analytics()` |
| `crates/unimatrix-store/src/query_log.rs` | Analytics writes via `enqueue_analytics()`; reads via `read_pool` |
| `crates/unimatrix-store/src/signal.rs` | Analytics writes via `enqueue_analytics()` |
| `crates/unimatrix-store/src/topic_deliveries.rs` | Analytics writes via `enqueue_analytics()` |
| `crates/unimatrix-store/src/counters.rs` | Integrity writes via `write_pool`; reads via `read_pool` |
| `crates/unimatrix-store/src/metrics.rs` | Analytics writes via `enqueue_analytics()`; reads via `read_pool` |
| `crates/unimatrix-store/src/lib.rs` | Remove `pub use rusqlite`; export new public types |
| `crates/unimatrix-store/src/test_helpers.rs` | Rewrite as async helpers; use `PoolConfig::test_default()`; call `Store::close().await` in teardown |
| `crates/unimatrix-store/Cargo.toml` | Replace `rusqlite` with `sqlx` (`sqlite`, `runtime-tokio`, `macros` features) |
| `crates/unimatrix-core/src/traits.rs` | `EntryStore` 18 methods → `async fn` (RPITIT); doc comment on non-object-safety; remove dyn compile-tests |
| `crates/unimatrix-core/src/async_wrappers.rs` | Delete `AsyncEntryStore<T>` (18 methods); retain `AsyncVectorStore` and `AsyncEmbedService` untouched |
| `crates/unimatrix-server/src/server.rs` | `Store::open()` → `SqlxStore::open().await`; remove `AsyncEntryStore::new()`; 3 transaction call sites rewritten |
| `crates/unimatrix-server/src/background.rs` | Remove all `spawn_blocking(|| store.X())` call sites (bulk of 101 total); methods become async |
| `crates/unimatrix-server/src/tools.rs` | Remove `spawn_blocking` wrappers; call `store.method().await` directly |
| `crates/unimatrix-server/src/store_correct.rs` | Transaction call site rewritten to `write_pool.begin().await?` |
| `crates/unimatrix-server/src/store_ops.rs` | Transaction call site rewritten to `write_pool.begin().await?` |
| `crates/unimatrix-server/src/audit.rs` | Transaction call site rewritten; rusqlite imports removed |
| `crates/unimatrix-server/src/` (remaining) | Remove `unimatrix_store::rusqlite::*` imports in `export.rs`, `registry.rs`, `contradiction.rs`, `embed_reconstruct.rs`, `listener.rs`, `import/inserters.rs` |
| `crates/unimatrix-observe/src/extraction/dead_knowledge.rs` | `lock_conn()` + `rusqlite::params!` → async sqlx query on `read_pool` |
| `crates/unimatrix-observe/Cargo.toml` | Remove rusqlite dependency |
| `.github/workflows/release.yml` | Add `SQLX_OFFLINE=true` env var; add `cargo sqlx check --workspace` pre-build step |

## Files to Delete

| File | Reason |
|------|--------|
| `crates/unimatrix-store/src/txn.rs` | `SqliteWriteTransaction<'a>` retired (ADR-002) |

---

## Data Structures

### SqlxStore (replaces Store)
```
SqlxStore {
    read_pool:    SqlitePool,               // max 6–8, read-only (defense-in-depth)
    write_pool:   SqlitePool,               // max ≤2, integrity writes + drain task
    analytics_tx: mpsc::Sender<AnalyticsWrite>,  // capacity 1000
    shutdown_tx:  Option<oneshot::Sender<()>>,   // signals drain task; Option for Drop
    drain_handle: Option<JoinHandle<()>>,         // held for Store::close() to await
    shed_counter: Arc<AtomicU64>,                 // cumulative shed events
}
```

### PoolConfig
```
PoolConfig {
    read_max_connections:  u32,     // 1–8; default 8
    write_max_connections: u32,     // 1–2; >2 rejected at startup
    read_acquire_timeout:  Duration,  // default 2s; test_default 500ms
    write_acquire_timeout: Duration,  // default 5s; test_default 1s
}
```

### AnalyticsWrite (non-exhaustive enum)
11 variants covering the analytics tables: `CoAccess`, `SessionUpdate`, `InjectionLog`, `QueryLog`, `SignalQueue`, `Observation`, `ObservationMetric`, `ShadowEvaluation`, `FeatureEntry`, `TopicDelivery`, `OutcomeIndex`. Marked `#[non_exhaustive]` for Wave 1 extensibility.

### StoreError (new variants)
```
StoreError::InvalidPoolConfig { reason: String }   // write_max > 2 or invalid config
StoreError::PoolTimeout { pool: PoolKind, elapsed: Duration }
StoreError::Migration { source: Box<dyn Error + Send + Sync> }
StoreError::DrainTaskPanic
enum PoolKind { Read, Write }
```

---

## Key Function Signatures

```rust
// Store open/close
pub async fn open(path: impl AsRef<Path>, config: PoolConfig) -> Result<SqlxStore>
pub async fn close(mut self)

// Analytics queue (non-async, fire-and-forget)
pub fn enqueue_analytics(&self, event: AnalyticsWrite)
pub fn shed_events_total(&self) -> u64

// Pool configuration constants
pub const READ_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(2);
pub const WRITE_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
pub const ANALYTICS_QUEUE_CAPACITY: usize = 1000;
pub(crate) const DRAIN_BATCH_SIZE: usize = 50;
pub(crate) const DRAIN_FLUSH_INTERVAL: Duration = Duration::from_millis(500);

// Migration (adapted signature)
pub(crate) async fn migrate_if_needed(
    conn: &mut sqlx::SqliteConnection,
    db_path: &Path,
) -> Result<()>

// EntryStore trait (all 18 methods become async fn via RPITIT)
pub trait EntryStore: Send + Sync {
    async fn insert(&self, entry: NewEntry) -> Result<u64, CoreError>;
    async fn update(&self, entry: EntryRecord) -> Result<(), CoreError>;
    // ... 16 additional async fn methods
}

// Impl-completeness test replacement
fn assert_entry_store_impl<S: EntryStore + Send + Sync>(_: &S) {}
```

---

## Constraints

1. **SQLite WAL write cap**: `write_pool max_connections` hard cap ≤ 2. Enforced at `Store::open()` with `StoreError::InvalidPoolConfig`. Not configurable above this limit.
2. **Integrity writes never shed**: Analytics queue shed policy must never apply to `entries`, `entry_tags`, `audit_log`, `agent_registry`, `vector_map`, `counters`. Routing error is a critical defect.
3. **Migration precedes pool construction**: `migrate_if_needed()` on a dedicated non-pooled connection must succeed and be dropped before either pool is constructed.
4. **No spawn_blocking in store crate**: Zero `spawn_blocking` call sites after migration. Verifiable by CI grep.
5. **No async_trait crate**: RPITIT native async traits only (Rust 1.89). `dyn EntryStore` not supported.
6. **AsyncVectorStore and AsyncEmbedService untouched**: Any modification requires separate scope approval.
7. **No schema changes**: Schema version stays at 12. No DDL changes.
8. **AnalyticsWrite is #[non_exhaustive]**: Drain task match in unimatrix-store includes catch-all arm.
9. **unimatrix-observe migrates in the same wave**: Cannot be deferred; fails to compile the moment `pub use rusqlite` is removed.
10. **sqlx-data.json committed before PR merge**: Regenerated via `cargo sqlx prepare --workspace` after any `sqlx::query!()` change.
11. **Store::close() in every test**: TC-02 mandatory; drain task must not outlive the test's tokio runtime.

---

## Dependencies

| Dependency | Version / Notes | Scope |
|-----------|----------------|-------|
| `sqlx` | ≥ 0.8; features: `sqlite`, `runtime-tokio`, `macros` | `unimatrix-store` (replaces `rusqlite`) |
| `tokio` | Existing workspace version; features: `sync`, `time`, `rt` | `unimatrix-store` (drain task, mpsc, oneshot) |
| `rusqlite` | **Removed** from `unimatrix-store` and `unimatrix-server` | — |
| `sqlx-cli` | Dev tool only; not a Cargo dep | Developer workflow (`cargo sqlx prepare`) |

No new crates beyond `sqlx`. No `async_trait` crate.

---

## NOT in Scope

1. PostgreSQL migration — feature positions code for it; does not perform it.
2. `sqlx::migrate!()` runner — `migration.rs` logic adapted to sqlx connections, not replaced.
3. `AsyncVectorStore` and `AsyncEmbedService` — CPU-bound HNSW/ONNX wrappers, not DB calls.
4. `unimatrix-learn` `spawn_blocking` — CPU-bound neural training, not DB access.
5. `unimatrix-adapt` crate — has no rusqlite dependency.
6. Schema changes — no new tables, columns, or indexes; schema version stays at 12.
7. Rayon thread pool — comes in with NLI (W1-2).
8. Database file split — rejected (product vision Decision 4); single file with two pool handles.
9. HTTP transport — UDS-only; HTTP is a W2-2 concern.
10. Cross-pool transactions — no transactions spanning `read_pool` and `write_pool`.

---

## Implementation Sequencing

### Phase 1 — Store Crate Rewrite
Replace `rusqlite` with `sqlx` in `Cargo.toml`. Introduce `SqlxStore`, `PoolConfig`, and `AnalyticsWrite` in new source files. Convert all methods in `write.rs`, `read.rs`, `sessions.rs`, `injection_log.rs`, `query_log.rs`, `signal.rs`, `topic_deliveries.rs`, `counters.rs`, `metrics.rs` to `async fn`. Adapt `migration.rs` to accept `&mut SqliteConnection`. Delete `txn.rs`. Configure all 6 PRAGMAs via `SqliteConnectOptions`. Remove `pub use rusqlite` from `lib.rs`.

### Phase 2 — Analytics Write Queue
Implement `AnalyticsQueue` in `analytics.rs`. Define `AnalyticsWrite` enum with all 11 variants (verified against schema v12 DDL). Spawn drain task in `Store::open()`. Implement shed path in `enqueue_analytics()` with `AtomicU64` counter. Implement `Store::close()` with oneshot shutdown signal and 5s grace period. Implement `Drop` for non-blocking signal send.

### Phase 3 — Server and Observe Crate Migration
Remove all 101 `spawn_blocking(|| store.X())` call sites in the server crate. Remove `AsyncEntryStore` from `unimatrix-core/src/async_wrappers.rs`. Rewrite 5 `SqliteWriteTransaction` call sites to `write_pool.begin().await?`. Migrate `dead_knowledge.rs` in `unimatrix-observe` to async sqlx query. Remove all `unimatrix_store::rusqlite::*` imports (20+ server crate sites). Update `EntryStore` trait to `async fn` signatures (RPITIT). Replace `AsyncEntryStore` parameter in `server.rs` startup with direct `Arc<SqlxStore>`.

### Phase 4 — sqlx Offline Mode and CI
Run `cargo sqlx prepare --workspace` against a schema v12 database. Commit `sqlx-data.json` to workspace root. Add `SQLX_OFFLINE=true` to `.github/workflows/release.yml` for all `cargo build` and `cargo test` steps. Add `cargo sqlx check --workspace` as a pre-build CI step. Add developer regeneration instructions to README.

---

## Critical Path and Dependencies

The phases have a strict dependency order:

1. Phase 1 must complete before Phase 2 (drain task uses the async store API).
2. Phase 1 must complete before Phase 3 (server migration depends on async store methods existing).
3. Phase 3 must fully complete before Phase 4 (sqlx-data.json covers all final query!() call sites).

Within Phase 3, the `unimatrix-observe` migration (FR-14) is on the critical path with the server crate migration — `pub use rusqlite` removal in Phase 1 causes a compile break in `observe` immediately. The observe migration must land in the same PR or the same commit batch as Phase 1 completion.

The `AsyncEntryStore` retirement (Phase 3) depends on the `EntryStore` trait becoming async (Phase 1 final step). The trait migration must happen before — or atomically with — the server call-site updates that remove `spawn_blocking`.

---

## Open Questions — MUST Resolve Before Delivery Starts

### OQ-BLOCK-01: ExtractionRule Async Boundary (VARIANCE-02 — Delivery Blocker)

`ExtractionRule::evaluate()` in `unimatrix-observe` is called from the server's async background task. After nxs-011, the call site cannot use `store.lock_conn()` (removed). Two paths exist:

**Option A — Full async trait conversion**: Convert all 21 `ExtractionRule` implementations to `async fn evaluate(...)`. Correct, but touches all detection rules in `unimatrix-observe`. Adds implementation volume.

**Option B — spawn_blocking bridge at the call site**: Wrap the sync `evaluate()` call in `tokio::task::spawn_blocking` at the `background.rs` call site. Safe from tokio's perspective (spawn_blocking runs on a blocking thread pool, not a worker thread). Does not require async trait conversion of all 21 rules. The `evaluate()` method receives a cloned/snapshot of needed store data before dispatch.

The `block_on` bridge (as mentioned in ARCHITECTURE.md as an intermediate step) panics when called from within a tokio worker thread and must NOT be used.

The delivery agent cannot proceed with FR-14 (observe crate migration) without this decision recorded in the spec. Human decision required.

### OQ-BLOCK-02: call site count ambiguity for SqliteWriteTransaction

ARCHITECTURE.md background research counts "5 call sites" but lists 6 entries: `server.rs ×3`, `store_correct.rs`, `store_ops.rs`, `audit.rs`. The delivery agent must audit all 6 before starting Phase 3. Confirm the correct count and file locations before work on ADR-002 call sites begins.

---

## Open Questions — Can Resolve During Delivery

### OQ-DURING-01: `read_only(true)` on read pool and WAL checkpoint

ARCHITECTURE.md open question 3: `SqliteConnectOptions.read_only(true)` as defense-in-depth may prevent WAL auto-checkpoint from running, causing unbounded WAL growth (R-12, Low severity). If confirmed during delivery, remove `read_only(true)` — routing architecture already prevents accidental writes through `read_pool` at the code level. Safe to remove without an ADR revision.

### OQ-DURING-02: Drain task shutdown timeout configurability

The 5s grace period is a constant (`DRAIN_SHUTDOWN_TIMEOUT`). If test contexts need shorter timeouts, `PoolConfig::test_default()` may include a `drain_shutdown_timeout` field. Not a delivery blocker — current approach (constant) is acceptable for the initial implementation.

### OQ-DURING-03: AnalyticsWrite variant field completeness

Architecture defines the variant field sets. Delivery agent must cross-reference `migration.rs` schema v12 DDL to confirm every field in each `AnalyticsWrite` variant matches the current table schema. Mismatches surface as compile-time type errors via `sqlx::query!()` macros.

---

## Security Requirements Checklist

From product vision W0-1 security requirements (all must be satisfied by delivery):

- [ ] **[High] Write pool max_connections ≤ 2**: Enforced at `Store::open()` with `StoreError::InvalidPoolConfig` before any DB is touched. AC-09.
- [ ] **[High] Integrity writes never shed**: `entries`, `entry_tags`, `audit_log`, `agent_registry`, `vector_map`, `counters` bypass the analytics queue entirely. Verified under queue saturation. AC-08.
- [ ] **[Medium] sqlx-data.json regenerated and committed after every schema change**: Stale cache disables compile-time SQL validation. CI-01, CI-02, AC-12.
- [ ] **[Medium] SQLX_OFFLINE=true enforced in CI**: All `cargo build` and `cargo test` CI steps. NF-07, CI-01, AC-12.
- [ ] **[Low] acquire_timeout configured for structured error under write saturation**: Both pools have named constant timeouts. Callers receive `StoreError::PoolTimeout`, not a panic or indefinite block. AC-10.
- [ ] **[Additional] No SQL injection via format! string interpolation**: All SQL uses `sqlx::query!()` macros or explicit bind parameters. CI grep check rejects `format!("SELECT...{}")` patterns in store crate. CI-03.
- [ ] **[Additional] No rusqlite re-introduction in store/server crates**: CI grep check (`cargo deny` or grep) rejects rusqlite as a direct dependency in `unimatrix-store` or `unimatrix-server`. CI-03.

---

## Test Strategy Summary

Test baseline (must not decrease after migration):

| Suite | Baseline Count |
|-------|---------------|
| `unimatrix-store` unit | 103 |
| `unimatrix-store` integration | 85 |
| `unimatrix-server` unit | 1,406 |
| `unimatrix-server` integration | 39 |
| Migration integration | 16 |
| **Total** | **1,649** |

Tests converted from `#[test]` to `#[tokio::test]` count as preserved.

### Critical Risks (must pass before delivery gate)

| Risk | Description | Key Scenario |
|------|-------------|-------------|
| R-01 (Critical) | Pool starvation — write_pool cap 2 blocks callers under sustained write + drain concurrency | Saturate write pool with 10 concurrent callers; drain task + integrity write contention test |
| R-02 (Critical) | Drain task teardown race — task outlives test tokio runtime when `Store::close()` not called | `Store::close()` awaits drain exit; all enqueued events committed; zero "task panicked after runtime shutdown" across 1,445-test suite |
| R-03 (Critical) | Migration failure leaves DB in inconsistent state | All 12 version transitions individually in isolation; migration failure blocks pool construction |

### High Risks (must pass before delivery gate; R-08 blocks delivery start)

| Risk | Description |
|------|-------------|
| R-04 | Analytics shed silent data loss — shed counter must be visible in `context_status` (AC-18) |
| R-05 | sqlx-data.json drift — CI enforces structured error on stale cache |
| R-06 | Integrity write contamination via wrong queue routing — AC-08 under full queue |
| R-07 | RPITIT Send bound failures — impl-completeness test + spawn context compile test |
| R-08 | ExtractionRule block_on bridge panic — **BLOCKS DELIVERY START** until ExtractionRule path decided |
| R-09 | Transaction rollback gap — all 5 (or 6) rewritten call sites have rollback-on-failure tests |
| R-14 | Test count regression — 1,445 sync tests converted to `#[tokio::test]`; AC-14 gate |
| R-15 | spawn_blocking residual — zero grep matches for `spawn_blocking.*store` in server crate |

### Additional Required Tests (AC-17 through AC-20)

- **AC-17**: Migration regression harness — all 12 schema version transitions (v0→v12) on a sqlx connection.
- **AC-18**: `shed_events_total` in `context_status` output — integration test: induce N shed events, assert field reflects N.
- **AC-19**: `Store::close()` awaits drain task completion; pool connection count returns to 0.
- **AC-20**: Impl-completeness test replaces dyn object-safety tests.

---

## Alignment Status

Overall gate recommendation from vision guardian: **PASS WITH CONDITIONS**.

| Check | Status |
|-------|--------|
| Vision alignment (all 5 W0-1 pillars) | PASS |
| Milestone fit (Wave 0 prerequisite) | PASS |
| Architecture consistency | PASS (with VARIANCE-01 noted) |
| Risk completeness | PASS |
| Scope gaps | WARN — see VARIANCE-02 |
| Scope additions | WARN — see VARIANCE-03 |

### VARIANCE-01 (WARN) — analytics.db / knowledge.db File Split

W1+ vision text references `analytics.db` and `knowledge.db` as distinct files. nxs-011 intentionally uses a single file with two pool handles (Decision 4). No transition path from single-file to split-file is planned between nxs-011 and W1-1. Human must clarify before W1-1 scoping whether `analytics.db` language in W1+ is (a) shorthand for analytics tables in the current single file, or (b) requires a discrete file-split feature between nxs-011 and W1-1.

**This is not a blocker for nxs-011 delivery.** Flag before W1-1 scope is written.

### VARIANCE-02 (WARN) — ExtractionRule Async Boundary — DELIVERY BLOCKER

`ExtractionRule::evaluate()` in `unimatrix-observe` must be resolved before delivery begins. The `block_on` bridge panics in a tokio worker thread. Two safe options exist (full async trait conversion of 21 rules, OR `spawn_blocking` bridge at the call site in `background.rs`). The delivery agent cannot proceed with FR-14 without this decision recorded.

**This BLOCKS delivery start. Human decision required.**

### VARIANCE-03 (WARN) — shed_events_total in context_status Extends MCP Tool Contract

AC-18 / FR-16 / NF-05 add `shed_events_total` to the `context_status` MCP tool response schema. This was AC-15 (WARN log only) in the original SCOPE.md. The addition is architecturally sound and directly traceable to vision non-negotiable #7 (analytics observability), but it modifies a live MCP tool's response schema. Human sign-off required before delivery.

---

## Definition of Done

- All 20 acceptance criteria (AC-01 through AC-20) pass.
- All 15 risks addressed: 3 Critical + 8 High scenarios passing; R-08 resolution recorded in spec.
- Zero rusqlite dependency in `unimatrix-store` and `unimatrix-server` Cargo.toml files.
- Zero `lock_conn()`, `spawn_blocking.*store`, `AsyncEntryStore`, `unimatrix_store::rusqlite`, or `MutexGuard` in production code (CI grep gates).
- `sqlx-data.json` committed at workspace root; CI enforces `SQLX_OFFLINE=true`.
- Test count ≥ 1,649 (baseline); all tests `#[tokio::test]` where async.
- `Store::close().await` called in every test that opens a `SqlxStore`.
- Three WARN variances acknowledged by human before delivery starts.
