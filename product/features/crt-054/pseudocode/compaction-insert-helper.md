# Component 8 — compaction-INSERT helper + failure counter

**Files**:
- `crates/unimatrix-store/src/write_ext.rs` (or `write.rs`) (modify) — the raw single-statement INSERT on `Store`.
- `crates/unimatrix-server/src/services/store_ops.rs` (modify) — the thin `StoreService::insert_compaction_event` wrapper that the writer (Component 6) calls and that increments the named failure counter on error.

**Existing facilities (verified)**: `unimatrix-store/src/counters.rs` provides `increment_counter(conn, name, delta)` and the durable `counters` table — reuse this for the named failure counter (no new metrics subsystem needed). Store INSERTs use `sqlx::query("INSERT ...").bind(...).execute(...)` (e.g. `write.rs:33, :78`).

**ADRs**: ADR-007 (single autocommit INSERT, NO explicit transaction, named failure counter, non-blocking, content-free).

## Purpose

A thin, parameterized, single-statement autocommit INSERT for `compaction_events`, plus the named durable counter `compaction_events_insert_failed` incremented on failure. No explicit transaction (autocommit). The wrapper returns `Result<()>` so the writer can log + fall through.

## Constants

```
// In unimatrix-store (alongside the counters.rs well-known names, or in a crt-054 module).
const COMPACTION_EVENTS_INSERT_FAILED: &str = "compaction_events_insert_failed";
```

## Store-level raw INSERT (`unimatrix-store`)

```
// On Store (write_ext.rs / write.rs). Single autocommit statement, parameterized
// (no string interpolation → no SQL-injection surface for session_id).
pub async fn insert_compaction_event(
    &self,
    session_id: &str,
    compacted_at_secs: i64,
    high_water: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO compaction_events (session_id, compacted_at, high_water) VALUES (?1, ?2, ?3)"
    )
    .bind(session_id)
    .bind(compacted_at_secs)   // Unix SECONDS
    .bind(high_water)
    .execute(/* the write pool / acquired conn */)
    .await?;                   // autocommit — no BEGIN/COMMIT; id auto-assigned (INTEGER PRIMARY KEY rowid)
    Ok(())
}
```

- `id` is omitted from the column list → SQLite auto-assigns the rowid PK.
- Autocommit: a bare `execute` on the pool commits the single statement; no `begin()`/transaction object. Matches the "single autocommit INSERT, no explicit transaction" contract (ADR-007).
- Follow the existing write-pool acquisition convention (the store may route writes through a dedicated write pool — mirror how `write.rs::insert` / the GH #302 write-pool-starvation pattern acquires the connection). The helper does NOT hold any cross-statement lock.

## Service wrapper + failure counter (`services/store_ops.rs`)

```
impl StoreService
    pub(crate) async fn insert_compaction_event(
        &self,
        session_id: &str,
        compacted_at_secs: i64,
        high_water: i64,
    ) -> Result<(), ServiceError> {
        match self.store.insert_compaction_event(session_id, compacted_at_secs, high_water).await {
            Ok(()) => Ok(()),
            Err(e) => {
                // NAMED failure counter — durable, not a generic log (ADR-007 §6, R-15, AC-04a).
                // Best-effort, content-free: increment the well-known counter row. If the counter
                // bump ITSELF fails (same degraded store), swallow it — we must never panic the
                // compaction handler — but the primary Err still propagates so the writer logs + falls through.
                let _ = self.bump_compaction_insert_failed_counter().await;
                Err(ServiceError::from(e))   // writer logs ids/counts only + proceeds (non-blocking)
            }
        }
    }

    // Increments the durable named counter via counters::increment_counter on a write conn.
    async fn bump_compaction_insert_failed_counter(&self) -> Result<(), ServiceError> {
        let mut conn = self.store.acquire_write_conn().await?;     // or the store's conn-acquire convention
        counters::increment_counter(&mut conn, COMPACTION_EVENTS_INSERT_FAILED, 1).await?;
        Ok(())
    }
```

### Why the durable `counters` table (not an in-process atomic)

The existing `counters` table + `increment_counter` is the project's durable named-counter mechanism, queryable by crt-055 to cross-check row-count vs `increment_compaction` drift (ADR-007 §6 / R-15 downstream-detector note). An in-process `AtomicU64` would not survive restart and crt-055 could not read it. If a test or reviewer prefers a process metric, the durable counter is the load-bearing choice here because the drift check is cross-process.

**Content-free guarantee**: the counter name is a fixed literal; no `session_id`/bytes are in the counter name or value (R-15 sc.2, AC-04a). The value is a pure count.

## Error handling

- Primary INSERT failure → named counter incremented (best-effort) + `Err` returned to the writer, which logs ids/counts only and falls through (non-blocking; the ACK is never blocked).
- Counter-bump failure → swallowed (`let _ =`); never masks or replaces the primary error path; never panics.
- No transaction to roll back (single autocommit statement).

## Key test scenarios (hints)

- Happy path: helper inserts one row; `id` auto-assigned; `compacted_at`/`high_water` round-trip exactly (AC-02).
- Parameterized: a `session_id` containing SQL metacharacters is stored literally (no injection) (security).
- **Forced failure (AC-04a, R-15)**: make the INSERT fail (e.g. drop the table / read-only store in a fixture); assert (a) the named counter `compaction_events_insert_failed` increments by exactly 1, (b) the wrapper returns `Err`, (c) no row lands, (d) no content in the counter name/value or any log.
- Counter-bump-also-fails path: helper still returns the primary `Err`, does not panic.
- Autocommit: the row is visible to a subsequent read without an explicit commit (single-statement autocommit).
