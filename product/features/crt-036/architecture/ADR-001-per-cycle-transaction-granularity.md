## ADR-001: Per-Cycle Transaction Granularity for the Activity GC Pass

### Context

The GC pass must delete rows from `observations`, `query_log`, `injection_log`, and
`sessions` for each purgeable cycle. Three transaction strategies were considered:

**Option A — Single spanning transaction across all purgeable cycles.**
One `BEGIN` / `COMMIT` covers all cycles in one tick. Provides the strongest
all-or-nothing guarantee across the full pass.

Problem: `write_pool_server()` is configured with `max_connections = 1` (SQLite WAL
mode, entry #2249). A spanning DELETE on the 152 MB `observations` table can hold the
write connection for multiple seconds. During this window, the analytics drain task
and any synchronous write call (e.g., `store_cycle_review()`, `insert_session()`)
block waiting for the connection. This is the deadlock risk identified in SR-01 and
SR-02. The drain task uses the same write pool and cannot yield until the GC transaction
commits.

**Option B — Per-cycle transactions, connection released between cycles.**
Each cycle's four DELETEs (`observations`, `query_log`, `injection_log`, `sessions`)
run in one `pool.begin()` transaction. After `txn.commit()`, the connection is returned
to the pool. The next cycle acquires it fresh.

This bounds the write lock to the duration of one cycle's DELETE set. Between cycles,
the drain task and audit writer can acquire the connection. A tick with many purgeable
cycles interleaves GC work with other write activity rather than monopolizing the
connection.

The per-cycle transaction still guarantees atomic deletion of one cycle's data: if
the process crashes mid-cycle, the partial transaction is rolled back by SQLite and
that cycle is pruned cleanly on the next tick.

**Option C — Per-table DELETE statements, no transactions.**
No transaction per cycle; each of the four DELETEs runs independently. Cheapest
connection hold per statement.

Problem: If the process crashes between the `injection_log` DELETE and the `sessions`
DELETE for the same cycle, `injection_log` rows are gone but `sessions` rows remain,
leaving orphaned `injection_log` rows (the reverse of the cascade dependency). Also,
if `injection_log` is deleted but `sessions` is not, the GC will re-attempt the same
cycle on the next tick, which is harmless but wastes a tick slot.

The cascade dependency (`injection_log` references `session_id` from `sessions`)
requires atomicity of at minimum {injection_log, sessions}. Since `observations` and
`query_log` also join through `sessions`, including all four in one transaction is
the correct boundary.

**Decision: Option B — per-cycle `pool.begin()` transactions.**

Always use `sqlx::Pool::begin()` / `Transaction::commit()` API for per-cycle atomic
deletes. Never issue raw `BEGIN` SQL (entry #2159: raw BEGIN/COMMIT risks silent data
loss because sqlx pool does not guarantee connection identity across multiple `.execute()`
calls without an explicit transaction handle).

The cascade order inside the transaction is fixed: `observations`, `query_log`,
`injection_log` first, `sessions` last. Deleting `sessions` last ensures the subquery
`WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?)` still
resolves for the first three deletes within the same transaction.

### Decision

Use per-cycle `pool.begin()` / `txn.commit()` transactions. Each call to
`gc_cycle_activity(feature_cycle)` acquires exactly one transaction, executes the
four DELETEs in dependency order (observations → query_log → injection_log → sessions),
commits, and returns the connection to the pool.

The `mark_signals_purged()` call that follows each `gc_cycle_activity()` runs as a
separate single-statement write (no transaction needed — a single SQL statement is
already atomic in SQLite). This means it executes after the connection is released
from the GC transaction, preventing any nested connection acquisition.

`gc_unattributed_activity()` runs after the per-cycle loop. Each of its DELETEs is an
independent statement (no cascade dependency that requires a transaction).

`gc_audit_log()` is a single independent DELETE.

### Consequences

Easier:
- Write pool is not monopolized for the duration of the entire GC pass. Drain task
  and audit writer interleave with GC work between cycles.
- No deadlock risk from a long-running transaction holding the sole write connection.
- Crash safety: partial cycles roll back cleanly; the next tick retries the same cycle.
- The `pool.begin()` API pattern is already established in `gc_sessions()` in
  `sessions.rs` — no new patterns introduced.

Harder:
- There is no all-or-nothing guarantee across multiple cycles within a single tick.
  If the process crashes after pruning cycle A but before pruning cycle B, cycle A is
  gone and cycle B is pruned on the next tick. This is correct behavior — each cycle's
  pruning is independently idempotent.
- `mark_signals_purged()` is not in the same transaction as `gc_cycle_activity()`. If
  the process crashes after the DELETE commits but before the UPDATE to
  `raw_signals_available`, the cycle's data is gone but the flag still shows 1. On the
  next tick, `list_purgeable_cycles()` will no longer see this cycle (its sessions are
  gone), but the `raw_signals_available` flag is stale. This is a known minor
  inconsistency: the retrospective report remains valid (computed, stored), and the
  flag will be corrected if `mark_signals_purged()` is ever retried. A future
  consistency scan could identify and repair these. Accepted as low-severity.
