## ADR-001: Synchronous Write for cycle_review_index (write_pool_server, not analytics queue)

### Context

`store_cycle_review()` must persist the `CycleReviewRecord` before `context_cycle_review`
returns. GH #409 (retention pass) gates signal deletion on the presence of a
`cycle_review_index` row for the cycle. If the write is fire-and-forget (analytics queue),
there is a window — up to the 500ms DRAIN_FLUSH_INTERVAL — during which #409 could execute
its purge and find no row, incorrectly deleting raw signals for a cycle whose review was
actually computed.

Two write strategies were evaluated:

**Option A: analytics queue (fire-and-forget)**
The analytics drain (entry #2148) is suitable for eventually-consistent writes where the
caller never reads back the row within the same request. `co_access`, `query_log`, and
`outcome_index` use this path. `cycle_review_index` does NOT fit this pattern: the stored row
must be visible to #409 immediately after the handler returns. Entry #2125 explicitly lists
this as the disqualifying criterion: "methods whose results are read back by the caller
within the same request or test will see stale state."

**Option B: direct write via write_pool_server() (synchronous)**
Consistent with `insert_session`, `insert_signal`, and `record_feature_entries` — methods
that must be visible immediately. `write_pool_server()` acquires from the max-1 write pool,
serializing writes correctly. Adds latency to the first-call path only (subsequent calls hit
the memoization cache and never reach step 8a).

### Decision

Use `write_pool_server()` for `store_cycle_review()`. The write executes synchronously within
the handler's async task before the response is returned. The handler blocks on the `await`
until the INSERT OR REPLACE completes.

Pool contention note: `write_pool_server()` has max_connections=1. The
`context_cycle_review` computation path is long (observation load, hotspot detection, metric
computation) — by the time step 8a is reached, any competing write has almost certainly
completed. No dedicated connection is needed; the shared pool is correct here.

`store_cycle_review()` must NOT be called from `spawn_blocking`. It is an async sqlx query;
calling it from a blocking thread requires `block_in_place` and risks pool starvation (entry
#2266). Call it directly from the handler's async context.

### Consequences

**Easier**:
- #409 can safely read `cycle_review_index` immediately after any `context_cycle_review` call
  returns — no timing window.
- No drain task modifications needed.
- First-call latency addition is bounded by a single `INSERT OR REPLACE` on a small row.

**Harder**:
- Every first-call `context_cycle_review` incurs a synchronous write before returning. This
  is acceptable: the call was already expensive (full pipeline). The write is ~1ms on local
  SQLite.
- If `write_pool_server()` is under contention (concurrent first-call scenario), the write
  waits behind any in-progress write. The pool acquire timeout (READ_POOL_ACQUIRE_TIMEOUT)
  applies. This is the same behavior as all other synchronous writes in the codebase.
