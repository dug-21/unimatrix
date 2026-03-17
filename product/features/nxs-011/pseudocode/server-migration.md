# Component: Server Crate spawn_blocking Removal
## Files: `crates/unimatrix-server/src/` (multiple files, see breakdown below)

---

## Purpose

Removes all 101 `spawn_blocking(|| store.X())` call sites from the server crate and
rewrites the 5 production `begin_write()` / `SqliteWriteTransaction` call sites using
`write_pool.begin().await?` (ADR-002). Removes all `unimatrix_store::rusqlite::*` imports.
Replaces `AsyncEntryStore` in server startup with direct `Arc<SqlxStore>`.

---

## OQ-BLOCK-02 Resolution: Confirmed Production Call Site Count

After auditing `crates/unimatrix-server/src/` (grep for `begin_write`, `SqliteWriteTransaction`):

**5 production call sites** that acquire a `SqliteWriteTransaction`:

| File | Location | Operation |
|------|----------|-----------|
| `server.rs` | ~line 430 (inside `spawn_blocking`) | Entry insert + vector_map + audit: transactional store |
| `server.rs` | ~line 591 (inside `spawn_blocking`) | Correction insert + original deprecation + audit |
| `server.rs` | ~line 1034 | Third server.rs transaction (verify by reading) |
| `services/store_correct.rs` | ~line 88 | Correction chain update |
| `services/store_ops.rs` | ~line 191 | Multi-table atomic operation |

**`infra/audit.rs` is NOT a standalone call site.** `audit.rs` defines `write_in_txn()`,
a helper that accepts a `&SqliteWriteTransaction` passed by the server.rs callers above.
After migration, `write_in_txn()` becomes `async fn write_in_txn(txn: &mut Transaction<'_, Sqlite>, ...)`.
The 4 `begin_write().unwrap()` calls in `audit.rs` lines 322, 336, 358, 379 are in
`#[cfg(test)]` blocks (test functions) — they become `#[tokio::test]` bodies using
`store.write_pool.begin().await`.

**Architecture document is correct: 5 production call sites.**

---

## Pattern 1: spawn_blocking Removal (bulk of 101 sites)

Every call site follows this pattern:

```rust
// BEFORE:
let store = Arc::clone(&self.store);
let result = tokio::task::spawn_blocking(move || {
    store.some_method(args)
}).await
.map_err(|_| ServerError::TaskJoin)?
.map_err(ServerError::from)?;

// AFTER:
let result = self.store.some_method(args)
    .await
    .map_err(ServerError::from)?;
```

The `tokio::task::spawn_blocking` wrapper, the `Arc::clone` for the closure, and the
`JoinHandle` await are all removed. The inner lambda body becomes a direct `.await` call.
The outer function signature becomes `async fn` if it was not already.

---

## Pattern 2: begin_write → write_pool.begin().await (5 sites)

For each of the 5 production call sites (ADR-002):

```rust
// BEFORE (server.rs ~line 430, inside spawn_blocking):
tokio::task::spawn_blocking(move || -> Result<_, ServerError> {
    let txn = store.begin_write()
        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
    let conn = &*txn.guard;

    let id = unimatrix_store::counters::next_entry_id(conn)?;
    conn.execute("INSERT INTO entries ...", rusqlite::params![...])?;
    conn.execute("INSERT INTO vector_map ...", rusqlite::params![...])?;

    // Write audit event into same transaction:
    audit_log.write_in_txn(&txn, audit_event)?;

    // txn.commit() is called here OR txn is dropped (auto-rollback)
    txn.commit()?;
    Ok(record)
}).await...

// AFTER (direct async, no spawn_blocking):
async fn store_with_audit(...) -> Result<_, ServerError> {
    let mut txn = self.store.write_pool.begin().await
        .map_err(|e| ServerError::Core(CoreError::Store(
            StoreError::Database(e.into())
        )))?;

    let id = unimatrix_store::counters::next_entry_id_txn(&mut txn).await?;

    sqlx::query!("INSERT INTO entries ...", /* bind params */)
        .execute(&mut *txn)
        .await
        .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Database(e.into()))))?;

    sqlx::query!("INSERT INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)", ...)
        .execute(&mut *txn)
        .await
        .map_err(...)?;

    // Audit write into the same transaction (audit.rs helper updated):
    self.audit.write_in_txn(&mut txn, audit_event).await?;

    txn.commit().await
        .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Database(e.into()))))?;

    // Rollback on error: sqlx::Transaction's Drop impl rolls back if commit() not called.
    Ok(record)
}
```

The `&mut *txn` dereference syntax is idiomatic sqlx — it coerces `Transaction<'_, Sqlite>`
to `&mut SqliteConnection` for use as an `Executor`.

---

## Updated audit.rs helper

```rust
// infra/audit.rs — write_in_txn signature change:

// BEFORE:
pub fn write_in_txn(
    &self,
    txn: &SqliteWriteTransaction<'_>,
    event: AuditEvent,
) -> Result<u64, ServerError> {
    let conn = &*txn.guard;
    let id = unimatrix_store::counters::read_counter(conn, "next_audit_id")?;
    conn.execute("INSERT INTO audit_log ...", rusqlite::params![...])?;
    ...
}

// AFTER:
pub async fn write_in_txn(
    &self,
    txn: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: AuditEvent,
) -> Result<u64, ServerError> {
    let id = unimatrix_store::counters::read_counter_txn(txn, "next_audit_event_id").await?;
    let new_id = if id == 0 { 1 } else { id };
    unimatrix_store::counters::set_counter_txn(txn, "next_audit_event_id", new_id + 1).await?;

    let target_ids_json = serde_json::to_string(&event.target_ids)?;
    let now = current_unix_seconds();

    sqlx::query!(
        "INSERT INTO audit_log (event_id, timestamp, session_id, agent_id,
            operation, target_ids, outcome, detail)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        new_id as i64, now as i64, event.session_id, event.agent_id,
        event.operation, target_ids_json, event.outcome as i64, event.detail
    )
    .execute(&mut **txn)
    .await
    .map_err(|e| ServerError::Audit(e.to_string()))?;

    Ok(new_id)
}
```

Note: `read_counter_txn` and `set_counter_txn` are new async variants of the counter
functions that accept `&mut sqlx::Transaction` as executor. These are added to
`unimatrix-store/src/counters.rs` alongside the existing pool-based versions.

---

## Server Startup Change (server.rs)

```rust
// BEFORE (server.rs):
let store = Arc::new(
    Store::open(db_path)?  // sync
);
let async_store = AsyncEntryStore::new(Arc::clone(&store));
// use async_store in tool handlers...

// AFTER (server.rs):
let store = Arc::new(
    SqlxStore::open(db_path, PoolConfig::default()).await?  // async
);
// use Arc::clone(&store) directly in tool handlers — no wrapper needed
```

The server struct that previously held `async_store: AsyncEntryStore<Arc<Store>>` now holds
`store: Arc<SqlxStore>`. All tool handler function signatures that accepted
`async_store: &AsyncEntryStore<...>` accept `store: &Arc<SqlxStore>`.

---

## rusqlite Import Removal

All files in the server crate that import rusqlite-related items must have those imports removed:

```rust
// Remove from any file that has them:
use unimatrix_store::rusqlite;
use unimatrix_store::rusqlite::params;
use unimatrix_store::rusqlite::OptionalExtension;
// etc.
```

Known files with rusqlite imports (from IMPLEMENTATION-BRIEF.md + grep):
- `background.rs` — remove rusqlite params!, OptionalExtension usage
- `export.rs` — 23 lock_conn() call sites; all become async sqlx queries on read_pool
- `embed_reconstruct.rs` — 4 lock_conn() sites; async sqlx queries on read_pool
- `registry.rs` — rusqlite imports
- `contradiction.rs` — rusqlite imports
- `server.rs` — rusqlite params! in transaction bodies
- `store_correct.rs` — rusqlite params!
- `store_ops.rs` — rusqlite params!
- `import/inserters.rs` — rusqlite imports
- `listener.rs` — rusqlite imports (if any)

After removing `pub use rusqlite` from `unimatrix-store/src/lib.rs`, any remaining
`unimatrix_store::rusqlite::*` reference will produce a compile error — this is the
intended audit mechanism (SR-01).

---

## File Breakdown and Scope

| File | Change Category | Notes |
|------|----------------|-------|
| `server.rs` | startup + 3 txn sites | SqlxStore::open; remove AsyncEntryStore; rewrite 3 begin_write |
| `background.rs` | spawn_blocking removal (bulk) | All spawn_blocking wrappers on store methods removed; run_extraction_rules call site; see observe-migration.md |
| `tools.rs` | spawn_blocking removal | All spawn_blocking wrappers on store method calls |
| `services/store_correct.rs` | 1 txn site | begin_write → write_pool.begin() |
| `services/store_ops.rs` | 1 txn site | begin_write → write_pool.begin() |
| `infra/audit.rs` | helper fn signature | write_in_txn becomes async; rusqlite params removed |
| `export.rs` | 23 lock_conn() sites | All become async sqlx reads on read_pool |
| `embed_reconstruct.rs` | 4 lock_conn() sites | Async sqlx reads on read_pool |
| `registry.rs`, `contradiction.rs`, `listener.rs`, `import/inserters.rs` | rusqlite import removal | No lock_conn() — only import cleanup |

---

## Transaction Rollback Guarantee (R-09)

At each of the 5 rewritten call sites, rollback on error is automatic via
`sqlx::Transaction`'s `Drop` implementation:
- If any `?` propagates an error before `txn.commit().await`, the transaction is dropped
  without commit.
- `Drop` for `sqlx::Transaction<'_, Sqlite>` sends a ROLLBACK to SQLite.
- This mirrors the behavior of the old `SqliteWriteTransaction::Drop` (R-09).

Each site must NOT call `txn.commit()` in a conditional branch followed by more fallible
operations. The correct pattern is: all operations first (all use `?`), then commit last.

---

## Error Handling

At each `begin_write` → `write_pool.begin()` call site:
- `pool.begin().await` failure → `StoreError::Database` → `CoreError::Store` → `ServerError`
- Any query failure within the transaction → `?` propagates → transaction dropped → rollback

For the 101 `spawn_blocking` removal sites:
- The inner closure result type is unchanged; the outer `JoinHandle` and its error mapping
  (`JoinError::is_panic`) are removed since there is no longer a task to join.
- Functions that were previously `fn` or `async fn` returning after a `spawn_blocking.await`
  remain `async fn` but without the spawn overhead.

---

## Key Test Scenarios

1. **`test_no_spawn_blocking_store_patterns`** (AC-05, R-15): CI grep check —
   `grep -rn "spawn_blocking.*store" crates/unimatrix-server/src/` returns zero matches.

2. **`test_no_async_entry_store_imports`** (AC-04): CI grep check —
   `grep -r "AsyncEntryStore" crates/` returns zero matches.

3. **`test_no_rusqlite_imports_server`** (AC-13): CI grep check —
   `grep -r "unimatrix_store::rusqlite" crates/unimatrix-server/src/` returns zero matches.

4. **`test_no_mutex_guard_in_production`** (AC-16): CI grep check —
   `grep -r "MutexGuard" crates/` returns zero matches in non-test code.

5. **Per-txn-site rollback tests** (R-09): For each of the 5 call sites:
   - Inject a failure on the second operation within the transaction (constraint violation).
   - Assert the DB contains no partial write (neither first nor second operation committed).
   - Total: 5 integration tests, one per call site.

6. **`test_server_startup_uses_sqlx_store`**: Server startup integration test; assert the
   server reaches ready state without panic; assert `SqlxStore::open` was called (not old
   `Store::open`).

7. **`test_existing_server_unit_tests_pass`** (R-14, AC-14): All 1,406 server unit tests
   and 39 integration tests pass after migration. Tests converted from `#[test]` to
   `#[tokio::test]` count as preserved.

8. **`test_audit_write_in_txn_async`**: Call `audit.write_in_txn(&mut txn, event)` from a
   `#[tokio::test]`; assert event is readable after `txn.commit().await`.

---

## OQ-DURING Items Affecting This Component

None specific. The spawn_blocking removal is mechanical. The transaction rewrites follow
the pattern in ADR-002. All open questions for this component have been resolved.
