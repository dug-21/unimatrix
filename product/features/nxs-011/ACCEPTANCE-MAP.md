# nxs-011 Acceptance Criteria Map

---

## Pool Architecture

| AC-ID | Description | Verification Method | Verification Detail | Risk Ref | Priority | Status |
|-------|-------------|--------------------|--------------------|----------|----------|--------|
| AC-01 | `unimatrix-store` Cargo.toml depends on `sqlx` (features: `sqlite`, `runtime-tokio`, `macros`) and does not depend on `rusqlite` | grep | `grep -r rusqlite crates/unimatrix-store/Cargo.toml` returns zero matches; `cargo metadata` shows sqlx dependency | R-05, R-15 | Critical | PENDING |
| AC-02 | `Store::open()` constructs `read_pool` (max 6–8) and `write_pool` (max 2) via `SqliteConnectOptions` with all 6 PRAGMAs applied per connection | unit test | Open store, query `PRAGMA journal_mode`, `PRAGMA foreign_keys`, `PRAGMA synchronous`, `PRAGMA foreign_keys` from both pools; assert WAL mode and ON values | R-11 | Critical | PENDING |
| AC-03 | All `Store` methods that perform DB operations are `async fn`; none call `std::sync::Mutex::lock()` or any blocking synchronisation primitive | grep | `grep -r "Mutex::lock\|lock_conn\|spawn_blocking" crates/unimatrix-store/src/` returns zero matches | R-01 | Critical | PENDING |
| AC-09 | `write_pool max_connections` > 2 is rejected at startup with `StoreError::InvalidPoolConfig` | unit test | Construct `PoolConfig { write_max_connections: 3 }`; assert `Store::open()` returns `Err(StoreError::InvalidPoolConfig)` | R-01 | Critical | PENDING |
| AC-10 | Pool `acquire_timeout` is configured on both pools; timeout returns `StoreError::PoolTimeout` (not panic or indefinite block) within 2s (read) or 5s (write) | integration test | Saturate write pool with long-running transactions; assert a new write caller receives `StoreError::PoolTimeout { pool: PoolKind::Write, .. }` within 5s; repeat for read pool within 2s | R-01 | Critical | PENDING |

---

## Analytics Queue

| AC-ID | Description | Verification Method | Verification Detail | Risk Ref | Priority | Status |
|-------|-------------|--------------------|--------------------|----------|----------|--------|
| AC-06 | `AnalyticsQueue` implemented with bounded channel (capacity 1000), drain batch ≤50, drain interval 500ms, shed-under-load for analytics writes, bypass for integrity writes | unit test | Fill queue to capacity (1000 events); attempt 1001st event; assert shed counter == 1; assert WARN log present; integration test: drain task commits ≤50 events per transaction | R-04 | Critical | PENDING |
| AC-07 | Analytics writes (`co_access`, `sessions`, `injection_log`, `query_log`, `signal_queue`, `observations`, `observation_metrics`, `shadow_evaluations`, `feature_entries`, `topic_deliveries`, `outcome_index`) route through `AnalyticsQueue` | integration test + grep | Each write method calls `enqueue_analytics()`; `grep -rn "enqueue_analytics" crates/unimatrix-store/src/write.rs` matches each analytics table write; integration test writes to each analytics table and verifies drain task commits it | R-06 | High | PENDING |
| AC-08 | Integrity writes (`entries`, `entry_tags`, `audit_log`, `agent_registry`, `vector_map`, `counters`) bypass the analytics queue and go directly through `write_pool`; these writes are never dropped under any load condition | integration test | Fill analytics queue to capacity (1000 events); call `write_entry()` (integrity write); assert write succeeds and entry is readable via `get_entry()`; repeat for `audit_log` write | R-04, R-06 | Critical | PENDING |
| AC-15 | Shed events are logged at WARN level with dropped variant name and queue capacity | unit test | Induce a shed event; assert WARN log contains variant name, `queue_len == 1000`, `capacity == 1000` per NF-03 | R-04 | High | PENDING |
| AC-18 | Shed counter increments are visible in `context_status` health output; the field `shed_events_total` is present and reflects cumulative shed events since store open | integration test | Induce N shed events; call `context_status` MCP tool; assert `shed_events_total == N` in response | R-04 | High | PENDING |
| AC-19 | `Store::close()` awaits drain task completion before returning; no test exits with a live drain task holding a `write_pool` connection | integration test | Enqueue events; call `Store::close().await`; assert all events committed and join handle has exited; assert pool connection count returns to 0 | R-02 | Critical | PENDING |

---

## Async API Migration

| AC-ID | Description | Verification Method | Verification Detail | Risk Ref | Priority | Status |
|-------|-------------|--------------------|--------------------|----------|----------|--------|
| AC-04 | `AsyncEntryStore` in `unimatrix-core/src/async_wrappers.rs` is removed; no call sites remain in the server or observe crates | grep | `grep -r "AsyncEntryStore" crates/` returns zero matches | R-07, R-15 | Critical | PENDING |
| AC-05 | All `spawn_blocking(|| store.` call sites in the server crate are removed (baseline: 101) | grep | `grep -rn "spawn_blocking.*store\." crates/unimatrix-server/src/` returns zero matches | R-15 | Critical | PENDING |
| AC-13 | `pub use rusqlite` is removed from `unimatrix-store/src/lib.rs`; no downstream crate references `unimatrix_store::rusqlite` | grep | `grep -r "unimatrix_store::rusqlite" crates/` returns zero matches; `grep -r "pub use rusqlite" crates/unimatrix-store/src/lib.rs` returns zero matches | R-05 | Critical | PENDING |
| AC-16 | `SqliteWriteTransaction<'a>` is retired; no `MutexGuard` lifetime escapes any function boundary in the codebase | grep | `grep -r "SqliteWriteTransaction\|MutexGuard" crates/` returns zero matches in production code (excluding test-only patterns if any) | R-09 | High | PENDING |
| AC-20 | Impl-completeness tests replace object-safety tests for `SqlxStore` + `EntryStore`; `dyn EntryStore` compile tests are removed | compile check + unit test | File `unimatrix-core/tests/impl_completeness.rs` contains `fn assert_entry_store_impl<S: EntryStore>(_: &S) {}`; `#[tokio::test] async fn sqlx_store_implements_entry_store()` compiles and passes; no `dyn EntryStore` trait object construction in test suite | R-07 | High | PENDING |

---

## Migration System

| AC-ID | Description | Verification Method | Verification Detail | Risk Ref | Priority | Status |
|-------|-------------|--------------------|--------------------|----------|----------|--------|
| AC-11 | Existing schema migration logic (`migration.rs`) executes correctly through the sqlx connection infrastructure; all 16 migration integration tests pass | integration test | `cargo test -p unimatrix-store --test migration` passes; all 16 existing tests green | R-03 | Critical | PENDING |
| AC-17 | Migration regression harness covers all 12 schema version transitions (v0→v12) via the adapted `migration.rs` on a sqlx connection | integration test | Open an empty database; run `migrate_if_needed(&mut conn, path).await`; assert each intermediate schema version is reached in sequence; assert final schema version == 12; at least one test per version transition; each test uses a fresh temp DB | R-03 | Critical | PENDING |

---

## CI and Tooling

| AC-ID | Description | Verification Method | Verification Detail | Risk Ref | Priority | Status |
|-------|-------------|--------------------|--------------------|----------|----------|--------|
| AC-12 | `sqlx-data.json` is generated and committed; CI enforces `SQLX_OFFLINE=true` | file-check + shell | File exists at workspace root: `test -f sqlx-data.json`; CI log shows `SQLX_OFFLINE=true` in all `cargo build` and `cargo test` steps in `.github/workflows/release.yml`; `cargo build --offline` succeeds from clean checkout | R-05 | High | PENDING |

---

## Test Infrastructure

| AC-ID | Description | Verification Method | Verification Detail | Risk Ref | Priority | Status |
|-------|-------------|--------------------|--------------------|----------|----------|--------|
| AC-14 | All existing passing tests continue to pass; no net reduction in test count (baseline: 1,649 total) | shell | `cargo test --workspace` passes; count lines matching `^test .* ok$` in test output; assert total ≥ 1,649; tests converted from `#[test]` to `#[tokio::test]` count as preserved | R-10, R-14 | Critical | PENDING |

---

## Summary

| Theme | AC-IDs | Count |
|-------|--------|-------|
| Pool Architecture | AC-01, AC-02, AC-03, AC-09, AC-10 | 5 |
| Analytics Queue | AC-06, AC-07, AC-08, AC-15, AC-18, AC-19 | 6 |
| Async API Migration | AC-04, AC-05, AC-13, AC-16, AC-20 | 5 |
| Migration System | AC-11, AC-17 | 2 |
| CI and Tooling | AC-12 | 1 |
| Test Infrastructure | AC-14 | 1 |
| **Total** | **AC-01 through AC-20** | **20** |

All 20 acceptance criteria must reach PASSED status before the delivery gate closes.
