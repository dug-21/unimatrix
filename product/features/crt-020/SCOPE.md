# crt-020: Implicit Helpfulness from Outcome Signals

## Problem Statement

Unimatrix's confidence formula (calibrated in crt-019) relies heavily on `helpful_count` and
`unhelpful_count` votes to produce meaningful score differentiation. However, the vast majority of
agents that use Unimatrix through the hook pipeline — the primary delivery channel — never explicitly
call `context_search` with `helpful: true`. The injection pathway (UserPromptSubmit hook →
ContextSearch → injection_log) operates entirely without agent cooperation on votes.

The result: entries injected hundreds of times via hooks accumulate zero helpfulness votes, while
entries retrieved once via a human-initiated MCP search may carry several votes. The confidence
formula is signal-starved for its most-used entries.

The data to close this loop already exists: `injection_log` records every entry injected per
session, and `sessions.outcome` records whether each session succeeded, was reworked, or was
abandoned. Joining these tables post-session-close yields an implicit signal — not as strong as an
explicit agent vote, but real signal derived from actual pipeline outcomes.

## Goals

1. Run a background tick operation that joins resolved `sessions` with their `injection_log`
   records to derive implicit helpfulness votes
2. For entries injected in successful sessions: add 1 implicit helpful vote to `helpful_count`
3. For entries injected in rework, abandoned, or TimedOut sessions: **zero signal** — session
   failure cannot be reliably attributed to any specific injected entry. No implicit unhelpful
   votes in v1.
4. Deduplicate votes per session per entry — each (session_id, entry_id) pair contributes at most
   once, regardless of how many times the entry was injected in that session
5. Track which sessions have already been processed to avoid re-applying votes on subsequent ticks
   (persistent dedup, not in-memory)
6. Recompute confidence inline after vote application, using the existing
   `record_usage_with_confidence` path with the crt-019 Bayesian prior parameters

## Non-Goals

1. **Not a schema change for entries** — `helpful_count` and `unhelpful_count` already exist on
   `EntryRecord`. No new columns on the `entries` table.
2. **Not changing what gets written to injection_log** — The injection log write path (col-010) is
   unchanged. crt-020 only reads from it.
3. **Not changing what gets written to sessions** — Session outcome resolution and persistence
   (col-010) are unchanged.
4. **Not replacing the existing signal_queue / run_confidence_consumer path** — The existing
   path drains `signal_queue` on session close (real-time, triggered by Stop hook). crt-020 is
   a complementary background path that processes any sessions the real-time path missed or that
   closed without a proper Stop event.
5. **Not applying votes to non-injection entries** — Only entries that appear in `injection_log`
   for a given session receive implicit votes. Entries accessed via explicit MCP tool calls but
   not injected are out of scope.
6. **Not wiring effectiveness scores into re-ranking** — crt-020 writes votes that feed the
   confidence formula. Search re-ranking already uses confidence. No additional re-ranking changes
   are in scope (that was crt-018b).
7. **Not adding audit log entries per vote** — Individual implicit votes are not audit-logged.
   Tick completion is logged at the tracing level. Audit events are for agent-originated mutations.
8. **Not retroactively processing sessions older than the GC retention window** — Sessions deleted
   by `gc_sessions` (DELETE_THRESHOLD_SECS = 30 days) are gone. crt-020 processes only sessions
   currently in the `sessions` table.
9. **Not a new MCP tool** — No user-facing tool. The tick runs in the background loop only.
10. **No implicit unhelpful votes** — rework, abandoned, and TimedOut sessions produce zero
    signal. Session failure cannot be reliably attributed to individual injected entries.
    Implicit unhelpful is deferred to a future feature with a more reliable attribution
    mechanism. `unhelpful_count` is unchanged by crt-020.

## Background Research

### injection_log Table (col-010)

`crates/unimatrix-store/src/injection_log.rs`: SQLite table with columns `log_id`, `session_id`,
`entry_id`, `confidence`, `timestamp`. Indexed on `session_id` and `entry_id`. Write path:
`insert_injection_log_batch` (atomic batch write, allocates contiguous `log_id` range from
counters). Read paths: `scan_injection_log_by_session` and `scan_injection_log_by_sessions`
(chunks of 50 session IDs to avoid large IN clauses).

Key: multiple rows per (session_id, entry_id) are possible — the same entry may be injected
multiple times in one session via successive UserPromptSubmit hooks.

### sessions Table (col-010)

`crates/unimatrix-store/src/sessions.rs`: SQLite table with columns `session_id`,
`feature_cycle`, `agent_role`, `started_at`, `ended_at`, `status`, `compaction_count`,
`outcome`, `total_injections`, `keywords`.

`status` values: `Active(0)`, `Completed(1)`, `TimedOut(2)`, `Abandoned(3)`.
`outcome` values: `"success"`, `"rework"`, `"abandoned"`, or `None` (not yet resolved).

For crt-020: only sessions with `status = Completed` or `status = TimedOut` and
`outcome IS NOT NULL` should be processed. Active sessions have not closed; their outcome
is not yet resolved. Sessions with `outcome = NULL` (rare race condition or crash) are
excluded — their outcome cannot be determined.

GC: `gc_sessions` deletes sessions older than 30 days. The injection_log rows for deleted
sessions are cascade-deleted by the GC SQL. crt-020 must complete processing before GC
removes the data, which is satisfied by the tick running every 15 minutes and GC operating
on a 30-day window.

### Existing Signal Path (col-009, col-010)

The real-time path (`run_confidence_consumer` in `listener.rs`) drains `signal_queue` records
of type `SignalType::Helpful` and calls `record_usage_with_confidence` to increment
`helpful_count`. This fires immediately when a session's Stop hook arrives. crt-020 is a
complementary **background** path for sessions that either:
- Closed without a Stop hook (orphaned sessions swept by `sweep_stale_sessions`)
- Had their signal consumed but additional injection_log entries should also be credited

The two paths write to the same `helpful_count`/`unhelpful_count` columns. Idempotent
dedup (per session per entry, persisted) prevents double-counting.

### vote Write Path

`crates/unimatrix-store/src/write_ext.rs`: `record_usage_with_confidence` takes
`helpful_ids: &[u64]` and `unhelpful_ids: &[u64]` and applies `helpful_count += 1` /
`unhelpful_count += 1` per entry within a single `BEGIN IMMEDIATE` transaction. If a
`confidence_fn` is provided, confidence is recomputed after each entry update. This is
the correct write path for crt-020 — no new store methods needed.

### Deduplication Requirement

The problem: injection_log may contain many rows per (session_id, entry_id). The signal
must be applied at most once per (session_id, entry_id) pair. Additionally, if a session
is processed in tick N and a bug causes it to be re-scanned in tick N+1, the vote must not
be applied twice.

The existing `UsageDedup` struct (`infra/usage_dedup.rs`) is in-memory and process-scoped
— it does not survive across ticks or restarts. A **persistent** dedup is required.

Persistent dedup options identified during research:
- **Option A (new table)**: `implicit_vote_log (session_id, entry_id)` with UNIQUE
  constraint on (session_id, entry_id). INSERT OR IGNORE enforces dedup. This is the
  cleanest approach but requires a schema migration (v13).
- **Option B (sessions watermark)**: Track the highest `session.ended_at` processed in a
  counter. On each tick, query sessions with `ended_at > last_processed_watermark AND
  status IN (Completed, TimedOut) AND outcome IS NOT NULL`. Apply votes for all injection_log
  entries for those sessions, then advance the watermark. This does NOT require dedup at
  the entry level — only new sessions are scanned — but requires sessions to be ordered by
  `ended_at`, which is indexed (`idx_sessions_started_at` exists, but not `ended_at`).
  Also: a session that closes and is later GC'd after tick processes it has no issue.
- **Option C (sessions column)**: Add a boolean `implicit_votes_applied` column to the
  `sessions` table. Filter `WHERE implicit_votes_applied = 0 AND status != Active AND
  outcome IS NOT NULL`. Mark as applied after processing. This also requires a schema
  migration (v13).

**Recommended: Option C** — `implicit_votes_applied` flag on the sessions table. Rationale:
- Schema migration is already expected at v13 for this feature
- The flag naturally handles partial-tick failure: if a tick crashes mid-processing, only
  already-marked sessions are skipped on restart
- No separate table required
- Filter is efficient: adding an index on `(status, implicit_votes_applied)` keeps the
  scan fast as session count grows
- GC deletes sessions including the flag; no orphaned dedup records

**Per-entry dedup within a session**: The (session_id, entry_id) pair is deduplicated
in Rust before writing: scan injection_log for the session, collect distinct entry_ids
via a HashSet, then write one vote per unique entry_id. The `implicit_vote_log` table
(Option A approach) is not needed because:
- injection_log contains all entries for a session
- distinct entry_ids are computed in Rust in the tick body
- the `implicit_votes_applied` session flag prevents re-processing an already-handled session

### Background Tick Infrastructure

`crates/unimatrix-server/src/background.rs`: Tick loop runs every 15 minutes
(`TICK_INTERVAL_SECS = 900`). The `maintenance_tick` function is the correct place to add
the implicit feedback step. It already runs `StatusService::compute_report`, session GC,
confidence refresh, graph compaction, and co-access cleanup via `run_maintenance`. The
implicit feedback operation is a new step in `maintenance_tick`, running after GC (so we
don't process sessions that GC is about to delete) and before confidence refresh (so the
newly-applied votes are captured in the next refresh).

The tick body uses `tokio::task::spawn_blocking` for all synchronous store operations.
The existing pattern:
1. Query eligible sessions from DB (spawn_blocking)
2. Scan injection_log for those sessions (spawn_blocking, batch by 50)
3. Deduplicate entry_ids per session in Rust
4. Apply votes via `record_usage_with_confidence` (spawn_blocking)
5. Mark sessions as processed (spawn_blocking, UPDATE sessions SET implicit_votes_applied = 1)

### crt-019 Dependency

`crates/unimatrix-server/src/services/confidence.rs`: `ConfidenceState` holds
`alpha0`, `beta0` (Bayesian prior), `observed_spread`, and `confidence_weight`. The
`confidence_state: ConfidenceStateHandle` is already passed through `maintenance_tick`
via `spawn_background_tick`. The implicit feedback step must snapshot `alpha0`/`beta0`
before entering `spawn_blocking` (same pattern as `UsageService::record_mcp_usage`).

`compute_confidence(entry, now, alpha0, beta0)` from `crates/unimatrix-engine/src/confidence.rs`
is the function to pass as `confidence_fn` to `record_usage_with_confidence`. This matches
the existing usage in `UsageService`.

### Half-Weight Implementation for Rework/Abandoned

The product specification says 0.5 implicit unhelpful vote for rework/abandoned sessions.
`unhelpful_count` is a `u32` — fractional increments are not possible without changing the
storage type.

Three approaches:
1. **Integer threshold**: Add a separate `implicit_unhelpful_pending` counter (new column) that
   accumulates 0.5 votes. When it reaches 1.0, apply one unhelpful vote and reset. Requires
   schema change.
2. **Probabilistic rounding**: Apply the unhelpful vote with 50% probability (flip a coin per
   entry). Unbiased in expectation but noisy.
3. **Full integer vote at reduced rate**: Apply one unhelpful vote only every other qualifying
   rework session for a given entry. Track via a per-entry-per-session counter.
4. **Store as full integer, weight at formula time**: Continue writing full unhelpful votes but
   halve the Wilson/Bayesian weight for `ImplicitRework`-sourced votes in `compute_confidence`.
   Requires adding a source tag to each unhelpful vote, which is not tracked today.
5. **Accept full integer at lower count**: Apply 1 unhelpful vote per rework session per entry
   but cap implicit unhelpful votes at a lower ceiling than explicit votes (a formula-level
   concern, not storage).
6. **Defer to Open Questions**: Treat 0 unhelpful vote for rework/abandoned in v1 (conservative),
   revisit in v2 once vote volumes are observable.

This is an open question for the human (see Open Questions).

### Schema Migration

Current schema version: 12 (`CURRENT_SCHEMA_VERSION = 12` in `migration.rs`). Adding
`implicit_votes_applied` to the `sessions` table requires a migration to v13:
- `ALTER TABLE sessions ADD COLUMN implicit_votes_applied INTEGER NOT NULL DEFAULT 0;`
- Existing sessions default to 0 (unprocessed) — correct behavior, they will be picked up
  on the first tick after upgrade
- Add index: `CREATE INDEX idx_sessions_pending_votes ON sessions(implicit_votes_applied, status);`

### Performance Considerations

The tick scans sessions with `implicit_votes_applied = 0`. In steady state (regular operation):
- Each 15-minute tick processes sessions that closed in the preceding 15 minutes
- Typical load: 1-10 sessions per tick
- Edge case: first tick after upgrade processes all historical sessions (up to 30-day GC window)
- Mitigation for cold-start: cap the number of sessions processed per tick
  (`IMPLICIT_VOTE_BATCH_SIZE` constant, default 500) to bound tick duration

For each session, `scan_injection_log_by_session` reads from the indexed `session_id` column.
At typical injection rates (5 entries per session), the scan returns ~5 rows per session —
negligible. The `record_usage_with_confidence` call touches 1-5 entries per session in a
single transaction.

### Existing Test Infrastructure

Tests in `injection_log.rs`, `sessions.rs`, `write_ext.rs`, and `usage.rs` use `TestDb`
from `crates/unimatrix-store/src/test_helpers.rs`. The `UsageService` tests in
`services/usage.rs` show the pattern for testing vote recording with `tokio::time::sleep`
for spawn_blocking completion. The implicit feedback tests should extend this pattern.

## Proposed Approach

### Step 1: Schema Migration (v12 → v13)

Add `implicit_votes_applied INTEGER NOT NULL DEFAULT 0` to the `sessions` table via
`ALTER TABLE sessions ADD COLUMN`. Add a covering index on `(implicit_votes_applied, status)`
for the tick query. Update `CURRENT_SCHEMA_VERSION` to 13. Update `SessionRecord` struct
and all read/write paths in `sessions.rs`.

### Step 2: New Store Method — `apply_implicit_votes`

Add to `crates/unimatrix-store/src/write_ext.rs` (or a new `implicit_votes.rs` module):

```
fn apply_implicit_votes_for_sessions(
    &self,
    sessions: &[(SessionId, SessionOutcome)],
    batch_size: usize,
    confidence_fn: ConfidenceFn,
) -> Result<u32>  // returns count of sessions processed
```

Internal steps:
1. Query sessions where `implicit_votes_applied = 0 AND status != Active AND outcome IS NOT NULL`
   (up to `batch_size` sessions)
2. Batch scan injection_log for those sessions via `scan_injection_log_by_sessions`
3. Per session: collect distinct entry_ids
4. Separate into `helpful_ids` (outcome = "success") and `unhelpful_ids` (outcome = "rework"
   or "abandoned")
5. Apply votes via `record_usage_with_confidence`
6. UPDATE sessions SET implicit_votes_applied = 1 for processed sessions
7. Return count of sessions processed

All steps in a single logical operation with appropriate transaction boundaries.

### Step 3: Tick Integration

Add `run_implicit_vote_tick` step in `maintenance_tick` (background.rs):
- Runs after `gc_sessions` (avoid processing sessions GC will delete)
- Runs before confidence refresh (so new votes influence the refresh)
- Uses `spawn_blocking` with `Arc<Store>` and snapshotted `(alpha0, beta0)`
- Logs count of sessions processed at `tracing::debug` level
- Errors are warned but do not abort the tick

### Step 4: ConfidenceStateHandle Threading

The `confidence_state: ConfidenceStateHandle` is already passed to `maintenance_tick` via
`spawn_background_tick`. No signature changes needed — the implicit vote step snapshots
`alpha0`/`beta0` from the handle before entering `spawn_blocking`, same as the MCP path.

### Half-Weight Decision

Per the Open Questions, the human must decide between approaches. The recommended default
is **option A: integer threshold with persisted half-count** to faithfully implement 0.5
weight, or **option F: conservative v1 (zero unhelpful votes for rework)** as the simplest
no-schema-change approach for the unhelpful path. The helpful path (success → +1) is not
affected by this decision.

## Acceptance Criteria

- **AC-01**: A new column `implicit_votes_applied INTEGER NOT NULL DEFAULT 0` exists on the
  `sessions` table. Schema version is incremented to 13. Existing sessions default to 0.
- **AC-02**: Each background maintenance tick queries sessions where
  `implicit_votes_applied = 0 AND status IN (1, 2) AND outcome IS NOT NULL`, up to a
  configurable batch limit (`IMPLICIT_VOTE_BATCH_LIMIT`, default 500).
- **AC-03**: For each eligible session with `outcome = "success"`: all entries in
  `injection_log` for that `session_id` are deduplicated (one vote per unique entry_id),
  and `helpful_count` is incremented by 1 for each unique entry.
- **AC-04**: Sessions with any outcome other than `"success"` (rework, abandoned, or TimedOut)
  produce zero signal. No `unhelpful_count` increments are applied by crt-020 under any
  circumstances.
- **AC-05**: After processing, the session's `implicit_votes_applied` flag is set to 1.
  A session is never processed twice.
- **AC-06**: Confidence is recomputed for each entry that receives an implicit vote, using
  the current Bayesian prior parameters `(alpha0, beta0)` from `ConfidenceStateHandle`.
- **AC-07**: The implicit vote step runs inside `maintenance_tick` in background.rs, after
  GC and before confidence refresh, as a `spawn_blocking` call.
- **AC-08**: The tick processes at most `IMPLICIT_VOTE_BATCH_LIMIT` sessions per tick.
  If the backlog exceeds this limit, processing continues on subsequent ticks.
- **AC-09**: Entries that no longer exist (deleted or quarantined between injection and tick)
  are silently skipped — the existing `record_usage_with_confidence` behavior handles this.
- **AC-10**: Unit tests in `store/src/sessions.rs` or `store/src/implicit_votes.rs` verify:
  - Schema migration v12→v13 sets `implicit_votes_applied = 0` on existing rows
  - Session filtering returns only `implicit_votes_applied = 0` sessions with resolved outcomes
  - After processing, `implicit_votes_applied = 1` and vote counts are correct
  - Deduplication: a session with 3 injection_log rows for the same entry_id produces
    exactly 1 `helpful_count` increment (not 3)
- **AC-11**: Integration test: create entries, insert injection_log rows for a "success"
  session, run the implicit vote tick, verify `helpful_count` incremented and
  `implicit_votes_applied = 1`. Run the tick a second time: verify `helpful_count` unchanged
  (no double-counting).

## Constraints

1. **No schema change to `entries` table** — `helpful_count` and `unhelpful_count` are existing
   `u32` columns. The confidence formula in crt-019 consumes them as-is. No new columns on entries.
2. **Schema migration to v13 is required** — Adding `implicit_votes_applied` to `sessions` requires
   a migration. The migration must backfill `0` for all existing rows (the SQLite `DEFAULT`
   handles this for `ALTER TABLE ADD COLUMN`). The migration pattern follows the established
   `migration.rs` idiom (check version, apply DDL, update counter).
3. **Tick duration budget** — The maintenance tick has a `TICK_TIMEOUT = 120s` timeout.
   The implicit vote step must fit within the remaining budget after GC and before the timeout.
   The `IMPLICIT_VOTE_BATCH_LIMIT = 500` session cap bounds per-tick processing time.
4. **Bayesian prior snapshot pattern** — `alpha0`/`beta0` must be snapshotted from
   `ConfidenceStateHandle` on the async thread before entering `spawn_blocking`, matching the
   established pattern in `UsageService::record_mcp_usage`. No lock may be held across an
   `await` point.
5. **GC ordering** — The implicit vote step must run after `gc_sessions`. If it runs before GC,
   a session being processed could be deleted mid-tick. After GC, only sessions older than
   30 days have been removed — well within any session's processing window.
6. **`record_usage_with_confidence` interface** — The function takes `all_ids`, `access_ids`,
   `helpful_ids`, `unhelpful_ids`, `decrement_helpful_ids`, `decrement_unhelpful_ids`. For
   implicit votes: `all_ids = helpful_ids ∪ unhelpful_ids`, `access_ids = []` (no access
   count bump from background processing).
7. **Integer vote storage** — `helpful_count` and `unhelpful_count` are `u32`. The 0.5
   unhelpful weight must be implemented without fractional storage.
8. **Test infrastructure extension** — Extend `TestDb` and existing test patterns in
   `sessions.rs`, `write_ext.rs`. Do not create isolated scaffolding.

## Resolved Design Decisions

| # | Question | Decision |
|---|----------|----------|
| 1 | Unhelpful signal | **No implicit unhelpful votes in v1** — session failure cannot be reliably attributed to individual injected entries. Only success sessions produce a vote. Deferred to future feature with a more reliable attribution mechanism. |
| 2 | Cold-start batch cap size & ordering | **Architect proposes with rationale** — cap always applied; architect decides default and ordering (oldest-first vs newest-first). |
| 3 | Abandoned sessions signal | **Zero signal** — too ambiguous for quality inference. |
| 4 | TimedOut sessions | **Treat as abandoned → zero signal** — outcome unknown, excluded from processing. |
| 5 | Double-count with real-time Stop hook path | **Option A** — Stop hook (`listener.rs`) also sets `implicit_votes_applied = 1`. Schema column approved. |

## Open Questions

None. All design decisions resolved.

## Tracking

GitHub Issue: https://github.com/dug-21/unimatrix/issues/267
