# crt-020: Implicit Helpfulness from Outcome Signals — Specification

## Objective

Close the confidence feedback loop for automated delivery pipelines that never produce
explicit `helpful: true` votes. By joining `injection_log` with resolved session outcomes
at background tick time, the system derives implicit helpful votes from observable session
results for success sessions only. This converts session outcome data — already captured by
the hook pipeline — into confidence signal without requiring agent cooperation.

---

## Ubiquitous Language

| Term | Definition |
|------|------------|
| **Implicit vote** | A `helpful_count` increment derived from session outcome, not from an explicit agent call with `helpful: true`. In v1, only helpful implicit votes are produced; `unhelpful_count` is never incremented by this feature. |
| **Explicit vote** | An increment to `helpful_count` or `unhelpful_count` produced by the real-time Stop hook path (`run_confidence_consumer`) from a `SignalType::Helpful` or `SignalType::Unhelpful` signal. |
| **Vote source** | The taxonomic origin of a vote: `explicit` (Stop hook signal queue) or `implicit` (outcome join, this feature). |
| **Outcome** | The resolved string on `sessions.outcome`: `"success"`, `"rework"`, or `"abandoned"`. |
| **Session status** | The `SessionLifecycleStatus` enum: `Active (0)`, `Completed (1)`, `TimedOut (2)`, `Abandoned (3)`. |
| **Zero-signal session** | Any session that does not have `status = Completed (1)` and `outcome = "success"`. This includes rework, abandoned, TimedOut, and NULL-outcome sessions. Session failure cannot be attributed to individual entries; no implicit votes are produced. |
| **Success signal** | One full implicit helpful vote (`helpful_count + 1`) per injected entry for sessions with `status = Completed (1)` and `outcome = "success"`. |
| **Deduplication guard** | The `implicit_votes_applied` boolean column on the `sessions` table (schema v13). Set to `1` when implicit votes have been applied for a session, by either the background tick or the Stop hook path, to prevent double-counting. |
| **Cold-start batch** | The one-time backfill that applies implicit votes to all historical `Completed` sessions with `implicit_votes_applied = 0` when the feature is first deployed. Capped by `IMPLICIT_VOTE_BATCH_LIMIT`. |
| **Background tick** | The 15-minute maintenance loop in `background.rs` / `maintenance_tick`. The implicit vote sweep runs inside the maintenance tick as a new sub-step. |
| **Stop hook path** | The real-time path: `SessionClose` handler in `listener.rs`, currently calling `run_confidence_consumer`. The Stop hook sets `implicit_votes_applied = 1` on the session record to prevent the background tick from re-processing. |
| **injection_log** | The `injection_log` SQLite table: one row per entry served to a session during `ContextSearch`. Fields: `log_id`, `session_id`, `entry_id`, `confidence`, `timestamp`. |

---

## Functional Requirements

### Core Vote Derivation

**FR-01** — For each `Completed` session (status = 1) with `outcome = "success"` and
`implicit_votes_applied = 0`, the system SHALL apply one implicit helpful vote
(`helpful_count + 1`) to every distinct `entry_id` appearing in that session's
`injection_log` rows.

**FR-02** — Sessions with any outcome other than `"success"` — including `"rework"`,
`"abandoned"`, `NULL`, `status = TimedOut (2)`, and `status = Abandoned (3)` — SHALL NOT
produce any implicit votes. Session failure cannot be attributed to specific injected entries.
`unhelpful_count` is never modified by this feature.

**FR-03** — Each session SHALL have implicit votes applied at most once. The
`implicit_votes_applied` column on the `sessions` table serves as the deduplication guard.
After applying implicit votes for a session (including success sessions with zero
injection_log rows), the system SHALL set `implicit_votes_applied = 1` for that session.

**FR-04** — Implicit votes SHALL be applied via the existing
`record_usage_with_confidence` interface: `helpful_ids` receives the deduplicated entry IDs
for the session; `unhelpful_ids` is always empty (`[]`). The `confidence_fn` argument SHALL
be provided so confidence is recomputed inline for each affected entry.

**FR-05** — Implicit votes SHALL be deduped per session per entry: if a session's
`injection_log` contains multiple rows for the same `entry_id`, that entry receives at
most one helpful vote from that session. Deduplication is performed in Rust via a
`HashSet` over entry IDs before calling `record_usage_with_confidence`.

### Background Tick Integration

**FR-06** — The implicit vote sweep SHALL run as a sub-step of `maintenance_tick` in
`background.rs`, after the existing GC step (`gc_sessions`) and before confidence refresh.
It SHALL scan `sessions` for all rows with `status = Completed (1)` and
`implicit_votes_applied = 0`, process them in a single batch capped at
`IMPLICIT_VOTE_BATCH_LIMIT`, apply votes, and mark processed sessions with
`implicit_votes_applied = 1`.

**FR-07** — `IMPLICIT_VOTE_BATCH_LIMIT` SHALL be a configurable constant with a default
value of 500. The cap prevents the first-run cold-start from blocking the maintenance tick
for an excessive duration on large historical datasets.

**FR-08** — If the implicit vote sweep encounters a session with `outcome = "success"` and
no `injection_log` rows (a valid completed session that had zero injections), it SHALL mark
that session with `implicit_votes_applied = 1` and produce no votes. This prevents
repeated no-op processing.

**FR-09** — The background tick's implicit vote sweep SHALL be idempotent. If interrupted
(server restart during processing), re-running processes only sessions where
`implicit_votes_applied = 0`. Sessions with `implicit_votes_applied = 1` are skipped.

### Stop Hook Integration

**FR-10** — The Stop hook path (`process_session_close` in `listener.rs`) SHALL set
`implicit_votes_applied = 1` on the session record during the session close write. This
prevents the background tick from re-processing sessions that were closed in real-time
while the server was running.

**FR-11** — The Stop hook path SHALL NOT apply implicit votes itself at session close
time. The background tick is the sole applier of implicit votes. The Stop hook sets the
flag to prevent double-counting between the two paths.

Rationale: The Stop hook already applies explicit votes via `run_confidence_consumer`.
Applying implicit votes in the same synchronous path would double-count for sessions
that also have explicit votes.

### Schema

**FR-12** — Schema v12 → v13 migration SHALL add column
`implicit_votes_applied INTEGER NOT NULL DEFAULT 0` to the `sessions` table and an index
on `(implicit_votes_applied, status)` to support the tick query efficiently.

**FR-13** — The migration SHALL set `implicit_votes_applied = 0` for all existing session
rows (the DEFAULT handles this for ALTER TABLE ADD COLUMN in SQLite).

---

## Non-Functional Requirements

**NFR-01 — Latency: Background tick sub-step** — The implicit vote sweep SHALL complete
within the existing 120-second tick timeout (`TICK_TIMEOUT`). Given the `IMPLICIT_VOTE_BATCH_LIMIT`
cap of 500 sessions, the sweep for a typical production batch SHALL complete in under 5 seconds.

**NFR-02 — Atomicity** — `helpful_count` increments for a session batch SHALL be applied
within a single SQLite `BEGIN IMMEDIATE` transaction. If the transaction fails, no partial
votes are recorded and the session retains `implicit_votes_applied = 0` for retry.

**NFR-03 — Cold-start safety** — On first deployment (schema migration to v13), all
historical sessions have `implicit_votes_applied = 0`. The batch cap
(`IMPLICIT_VOTE_BATCH_LIMIT = 500`) ensures the cold-start does not exceed the tick timeout.
Subsequent ticks process remaining sessions until the backlog is cleared.

**NFR-04 — No signal amplification** — Implicit votes SHALL NOT be applied to sessions
that already have explicit votes for the same entries. The `implicit_votes_applied` flag
applies at session granularity. Since the Stop hook sets the flag at session close time
(FR-10), real-time sessions that generate explicit votes will have `implicit_votes_applied = 1`
before the background tick processes them.

**NFR-05 — Confidence recomputation** — Each `helpful_count` increment from implicit votes
SHALL trigger inline confidence recomputation via `record_usage_with_confidence`'s
`confidence_fn` parameter. This preserves the established pattern from explicit vote handling.

**NFR-06 — Observability** — The background tick SHALL emit a `tracing::debug!` log line
after each implicit vote sweep with: sessions processed, implicit helpful votes applied,
sessions skipped (zero injections or zero-signal outcome).

**NFR-07 — Entry deletion safety** — Applying implicit votes to a deleted entry SHALL be
a no-op. `record_usage_with_confidence` already skips non-existent entry IDs (verified
in `write_ext.rs`).

---

## Acceptance Criteria

### Vote Derivation

**AC-01** — Schema migration v12 → v13 adds the `implicit_votes_applied` column to
`sessions` (INTEGER NOT NULL DEFAULT 0). All existing sessions in a v12 database have
`implicit_votes_applied = 0` after migration. Verified by inspecting the schema of an
upgraded database.

**AC-02** — Each background maintenance tick queries sessions where
`implicit_votes_applied = 0 AND status = 1 AND outcome IS NOT NULL`, up to
`IMPLICIT_VOTE_BATCH_LIMIT` (default 500) sessions per tick.

**AC-03** — For each eligible session with `outcome = "success"` and 3 distinct entries in
`injection_log`: after the implicit vote sweep runs, each of the 3 entries has `helpful_count`
incremented by exactly 1, and the session's `implicit_votes_applied = 1`.

**AC-04** — Sessions with `outcome != "success"` (rework, abandoned, NULL) and sessions
with `status = TimedOut (2)` or `status = Abandoned (3)` produce zero signal. No
`helpful_count` or `unhelpful_count` increments occur for these sessions. `unhelpful_count`
is not modified by this feature under any circumstances. Verified by: (a) session with
`outcome = "rework"` — no vote increment after sweep; (b) TimedOut session — no vote increment
after sweep; (c) session with `outcome = NULL` — excluded from sweep query.

**AC-05** — After processing, the session's `implicit_votes_applied` flag is set to 1.
A session is never processed twice. Verified by: run the sweep twice; confirm `helpful_count`
unchanged after the second run.

**AC-06** — Given a `Completed` session with `outcome = "success"` and `injection_log`
containing 2 rows for the same `entry_id` (duplicate entries): after the sweep, that entry
receives exactly 1 `helpful_count` increment, not 2 (per-session per-entry deduplication
via HashSet).

**AC-07** — Confidence is recomputed for each entry that receives an implicit vote, using
the current Bayesian prior parameters `(alpha0, beta0)` from `ConfidenceStateHandle`.
The recomputed confidence is persisted to the `entries` table before the transaction commits.

### Double-Counting Prevention

**AC-08** — Given a session closed by the Stop hook in real-time: the session record has
`implicit_votes_applied = 1` set at session close time. When the next background tick runs,
this session is excluded from the implicit vote sweep (it is not re-processed). Verified by
inspecting `implicit_votes_applied = 1` before the tick runs and confirming `helpful_count`
does not change after the tick.

**AC-09** — The background tick implicit vote sweep and the Stop hook real-time path are
disjoint: a session processed by the Stop hook (with `implicit_votes_applied = 1`) is not
re-processed by the background sweep, and a session closed before server start (with
`implicit_votes_applied = 0`) IS processed by the background sweep.

### Cold-Start and Ordering

**AC-10** — Given a database with 1,000 historical `Completed` sessions with
`outcome = "success"` and `implicit_votes_applied = 0`: the first background tick processes
exactly `IMPLICIT_VOTE_BATCH_LIMIT` sessions (oldest-first), and subsequent ticks process
the remainder until all sessions have `implicit_votes_applied = 1`. The implicit vote sweep
completes each batch without exceeding the tick timeout.

**AC-11** — Given a `Completed` session with `outcome = "success"` and 0 rows in
`injection_log` (session had no injections): `implicit_votes_applied` is set to 1 and no
vote counters are modified (no-op path, no error).

---

## Domain Models

### Session Outcome Mapping

```
SessionLifecycleStatus x outcome -> Vote Signal
─────────────────────────────────────────────────
Completed (1) + "success"   -> 1 helpful vote per distinct injected entry
Completed (1) + "rework"    -> 0 (zero signal — cannot attribute failure to entries)
Completed (1) + "abandoned" -> 0 (zero signal — this combination is unusual; zero signal)
Completed (1) + NULL        -> 0 (zero signal — excluded from sweep query)
Abandoned (3) + any         -> 0 (excluded — not matched by WHERE status = 1)
TimedOut  (2) + any         -> 0 (excluded — not matched by WHERE status = 1)
Active    (0) + any         -> 0 (excluded — session not yet complete)
```

### Vote Source Taxonomy

```
Vote Source
├── Explicit (existing, unchanged)
│   └── SignalType::Helpful / Unhelpful in SIGNAL_QUEUE
│       └── Applied by run_confidence_consumer at Stop hook time
└── Implicit (new, this feature)
    └── Derived from injection_log JOIN sessions WHERE implicit_votes_applied = 0
        └── Success path only -> helpful_count + 1 (full weight, per distinct entry)
            (unhelpful_count: never modified by crt-020)
```

### Key Entities

**SessionRecord** (`sessions` table): `session_id`, `status` (u8),
`outcome` (Option<String>), `total_injections` (u32),
`implicit_votes_applied` (INTEGER, schema v13 addition).

**InjectionLogRecord** (`injection_log` table): `log_id`, `session_id`,
`entry_id`, `confidence`, `timestamp`. The join key for implicit vote derivation is
`session_id -> entry_id`.

**EntryRecord** (`entries` table): `helpful_count`, `unhelpful_count`, `confidence`.
`helpful_count` is the write target of the implicit vote sweep. `unhelpful_count` is
not modified by this feature.

---

## User Workflows

### Workflow 1: Automated Pipeline Session (primary use case)

1. Agent session starts: `SessionRegister` hook creates `SessionRecord` with status=Active.
2. During session: `ContextSearch` requests produce `InjectionLogRecord` rows.
3. Session ends via Stop hook: `SessionClose` handler runs.
   - Resolves outcome ("success" or "rework" based on rework threshold).
   - Updates `sessions` record with `status=Completed`, `outcome`, `implicit_votes_applied=1`.
   - Explicit votes from `SIGNAL_QUEUE` are applied by `run_confidence_consumer`.
   - No implicit votes are applied at this point.
4. Next background tick (within 15 minutes): implicit vote sweep runs.
   - Session has `implicit_votes_applied = 1` → skipped (no double-counting).
5. Net result: explicit votes applied; implicit votes deferred and then skipped (Stop hook
   set the flag). No implicit votes are double-applied for real-time sessions.

### Workflow 2: Session Completed Before Server Start (cold-start / offline session)

1. Historical session exists in `sessions` with `status=Completed`, `outcome="success"`,
   `implicit_votes_applied=0` (pre-crt-020 sessions or sessions from downtime).
2. Server starts, runs first background tick.
3. Implicit vote sweep finds this session.
4. For each distinct `entry_id` in `injection_log` for this session:
   - Applies `helpful_count + 1` and recomputes confidence.
5. Sets `implicit_votes_applied = 1` on session.
6. Net result: entries that contributed to successful sessions receive retroactive
   confidence signal.

### Workflow 3: Zero-Signal Session

1. Session closes with `outcome = "rework"`, `outcome = "abandoned"`, `outcome = NULL`,
   or `status = TimedOut`.
2. Stop hook sets `implicit_votes_applied = 1` (for real-time closes) or the background
   tick excludes the session via `WHERE status = 1` filter (for TimedOut/Abandoned).
3. No implicit votes are produced. `unhelpful_count` is unchanged.

---

## Constraints

**C-01** — No new columns on the `entries` table. Implicit votes write to the existing
`helpful_count` column via the existing `record_usage_with_confidence` interface. Schema
changes are limited to the `sessions` table (`implicit_votes_applied` column).

**C-02** — `IMPLICIT_VOTE_BATCH_LIMIT` is a configurable constant with a default of 500.
Whether the constant is configurable via environment variable is an architect decision.
It must fit within the `TICK_TIMEOUT` of 120 seconds.

**C-03** — `Abandoned` and `TimedOut` sessions are permanently excluded from implicit
vote processing. They are not matched by the sweep query (`WHERE status = 1`).
They are never marked with `implicit_votes_applied = 1` by the background tick.

**C-04** — The implicit vote sweep runs inside `spawn_blocking` (synchronous SQLite
operations within an async tick). It must not hold the SQLite connection lock longer
than necessary. The batch size cap (C-02) enforces this.

**C-05** — The `implicit_votes_applied` flag is set at session granularity, not entry
granularity. There is no per-entry tracking of which entries have received implicit votes
from which sessions. Deduplication is achieved by processing each eligible session exactly once.

**C-06** — Depends on crt-019 (confidence formula calibrated to use votes) and crt-018b
(session outcome infrastructure stable). crt-020 must not ship before both dependencies
are complete.

**C-07** — The implicit vote step must run after `gc_sessions` in `maintenance_tick`.
If it runs before GC, a session being processed could be deleted mid-tick. After GC, only
sessions older than 30 days have been removed — well within any session's processing window.

**C-08** — `alpha0`/`beta0` must be snapshotted from `ConfidenceStateHandle` on the async
thread before entering `spawn_blocking`. No lock may be held across an `await` point.
This matches the established pattern in `UsageService::record_mcp_usage`.

---

## Dependencies

| Dependency | Type | Rationale |
|------------|------|-----------|
| crt-019 | Feature prerequisite | Confidence formula weights recalibrated to use `helpful_count` meaningfully. Implicit votes are noise without formula calibration. |
| crt-018b | Feature prerequisite | Session outcome infrastructure and effectiveness analysis pipeline must be stable before crt-020 joins on session outcomes. |
| `unimatrix-store` `record_usage_with_confidence` | Existing API | Write target for implicit vote increments and confidence recomputation. Interface is stable (verified in `write_ext.rs`). |
| `unimatrix-store` `scan_injection_log_by_sessions` | Existing API | Join source for implicit vote derivation. Chunking (50 IDs per IN clause) already implemented. |
| `unimatrix-store` `sessions` table | Existing schema | Source of `outcome` and `status` fields. Modified by schema v13 migration to add `implicit_votes_applied`. |
| SQLite schema v12 | Current baseline | Migration target: v12 → v13. Current version confirmed in `migration.rs`. |
| `background.rs` `maintenance_tick` | Integration point | The implicit vote sweep is added as a sub-step here, after GC. Existing 120-second `TICK_TIMEOUT` applies. |
| `listener.rs` `process_session_close` | Integration point | Must set `implicit_votes_applied = 1` on session close (FR-10). |

---

## NOT In Scope

- **Implicit unhelpful votes.** `unhelpful_count` is not modified by crt-020 under any
  circumstances. Session failure (rework, abandoned, TimedOut) cannot be reliably attributed
  to individual injected entries. Implicit unhelpful is deferred to a future feature with a
  more reliable attribution mechanism.
- **Applying implicit votes in real-time at Stop hook time.** The Stop hook sets the
  `implicit_votes_applied` flag only; the background tick applies votes.
- **Per-entry session tracking.** There is no per-entry record of which sessions contributed
  implicit votes. Only the session-level flag exists.
- **Fractional vote storage.** Votes are stored as integers. No pair accumulation or
  fractional counter mechanism is introduced in v1.
- **Retroactive chronological replay of historical sessions.** Historical sessions are
  processed in oldest-first batch order, not in strict chronological outcome sequence.
- **Session duration as a signal quality filter.** Short sessions are not excluded from
  implicit vote generation. Duration is irrelevant to the outcome signal.
- **Explicit vote decrement / correction for implicit votes.** If an implicit helpful vote
  was applied and later an agent provides an explicit unhelpful signal, the existing
  `decrement_helpful_ids` correction path handles it. crt-020 does not add new correction logic.
- **Surfacing implicit vote counts in `context_status` or `context_get`.** The implicit
  vote count is not separately tracked from explicit votes in the entry record.
- **Environment variable configuration of `IMPLICIT_VOTE_BATCH_LIMIT`.** Whether the
  constant is configurable via environment variable is an architect decision (C-02).
- **TimedOut session recovery.** Sessions marked `TimedOut` by GC are permanently excluded
  from implicit votes and will never be marked `implicit_votes_applied = 1`.

---

## Open Questions for Architect

**OQ-01 — `IMPLICIT_VOTE_BATCH_LIMIT` default value confirmation.** The specification sets
the default at 500. Architect should confirm this fits within the tick timeout given expected
injection_log cardinality per session. Key input: p95 latency per session in the sweep.

**OQ-02 — `IMPLICIT_VOTE_BATCH_LIMIT` env var vs. compile-time constant.** Should this be
runtime-configurable (like `UNIMATRIX_AUTO_QUARANTINE_CYCLES`) or a compile-time constant?
If runtime, the same parse/validate pattern from `parse_auto_quarantine_cycles` applies.

**OQ-03 — Stop hook flag-set timing.** FR-10 specifies that the Stop hook sets
`implicit_votes_applied = 1` during the session close write. This is inside the
fire-and-forget `spawn_blocking` that updates the session record. Confirm: should the flag
be set in the same `update_session` call (adding it to the session update closure), or as
a separate write?

**OQ-04 — Ordering of implicit vote sweep within `maintenance_tick`.** The specification
requires the sweep to run after GC (C-07) and before confidence refresh. Confirm the exact
position relative to co-access cleanup and graph compaction. Either ordering relative to
those two steps is correct; architect should document the chosen position.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for implicit helpfulness, session outcome confidence signals, background tick patterns — found entry #1611 (Real-Time + Background Path Disjointness Must Be Explicit, pattern) confirming the double-counting risk is a known architectural concern; found ADR-002 (Server-Layer Deduplication with Vote Correction) confirming vote correction atomicity patterns.
