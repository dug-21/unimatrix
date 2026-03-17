# Test Plan: Server Crate spawn_blocking Removal (server-migration.md)

**Component**: `crates/unimatrix-server/src/` (background.rs, tools.rs, server.rs, store_correct.rs, store_ops.rs, audit.rs)
**Risks**: R-09 (transaction rollback gap), R-15 (spawn_blocking residual)
**ACs**: AC-05, AC-16, AC-18
**Spec reference**: FR-11, FR-12, ADR-002

---

## Transaction Call Sites (ADR-002 — 6 call sites)

The 6 call sites that used `SqliteWriteTransaction` are rewritten to `write_pool.begin().await?`. Each call site requires an individual rollback-on-failure test.

| Call Site | File | Test ID |
|-----------|------|---------|
| server.rs (×3) | `crates/unimatrix-server/src/server.rs` | SM-I-01, SM-I-02, SM-I-03 |
| store_correct.rs | `crates/unimatrix-server/src/store_correct.rs` | SM-I-04 |
| store_ops.rs | `crates/unimatrix-server/src/store_ops.rs` | SM-I-05 |
| audit.rs | `crates/unimatrix-server/src/audit.rs` | SM-I-06 |

---

## Integration Tests — Transaction Rollback (`#[tokio::test]` in `unimatrix-server/tests/`)

For each call site, simulate a failure mid-transaction and assert full rollback.

### SM-I-01: `test_server_txn_site_1_rolls_back_on_error`
- **Arrange**: Open server with SqlxStore; set up state to trigger the first server.rs transaction path
- **Act**: Inject a constraint violation or force an error in the second SQL statement within the transaction
- **Assert**: Database contains neither the first nor the second write (full rollback)
- **Assert**: Caller receives an error (`Err`), not a partial success
- **Teardown**: `store.close().await`
- **Risk**: R-09

### SM-I-02: `test_server_txn_site_2_rolls_back_on_error`
- Same pattern as SM-I-01 for the second server.rs transaction path
- **Risk**: R-09

### SM-I-03: `test_server_txn_site_3_rolls_back_on_error`
- Same pattern for the third server.rs transaction path
- **Risk**: R-09

### SM-I-04: `test_store_correct_txn_rolls_back_on_error` — (context_correct MCP tool)
- **Arrange**: Open server; invoke `context_correct` with an entry ID that causes the second write to fail
- **Assert**: Entry is not partially updated; original state is preserved; `Err` returned to caller
- **Teardown**: `store.close().await`
- **Risk**: R-09

### SM-I-05: `test_store_ops_txn_rolls_back_on_error` — (context_store or context_deprecate MCP tool)
- **Arrange**: Open server; invoke a tool that hits the store_ops.rs transaction path; force mid-transaction failure
- **Assert**: No partial write committed; error returned
- **Teardown**: `store.close().await`
- **Risk**: R-09

### SM-I-06: `test_audit_txn_rolls_back_on_error`
- **Arrange**: Open server; trigger an audit write path; inject a failure
- **Assert**: No partial audit_log entry committed
- **Teardown**: `store.close().await`
- **Risk**: R-09

---

## Integration Tests — Shed Counter in context_status (AC-18)

### SM-I-07: `test_context_status_includes_shed_events_total_zero` — (AC-18)
- **Arrange**: Start server with fresh SqlxStore; no shed events
- **Act**: Call `context_status` MCP tool (via test harness or direct function call)
- **Assert**: Response JSON/struct contains field `shed_events_total` with value `0`
- **Assert**: Field is present even when no events have been shed
- **Teardown**: `store.close().await`
- **Risk**: R-04 (AC-18)

### SM-I-08: `test_context_status_reflects_shed_count_after_saturation` — (AC-18)
- **Arrange**: Start server; saturate analytics queue; trigger 3 shed events
- **Act**: Call `context_status`
- **Assert**: `shed_events_total == 3` in response
- **Teardown**: `store.close().await`
- **Risk**: R-04 (AC-18)

---

## Integration Tests — server.rs startup path

### SM-I-09: `test_server_starts_with_sqlx_store_no_async_entry_store`
- **Arrange**: Build server binary; inspect `server.rs` startup function
- **Act**: Start server via test helper; verify it opens `SqlxStore::open().await` directly
- **Assert**: Server starts without error; no `AsyncEntryStore::new()` in startup code
- **Assert**: `Arc<SqlxStore>` is passed to handlers, not `Arc<AsyncEntryStore<Store>>`
- **Teardown**: Signal shutdown; server closes store
- **Risk**: R-15 (AC-04)

---

## Static Verification (grep gates)

### SM-S-01: Zero `spawn_blocking.*store` in server crate — (AC-05)
- **Check**: `grep -rn "spawn_blocking.*store\." crates/unimatrix-server/src/` returns zero matches
- **Risk**: R-15

### SM-S-02: Zero `AsyncEntryStore` in server crate — (AC-04)
- **Check**: `grep -rn "AsyncEntryStore" crates/unimatrix-server/src/` returns zero matches
- **Risk**: R-15

### SM-S-03: Zero `unimatrix_store::rusqlite` imports in server crate — (AC-13)
- **Check**: `grep -rn "unimatrix_store::rusqlite" crates/unimatrix-server/src/` returns zero matches
- **Risk**: R-05, R-15

### SM-S-04: Zero `SqliteWriteTransaction` or `MutexGuard` in production code — (AC-16)
- **Check**: `grep -rn "SqliteWriteTransaction\|MutexGuard" crates/unimatrix-server/src/ crates/unimatrix-store/src/` returns zero matches
- **Risk**: R-09

### SM-S-05: Zero `lock_conn` in server and observe crates — (AC-03, AC-13)
- **Check**: `grep -rn "lock_conn" crates/unimatrix-server/src/ crates/unimatrix-observe/src/` returns zero matches
- **Risk**: R-15

### SM-S-06: `txn.rs` file deleted
- **Check**: `test -f crates/unimatrix-store/src/txn.rs` must return failure (file does not exist)
- **Risk**: AC-16

---

## Notes

- The exact failure injection mechanism for SM-I-01 through SM-I-06 depends on the specific SQL operations at each call site. The delivery agent must read each call site to determine the most natural injection point:
  - For sites with a unique constraint, insert a conflicting row before the transaction.
  - For sites with FK constraints, reference a non-existent parent row.
  - For audit.rs, an empty/invalid entry_id may trigger the failure.
- If a call site's transaction contains only a single SQL statement (no partial state possible), the rollback test verifies that `?` propagation returns `Err` without panicking — not that partial rows are absent.
- AC-05 (zero spawn_blocking) is verified by SM-S-01, not by a runtime test. The grep gate is the formal criterion.
- SM-I-07 and SM-I-08 may be tested at the Rust unit test level (direct function call to the context_status handler) rather than via the infra-001 MCP harness, which is also acceptable and faster.
