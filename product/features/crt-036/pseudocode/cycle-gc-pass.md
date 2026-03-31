# crt-036: CycleGcPass Store Methods — Pseudocode

**Files:**
- `crates/unimatrix-store/src/retention.rs` (new file)
- `crates/unimatrix-store/src/lib.rs` (add `pub mod retention`)

---

## Purpose

All SQL for the cycle-based GC pass lives in the store crate alongside the table
definitions it operates on. Four new `impl SqlxStore` methods plus two public stats
types. The reference implementation for transaction structure is `gc_sessions()` in
`sessions.rs` — use `pool.begin()` / `txn.commit()` pattern verbatim (entry #2159).

---

## lib.rs Addition

```
// In crates/unimatrix-store/src/lib.rs
// Add alongside existing module declarations:

pub mod retention;
```

---

## File Header

```rust
//! Cycle-based activity GC methods for crt-036.
//!
//! Implements the retention policy: retain activity data (observations, query_log,
//! sessions, injection_log) for the K most recently reviewed feature cycles;
//! prune all older reviewed cycles. Gate: cycle must have a cycle_review_index row.
//!
//! All write methods use write_pool_server(). list_purgeable_cycles uses read_pool().
//! Per-cycle transaction: pool.begin() / txn.commit() (entry #2159, ADR-001).
//! Connection released between cycles (NFR-02, write_pool_server max_connections=1).

use sqlx::Row;

use crate::db::SqlxStore;
use crate::error::{Result, StoreError};
```

---

## Stats Types

```
/// Row counts returned after gc_cycle_activity() commits for one feature cycle.
#[derive(Debug, Default)]
pub struct CycleGcStats {
    pub observations_deleted:  u64,
    pub query_log_deleted:     u64,
    pub injection_log_deleted: u64,
    pub sessions_deleted:      u64,
}

/// Row counts returned by gc_unattributed_activity().
#[derive(Debug, Default)]
pub struct UnattributedGcStats {
    pub observations_deleted:  u64,
    pub query_log_deleted:     u64,
    pub sessions_deleted:      u64,
    pub injection_log_deleted: u64,
}
```

---

## list_purgeable_cycles

```
impl SqlxStore {
    /// Returns feature_cycle IDs for all reviewed cycles outside the K-window,
    /// ordered oldest-first (lowest computed_at). Result is capped to max_per_tick.
    ///
    /// Uses read_pool() — read-only query (entry #3619).
    ///
    /// If total reviewed cycles <= k, returns an empty Vec (nothing to prune).
    /// The cap (max_per_tick) is applied in the SQL LIMIT clause — no post-fetch slicing.
    ///
    /// Also returns the computed_at of the K-th retained cycle (oldest in K-window)
    /// for the PhaseFreqTable alignment check. Returns None when fewer than K cycles
    /// exist in cycle_review_index (no pruning has occurred, no gap is possible).
    pub async fn list_purgeable_cycles(
        &self,
        k: u32,
        max_per_tick: u32,
    ) -> Result<(Vec<String>, Option<i64>)> {

        // Query 1: Fetch the purgeable cycles (all reviewed cycles outside top-K window).
        // The NOT IN subquery with ORDER BY + LIMIT is valid SQLite syntax.
        // Oldest-first ordering (ORDER BY computed_at ASC) ensures the cap processes
        // the oldest cycles before newer ones.
        let purgeable_rows = sqlx::query(
            "SELECT feature_cycle FROM cycle_review_index \
             WHERE feature_cycle NOT IN ( \
                 SELECT feature_cycle FROM cycle_review_index \
                 ORDER BY computed_at DESC \
                 LIMIT ?1 \
             ) \
             ORDER BY computed_at ASC \
             LIMIT ?2",
        )
        .bind(k as i64)
        .bind(max_per_tick as i64)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        let purgeable: Vec<String> = purgeable_rows
            .into_iter()
            .map(|row| row.get::<String, _>(0))
            .collect();

        // Query 2: Fetch the computed_at of the K-th oldest retained cycle.
        // This is the oldest cycle in the K-window — used by the alignment guard (ADR-003).
        // Returns None if fewer than K rows exist (OFFSET k-1 returns no row).
        // When k = 0 this would be a programming error — validated at startup.
        let oldest_retained_computed_at: Option<i64> = sqlx::query(
            "SELECT computed_at FROM cycle_review_index \
             ORDER BY computed_at DESC \
             LIMIT 1 OFFSET ?1",
        )
        .bind((k.saturating_sub(1)) as i64)
        .fetch_optional(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?
        .map(|row| row.get::<i64, _>(0));

        Ok((purgeable, oldest_retained_computed_at))
    }
}
```

---

## gc_cycle_activity

```
impl SqlxStore {
    /// Delete all activity data for one feature cycle in a single transaction.
    ///
    /// Transaction acquired via pool.begin() / txn.commit() (ADR-001, entry #2159).
    /// Connection is released (returned to pool) on return from this method.
    ///
    /// Delete order is mandatory (ADR-001):
    ///   1. observations  — joined through sessions (no direct feature_cycle column)
    ///   2. query_log     — joined through sessions (no direct feature_cycle column)
    ///   3. injection_log — joined through sessions; must precede sessions (FK dependency)
    ///   4. sessions      — deleted last; subqueries in steps 1-3 resolve while sessions exist
    ///
    /// Returns CycleGcStats with per-table row counts.
    /// On error: transaction is rolled back by sqlx; the cycle remains purgeable on
    /// the next tick. Caller must NOT call store_cycle_review() when this returns Err.
    pub async fn gc_cycle_activity(&self, feature_cycle: &str) -> Result<CycleGcStats> {

        // Acquire per-cycle transaction via pool.begin().
        // This is the ONLY transaction for this cycle — never opened outside the cycle loop.
        // The connection is held from here until txn.commit() at the end of this method.
        let mut txn = self
            .write_pool_server()
            .begin()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Step 1: Delete observations for sessions in this cycle.
        // Two-hop join: observations.session_id -> sessions.session_id -> sessions.feature_cycle
        // Uses idx_observations_session index (NFR-03 performance requirement).
        let obs = sqlx::query(
            "DELETE FROM observations \
             WHERE session_id IN ( \
                 SELECT session_id FROM sessions WHERE feature_cycle = ?1 \
             )",
        )
        .bind(feature_cycle)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 2: Delete query_log rows for sessions in this cycle.
        // Two-hop join: query_log.session_id -> sessions.session_id -> sessions.feature_cycle
        // Uses idx_query_log_session index (NFR-03).
        let qlog = sqlx::query(
            "DELETE FROM query_log \
             WHERE session_id IN ( \
                 SELECT session_id FROM sessions WHERE feature_cycle = ?1 \
             )",
        )
        .bind(feature_cycle)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 3: Delete injection_log rows for sessions in this cycle.
        // Must precede sessions DELETE — injection_log.session_id is a logical FK.
        // Uses idx_injection_log_session index (NFR-03, if exists; otherwise verify plan).
        let ilog = sqlx::query(
            "DELETE FROM injection_log \
             WHERE session_id IN ( \
                 SELECT session_id FROM sessions WHERE feature_cycle = ?1 \
             )",
        )
        .bind(feature_cycle)
        .execute(&mut *txn)
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 4: Delete the sessions themselves.
        // Sessions row deleted last — subqueries in steps 1-3 resolved while sessions existed.
        let sess = sqlx::query("DELETE FROM sessions WHERE feature_cycle = ?1")
            .bind(feature_cycle)
            .execute(&mut *txn)
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        // Commit the transaction and release the write pool connection.
        // After this line the connection is available for the next cycle or other writers.
        txn.commit()
            .await
            .map_err(|e| StoreError::Database(e.into()))?;

        Ok(CycleGcStats {
            observations_deleted:  obs.rows_affected(),
            query_log_deleted:     qlog.rows_affected(),
            injection_log_deleted: ilog.rows_affected(),
            sessions_deleted:      sess.rows_affected(),
        })
    }
}
```

---

## gc_unattributed_activity

```
impl SqlxStore {
    /// Delete orphaned and unattributed activity rows.
    ///
    /// Runs after the per-cycle loop. No transaction required — each DELETE is
    /// independently atomic in SQLite.
    ///
    /// Targets:
    ///   1. Observations whose session_id does not exist in sessions (truly orphaned).
    ///   2. query_log rows whose session_id does not exist in sessions (truly orphaned).
    ///   3. injection_log rows for sessions with feature_cycle IS NULL and status != Active.
    ///   4. Sessions with feature_cycle IS NULL and status != Active (unattributed, closed).
    ///
    /// Active-session guard (SR-06): sessions with status = 0 (Active) are NEVER deleted.
    /// status = 0 is the numeric representation of SessionLifecycleStatus::Active.
    pub async fn gc_unattributed_activity(&self) -> Result<UnattributedGcStats> {

        // Step 1: Delete observations with no matching session.
        // idx_observations_session used for the NOT IN subquery (NFR-03).
        let obs = sqlx::query(
            "DELETE FROM observations \
             WHERE session_id NOT IN (SELECT session_id FROM sessions)",
        )
        .execute(self.write_pool_server())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 2: Delete query_log rows with no matching session.
        // idx_query_log_session used for the NOT IN subquery (NFR-03).
        let qlog = sqlx::query(
            "DELETE FROM query_log \
             WHERE session_id NOT IN (SELECT session_id FROM sessions)",
        )
        .execute(self.write_pool_server())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 3: Delete injection_log for unattributed non-active sessions.
        // Must precede the sessions DELETE in step 4 (FK dependency).
        // status != 0 excludes Active sessions (numeric 0 = Active per SessionLifecycleStatus).
        let ilog = sqlx::query(
            "DELETE FROM injection_log \
             WHERE session_id IN ( \
                 SELECT session_id FROM sessions \
                 WHERE feature_cycle IS NULL AND status != 0 \
             )",
        )
        .execute(self.write_pool_server())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        // Step 4: Delete unattributed non-active sessions.
        // Active sessions (status = 0) are guarded — in-flight retrospectives must not
        // lose their session anchor.
        let sess = sqlx::query(
            "DELETE FROM sessions WHERE feature_cycle IS NULL AND status != 0",
        )
        .execute(self.write_pool_server())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        Ok(UnattributedGcStats {
            observations_deleted:  obs.rows_affected(),
            query_log_deleted:     qlog.rows_affected(),
            sessions_deleted:      sess.rows_affected(),
            injection_log_deleted: ilog.rows_affected(),
        })
    }
}
```

---

## gc_audit_log

```
impl SqlxStore {
    /// Delete audit_log rows older than retention_days.
    ///
    /// audit_log.timestamp is stored in Unix seconds (not milliseconds).
    /// Cutoff: strftime('%s', 'now') - retention_days * 86400 (also in seconds).
    /// Uses idx_audit_log_timestamp index for performance.
    ///
    /// Returns the number of rows deleted.
    /// Runs as a single independent DELETE (no transaction needed — single statement).
    pub async fn gc_audit_log(&self, retention_days: u32) -> Result<u64> {

        let result = sqlx::query(
            "DELETE FROM audit_log \
             WHERE timestamp < (strftime('%s', 'now') - ?1 * 86400)",
        )
        .bind(retention_days as i64)
        .execute(self.write_pool_server())
        .await
        .map_err(|e| StoreError::Database(e.into()))?;

        Ok(result.rows_affected())
    }
}
```

---

## Error Handling Summary

| Method | Error behavior |
|--------|---------------|
| `list_purgeable_cycles` | Returns `Err(StoreError::Database(_))` on SQL failure; caller logs + skips GC pass this tick |
| `gc_cycle_activity` | Returns `Err(StoreError::Database(_))` on SQL failure; transaction rolled back by sqlx; caller must skip `store_cycle_review()` call; cycle is retried next tick |
| `gc_unattributed_activity` | Returns `Err(StoreError::Database(_))` on SQL failure; caller logs warn; does not abort remaining tick steps |
| `gc_audit_log` | Returns `Err(StoreError::Database(_))` on SQL failure; caller logs warn; non-critical path |

---

## Key Test Scenarios

- `gc_cycle_activity()` with a cycle that has sessions + observations + query_log + injection_log:
  all four tables pruned to zero rows for that cycle after commit (AC-02, AC-07, AC-08).
- `gc_cycle_activity()` cascade order: delete sessions before injection_log (intentional inversion)
  → test must fail with orphaned injection_log rows (AC-08 mutation test, R-02).
- `gc_cycle_activity()` idempotency: running twice on the same cycle yields zero rows_affected
  on second run, no error (NFR-04, R-06).
- `gc_unattributed_activity()` Active guard: session with feature_cycle IS NULL + status = Active
  → observations NOT deleted; session with feature_cycle IS NULL + status = Closed → deleted (AC-06, R-07).
- `gc_audit_log()` timestamp unit: rows at now-200d deleted, rows at now-100d preserved with
  retention_days = 180 (AC-09, R-12).
- `list_purgeable_cycles()` with total reviewed cycles = K: returns empty Vec (no pruning needed).
- `list_purgeable_cycles()` with max_per_tick = 5 and 20 purgeable: returns exactly 5 oldest (AC-16).
- `list_purgeable_cycles()` oldest_retained: with exactly K cycles, returns computed_at of K-th
  (oldest retained). With fewer than K cycles, returns None (R-16 boundary test).
- EXPLAIN QUERY PLAN for observations DELETE subquery: must reference idx_observations_session (NFR-03, R-09).
- EXPLAIN QUERY PLAN for query_log DELETE subquery: must reference idx_query_log_session (NFR-03, R-09).
- `gc_cycle_activity()` returns `Err` does not advance `raw_signals_available` to 0 (R-06 scenario 2).
