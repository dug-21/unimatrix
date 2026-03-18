# Risk-Based Test Strategy: nxs-011
## sqlx Migration — Connection Pools + Async-Native Storage

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Pool starvation: write_pool capped at 2 connections blocks callers under sustained write load — drain task holds one connection during batch commit, leaving one for integrity writes | Critical | High | Critical |
| R-02 | Drain task teardown race: drain task outlives test's tokio runtime when `Store::close()` is not called; ~1,445 async test bodies each open/close a Store, raising race probability | Critical | High | Critical |
| R-03 | Migration failure leaves db in inconsistent state: partial schema upgrade followed by pool construction produces undefined behaviour — wrong column layout, missing tables | Critical | Med | High |
| R-04 | Analytics shed silent data loss: co_access, outcome_index, sessions dropped under load; cumulative loss invisible until `context_status` is consulted; operational data gap grows undetected | High | High | High |
| R-05 | sqlx-data.json drift: stale cache after schema change silently disables compile-time SQL validation; build succeeds but query!() macros degrade to unchecked runtime strings | High | High | High |
| R-06 | Integrity write contamination: an analytics write accidentally routed through `write_pool` directly (bypassing queue) contends with integrity writes but is never dropped under shed — wrong table categorisation becomes a correctness defect | High | Med | High |
| R-07 | RPITIT Send bound failure: native async fn trait adoption — associated future types lack explicit `Send` bound; `tokio::spawn` of a task that holds `impl EntryStore` fails at compile time with a non-obvious error | High | Med | High |
| R-08 | ExtractionRule bridge panic: block_on() called inside async context (tokio worker thread) panics with "cannot start a runtime from within a runtime"; 21 detection rules affected by this one call site | High | High | High |
| R-09 | Transaction rollback gap: 5 call sites rewritten from SqliteWriteTransaction to pool.begin().await — any site that returns early via `?` without the sqlx::Transaction Drop path commits a partial write | High | Med | High |
| R-10 | Concurrent read correctness under WAL: read_pool with 6–8 connections; queries that span multiple pool acquisitions may observe inconsistent MVCC snapshots across two read pool connections | Med | Med | Med |
| R-11 | PRAGMA application not per-connection: if SqliteConnectOptions PRAGMA settings are applied once at pool construction rather than per new connection, lazily created pool connections skip WAL mode and foreign_keys | Med | Med | Med |
| R-12 | read_only(true) on read pool breaks WAL checkpoint: SqliteConnectOptions.read_only(true) as defense-in-depth may prevent WAL auto-checkpoint from running, causing unbounded WAL growth | Med | Low | Low |
| R-13 | AnalyticsWrite variant field mismatch: enum variants defined in architecture carry fields that do not exactly match migration.rs schema v12 DDL — silent data truncation or type mismatch at drain commit | Med | Med | Med |
| R-14 | Test count regression: 1,445 sync tests converted to #[tokio::test] — conversion errors (missed test, dropped assert, wrong await placement) reduce effective coverage without failing the test count gate | Med | High | High |
| R-15 | spawn_blocking residual: one or more of the 101 server crate call sites is missed during conversion; synchronous lock acquisition on tokio worker thread re-introduces blocking under load | High | Med | High |

---

## Risk-to-Scenario Mapping

### R-01: Pool Starvation Under Sustained Write Load
**Severity**: Critical
**Likelihood**: High
**Impact**: Integrity writes stall waiting for a write_pool connection while the drain task holds the only available connection. MCP tool callers receive `StoreError::PoolTimeout` for entry writes — data loss on core operations.

**Test Scenarios**:
1. Unit — saturate write pool: spawn 10 concurrent `write_entry` calls against a `PoolConfig { write_max: 2 }` store with `write_acquire_timeout: 1s`; assert that callers beyond the 2-connection cap receive `StoreError::PoolTimeout` within 1s, not a panic or indefinite block.
2. Unit — drain task + integrity write contention: fill analytics queue with 50 events (triggering a drain batch), then immediately call `write_entry` (integrity write); assert write_entry completes within configured timeout (drain batch holds one connection, integrity write must get the other).
3. Integration — write_pool max_connections=3 rejected: construct PoolConfig with write_max=3; assert `Store::open()` returns `Err(StoreError::InvalidPoolConfig)` before any DB is touched.

**Coverage Requirement**: Both pools must be exercised at their connection cap simultaneously. At least one test must show timeout producing `StoreError::PoolTimeout { pool: PoolKind::Write, .. }`.

---

### R-02: Drain Task Teardown Race in Tests
**Severity**: Critical
**Likelihood**: High
**Impact**: Drain task holds a write_pool connection after the test body exits. When the test runtime shuts down, the task is cancelled mid-commit. Co-access and outcome writes are lost. Subsequent tests in the same process may observe stale pool state or spurious panics from cancelled futures accessing a dropped pool.

**Historical evidence**: Entry #2057 — "Store-owned background task requires explicit shutdown protocol before spec — drain task lifecycle risk". This pattern has caused silent test corruption in adjacent features.

**Test Scenarios**:
1. Unit — close() awaits drain task exit: enqueue 10 analytics events, call `Store::close().await`, then assert `drain_handle` has exited (join handle resolved); assert all 10 events are present in the DB.
2. Integration — every test calls close(): write a custom test macro or assertion that verifies `Store::close()` is invoked; rely on TC-02 constraint and add a canary test that deliberately omits close() and asserts it triggers the drop-path warning log.
3. Integration — grace period expiry: inject a drain task that hangs (long sleep); assert `Store::close()` returns within 5s + margin and emits WARNING log; assert no panic.
4. Load — 1,445-test suite run: run full test suite; assert zero "task panicked after runtime shutdown" messages in output.

**Coverage Requirement**: TC-02 (Store::close() in every test) must be mechanically enforced. AC-19 is the formal gate.

---

### R-03: Migration Failure Leaves DB in Inconsistent State
**Severity**: Critical
**Likelihood**: Med
**Impact**: Partial schema upgrade (e.g., version 7 DDL applied, version 8 fails) leaves tables in a mixed-schema state. If pool construction proceeded, queries against old-layout tables produce wrong results — silently corrupted reads, phantom columns, constraint violations.

**Historical evidence**: Entry #378 — "Schema migration tests must include old-schema databases, not only fresh ones." Entry #2060 (ADR-003) directly addresses sequencing; the risk is in the migration SQL translation from rusqlite to sqlx.

**Test Scenarios**:
1. Integration — migration regression harness (AC-17): start with a schema-less temp DB; run `migrate_if_needed` via sqlx connection; assert schema_version == 12 and all 13 expected tables exist; repeat for each intermediate version (v0→v1, v1→v2, … v11→v12) in isolated test cases.
2. Integration — migration failure blocks pool construction: inject a failure at schema version 7 transition (mock SQL error); assert `Store::open()` returns `Err(StoreError::Migration)` and no pool connections are ever opened (verify with connection count = 0).
3. Integration — idempotency: run `migrate_if_needed` twice against a v12 DB; assert no error and schema_version remains 12.
4. Integration — dirty migration connection isolation: verify the migration connection is explicitly dropped before read_pool/write_pool are constructed by asserting pool connections cannot see WAL state from migration in-flight (timing assertion via begin/commit sequence).

**Coverage Requirement**: All 12 schema version transitions must have individual integration tests. Migration failure must produce `StoreError::Migration`, not `StoreError::Open` or a panic.

---

### R-04: Analytics Shed Silent Data Loss
**Severity**: High
**Likelihood**: High
**Impact**: co_access, outcome_index, sessions, query_log writes dropped under sustained MCP tool load. Confidence scoring degraded (co_access is an input to re-ranking). Outcome tracking incomplete. Cumulative loss is invisible unless `context_status` is consulted.

**Test Scenarios**:
1. Unit — shed counter increments: fill the channel to capacity (1000 events), attempt one more `enqueue_analytics` call; assert shed counter == 1 and WARN log contains variant name + capacity.
2. Unit — shed counter cumulates: induce N shed events; call `shed_events_total()`; assert return == N.
3. Integration — context_status exposes shed count (AC-18): induce 5 shed events; call `context_status` MCP tool; assert `shed_events_total == 5` in response.
4. Integration — integrity write unaffected during full queue: fill analytics queue to capacity; call `write_entry` (integrity); assert write_entry succeeds without error (queue fullness must not block integrity path).
5. Unit — WARN log contents: assert shed WARN log includes variant name, queue_len == 1000, capacity == 1000 per NF-03.

**Coverage Requirement**: Shed counter must be tested as both atomic increment and cumulative read. AC-18 must be verified end-to-end through `context_status` tool output.

---

### R-05: sqlx-data.json Drift
**Severity**: High
**Likelihood**: High
**Impact**: Developer adds a `sqlx::query!()` call site without regenerating sqlx-data.json. With `SQLX_OFFLINE=true` set in CI, the build fails with a cryptic macro expansion error rather than a meaningful message. Alternatively, if SQLX_OFFLINE is not enforced, the build silently degrades to unchecked runtime queries — type safety is lost without any visible signal.

**Test Scenarios**:
1. CI — SQLX_OFFLINE enforcement: verify `.github/workflows/release.yml` contains `SQLX_OFFLINE=true` in all cargo build/test steps; CI step fails if absent (AC-12, CI-01).
2. CI — cargo sqlx check pre-build: verify a `cargo sqlx check --workspace` step exists before the build step; assert it produces a structured error message (not a cryptic one) when sqlx-data.json is absent or stale (CI-02).
3. Integration — offline build succeeds: run `cargo build --offline` from a clean checkout with sqlx-data.json committed; assert build succeeds without a live DATABASE_URL.
4. Regression — no rusqlite in store/server: `grep -r "rusqlite" crates/unimatrix-store/Cargo.toml crates/unimatrix-server/Cargo.toml` returns zero matches (AC-01, AC-13, CI-03).

**Coverage Requirement**: Both the CI enforcement path and the developer regeneration path must be tested. The human-readable error requirement (CI-02) must be verified against the actual CI log output.

---

### R-06: Integrity Write Contamination via Wrong Queue Routing
**Severity**: High
**Likelihood**: Med
**Impact**: An `entries` or `audit_log` write is accidentally sent to `enqueue_analytics()` rather than `write_pool` directly. Under normal load: no observable difference (drain task commits it). Under analytics queue saturation: the write is shed. An audit log or entry write is permanently lost without error propagation to the caller.

**Test Scenarios**:
1. Code review gate — static assertion: for each of the 6 integrity tables (entries, entry_tags, audit_log, agent_registry, vector_map, counters), verify the write path calls `write_pool.acquire()` or `write_pool.begin()` — not `enqueue_analytics`. Verify by grep: `grep -rn "enqueue_analytics" crates/unimatrix-store/src/write.rs` must not match any integrity table write.
2. Integration — integrity write survives full analytics queue (AC-08): fill analytics queue to capacity (1000 events), then write an entry via `write_entry()`; assert write succeeds and the entry is readable via `get_entry()`.
3. Integration — audit_log write survives full analytics queue: same pattern for audit_log table.

**Coverage Requirement**: Every integrity table's write path must be explicitly tested under analytics queue saturation to confirm it bypasses the queue.

---

### R-07: RPITIT Send Bound Failures in spawn Contexts
**Severity**: High
**Likelihood**: Med
**Impact**: Server code that calls `tokio::spawn(async move { store.method().await })` fails at compile time with a non-obvious error: "future returned by `method` is not `Send`". This is not a runtime error — it surfaces during compilation of server crate after trait migration. Depending on the number of affected spawn sites, this could be a large refactor blocker.

**Historical evidence**: Entry #2044 — "sqlx dual-pool migration: async fn in traits is not object-safe without boxing" — documents that RPITIT futures do not automatically carry Send bounds through generic parameters; each future must be Send independently.

**Test Scenarios**:
1. Compile-time test — impl-completeness with Send (AC-20): `fn assert_entry_store_impl<S: EntryStore + Send + Sync>(_: &S) {}` called with `&SqlxStore`; verifies that `SqlxStore` itself is `Send + Sync`.
2. Compile-time test — spawn context: write a test that does `tokio::spawn(async move { store.method().await })` where store is `Arc<SqlxStore>`; assert it compiles (Send bound satisfied through Arc).
3. Integration — background task spawn: verify `background.rs` task spawns that hold `Arc<SqlxStore>` compile without Send-bound errors in CI (cargo test --workspace must not produce Send-related compile errors).

**Coverage Requirement**: All `tokio::spawn` sites in `background.rs` and `tools.rs` that hold a store reference must be exercised in the compile-time test suite.

---

### R-08: ExtractionRule block_on Bridge Panic in Async Context
**Severity**: High
**Likelihood**: High
**Impact**: `ExtractionRule::evaluate()` is called from the server's async background task. If the implementation uses `Handle::current().block_on()` (the architect's intermediate-step recommendation) from within a tokio worker thread, this panics: "cannot start a runtime from within a runtime". All 21 detection rules in `unimatrix-observe` are affected.

**This is the open question identified by the architect.** Neither resolution option is free of risk:
- Full async trait conversion: correct but touches all 21 rule implementations — scope expansion risk.
- block_on bridge: panics on the active tokio runtime — correctness defect in production.

**Test Scenarios**:
1. Integration — observe background task does not panic: run the full server integration test suite with `dead_knowledge.rs` migrated; assert no "cannot start a runtime from within a runtime" panic in test output.
2. Unit — ExtractionRule::evaluate() called from async context: write a `#[tokio::test]` that directly calls the dead knowledge extraction rule's evaluate method; assert it completes without panic.
3. Integration — all 21 detection rules execute: run a session that triggers the observation pipeline; assert all rule evaluations complete; assert no panics logged at ERROR.
4. Compile-time — if full async conversion chosen: verify all 21 rule implementations compile with async evaluate signature.

**Coverage Requirement**: This risk must be resolved before delivery begins. The tester must confirm which resolution path was chosen (async trait vs bridge) and adapt scenario 2 accordingly. If the block_on bridge is chosen, scenario 1 and 2 are regression tests for the panic path. If full async conversion is chosen, scenario 4 is required.

---

### R-09: Transaction Rollback Gap at Rewritten Call Sites
**Severity**: High
**Likelihood**: Med
**Impact**: A rewritten call site (one of 5) uses `pool.begin().await?` but contains an early-return error path that does not reach `txn.commit().await?`. With the old `SqliteWriteTransaction`, the MutexGuard drop rolled back automatically. With `sqlx::Transaction`, the Drop impl also rolls back — but only if the transaction is still open. If the implementer accidentally calls `txn.commit()` before an operation that can fail, a partial commit is permanent.

**Test Scenarios**:
1. Integration — per call site rollback on error: for each of the 5 call sites (server.rs ×3, store_correct.rs, store_ops.rs, audit.rs), write a test that injects a failure mid-transaction (e.g., constraint violation on second insert); assert the DB contains neither the first nor the second write (full rollback).
2. Unit — commit not called on early return: for each rewritten call site, simulate an `Err` return before `txn.commit()`; assert the transaction was rolled back by reading the table and confirming no partial write.
3. Code review gate: verify each of the 5 call sites has the pattern `let mut txn = write_pool.begin().await?; ...; txn.commit().await?` with no `commit()` calls inside conditional branches that skip remaining operations.

**Coverage Requirement**: All 5 call sites must have individual rollback-on-failure tests. No call site may be untested.

---

### R-10: Concurrent Read Correctness Under WAL MVCC
**Severity**: Med
**Likelihood**: Med
**Impact**: A logical operation that requires read-then-write consistency (e.g., read an entry's confidence score, update based on it) uses two pool connections — one from read_pool for the read, one from write_pool for the write. Under WAL MVCC, the read snapshot is taken at the read connection's BEGIN. Between the read and the write, another writer may have updated the same row. The write proceeds on stale data.

**Test Scenarios**:
1. Integration — concurrent read after write sees committed data: two goroutines: one writes an entry (write_pool), one reads it (read_pool); assert the reader eventually sees the written value (WAL visibility with read pool read-only mode).
2. Integration — no dirty reads: hold a write transaction open (but uncommitted); concurrently read the same row via read_pool; assert the reader does not see the uncommitted write.
3. Integration — read_only(true) does not block WAL checkpoint: open a read pool, run 1001 writes (above wal_autocheckpoint=1000), assert WAL checkpoint completes and WAL file does not grow unboundedly.

**Coverage Requirement**: At minimum, dirty-read isolation test (scenario 2) must pass. Scenario 3 addresses the related R-12 risk.

---

## Integration Risks

### Pool–Drain Task Interaction
The drain task holds a `write_pool` connection for the duration of a batch commit. With `write_pool max_connections=2`, this leaves exactly one connection for concurrent integrity writes. Under burst analytics load (50-event batch committing), an integrity write caller must wait for the drain batch to complete. The 5s write_pool acquire_timeout is the safety net, but the nominal case (drain batch at SQLite speeds, <50ms) is expected to clear before the timeout. Any drain task implementation that holds the connection longer than expected (e.g., 1000-event backlog drained in multiple slow batches) degrades integrity write latency.

**Test coverage**: R-01 scenario 2 covers this interaction directly.

### Migration Connection vs Pool Construction Sequencing (ADR-003)
`Store::open()` must drop the migration connection before constructing read_pool and write_pool. If the migration connection is not explicitly dropped (relying on end-of-scope drop), and the scope extends past pool construction, SQLite may see two concurrent connections to the same file during pool construction. On WAL mode this is safe but unexpected. The explicit `drop(migration_conn)` before pool construction is verified by the architecture — the risk is in implementation drift.

**Test coverage**: R-03 scenario 4 verifies migration connection isolation.

### Analytics Queue Full → Integrity Write Path Isolation
When the analytics queue is full, `enqueue_analytics()` returns immediately (shed). This must not affect the integrity write path. The two paths share `write_pool` but the analytics queue is a pure-mpsc channel; queue fullness does not propagate to pool acquisition. The risk is in accidental code that checks queue state before routing integrity writes.

**Test coverage**: R-06 scenario 2 and 3 cover this.

### ExtractionRule Async Boundary (observe crate)
`dead_knowledge.rs` is the only file in `unimatrix-observe` that touches the store. Its migration to async sqlx is contained. The broader `ExtractionRule` trait and 21 rule implementations may or may not need async signatures depending on the R-08 resolution path. If the block_on bridge is used as an intermediate step, the observe crate's async boundary is fragile: any future caller that invokes `evaluate()` from a new async context will hit the same panic.

---

## Edge Cases

1. **Zero-event drain flush**: drain task waits 500ms with an empty channel (no events since last flush); assert the task does not spin-loop or busy-wait; CPU usage must be negligible during idle periods.
2. **Single-event batch**: one event in queue; drain task collects it, waits 500ms for more (none arrive), commits a batch of one; assert the single event is committed and not duplicated.
3. **Exactly 50 events in batch**: queue has exactly 50 events; drain task collects all 50 without waiting the 500ms flush interval; assert batch commits exactly 50 rows.
4. **51 events in queue**: drain task collects 50, commits, then collects the 51st in the next batch; assert 51 rows total across two commits with no duplicates.
5. **Shutdown signal during active batch**: shutdown signal arrives while the drain task is mid-batch (awaiting write_pool.begin()); assert the task finishes the current batch and then commits any remaining channel items before exiting.
6. **Shutdown signal with empty queue**: shutdown signal arrives when channel has zero pending events; assert `Store::close()` returns promptly (well under 5s grace period).
7. **Pool acquire timeout at exactly the boundary**: acquire_timeout of 500ms (test config); inject a 499ms delay on pool release; assert the acquire succeeds. Inject a 501ms delay; assert `StoreError::PoolTimeout` is returned.
8. **Database file path with UTF-8 special characters**: path containing spaces or non-ASCII characters; assert `Store::open()` succeeds (panic guard: `expect("valid UTF-8 path")` in migration connection construction).
9. **write_pool max_connections=1 (boundary)**: valid config (1 ≤ 2); assert pool construction succeeds and integrity writes serialize correctly.
10. **Shed counter overflow**: `AtomicU64` shed counter wraps at u64::MAX after ~1.8×10^19 shed events — not a practical concern but the counter must use `Ordering::Relaxed` (not sequentially consistent) for performance; verify the ordering choice is not incorrectly changed to `SeqCst` under review.
11. **AnalyticsWrite::Unknown variant catch-all**: send a variant that matches the `_ =>` arm in the drain task match (simulated by adding a test-only variant via cfg(test)); assert the catch-all logs at DEBUG and does not panic.

---

## Security Risks

### Untrusted Input Surface
`SqlxStore` accepts data from MCP tool callers (external agents) and stores it. The migration from rusqlite to sqlx changes the SQL execution mechanism but not the data model. The relevant trust surface is:

**Query strings stored in `query_log`**: Free-text query strings from agents are stored in the `query_log` analytics table. These are routed through `AnalyticsWrite::QueryLog { query_text }` and written via parameterised `sqlx::query!()`. SQL injection is not possible through parameterised queries. The risk is in any `format!()` string interpolation into a query — verifiable by ensuring all queries use `sqlx::query!()` macros or `sqlx::query()` with explicit bind parameters.

**Database file path**: `Store::open()` accepts a path. The architecture includes `expect("valid UTF-8 path")` for the migration connection, which panics on non-UTF-8 paths. This is not a security risk for the current deployment (local file paths from server startup config) but would be a panic vector if path input were ever externally controlled.

**Blast radius if analytics queue is exploited**: The analytics queue is write-only (fire-and-forget from the caller's perspective). A malicious caller that floods analytics events can exhaust the queue (capacity 1000) and cause shed events for legitimate analytics writes. This is a denial-of-service against analytics observability, not against integrity writes. The shed counter and WARN log are the detection mechanism.

**Blast radius if write_pool is exhausted by integrity write callers**: A malicious or buggy MCP client that issues rapid successive integrity write calls can exhaust write_pool (2 connections) and cause `StoreError::PoolTimeout` for legitimate callers. The 5s timeout limits the blast radius — callers receive errors rather than hanging indefinitely. This is the correct behavior (fail fast, structured error).

**sqlx::query!() macro safety**: Compile-time verified queries cannot be injected at runtime. The risk is in any query built with string formatting. The CI grep check (CI-03) that rejects rusqlite re-introduction should be paired with a CI check that rejects `format!("SELECT...{}")` patterns in store crate SQL.

---

## Failure Modes

| Failure | Expected Behavior | Observable Signal |
|---------|------------------|------------------|
| write_pool saturated (both connections held) | `StoreError::PoolTimeout { pool: Write, elapsed }` returned to caller within 5s | Structured error to MCP tool; no panic; no indefinite block |
| read_pool saturated | `StoreError::PoolTimeout { pool: Read, elapsed }` returned within 2s | Same |
| analytics queue full | Analytics write shed; shed counter incremented; WARN log emitted | `shed_events_total` in `context_status` output |
| drain task batch commit failure | ERROR log with batch size and error; batch discarded; task continues | No error propagated to callers; analytics loss is acceptable |
| drain task panic | `DrainTaskPanic` returned by `Store::close()` via join handle resolution | `StoreError::DrainTaskPanic` surfaced; server may restart |
| migration failure | `StoreError::Migration { source }` from `Store::open()`; pool construction blocked; server does not start | Structured error at server startup; DB file left in pre-migration state (no partial schema applied if transaction rolled back) |
| drain task does not exit within 5s grace period | `Store::close()` logs WARNING and returns; pool is dropped | WARNING log; no panic; potential analytics loss for events not yet committed |
| `Store::open()` with write_max > 2 | `StoreError::InvalidPoolConfig` before any DB touch | Server does not start; config error is clear and actionable |
| ExtractionRule block_on called in async context | Panic: "cannot start a runtime from within a runtime" | Server crash (if not caught); requires R-08 resolution |
| sqlx-data.json absent in CI | `compile_error!` macro expansion with human-readable message | CI build fails with structured error; not a cryptic macro failure |

---

## Scope Risk Traceability

| Scope Risk | Description | Architecture Risk | Resolution |
|-----------|-------------|------------------|------------|
| SR-01 | 312 rusqlite call sites migrated — missed site compiles silently | R-15 | `pub use rusqlite` removal triggers compile-time audit; CI-03 grep check rejects re-introduction; AC-13 verifies zero remaining imports |
| SR-02 | RPITIT `async fn` in traits sheds dyn-dispatch — future `dyn EntryStore` fails with non-obvious error | R-07 | ADR-005: trait documented as non-object-safe; impl-completeness tests replace dyn compile tests (AC-20); doc comment on trait (ADR-005 consequence) |
| SR-03 | `sqlx-data.json` becomes required committed artefact — stale cache causes confusing compile errors | R-05 | ADR-004: workspace-level single file; CI-02 `cargo sqlx check` pre-build step; human-readable error on stale cache (AC-12, NF-07) |
| SR-04 | `migration.rs` adapted to sqlx — migration logic never tested against sqlx; regression silently corrupts schema version | R-03 | ADR-003: dedicated non-pooled migration connection before pool construction; AC-17 migration regression harness covering all 12 version transitions |
| SR-05 | `AsyncVectorStore` / `AsyncEmbedService` disposition unresolved — scope creep temptation during implementation | — | C-06 hard constraint: non-DB wrappers untouched; any removal requires separate scope approval; not an architecture-level risk — it is a delivery discipline constraint |
| SR-06 | `AnalyticsWrite` enum without sealed extension pattern — Wave 1 additions break match exhaustiveness | — | C-08: `#[non_exhaustive]` on `AnalyticsWrite` (FR-17); drain task catch-all arm; Wave 1 additions do not require drain task match changes in dependent crates |
| SR-07 | `unimatrix-observe` direct rusqlite use — fails to compile the moment `pub use rusqlite` removed | — | C-09: observe crate migrates in same delivery wave (FR-14); AC-13 covers observe compile check |
| SR-08 | Analytics shed silently drops writes — co_access / outcome_index loss invisible beyond WARN log | R-04 | FR-16 / NF-05: `shed_events_total` AtomicU64 exposed in `context_status` output (AC-18); cumulative counter since store open |
| SR-09 | Store-owned drain task outlives Store drop in short-lived test runtimes | R-02 | FR-04 drain task lifecycle with oneshot shutdown + 5s grace; TC-02 mandates `Store::close()` in every test; AC-19 integration test verifies close() awaits task exit |
| SR-10 | 1,445 sync tests converted to `#[tokio::test]` — conversion errors change behavior silently | R-14 | NF-06 test count baseline (1,649 total); TC-01 through TC-05 test infrastructure constraints; AC-14 no net reduction gate |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 11 scenarios minimum; all must pass before delivery gate |
| High | 8 (R-04, R-05, R-06, R-07, R-08, R-09, R-14, R-15) | 24 scenarios minimum; R-08 blocks delivery until ExtractionRule path is resolved |
| Medium | 4 (R-10, R-11, R-13, R-14) | 8 scenarios minimum |
| Low | 1 (R-12) | 1 scenario (scenario 3 under R-10 covers WAL checkpoint) |

**Total risks**: 15
**Total required scenarios**: 44 minimum across unit, integration, compile-time, CI, and load categories

---

## Test Categories

### Unit Tests (unimatrix-store/src/)
Target individual components without full server lifecycle:
- Pool construction with valid/invalid PoolConfig (R-01 scenario 3, NF-01)
- Shed counter increment and read (R-04 scenarios 1, 2)
- WARN log content on shed event (R-04 scenario 5)
- Drain batch size boundaries: 1, 49, 50, 51 events (edge cases 2, 3, 4)
- Pool acquire timeout exact boundary (edge case 7)
- `enqueue_analytics()` returns immediately without acquiring write_pool (R-06)
- Integrity write under full analytics queue (R-06 scenario 2)

### Integration Tests (unimatrix-store/tests/ + unimatrix-server/tests/)
Full Store lifecycle including migration:
- Migration regression harness: all 12 version transitions, isolated temp DBs (R-03, AC-17)
- Migration failure blocks pool construction (R-03 scenario 2)
- `Store::close()` awaits drain task; events committed before return (R-02 scenario 1, AC-19)
- `context_status` shed_events_total field (R-04 scenario 3, AC-18)
- Transaction rollback at all 5 rewritten call sites (R-09)
- Concurrent read does not see uncommitted write (R-10 scenario 2)
- PRAGMA verification: query `PRAGMA journal_mode` and `PRAGMA foreign_keys` from both pools (AC-02)
- Impl-completeness test: `assert_entry_store_impl(&store)` compiles and runs (R-07, AC-20)
- ExtractionRule evaluate from async context (R-08 scenarios 1, 2)

### Regression Tests
- Full test suite: all 1,649 baseline tests pass (AC-14, NF-06, R-14)
- Zero `spawn_blocking.*store` patterns in server crate (AC-05, R-15)
- Zero `AsyncEntryStore` import sites (AC-04)
- Zero `unimatrix_store::rusqlite` import sites (AC-13)
- Zero `MutexGuard` in production code (AC-16)
- Zero `lock_conn()` call sites (AC-03)

### Performance / Load Tests
- Pool saturation under 10 concurrent write callers: timeout behavior, no panic (R-01 scenario 1)
- Drain throughput under 1000-event burst: all events committed within 10s of enqueueing (R-02 scenario 4 proxy)
- Drain task CPU during idle: assert no spin-loop during 2s quiet period (edge case 1)

---

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" — found entry #1105 (outcome, pass), #1775 (bugfix outcome), #141 (glass box validation convention), #167 (gate result handling procedure); no directly applicable lesson-learned entries for this domain.
- Queried: /uni-knowledge-search for "risk pattern async storage pool migration" — found entry #2044 (sqlx dual-pool RPITIT non-object-safety pattern, directly informs R-07), #2057 (drain task shutdown protocol pattern, directly informs R-02).
- Queried: /uni-knowledge-search for "SQLite migration connection pool async spawn_blocking" — found entry #2060 (ADR-003 migration connection sequencing, directly informs R-03), #378 (migration tests must include old-schema DBs, informs R-03 scenario 1).
- Stored: nothing novel to store — the risk patterns identified here (pool starvation, drain task teardown race, ExtractionRule bridge panic) are feature-specific manifestations of existing patterns already in Unimatrix (#2044, #2057). No cross-feature pattern emerged that is not already captured.
