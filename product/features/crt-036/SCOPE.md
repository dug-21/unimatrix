# crt-036: Intelligence-Driven Retention Framework

## Problem Statement

Unimatrix's activity and observation tables grow unbounded because retention is either
absent or governed by wall-clock age instead of learning utility. The `observations`
table alone accounts for 152 MB — 83% of the total database — and is pruned only by a
hard 60-day DELETE in the background maintenance tick (status.rs, step 4, line 1380).
`query_log`, `sessions`, and `injection_log` have no retention policy beyond the existing
30-day `gc_sessions` sweep that is triggered by time, not cycle boundaries.

Age-based retention is wrong for a learning engine: a row produced last week may be
valueless noise while a row produced six months ago may still feed the retrospective
pipeline. The correct criterion is: does this data still have learning value? Learning
value ends when the cycle it belongs to has been reviewed (i.e., a `cycle_review_index`
row exists) AND the cycle falls outside the configured retention window of K completed
cycles.

## Goals

1. Replace the 60-day observation hard delete with a cycle-based GC policy: retain
   observations for sessions belonging to the last K completed cycles; prune the rest.
2. Align `query_log` retention to the same K-cycle boundary: retain rows belonging to
   sessions in the last K completed cycles plus all rows whose session belongs to an
   open (not-yet-reviewed) cycle.
3. Derive `sessions` and `injection_log` retention from K completed `feature_cycles`
   (not K sessions, not elapsed time). A single cycle may span multiple sessions.
4. Apply a 180-day time-based hard delete to `audit_log` — accountability data, not
   a learning signal. Time-based is appropriate here.
5. Expose `[retention]` block in `config.toml` with `activity_detail_retention_cycles`
   (default 50) and `audit_log_retention_days` (default 180). Domain-configurable
   without code changes.
6. Gate ALL cycle-based pruning on a `cycle_review_index` row existing for the cycle.
   Cycles not yet reviewed are never pruned, regardless of how old they are.
7. Disable the existing 60-day observation DELETE when cycle-based retention is active
   so the two mechanisms never run concurrently.
8. Document `activity_detail_retention_cycles` as the governing ceiling for the
   `PhaseFreqTable` frequency table lookback window and future GNN training window.

## Non-Goals

- `co_access` table changes. The 0.34 MB co_access table is handled by the 1-year
  staleness threshold introduced in GH #408. Pruning co_access rows kills graph weight
  updates because the promotion tick reads them every cycle.
- Entry auto-deprecation. Separate knowledge lifecycle concern, tracked as a future
  issue.
- Changes to `cycle_events`, `cycle_review_index`, `observation_phase_metrics`,
  `entries`, or `GRAPH_EDGES`. These tables are never pruned by this feature.
- Scoring or distribution eval changes. This is pure data pruning; the confidence
  pipeline is untouched.
- Any opt-in flag for enabling the GC. The new GC is always-on once
  `activity_detail_retention_cycles > 0` (which it always is given the default of 50).
- Pruning `cycle_start` / `cycle_stop` / `cycle_phase_end` lifecycle hook events from
  `cycle_events`. These events are the structural record of cycle history and are
  explicitly excluded.

## Background Research

### Current Observation Retention — status.rs step 4 (lines 1372–1384)

```
// 4. Observation retention cleanup (col-012: SQL DELETE)
let now_millis = SystemTime::now()...as_millis() as i64;
let sixty_days_millis = 60_i64 * 24 * 60 * 60 * 1000;
let cutoff = now_millis - sixty_days_millis;
let _ = sqlx::query("DELETE FROM observations WHERE ts_millis < ?1")
    .bind(cutoff)
    .execute(self.store.write_pool_server())
    .await;
```

This is the sole retention mechanism for `observations`. It fires every background tick
(inside `run_maintenance()`), deletes based on the millisecond timestamp column
`ts_millis`, and has no awareness of cycles or whether the retrospective pipeline has
consumed the data. A companion FR-07 note in `tools.rs` at line 1638 performs the same
DELETE, providing a redundant in-tool path.

### observations Table Schema (db.rs lines 720–738)

```sql
CREATE TABLE observations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      TEXT    NOT NULL,
    ts_millis       INTEGER NOT NULL,
    hook            TEXT    NOT NULL,
    tool            TEXT,
    input           TEXT,
    response_size   INTEGER,
    response_snippet TEXT,
    topic_signal    TEXT
)
-- Indexes: idx_observations_session (session_id), idx_observations_ts (ts_millis)
```

`observations` does not have a `feature_cycle` column. Cycle resolution flows through
`sessions.feature_cycle`: `observations.session_id` → `sessions.session_id` →
`sessions.feature_cycle`. The `load_sessions_for_feature()` method (observations.rs
line 112) already encodes this join for the retrospective pipeline.

### query_log Table Schema (db.rs lines 779–803)

```sql
CREATE TABLE query_log (
    query_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT    NOT NULL,
    query_text TEXT    NOT NULL,
    ts         INTEGER NOT NULL,
    result_count INTEGER NOT NULL,
    result_entry_ids TEXT,
    similarity_scores TEXT,
    retrieval_mode TEXT,
    source TEXT NOT NULL,
    phase TEXT
)
-- Indexes: idx_query_log_session (session_id), idx_query_log_ts (ts), idx_query_log_phase (phase)
```

`query_log` uses `session_id` (not `feature_cycle`) as its only cycle-linkage. There is
no `feature_cycle` column. Confirmed by ADR-002 col-031 (Unimatrix entry #3686), which
explicitly decided against adding a cycle column to `query_log` and deferred cycle-based
GC to this issue (#409).

### sessions / injection_log — The gc_sessions Pattern (sessions.rs lines 294–349)

`gc_sessions(timed_out_threshold_secs, delete_threshold_secs)` is the reference
implementation for session cascade deletion. It runs in one transaction:

1. Phase 1: DELETE from `injection_log` WHERE `session_id IN (SELECT session_id FROM sessions WHERE started_at < boundary)`
2. Phase 2: DELETE from `sessions` WHERE `started_at < boundary`
3. Phase 3: UPDATE sessions SET status = TimedOut WHERE status = Active AND started_at < timed_out_boundary

Called at `run_maintenance()` step 6 with `DELETE_THRESHOLD_SECS = 30 * 24 * 3600`
(30 days). The new cycle-based retention replaces this time-based delete for sessions
belonging to reviewed cycles.

Key schema detail: `sessions` has a `feature_cycle TEXT` column. A single cycle may
span multiple sessions (N sessions per feature_cycle).

### injection_log — Cascade via session_id (db.rs lines 661–680)

```sql
CREATE TABLE injection_log (
    log_id     INTEGER PRIMARY KEY,
    session_id TEXT    NOT NULL,
    entry_id   INTEGER NOT NULL,
    confidence REAL    NOT NULL,
    timestamp  INTEGER NOT NULL
)
-- Indexes: idx_injection_log_session (session_id), idx_injection_log_entry (entry_id)
```

No direct `feature_cycle` column. Cascade delete via `session_id`, identical to
the existing `gc_sessions` Phase 1 pattern.

### audit_log Schema (db.rs lines 697–716)

```sql
CREATE TABLE audit_log (
    event_id   INTEGER PRIMARY KEY,
    timestamp  INTEGER NOT NULL,
    session_id TEXT    NOT NULL,
    agent_id   TEXT    NOT NULL,
    operation  TEXT    NOT NULL,
    target_ids TEXT    NOT NULL DEFAULT '[]',
    outcome    INTEGER NOT NULL,
    detail     TEXT    NOT NULL DEFAULT ''
)
-- Indexes: idx_audit_log_agent (agent_id), idx_audit_log_timestamp (timestamp)
```

No `feature_cycle` column. `timestamp` is the only GC key. 180-day time-based delete
is appropriate; audit data is an accountability record, not a learning signal.
`idx_audit_log_timestamp` already exists for this query.

### cycle_review_index — The Gate Table (cycle_review_index.rs)

```
CycleReviewRecord {
    feature_cycle: String,   -- PRIMARY KEY, matches cycle_events.cycle_id
    schema_version: u32,
    computed_at: i64,        -- unix seconds
    raw_signals_available: i32,  -- 1 = signals present, 0 = purged (this feature sets to 0)
    summary_json: String,
}
```

A `cycle_review_index` row exists IFF a `context_cycle_review` has been computed and
memoized for that `feature_cycle`. This is the crt-033 gate: GC must not proceed for
a cycle until this row exists. `get_cycle_review(feature_cycle)` returns `Ok(None)` if
absent — the GC pass must check for non-None before pruning.

The `raw_signals_available` field exists specifically for this use case (GH #409 per
the module docstring). After pruning, the GC pass must set `raw_signals_available = 0`
for pruned cycles to signal that the report can no longer be regenerated from raw data.

### cycle_events — K-cycle Resolution (db.rs lines 534–551)

```sql
CREATE TABLE cycle_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    cycle_id   TEXT    NOT NULL,
    seq        INTEGER NOT NULL,
    event_type TEXT    NOT NULL,
    phase      TEXT,
    outcome    TEXT,
    next_phase TEXT,
    timestamp  INTEGER NOT NULL,
    goal       TEXT
)
-- Index: idx_cycle_events_cycle_id (cycle_id)
```

Hook type is stored in the `event_type` column (`cycle_start`, `cycle_stop`,
`cycle_phase_end`, etc.). A cycle is "started" when a `cycle_start` event exists for
`cycle_id`. The `pending_cycle_reviews()` method already demonstrates the pattern for
querying cycles within a K-window via `WHERE event_type = 'cycle_start' AND timestamp >= ?`.

"Completed" cycles (for the purposes of this feature) are cycles with a
`cycle_review_index` row — i.e., cycles where the retrospective has been computed. The
last K completed cycles are resolved as:

```sql
SELECT feature_cycle FROM cycle_review_index
ORDER BY computed_at DESC
LIMIT ?1   -- K
```

This gives the most recently reviewed K cycles. Observations, query_log rows, and
sessions belonging to cycles NOT in this set (and whose cycle HAS been reviewed) are
eligible for pruning.

### InferenceConfig / UnimatrixConfig — Config Shape (config.rs)

`UnimatrixConfig` is the top-level config struct with sections: `profile`, `knowledge`,
`server`, `agents`, `confidence`, `inference`, `observation`. A new `[retention]`
section maps to a `RetentionConfig` struct added to `UnimatrixConfig`. Following the
`#[serde(default)]` pattern used by all existing sections, an absent `[retention]`
block uses compiled defaults. The `InferenceConfig` struct is the precedent for
config field addition with `validate()` range checks (entry #3759 / #3911).

### Background Tick Integration — run_maintenance() (status.rs, background.rs)

`run_maintenance()` is called from `background.rs` at line 959 via
`status_svc.run_maintenance(...)`. The current step ordering is:
- 0a. Prune quarantined vectors
- 0b. Heal pass (re-embed)
- 1. Co-access stale pair cleanup
- 2. Confidence refresh
- 2b. Empirical prior computation
- 3. Graph compaction
- 4. Observation retention (60-day DELETE — **replaced by this feature**)
- 5. Stale session sweep
- 6. Session GC

New step 4 replaces the existing 60-day DELETE with cycle-based GC for observations,
query_log, and sessions/injection_log. The `audit_log` 180-day DELETE is a new step 4b
(or renaming 4 as the cycle GC block and 4b as the audit block).

### Phase Frequency Table — ADR-002 col-031 Linkage (entry #3686)

`query_log_lookback_days` in `InferenceConfig` governs how far back `PhaseFreqTable`
looks when rebuilding. The docstring for that field explicitly notes: "This governs the
rebuild SQL window only — not data deletion. Data GC belongs to #409 (cycle-aligned
GC)." The `activity_detail_retention_cycles` from this feature becomes the ceiling for
that lookback in terms of available data — if K = 50 cycles are retained, the frequency
table rebuild window must not request data older than K cycles.

### Current Schema Version

Migration is at version 19 (crt-035). This feature does NOT require a schema migration
because:
- No new columns are added to any table
- Retention operates on existing indexed columns (`session_id`, `ts`, `timestamp`)
- The `raw_signals_available` flag flip is a data update, not a schema change

### Unimatrix Entry #3911 — Maintenance Tick Procedure

The `run_maintenance()` procedure (entry #3911) documents the ordering rule for new
passes: prune < heal < compact. The cycle-based GC is a prune-style pass (DELETE).
Per the procedure, it must be capped by a configurable batch parameter in
`RetentionConfig` (or `InferenceConfig`) and must use parameterized status binds.
The retention pass does not touch entries or the HNSW index, so it does not need the
prune/heal/compact ordering constraint — it is independent and can follow step 3.

## Proposed Approach

### Retention Principle

Data is retained until its cycle has been reviewed (crt-033 gate) AND the cycle falls
outside the K-cycle retention window. Data for open cycles is always retained. Data for
reviewed cycles within the K window is always retained. Only data for reviewed cycles
outside the K window is eligible for deletion.

### K-cycle Resolution Algorithm

At GC time, resolve the set of "purgeable" cycles as follows:

```sql
-- Cycles to RETAIN (last K reviewed cycles)
SELECT feature_cycle FROM cycle_review_index
ORDER BY computed_at DESC
LIMIT :k

-- Cycles to PRUNE: have a review, are NOT in the retain set, and are NOT open
SELECT feature_cycle FROM cycle_review_index
WHERE feature_cycle NOT IN (
    SELECT feature_cycle FROM cycle_review_index
    ORDER BY computed_at DESC
    LIMIT :k
)
```

Unattributed sessions (`feature_cycle IS NULL`) are pruned unconditionally (they cannot
belong to any cycle and carry no learning signal).

### Step 4 Replacement: Cycle-Based GC (run_maintenance)

The new step 4 block replaces the existing 60-day DELETE. It runs unconditionally when
`retention_config.activity_detail_retention_cycles > 0`.

**Sub-step 4a: Resolve purgeable cycles**
Query `cycle_review_index` to find cycles outside the K window (SQL above).

**Sub-step 4b: Prune observations**
For each purgeable cycle, delete observations belonging to its sessions:
```sql
DELETE FROM observations
WHERE session_id IN (
    SELECT session_id FROM sessions WHERE feature_cycle = :cycle_id
)
```
Then set `raw_signals_available = 0` in `cycle_review_index` for the pruned cycle.

**Sub-step 4c: Prune query_log**
```sql
DELETE FROM query_log
WHERE session_id IN (
    SELECT session_id FROM sessions WHERE feature_cycle = :cycle_id
)
```

**Sub-step 4d: Prune sessions + injection_log (cascade)**
Mirrors the existing `gc_sessions` Phase 1 + Phase 2 pattern, but scoped to the
purgeable cycle's sessions rather than a time boundary:
```sql
DELETE FROM injection_log
WHERE session_id IN (
    SELECT session_id FROM sessions WHERE feature_cycle = :cycle_id
)
DELETE FROM sessions WHERE feature_cycle = :cycle_id
```

**Sub-step 4e: Prune unattributed rows**
```sql
DELETE FROM observations WHERE session_id NOT IN (SELECT session_id FROM sessions)
DELETE FROM query_log WHERE session_id NOT IN (SELECT session_id FROM sessions)
```
These catch observations/queries whose sessions were already deleted (by the existing
`gc_sessions` time-based sweep) as well as rows written without a valid session.

**Step 4f: audit_log retention**
```sql
DELETE FROM audit_log
WHERE timestamp < (strftime('%s','now') - :audit_retention_days * 86400)
```
Uses the existing `idx_audit_log_timestamp` index. This is independent of the cycle loop
and runs unconditionally after sub-steps 4a–4e complete.

### Config Block Specification

New section `[retention]` in `config.toml`, mapped to a new `RetentionConfig` struct
added to `UnimatrixConfig`:

```toml
[retention]
# Number of completed (reviewed) feature cycles to retain activity data for.
# Observations, query_log, sessions, and injection_log for cycles beyond this
# window are deleted after their cycle_review_index row exists.
# Governs the ceiling for PhaseFreqTable lookback and future GNN training window.
# Range: [1, 10000]. Default: 50.
activity_detail_retention_cycles = 50

# Retention window in days for audit_log rows.
# Audit data is an accountability record (not a learning signal); time-based
# retention is appropriate. Range: [1, 3650]. Default: 180.
audit_log_retention_days = 180
```

`RetentionConfig` follows the `#[serde(default)]` pattern. `validate()` checks:
- `activity_detail_retention_cycles` in `[1, 10000]`
- `audit_log_retention_days` in `[1, 3650]`

### Interaction with Existing Step 6: gc_sessions

The existing step 6 `gc_sessions()` (30-day time-based cascade) is **unchanged** and
continues to run after the new cycle-based GC at step 4. These two mechanisms target
different populations:

- Step 4 (new): prunes sessions belonging to reviewed cycles outside the K window —
  cycle-attributed sessions for which the retrospective is complete.
- Step 6 (existing): prunes sessions by elapsed time — primarily covers sessions with no
  `feature_cycle` attribution and sessions that timed out without completing a cycle.

There is no conflict: a session pruned by the cycle-based GC at step 4 will no longer
exist when step 6 runs, so step 6's subquery simply finds zero rows for that session.
Both steps are necessary; neither replaces the other.

### crt-033 Gate

Before deleting any row for a cycle, the GC pass must verify that
`get_cycle_review(feature_cycle)` returns `Ok(Some(_))`. If the cycle has no
`cycle_review_index` row (retrospective not yet computed), it is skipped entirely.
This guard applies per-cycle: a batch of purgeable cycles is computed, then each is
checked individually before its rows are deleted.

### raw_signals_available Flag Update

After successfully pruning observations for a cycle, the GC updates
`cycle_review_index` to mark signals as gone:

```sql
UPDATE cycle_review_index
SET raw_signals_available = 0
WHERE feature_cycle = :cycle_id
```

This uses the existing `store_cycle_review()` path with an overwrite (INSERT OR REPLACE)
rather than a raw UPDATE — keeping the write path consistent with crt-033's ADR-001.

## Acceptance Criteria

- AC-01: Both 60-day observation DELETE sites are removed and replaced by the cycle-based
  GC pass: `status.rs` line 1380 (background tick step 4) and `tools.rs` line 1638
  (FR-07 in-tool path). Neither may remain after this feature ships. When
  `activity_detail_retention_cycles > 0` (always true with default), only cycle-based
  GC runs.
- AC-02: Integration test: insert N > K cycles of observations (each with sessions and
  query_log rows); run GC with K; verify observations for the oldest (N - K) reviewed
  cycles are deleted; verify observations for the newest K cycles are retained.
- AC-03: Regression assertions: after GC, ENTRIES count, GRAPH_EDGES count, and
  CO_ACCESS count are identical to pre-GC values.
- AC-04: crt-033 gate: cycles whose `cycle_review_index` row is absent are never pruned
  regardless of age or position outside the K window.
- AC-05: After GC prunes a cycle's observations, `cycle_review_index.raw_signals_available`
  for that cycle is set to 0.
- AC-06: Unattributed sessions (`feature_cycle IS NULL`) are pruned unconditionally from
  `sessions`; their `injection_log`, `observations`, and `query_log` rows are also pruned.
- AC-07: `query_log` rows for sessions belonging to the purgeable cycles are deleted.
  `query_log` rows for sessions in retained cycles are preserved.
- AC-08: The per-cycle transaction is atomic across all four tables: observations,
  query_log, injection_log, and sessions are all deleted or none are (rollback on error).
  Within the transaction, injection_log is deleted before sessions (cascade order).
  Verified by: simulating a mid-transaction failure and asserting all four tables retain
  their rows (no partial state).
- AC-09: `audit_log` rows older than `audit_log_retention_days` (default 180) are
  deleted on each maintenance tick. Rows within the retention window are preserved.
- AC-10: `[retention]` config block parses correctly from `config.toml`.
  `activity_detail_retention_cycles` defaults to 50; `audit_log_retention_days`
  defaults to 180.
- AC-11: `validate()` rejects `activity_detail_retention_cycles = 0` with a structured
  error naming the field.
- AC-12: `validate()` rejects `audit_log_retention_days = 0` with a structured error
  naming the field.
- AC-13: `activity_detail_retention_cycles` is documented (code comment + docstring) as
  the governing ceiling for `PhaseFreqTable` lookback and future GNN training window.
- AC-14: No `cycle_events`, `cycle_review_index`, `observation_phase_metrics`, `entries`,
  or `GRAPH_EDGES` rows are touched or deleted by the GC pass.
- AC-15: The GC pass produces structured log output (tracing::info!) reporting cycle
  IDs pruned, row counts deleted per table, and any per-cycle gate skips.

## Constraints

1. **No schema migration required.** All GC logic operates on existing indexed columns.
   `session_id` indexes exist on `observations`, `query_log`, and `injection_log`.
   `feature_cycle` column already exists on `sessions`. `timestamp` index exists on
   `audit_log`.

2. **observations has no direct feature_cycle column.** Cycle resolution always flows
   through `sessions.feature_cycle`. Any SQL that prunes observations must join through
   `sessions`.

3. **query_log has no feature_cycle column.** Same two-hop join as observations:
   `query_log.session_id` → `sessions.session_id` → `sessions.feature_cycle`.

4. **Write pool discipline.** GC deletes are persistent state changes and must use
   `write_pool_server()` directly (not the analytics drain queue). Mirrors the existing
   `gc_sessions` pattern.

5. **Transaction atomicity per cycle.** Prune operations for a single cycle (observations
   + query_log + injection_log + sessions) should run in one transaction to avoid partial
   deletes that could leave orphaned injection_log rows.

6. **crt-033 gate is mandatory.** The `cycle_review_index` existence check cannot be
   skipped or bypassed. Cycles without a review row are never pruned.

7. **Both 60-day DELETE sites must be removed.** There are two independent locations:
   `status.rs` line 1380 (background tick step 4) and `tools.rs` line 1638 (FR-07
   in-tool path). Both must be deleted — not conditionally skipped. Either one left in
   place will continue running the time-based policy alongside the cycle-based GC.

8. **InferenceConfig validation pattern.** New `RetentionConfig.validate()` must follow
   the established `validate()` pattern in `InferenceConfig` (range checks, structured
   error with field names, abort startup on out-of-range).

9. **Performance.** The GC pass runs in the background tick (not on the hot path).
   SQLite subquery joins through `sessions` for large observation tables must use
   the existing `idx_observations_session` and `idx_query_log_session` indexes. The
   K-cycle resolution query against `cycle_review_index` is a small bounded read.

## Open Questions

None. The scoped boundaries were pre-agreed with the human and all implementation
details are resolved by codebase exploration.

## Tracking

https://github.com/dug-21/unimatrix/issues/409
