# SPECIFICATION: crt-036 — Intelligence-Driven Retention Framework

## Objective

Replace Unimatrix's age-based observation pruning (a hard 60-day DELETE) with a
cycle-aligned GC policy that retains data for the most recently reviewed K feature
cycles and prunes all older reviewed cycles. The correct retention criterion is learning
utility — a row produced six months ago for a reviewed cycle still in the K window is
kept; a row produced last week for a reviewed cycle outside the window is eligible for
deletion. A new `[retention]` block in `config.toml` exposes `activity_detail_retention_cycles`
(default 50) and `audit_log_retention_days` (default 180) without requiring code changes.

---

## Functional Requirements

### FR-01: RetentionConfig Struct

A new `RetentionConfig` struct is added to `unimatrix-server/src/config.rs` and
embedded in `UnimatrixConfig` under the key `retention`.

Fields, types, and defaults:

| Field | Type | Default | Validation Range |
|-------|------|---------|-----------------|
| `activity_detail_retention_cycles` | `u32` | `50` | `[1, 10000]` |
| `audit_log_retention_days` | `u32` | `180` | `[1, 3650]` |
| `max_cycles_per_tick` | `u32` | `10` | `[1, 1000]` |

Struct-level rules:

- The struct derives `serde::Deserialize` and uses `#[serde(default)]` so an absent
  `[retention]` block in `config.toml` silently applies all defaults.
- A `validate(&self) -> Result<(), ConfigError>` method is required. It must emit a
  structured error that names the offending field for every out-of-range value.
- `validate()` is called during server startup (same call site as `InferenceConfig::validate()`).
  An out-of-range value aborts startup with a clear error message.
- `UnimatrixConfig` threads `RetentionConfig` into `run_maintenance()` alongside the
  existing `InferenceConfig` parameter.

### FR-02: K-Cycle Resolution Algorithm

At the start of each GC pass, the set of purgeable cycles is resolved by querying
`cycle_review_index`:

**Retained cycles (K-window):**
```sql
SELECT feature_cycle FROM cycle_review_index
ORDER BY computed_at DESC
LIMIT :k
```
where `:k` = `retention_config.activity_detail_retention_cycles`.

**Purgeable cycles (all reviewed cycles outside the K-window):**
```sql
SELECT feature_cycle FROM cycle_review_index
WHERE feature_cycle NOT IN (
    SELECT feature_cycle FROM cycle_review_index
    ORDER BY computed_at DESC
    LIMIT :k
)
```

Open cycles (cycles that have no `cycle_review_index` row) are never included in the
purgeable set. The algorithm returns a list of `feature_cycle` strings. If the list is
empty (total reviewed cycles <= K), the GC pass exits immediately with no deletes.

The list is capped to `max_cycles_per_tick` purgeable cycles per tick. If more purgeable
cycles exist than the cap allows, the pass processes the oldest (lowest `computed_at`)
first and defers the remainder to the next tick. This ensures unbounded backlogs are
drained incrementally without monopolising the tick budget.

### FR-03: Per-Cycle GC Transaction

For each purgeable cycle identified by FR-02, the GC pass executes the following
sub-steps. Each cycle is processed in its own transaction, acquired via
`pool.begin()` / `tx.commit()`. The connection is released between cycles (not held
for the entire multi-cycle batch).

Sub-steps within a single-cycle transaction (must execute in this order):

1. **crt-033 gate check** (FR-04) — verify `cycle_review_index` row exists.
2. **Prune `observations`** — `DELETE FROM observations WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = :cycle_id)`.
3. **Prune `query_log`** — `DELETE FROM query_log WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = :cycle_id)`.
4. **Prune `injection_log`** — `DELETE FROM injection_log WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = :cycle_id)`.
5. **Prune `sessions`** — `DELETE FROM sessions WHERE feature_cycle = :cycle_id`.

The order of steps 4 and 5 (injection_log before sessions) is mandatory. `injection_log`
uses `session_id` as a foreign key; deleting sessions first would leave orphaned
`injection_log` rows. All DELETE statements within the transaction use `write_pool_server()`.

After the transaction commits and the connection is released, the GC pass executes:

6. **Set `raw_signals_available = 0`** (FR-06) — call `store_cycle_review()` with the
   `CycleReviewRecord` fetched in step 1 (gate check), using struct update syntax
   `CycleReviewRecord { raw_signals_available: 0, ..record }`. The record from step 1
   must be kept in scope for this call.

### FR-04: crt-033 Gate

Before executing any DELETE for a cycle, the GC pass must call
`get_cycle_review(feature_cycle)` and verify the result is `Ok(Some(_))`.

- If the result is `Ok(None)`: the cycle has no `cycle_review_index` row. Skip the cycle
  entirely. Emit `tracing::warn!` naming the cycle ID and reason ("no cycle_review_index
  row — skipping"). This is a gate skip.
- If the result is `Err(_)`: treat as a transient error. Skip the cycle, emit
  `tracing::warn!`, and continue to the next cycle. Do not abort the entire pass.
- The gate cannot be disabled or bypassed by configuration.

### FR-05: Unattributed Row Cleanup

After all per-cycle transactions complete, the GC pass runs two unconditional cleanup
queries (outside any per-cycle transaction, but within the same tick pass):

```sql
DELETE FROM observations WHERE session_id NOT IN (SELECT session_id FROM sessions)
```
```sql
DELETE FROM query_log WHERE session_id NOT IN (SELECT session_id FROM sessions)
```

These catch rows whose sessions were already deleted by the existing `gc_sessions`
time-based sweep, and rows written without a valid session. They do not require a
`cycle_review_index` gate because the sessions they reference no longer exist.

Guard for active sessions: unattributed rows belonging to sessions with `status =
'Active'` must NOT be deleted. The cleanup queries must exclude active sessions:
```sql
DELETE FROM observations
WHERE session_id NOT IN (SELECT session_id FROM sessions)
   OR (session_id IN (SELECT session_id FROM sessions WHERE feature_cycle IS NULL)
       AND session_id NOT IN (SELECT session_id FROM sessions WHERE status = 'Active'))
```

Simplified: rows whose `session_id` is absent from `sessions` entirely are deleted;
rows whose session exists but has `feature_cycle IS NULL` and `status != 'Active'` are
deleted; rows belonging to an Active session are never deleted.

### FR-06: raw_signals_available Flag Update

After the per-cycle transaction commits (FR-03 steps 1–5), call `store_cycle_review()`
reusing the `CycleReviewRecord` already fetched in FR-04 (the gate check):

```rust
store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record }).await?
```

The struct update syntax (`..record`) preserves `summary_json` and all other fields.
This runs outside the per-cycle transaction — `store_cycle_review()` takes `&self`,
not a transaction handle.

**Critical implementation constraint**: the `CycleReviewRecord` returned by
`get_cycle_review()` in FR-04 must be retained in scope and passed here. It must NOT
be discarded after the gate check and reconstructed from partial data. Reconstruction
would supply only `raw_signals_available = 0` and default-initialize all other fields,
clobbering `summary_json` with an empty string and invalidating the retrospective report.

This call runs outside the per-cycle transaction. Using `store_cycle_review()` with the
struct update pattern is safe precisely because the full `record` is supplied — INSERT
OR REPLACE correctly preserves all fields since `..record` carries them.

### FR-07: audit_log Time-Based GC

A separate, independent GC sub-step runs after the cycle-based block each tick:

```sql
DELETE FROM audit_log
WHERE timestamp < (strftime('%s', 'now') - :audit_retention_days * 86400)
```

where `:audit_retention_days` = `retention_config.audit_log_retention_days`.

This uses the existing `idx_audit_log_timestamp` index. `audit_log` has no
`feature_cycle` column and carries accountability data rather than a learning signal;
time-based retention is correct and appropriate.

### FR-08: Remove Both 60-Day DELETE Sites

Two existing 60-day hard-DELETE sites must be removed entirely (not conditionally
skipped, not guarded by a flag):

1. **`status.rs` line ~1380**: the `DELETE FROM observations WHERE ts_millis < ?1`
   statement inside `run_maintenance()` step 4.
2. **`tools.rs` line ~1638**: the companion FR-07 in-tool path that performs the same
   DELETE via the MCP tool handler.

Both sites are independently verified. Neither may remain in the codebase after this
feature ships. A grep assertion in the integration test suite confirms absence of the
pattern `DELETE FROM observations WHERE ts_millis`.

### FR-09: Structured Tracing Output

The GC pass emits structured `tracing::info!` and `tracing::warn!` messages at the
following points:

| Event | Level | Required Fields |
|-------|-------|----------------|
| GC pass starts | `info` | `k`, `purgeable_count`, `capped_to` |
| Cycle pruned | `info` | `cycle_id`, `observations_deleted`, `query_log_deleted`, `injection_log_deleted`, `sessions_deleted` |
| Gate skip (no review row) | `warn` | `cycle_id`, reason |
| Gate skip (get_cycle_review error) | `warn` | `cycle_id`, `error` |
| Unattributed cleanup | `info` | `observations_deleted`, `query_log_deleted` |
| audit_log cleanup | `info` | `rows_deleted`, `cutoff_days` |
| GC pass complete | `info` | `cycles_pruned`, `cycles_skipped`, `total_rows_deleted` |

### FR-10: PhaseFreqTable Lookback Guard

At tick time, before the GC pass runs, emit `tracing::warn!` if the
`query_log_lookback_days` from `InferenceConfig` implies a data window that exceeds
the retention coverage implied by K cycles.

The check compares:
- Oldest retained data boundary: the `computed_at` timestamp of the K-th most recent
  cycle review (the oldest cycle in the K-window).
- `query_log_lookback_days` coverage: `now - query_log_lookback_days * 86400` seconds.

If `query_log_lookback_days` coverage extends before the oldest retained cycle's
`computed_at`, emit:
```
tracing::warn!(
    "query_log_lookback_days={} extends beyond the retention window \
     (oldest retained cycle computed_at={}); PhaseFreqTable will operate \
     on a truncated window after GC",
    query_log_lookback_days, oldest_cycle_computed_at
)
```

This is a warning only; it does not block GC execution or startup.

### FR-11: config.toml [retention] Block

The `[retention]` section is added to `config.toml` with documentation comments:

```toml
[retention]
# Number of completed (reviewed) feature cycles to retain activity data for.
# Observations, query_log, sessions, and injection_log for cycles beyond this
# window are deleted after their cycle_review_index row exists.
# Governs the ceiling for PhaseFreqTable lookback and future GNN training window.
# Range: [1, 10000]. Default: 50.
activity_detail_retention_cycles = 50

# Maximum number of purgeable cycles to process in a single maintenance tick.
# Limits tick budget consumed by GC. Older cycles are processed first.
# Deferred cycles are picked up on the next tick.
# Range: [1, 1000]. Default: 10.
max_cycles_per_tick = 10

# Retention window in days for audit_log rows.
# Audit data is an accountability record, not a learning signal.
# Range: [1, 3650]. Default: 180.
audit_log_retention_days = 180
```

`activity_detail_retention_cycles` docstring (code comment) must explicitly state: "This
value is the governing ceiling for PhaseFreqTable lookback and the future GNN training
window. Reducing this value will truncate the data available to PhaseFreqTable::rebuild."

### FR-12: run_maintenance() Step Ordering

The new GC pass replaces the existing step 4 (`observations` 60-day DELETE) and is
inserted at the same position. Step ordering after this feature:

- 0a. Prune quarantined vectors
- 0b. Heal pass (re-embed)
- 1. Co-access stale pair cleanup
- 2. Confidence refresh
- 2b. Empirical prior computation
- 3. Graph compaction
- **4. Cycle-based GC** (observations + query_log + injection_log + sessions; replaces old step 4)
- **4f. audit_log time-based GC** (new; "4f" avoids collision with sub-steps 4a–4e)
- 5. Stale session sweep
- 6. Session GC (existing time-based gc_sessions — continues to run for sessions
   not covered by cycle-based GC, e.g. sessions with no feature_cycle)

The cycle-based GC pass is independent of the prune/heal/compact ordering constraint
because it does not touch ENTRIES, VECTOR_MAP, or HNSW index structures.

---

## Non-Functional Requirements

### NFR-01: Hot Path Non-Blocking

The GC pass runs exclusively inside `run_maintenance()` which is called from the
background tick. It must not be called from any MCP request handler, the analytics
drain, or any synchronous path.

### NFR-02: Connection Release Between Cycles

`write_pool_server()` has `max_connections = 1`. The per-cycle transaction must acquire
the connection, execute all DELETEs and the UPDATE for that cycle, commit, and release
the connection before the next cycle begins. A single transaction spanning all purgeable
cycles would hold the write pool for the entire multi-cycle batch, stalling all
concurrent writes (drain flush, audit writes, session inserts).

### NFR-03: Performance Baseline

DELETE sub-queries through `sessions` must use the existing indexes:
- `idx_observations_session` on `observations(session_id)`
- `idx_query_log_session` on `query_log(session_id)`
- `idx_injection_log_session` on `injection_log(session_id)` (or equivalent)

The integration test must execute `EXPLAIN QUERY PLAN` for at least one representative
DELETE sub-query and assert the plan uses the session index rather than a full-table
scan. If the query planner chooses a full-table scan, the test must fail with a
diagnostic message.

### NFR-04: Idempotency

Each per-cycle GC pass is naturally idempotent: re-running the GC for an already-pruned
cycle results in zero rows affected (the rows are already gone). The `raw_signals_available`
flag UPDATE to `0` on an already-`0` row is a no-op at the data level.

### NFR-05: No Schema Migration

No schema changes are required. All GC logic operates on existing indexed columns.
`session_id` indexes exist on `observations`, `query_log`, and `injection_log`.
`feature_cycle` column exists on `sessions`. `timestamp` index exists on `audit_log`.
This feature does not increment the migration version.

### NFR-06: Config Loaded Once at Startup

`RetentionConfig` must be loaded and validated once at server startup and passed by
value into `run_maintenance()`. It must not be re-read from `config.toml` on each tick.
This avoids concurrent config reload races with the background tick (entry #1560 pattern:
`Arc<RwLock<T>>` shared-state sole-writer rule does not apply here — pass by value is
sufficient and simpler).

### NFR-07: Observability at Appropriate Granularity

Every pruned cycle emits one `info` log line with per-table row counts. Aggregate
tick-level summary emitted on pass completion. Gate skips emit `warn`. The tracing
output must be parseable by structured log consumers (use field=value syntax, not
interpolated strings).

---

## Acceptance Criteria

### AC-01a: status.rs 60-Day DELETE Removed

**Verification:** The pattern `DELETE FROM observations WHERE ts_millis` does not appear
anywhere in `crates/unimatrix-server/src/services/status.rs`. An integration test asserts
this via a compile-time absence check (or a grep assertion that fails the build if the
pattern is found). The original step 4 block (lines ~1372–1384) is replaced entirely by
the cycle-based GC invocation.

**File:** `crates/unimatrix-server/src/services/status.rs`

### AC-01b: tools.rs 60-Day DELETE Removed

**Verification:** The pattern `DELETE FROM observations WHERE ts_millis` does not appear
anywhere in `crates/unimatrix-server/src/tools.rs`. The FR-07 in-tool path (line ~1638)
is removed in its entirety, not guarded by a flag.

**File:** `crates/unimatrix-server/src/tools.rs`

Note: AC-01a and AC-01b are independently verifiable. A reviewer must confirm removal
at both sites. Missing either one leaves the time-based policy running concurrently
with the cycle-based GC.

### AC-02: Cycle-Based Pruning Correctness

**Verification:** Integration test inserts N > K cycles of data (each cycle: sessions
rows with matching `feature_cycle`, observations rows, query_log rows, injection_log
rows, and a `cycle_review_index` row). Runs GC with `K`. Asserts:
- Observations, query_log, injection_log, and sessions rows for the oldest (N - K)
  reviewed cycles are deleted.
- Observations, query_log, injection_log, and sessions rows for the newest K cycles
  are present and unmodified.
- Row counts before and after match expectations exactly (no partial deletes).

### AC-03: Regression: Untouched Tables

**Verification:** After GC runs in AC-02 scenario:
- `SELECT COUNT(*) FROM entries` is identical before and after GC.
- `SELECT COUNT(*) FROM GRAPH_EDGES` is identical before and after GC.
- `SELECT COUNT(*) FROM co_access` is identical before and after GC.
- `SELECT COUNT(*) FROM cycle_events` is identical before and after GC.
- `SELECT COUNT(*) FROM cycle_review_index` is identical before and after GC.
- `SELECT COUNT(*) FROM observation_phase_metrics` is identical before and after GC.

### AC-04: crt-033 Gate — No Review Row Prevents Pruning

**Verification:** Integration test inserts a cycle with sessions and observations but
NO `cycle_review_index` row. Runs GC. Asserts the observations and sessions rows for
that cycle are still present after GC.

Note: if `context_cycle_review` is never called for a cycle (operator skips retro),
that cycle's data is retained indefinitely. This is the correct and documented behaviour:
the K-window never advances past unreviewed cycles. This is a known operational
constraint, not a bug.

### AC-05: raw_signals_available Set to 0 After Pruning

**Verification:** Integration test: after AC-02 GC run, `SELECT raw_signals_available
FROM cycle_review_index WHERE feature_cycle = :pruned_cycle_id` returns `0` for every
pruned cycle. Retained cycles still have `raw_signals_available = 1` (or their original
value).

**Additional guard:** `summary_json` for each pruned cycle is byte-for-byte unchanged
before and after GC. This verifies the struct update pattern preserved the review
content and did not clobber it with a default-initialized value.

### AC-06: Unattributed Session Pruning

**Verification:** Integration test:
- Insert sessions with `feature_cycle IS NULL` and `status = 'Closed'`; insert
  matching observations and query_log rows.
- Insert sessions with `feature_cycle IS NULL` and `status = 'Active'`; insert
  matching observations.
- Run GC.
- Assert: closed unattributed sessions and their observations/query_log rows are deleted.
- Assert: Active unattributed sessions and their observations are NOT deleted.

### AC-07: query_log Rows Pruned with Purgeable Cycles

**Verification:** As part of AC-02 scenario, assert `query_log` row counts per session
are zero for sessions belonging to pruned cycles, and non-zero for sessions belonging
to retained cycles. Verify via direct `SELECT COUNT(*) FROM query_log WHERE session_id
IN (SELECT session_id FROM sessions WHERE feature_cycle = :cycle_id)`.

### AC-08: Per-Cycle Transaction Atomicity and Cascade Delete Order

**Verification — atomicity:** Integration test: simulate a mid-transaction failure
(e.g. a deliberately failing DELETE on one table) for a purgeable cycle. Assert that
after the rollback, all four tables (observations, query_log, injection_log, sessions)
retain their rows for that cycle — no partial state. The cycle must remain purgeable
on the next tick.

**Verification — cascade order:** Integration test: insert a cycle with sessions,
injection_log, and observations. Verify that after a successful GC:
- observations rows for those sessions are deleted.
- query_log rows for those sessions are deleted.
- injection_log rows for those sessions are deleted.
- sessions rows for that cycle are deleted.
- No orphaned injection_log rows remain.

**Order mutation test:** Modify the implementation to delete sessions before
injection_log (intentional inversion); verify the test fails with orphaned
injection_log rows. Restore correct order. This confirms the test enforces the
cascade constraint, not just observes incidental correctness.

### AC-09: audit_log Time-Based GC

**Verification:** Integration test:
- Insert audit_log rows with `timestamp` = (now - 200 days in seconds).
- Insert audit_log rows with `timestamp` = (now - 100 days in seconds).
- Run GC with `audit_log_retention_days = 180`.
- Assert rows with 200-day-old timestamps are deleted.
- Assert rows with 100-day-old timestamps are present.

### AC-10: Config Parsing and Defaults

**Verification:**
- Unit test: `RetentionConfig::default()` has `activity_detail_retention_cycles = 50`,
  `audit_log_retention_days = 180`, `max_cycles_per_tick = 10`.
- Integration test: parse a `config.toml` with an absent `[retention]` block; assert
  resulting `RetentionConfig` matches defaults.
- Integration test: parse a `config.toml` with explicit `[retention]` values; assert
  they are applied.

### AC-11: validate() Rejects activity_detail_retention_cycles = 0

**Verification:** Unit test: `RetentionConfig { activity_detail_retention_cycles: 0, .. }
.validate()` returns `Err(_)`. The error message contains the string
`"activity_detail_retention_cycles"`.

### AC-12: validate() Rejects audit_log_retention_days = 0

**Verification:** Unit test: `RetentionConfig { audit_log_retention_days: 0, .. }
.validate()` returns `Err(_)`. The error message contains the string
`"audit_log_retention_days"`.

### AC-12b: validate() Rejects max_cycles_per_tick = 0

**Verification:** Unit test: `RetentionConfig { max_cycles_per_tick: 0, .. }.validate()`
returns `Err(_)`. The error message contains the string `"max_cycles_per_tick"`.

### AC-13: activity_detail_retention_cycles Documented as GNN/FreqTable Ceiling

**Verification:** The field declaration in `RetentionConfig` has a doc comment
(triple-slash `///`) containing both:
- "PhaseFreqTable lookback"
- "GNN training window"

A compile-time assertion is not feasible; reviewer confirms doc comment content during
PR review.

### AC-14: Protected Tables Untouched

**Verification:** AC-03 regression check covers this. Additionally, a targeted test
inserts one row in each protected table (`cycle_events`, `cycle_review_index`,
`observation_phase_metrics`, `entries`, `GRAPH_EDGES`) before running GC, and asserts
each row survives GC unchanged.

### AC-15: Structured Tracing Output

**Verification:** Integration test captures tracing output (using `tracing-test` or
equivalent). For a GC run that prunes 2 cycles:
- Asserts one `info` log event with `field purgeable_count` present.
- Asserts two `info` log events each containing `observations_deleted` and `cycle_id`.
- Asserts one `info` log event with `cycles_pruned = 2` at pass completion.
- For a gate-skipped cycle (no review row), asserts a `warn` event containing the
  cycle ID.

### AC-16: max_cycles_per_tick Cap

**Verification:** Integration test: insert N = 20 purgeable cycles with `max_cycles_per_tick = 5`.
- After first GC tick: assert exactly 5 cycles pruned, 15 remain purgeable.
- After second GC tick: assert 5 more cycles pruned (10 remain).
- After fourth GC tick: assert all 20 cycles pruned.

Test verifies that oldest cycles (lowest `computed_at`) are processed first.

### AC-17: PhaseFreqTable Mismatch Warning

**Verification:** Integration test: configure `query_log_lookback_days = 365` and
`activity_detail_retention_cycles = 5` with only 5 review cycles in the database, all
computed within the past 7 days. Run the GC pass. Assert a `warn` log event is emitted
containing `"query_log_lookback_days"` and `"retention window"`.

---

## Domain Model / Ubiquitous Language

**Purgeable cycle**: A `feature_cycle` that has a `cycle_review_index` row AND is NOT
among the K most recently reviewed cycles (ordered by `computed_at` DESC). Purgeable
cycles are eligible for data deletion. The purgeable set is computed fresh each tick.

**Retained cycle**: A `feature_cycle` that is among the K most recently reviewed cycles
(by `computed_at`), OR any open cycle (no `cycle_review_index` row). Data for retained
cycles is never deleted by the cycle-based GC.

**Open cycle**: A cycle with no `cycle_review_index` row. This means `context_cycle_review`
has not been called for it. Open cycles are always retained, regardless of age.

**K-cycle window**: The set of the K most recently reviewed cycles, where K =
`activity_detail_retention_cycles`. K governs data retention boundaries, the
PhaseFreqTable rebuild ceiling, and the future GNN training window.

**Unattributed row**: An `observations` or `query_log` row whose `session_id` does not
exist in `sessions`, or whose session has `feature_cycle IS NULL` and `status != 'Active'`.
Unattributed rows carry no learning signal and are pruned unconditionally (except for
active sessions, which are guarded).

**crt-033 gate**: The mandatory check that `get_cycle_review(feature_cycle)` returns
`Ok(Some(_))` before any data for a cycle is deleted. Named after the feature (crt-033)
that introduced `cycle_review_index`. The gate cannot be skipped.

**raw_signals_available**: A field on `CycleReviewRecord` (`i32`, values 0 or 1) that
indicates whether the raw observation data for a cycle is still present in the database.
Set to `0` by the GC pass after pruning. Downstream tooling uses this to determine
whether `context_cycle_review` can be regenerated from raw data.

**Two-hop join**: The query pattern required for `observations` and `query_log` to reach
`feature_cycle` scope: `session_id` → `sessions.session_id` → `sessions.feature_cycle`.
Neither table has a direct `feature_cycle` column.

**Per-cycle transaction**: A single `BEGIN` / `COMMIT` block scoped to one
`feature_cycle`, covering observations DELETE, query_log DELETE, injection_log DELETE,
and sessions DELETE. The `raw_signals_available = 0` UPDATE runs after commit, outside
the transaction. The connection is released before processing the next cycle.

---

## User Workflows

### Workflow 1: Steady-State Pruning (Normal Operation)

1. Background tick fires; `run_maintenance()` reaches step 4.
2. GC pass resolves purgeable cycles via K-cycle query against `cycle_review_index`.
3. If purgeable cycles > `max_cycles_per_tick`, oldest `max_cycles_per_tick` are selected.
4. For each selected cycle:
   a. crt-033 gate: `let record = get_cycle_review()` → `Ok(Some(record))` — proceed. Retain `record`.
   b. Per-cycle transaction: delete observations, query_log, injection_log, sessions. Commit + release connection.
   c. `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` (outside transaction, uses record from step a).
   d. Emit `info` log with per-table row counts.
5. Unattributed cleanup runs (outside per-cycle loop).
6. audit_log time-based cleanup runs.
7. Emit pass-complete `info` log.

### Workflow 2: Gate Skip (No Review Row)

1. GC resolves a cycle as purgeable (it appears in `cycle_review_index` with old `computed_at`).
2. Gate check: `get_cycle_review()` returns `Ok(None)` — this should not normally happen
   (the cycle was found in `cycle_review_index`), but transient read inconsistency is
   possible.
3. Emit `warn` log; skip cycle; continue to next purgeable cycle.

### Workflow 3: First Run After Feature Ships (Backlog Drain)

1. On first tick after deployment, many cycles may be purgeable (all historical cycles
   outside the K window).
2. GC processes at most `max_cycles_per_tick` per tick; defers the rest.
3. Over successive ticks, the backlog drains at `max_cycles_per_tick` cycles per tick.
4. Once drained, subsequent ticks find few or zero purgeable cycles and execute quickly.

### Workflow 4: Operator Changes K via config.toml

1. Operator sets `activity_detail_retention_cycles = 20` (down from 50).
2. Server restarts; `validate()` confirms 20 is in `[1, 10000]`.
3. Next GC tick resolves the new K-window; cycles 21–50 (previously retained) are now
   purgeable.
4. They are pruned incrementally at `max_cycles_per_tick` per tick.
5. PhaseFreqTable mismatch warning fires if `query_log_lookback_days` now exceeds the
   smaller retention window.

---

## Constraints

1. **No schema migration.** All GC logic operates on existing indexed columns. The
   `raw_signals_available` flag update is a data write, not a schema change.
   Migration version remains at 19 (crt-035).

2. **observations and query_log have no feature_cycle column.** Every GC query that
   scopes these tables to a cycle MUST join through `sessions`. Direct cycle-column
   lookups are not possible.

3. **injection_log must be deleted before sessions.** `injection_log.session_id` is a
   logical foreign key to `sessions.session_id`. Deleting sessions first orphans
   injection_log rows. The cascade order is mandatory and must be enforced by the
   per-cycle transaction structure.

4. **write_pool_server() has max_connections = 1.** All GC DELETEs and UPDATEs use
   `write_pool_server()`. A per-cycle transaction must acquire, operate, and release
   the connection before the next cycle begins. A single transaction spanning all cycles
   would hold the pool for the entire batch duration.

5. **pool.begin() / tx.commit() API required.** Raw `BEGIN` SQL issued via `execute()`
   does not guarantee connection identity across multiple calls in sqlx. Use the
   `pool.begin()` → `tx.commit()` API for all per-cycle atomic deletes (entry #2159
   pattern; SR-03 mitigation).

6. **raw_signals_available update must use `store_cycle_review()` with struct update
   syntax.** The `CycleReviewRecord` fetched in FR-04 (gate check) must be retained in
   scope and used as: `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0,
   ..record })`. The struct update preserves `summary_json` and all other fields. The
   record must NOT be discarded and reconstructed — reconstruction with partial data
   clobbers `summary_json` (SR-05 mitigation).

7. **Both 60-day DELETE sites removed unconditionally.** The two sites in `status.rs`
   (~1380) and `tools.rs` (~1638) are removed, not conditioned on a feature flag.
   Running both the old and new GC concurrently is not supported.

8. **crt-033 gate is unconditional.** There is no configuration option to bypass the
   `cycle_review_index` existence check. Any cycle without a `cycle_review_index` row
   is never pruned.

9. **RetentionConfig loaded once at startup, passed by value.** Do not re-read
   `config.toml` on each tick. Avoids partial-write races with hypothetical config reload
   paths (SR-09 mitigation).

10. **Unattributed prune guards active sessions.** Sessions with `feature_cycle IS NULL`
    and `status = 'Active'` are excluded from unattributed cleanup. Pruning an active
    session's observations could disrupt an in-flight retrospective (SR-06 mitigation).

---

## Dependencies

### Crate Dependencies

| Dependency | Version / Location | Reason |
|------------|--------------------|--------|
| `sqlx` | existing (`sqlite`, `runtime-tokio`, `macros`) | All GC queries |
| `tracing` | existing | Structured log output (FR-09) |
| `serde` | existing | `RetentionConfig` deserialisation |

### Internal Component Dependencies

| Component | Location | Role |
|-----------|----------|------|
| `cycle_review_index` | `crates/unimatrix-store/src/cycle_review_index.rs` | Gate check (`get_cycle_review`), flag update |
| `run_maintenance()` | `crates/unimatrix-server/src/services/status.rs` | GC pass host |
| `UnimatrixConfig` | `crates/unimatrix-server/src/config.rs` | `RetentionConfig` embedding |
| `InferenceConfig` | `crates/unimatrix-server/src/config.rs` | Validation pattern precedent; `query_log_lookback_days` for FR-10 |
| `gc_sessions()` | `crates/unimatrix-store/src/sessions.rs` | Reference implementation for cascade delete pattern |
| `write_pool_server()` | `crates/unimatrix-store/src/lib.rs` | All GC write operations |
| `background.rs` | `crates/unimatrix-server/src/background.rs` | Threads `RetentionConfig` into `run_maintenance()` |

### Feature Dependencies

| Feature | Status | Dependency Reason |
|---------|--------|-------------------|
| crt-033 | Shipped | Provides `cycle_review_index` table and `get_cycle_review()` API — the crt-033 gate requires this in production |

---

## NOT in Scope

- **co_access table pruning.** The 0.34 MB `co_access` table is managed by the 1-year
  staleness threshold from GH #408. Out of scope for this feature.
- **Entry auto-deprecation.** Separate knowledge lifecycle concern tracked as a future
  issue.
- **Changes to cycle_events, cycle_review_index schema, observation_phase_metrics,
  entries, or GRAPH_EDGES.** None of these tables are pruned. cycle_events lifecycle
  hook events (`cycle_start`, `cycle_stop`, `cycle_phase_end`) are explicitly excluded.
- **Scoring or distribution eval changes.** This is pure data pruning; the confidence
  pipeline is untouched.
- **Opt-in feature flag.** The GC is always-on when
  `activity_detail_retention_cycles > 0` (always true given the default of 50). There
  is no `enabled = false` escape hatch.
- **Cycle-based filter in PhaseFreqTable::rebuild.** The ADR (entry #3686) deferred this
  to crt-036. crt-036 delivers the data retention boundary; updating the PhaseFreqTable
  rebuild query to use the cycle boundary instead of `query_log_lookback_days` is
  a follow-on task.
- **NLI model or scoring changes.** The confidence pipeline is read-only from this
  feature's perspective.
- **Retention for cycle_events lifecycle rows** (`cycle_start`, `cycle_stop`,
  `cycle_phase_end`). These are the structural record of cycle history and are never
  pruned.

---

## Open Questions

None at specification time. All implementation details were pre-agreed with the scope
author and resolved by codebase exploration per SCOPE.md "Open Questions" section.

Two items escalated to architect as design notes (not blocking this specification):

1. **SR-05 (raw_signals_available update path):** Resolved. Architecture specifies
   `store_cycle_review()` with struct update syntax `{ raw_signals_available: 0, ..record }`
   using the record retained from the FR-04 gate check. Constraint 6 updated accordingly.

2. **SR-07 (PhaseFreqTable mismatch check timing):** The tick-time guard (FR-10) is
   specified as a `tracing::warn!` only. Architect confirms whether the oldest-cycle
   boundary computation requires a dedicated query or can reuse the purgeable-cycle
   resolution result.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- returned 16 results; entries #3914
  (two-hop join pattern for observations/query_log GC), #3911 (run_maintenance procedure),
  #3793 (crt-033 ADR-001: synchronous write + write_pool_server constraint), #3686
  (col-031 ADR-002: query_log_lookback_days and PhaseFreqTable), and #3822
  (background tick idempotency pattern) were directly applied to this specification.
