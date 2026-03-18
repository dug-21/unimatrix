# Agent Report: nxs-011-agent-1-architect

## Status: COMPLETE

## Files Produced

- `/workspaces/unimatrix/product/features/nxs-011/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/nxs-011/architecture/ADR-001-pool-acquire-timeout.md`
- `/workspaces/unimatrix/product/features/nxs-011/architecture/ADR-002-write-transaction-retirement.md`
- `/workspaces/unimatrix/product/features/nxs-011/architecture/ADR-003-migration-connection-sequencing.md`
- `/workspaces/unimatrix/product/features/nxs-011/architecture/ADR-004-sqlx-data-json-placement.md`
- `/workspaces/unimatrix/product/features/nxs-011/architecture/ADR-005-native-async-trait.md`

## ADRs Produced

| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | Pool Acquire Timeout Values | #2058 |
| ADR-002 | SqliteWriteTransaction Retirement — Direct pool.begin() at call sites | #2059 |
| ADR-003 | Migration Connection Sequencing — Dedicated non-pooled connection before pool construction | #2060 |
| ADR-004 | sqlx-data.json Placement — Single workspace-level file | #2061 |
| ADR-005 | Native async fn in EntryStore Trait (RPITIT, Rust 1.89) | #2062 |

## Key Decisions

### ADR-001: Timeout Values (resolves OQ-01)
- `read_pool` acquire timeout: **2 seconds**
- `write_pool` acquire timeout: **5 seconds**
- Test default: 500ms read / 1s write via `PoolConfig::test_default()`
- Named constants: `READ_POOL_ACQUIRE_TIMEOUT`, `WRITE_POOL_ACQUIRE_TIMEOUT` in `pool_config.rs`

### ADR-002: WriteTransaction retirement (resolves OQ-02)
- Direct `write_pool.begin().await?` at all 5 call sites — no typed wrapper
- `txn.rs` deleted; `SqliteWriteTransaction` removed from public API
- Rollback on `Drop` preserved via `sqlx::Transaction`'s Drop impl

### ADR-003: Migration sequencing (resolves SR-04)
- Non-pooled `SqliteConnection` opened before pool construction
- `migrate_if_needed` becomes `async fn (&mut SqliteConnection, &Path)`
- Failure returns `StoreError::Migration` — pool construction aborted

### ADR-004: sqlx-data.json (resolves OQ-04)
- Single workspace-level file via `cargo sqlx prepare --workspace`
- CI: `SQLX_OFFLINE=true` + `cargo sqlx check --workspace` pre-build step

### ADR-005: Async trait strategy (resolves SCOPE Q1)
- Native RPITIT `async fn` — no `async_trait` crate (C-02 compliance)
- Trait is non-object-safe by design; `dyn EntryStore` forbidden
- Object-safety tests replaced with impl-completeness tests (AC-20)

## Architecture Highlights

### New source files
- `unimatrix-store/src/pool_config.rs` — `PoolConfig`, timeout constants
- `unimatrix-store/src/analytics.rs` — `AnalyticsWrite` enum, drain task, constants

### Deleted files
- `unimatrix-store/src/txn.rs` — `SqliteWriteTransaction` retired

### Key struct fields
```rust
pub struct SqlxStore {
    read_pool:    SqlitePool,
    write_pool:   SqlitePool,
    analytics_tx: mpsc::Sender<AnalyticsWrite>,
    shutdown_tx:  Option<oneshot::Sender<()>>,
    drain_handle: Option<JoinHandle<()>>,
    shed_counter: Arc<AtomicU64>,
}
```

### AnalyticsWrite enum
`#[non_exhaustive]` with 11 variants: CoAccess, SessionUpdate, InjectionLog, QueryLog,
SignalQueue, Observation, ObservationMetric, ShadowEvaluation, FeatureEntry, TopicDelivery,
OutcomeIndex. Wave 1 additions (GraphEdge, ConfidenceWeightUpdate) add without breaking
drain task match in dependent crates.

### observe crate migration
`dead_knowledge.rs` `query_accessed_active_entries` becomes async sqlx query on `read_pool`.
`ExtractionRule::evaluate()` trait update flagged as open question for delivery team.

## Knowledge Stewardship

### Stored in Unimatrix

| Entry | Title | Category |
|-------|-------|----------|
| #2058 | ADR-001: Pool Acquire Timeout Values | decision |
| #2059 | ADR-002: SqliteWriteTransaction Retirement | decision |
| #2060 | ADR-003: Migration Connection Sequencing | decision |
| #2061 | ADR-004: sqlx-data.json Placement | decision |
| #2062 | ADR-005: Native async fn in EntryStore Trait (RPITIT) | decision |
| #2065 | ADR-006: ExtractionRule Async Conversion (Option A — RPITIT) | decision |

### Queried

Unimatrix was queried for existing SQLite connection pool patterns, async trait conventions, and migration sequencing precedents before producing architecture decisions.

---

## Open Questions for Delivery Team

1. **`ExtractionRule` trait signature** — `evaluate()` is currently sync. `dead_knowledge.rs`
   needs async store access. Options: (a) convert the entire trait to async, (b) spawn a
   blocking runtime handle inside the sync evaluate method. Affects all 21 detection rules.
   Needs delivery team confirmation before implementing `dead_knowledge.rs` migration.

2. **Drain shutdown timeout configurability** — Defined as constant `DRAIN_SHUTDOWN_TIMEOUT = 5s`.
   If test suites need shorter values, `PoolConfig::test_default()` can carry it. Not blocking.

3. **`read_only(true)` on read pool** — defense-in-depth. If sqlx SQLite driver's `read_only`
   mode causes issues with WAL checkpoint, remove it. Routing architecture prevents writes
   through read_pool at the code level regardless.
