# crt-020: Implicit Helpfulness from Outcome Signals — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Specification | product/features/crt-020/specification/SPECIFICATION.md |
| Architecture | product/features/crt-020/architecture/ARCHITECTURE.md |
| ADR-005 | product/features/crt-020/architecture/ADR-005-apply-implicit-votes-location.md |

Note: SCOPE.md, SCOPE-RISK-ASSESSMENT.md, RISK-TEST-STRATEGY.md, and ALIGNMENT-REPORT.md
were produced during Session 1 but not written to disk. Their content is captured in
SPECIFICATION.md, ARCHITECTURE.md, and the spawn-prompt design summary.

---

## Goal

Close the confidence feedback loop for automated delivery pipelines that never produce
explicit `helpful: true` votes. By joining `injection_log` with resolved session outcomes
during the background maintenance tick, the system derives one implicit `helpful_count`
increment per distinct injected entry for every `Completed` session with `outcome = "success"`.
Sessions with any other outcome (rework, abandoned, TimedOut, NULL) produce zero signal;
`unhelpful_count` is never modified by this feature.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| schema-migration | pseudocode/schema-migration.md | test-plan/schema-migration.md |
| store-primitives | pseudocode/store-primitives.md | test-plan/store-primitives.md |
| implicit-vote-sweep | pseudocode/implicit-vote-sweep.md | test-plan/implicit-vote-sweep.md |
| stop-hook-flag | pseudocode/stop-hook-flag.md | test-plan/stop-hook-flag.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| No implicit unhelpful votes in v1 | Sessions with rework/abandoned/TimedOut produce zero signal; `unhelpful_count` never written by this feature. Success sessions only. | Scope revision (agent-2-spec rev2) | ADR-001 (Unimatrix #1612, superseded by scope change; retained for history) |
| Batch cap default and oldest-first ordering | `IMPLICIT_VOTE_BATCH_LIMIT = 500` compile-time constant; sweep processes oldest sessions first (`ended_at ASC`) to drain cold-start backlog predictably. Runtime configurability deferred. | Architecture OQ-01, OQ-02 resolved | architecture/ADR-002 (Unimatrix #1613) |
| Double-count prevention strategy | Session-level `implicit_votes_applied` flag on `sessions` table (schema v13). Stop hook sets flag=1 at session close; background tick skips flagged sessions. No per-entry tracking. | Architecture OQ-03 resolved | architecture/ADR-003 (Unimatrix #1614) |
| Inline confidence recomputation | `record_usage_with_confidence` called with `confidence_fn=Some(...)` so confidence is recomputed atomically with the vote write — matching the explicit vote path. | Architecture OQ resolved | architecture/ADR-004 (Unimatrix #1615) |
| `apply_implicit_votes` location | Free `async fn` in `crates/unimatrix-server/src/background.rs` alongside `process_auto_quarantine`. Server crate owns async orchestration; store crate exposes only synchronous primitives. | Architecture OQ resolved | architecture/ADR-005-apply-implicit-votes-location.md (Unimatrix #1639) |
| Stop hook flag-set timing | `implicit_votes_applied=1` set inside the existing `update_session` call — no separate round-trip write. | Architecture OQ-03 resolved | architecture/ADR-003 (Unimatrix #1614) |
| Ordering within maintenance_tick | After `gc_sessions`, before co-access cleanup and confidence refresh. Implicit votes must be stable when co-access boost is computed. | Architecture OQ-04 resolved | architecture/ADR-005 (Unimatrix #1639) |

---

## Files to Create or Modify

### unimatrix-store

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/migration.rs` | Modify | Add v12→v13 migration: `implicit_votes_applied INTEGER NOT NULL DEFAULT 0` on `sessions`, plus index on `(implicit_votes_applied, status)`. Bump `CURRENT_SCHEMA_VERSION`. |
| `crates/unimatrix-store/src/implicit_votes.rs` | Create | Three new synchronous public functions: `scan_implicit_vote_candidates`, `get_injection_entry_ids`, `mark_implicit_votes_applied`. |
| `crates/unimatrix-store/src/lib.rs` | Modify | Declare `mod implicit_votes` and re-export the three new public functions. |

### unimatrix-server

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/background.rs` | Modify | Add `IMPLICIT_VOTE_BATCH_LIMIT` constant, `ImplicitVoteSweepStats` struct, and `apply_implicit_votes` free async fn. Wire call into `maintenance_tick` after `gc_sessions`. |
| `crates/unimatrix-server/src/listener.rs` | Modify | In `process_session_close`, set `implicit_votes_applied=1` inside the existing `update_session` write. |

### Tests

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/tests/implicit_votes_tests.rs` | Create | Unit tests for the three store primitives: candidate scan, injection log join, mark-applied idempotency. |
| `crates/unimatrix-server/tests/implicit_vote_sweep_integration.rs` | Create | Integration tests for `apply_implicit_votes`: success session gets vote, rework session skipped, double-count prevention, cold-start batch capping, zero-injection no-op. |
| `crates/unimatrix-server/tests/stop_hook_flag_integration.rs` | Create | Integration test: session closed by Stop hook has `implicit_votes_applied=1` before tick; tick does not re-process it. |

---

## Data Structures

### Schema Addition (sessions table, v13)

```sql
ALTER TABLE sessions ADD COLUMN
  implicit_votes_applied INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_sessions_implicit_votes
  ON sessions (implicit_votes_applied, status);
```

### ImplicitVoteSweepStats (background.rs)

```rust
pub struct ImplicitVoteSweepStats {
    pub sessions_processed: u32,  // sessions with outcome="success" that received votes
    pub votes_applied: u32,       // total helpful_count increments written
    pub sessions_skipped: u32,    // sessions with non-success outcome or zero injections
}
```

### Session Outcome Mapping

```
SessionLifecycleStatus x outcome -> Vote Signal
────────────────────────────────────────────────
Completed (1) + "success"   -> 1 helpful vote per distinct injected entry
Completed (1) + "rework"    -> 0  (zero signal)
Completed (1) + "abandoned" -> 0  (zero signal)
Completed (1) + NULL        -> 0  (excluded by WHERE outcome IS NOT NULL)
Abandoned (3) + any         -> 0  (excluded by WHERE status = 1)
TimedOut  (2) + any         -> 0  (excluded by WHERE status = 1)
Active    (0) + any         -> 0  (excluded by WHERE status = 1)
```

### Vote Source Taxonomy

```
Vote Source
├── Explicit (existing, unchanged)
│   └── SignalType::Helpful / Unhelpful in SIGNAL_QUEUE
│       └── Applied by run_confidence_consumer at Stop hook time
└── Implicit (new, crt-020)
    └── Derived from injection_log JOIN sessions WHERE implicit_votes_applied = 0
        └── Success path only -> helpful_count + 1 (per distinct entry, per session)
            (unhelpful_count: never modified by crt-020)
```

---

## Function Signatures

### Store Primitives (unimatrix-store/src/implicit_votes.rs)

```rust
/// Returns up to `limit` session IDs with status=Completed(1),
/// implicit_votes_applied=0, and outcome IS NOT NULL, ordered oldest-first.
pub fn scan_implicit_vote_candidates(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<u64>, rusqlite::Error>;

/// For each session_id, returns the distinct entry_ids logged in injection_log.
/// Chunks session_ids into batches of 50 for the SQL IN clause.
pub fn get_injection_entry_ids(
    conn: &Connection,
    session_ids: &[u64],
) -> Result<HashMap<u64, Vec<u64>>, rusqlite::Error>;

/// Sets implicit_votes_applied=1 for all given session IDs.
/// Called within the same transaction as the helpful_count writes.
pub fn mark_implicit_votes_applied(
    conn: &Connection,
    session_ids: &[u64],
) -> Result<(), rusqlite::Error>;
```

### Server Layer (unimatrix-server/src/background.rs)

```rust
const IMPLICIT_VOTE_BATCH_LIMIT: usize = 500;

/// Runs the implicit vote sweep as a sub-step of maintenance_tick.
/// Snapshots alpha0/beta0 on the async thread before spawn_blocking.
async fn apply_implicit_votes(
    store: &Store,
    confidence_state: &ConfidenceStateHandle,
) -> Result<ImplicitVoteSweepStats, ServiceError>;
```

### Maintenance Tick Integration (existing function, modified)

```rust
// Inside maintenance_tick, after gc_sessions:
let sweep_stats = apply_implicit_votes(&store, &confidence_state).await?;
tracing::info!(
    sessions_processed = sweep_stats.sessions_processed,
    votes_applied = sweep_stats.votes_applied,
    sessions_skipped = sweep_stats.sessions_skipped,
    "implicit vote sweep complete"
);
```

### Stop Hook Change (listener.rs, existing function modified)

```rust
// Inside process_session_close -> update_session call:
// Add implicit_votes_applied = 1 to the session update closure.
// No separate write; extends the existing update_session parameters.
```

---

## Constraints

| ID | Constraint |
|----|------------|
| C-01 | No new columns on the `entries` table. Only `sessions` gains `implicit_votes_applied`. |
| C-02 | `IMPLICIT_VOTE_BATCH_LIMIT = 500` compile-time constant. Must complete within `TICK_TIMEOUT` (120s). |
| C-03 | Abandoned (3) and TimedOut (2) sessions are permanently excluded. Never marked `implicit_votes_applied=1` by the background tick. |
| C-04 | Implicit vote sweep runs inside `spawn_blocking`. Must not hold SQLite connection lock longer than one batch. |
| C-05 | `implicit_votes_applied` flag is session-granularity only. No per-entry-per-session tracking. |
| C-06 | Depends on crt-018b (session outcome infrastructure) and crt-019 (confidence formula calibrated). Must not ship before both. |
| C-07 | Implicit vote sweep must run after `gc_sessions` in `maintenance_tick`. If it runs before GC, a session could be deleted mid-tick. |
| C-08 | `alpha0`/`beta0` must be snapshotted from `ConfidenceStateHandle` on the async thread before `spawn_blocking`. No lock held across an `await` point. |

---

## Dependencies

| Dependency | Kind | Notes |
|------------|------|-------|
| crt-018b | Feature prerequisite | Session outcome infrastructure (`outcome` field, `injection_log`) must be stable. |
| crt-019 | Feature prerequisite | Confidence formula calibrated to use `helpful_count` meaningfully; implicit votes are noise without calibration. |
| `record_usage_with_confidence` | Existing API (unimatrix-store) | Write target for `helpful_count` increments and inline confidence recomputation. Stable — verified in `write_ext.rs`. |
| `scan_injection_log_by_sessions` | Existing API (unimatrix-store) | Reference pattern for chunked `IN` clause queries (50 IDs per chunk). |
| `sessions` table | Existing schema (SQLite v12) | Migration target: v12→v13. |
| `background.rs` `maintenance_tick` | Integration point (unimatrix-server) | `apply_implicit_votes` added as sub-step after GC. |
| `listener.rs` `process_session_close` | Integration point (unimatrix-server) | Gains one new field write: `implicit_votes_applied=1`. |
| `ConfidenceStateHandle` | Server-layer type (unimatrix-server) | Provides `alpha0()`/`beta0()` for the confidence closure passed to `record_usage_with_confidence`. |
| rusqlite | Existing crate dependency | Synchronous SQLite access in store primitives. |
| tokio | Existing crate dependency | `spawn_blocking` for the sweep; async orchestration in `apply_implicit_votes`. |
| tracing | Existing crate dependency | `tracing::info!` observability log after each sweep. |

---

## NOT in Scope

- **Implicit unhelpful votes.** `unhelpful_count` is never modified by crt-020. Rework/abandoned/TimedOut sessions cannot reliably attribute failure to individual entries. Deferred to a future feature.
- **Real-time implicit vote application at Stop hook time.** The Stop hook sets the flag only; the background tick applies votes.
- **Per-entry session tracking.** No record of which sessions contributed implicit votes to which entries.
- **Fractional vote storage.** Votes are integers; no pair accumulation or fractional counters.
- **Retroactive strict chronological replay.** Historical sessions processed oldest-first batch order, not strict chronological outcome sequence.
- **Session duration as a signal quality filter.** Short sessions are not excluded.
- **Surfacing implicit vote counts separately** in `context_status` or `context_get`.
- **Environment variable configuration of `IMPLICIT_VOTE_BATCH_LIMIT`.** Compile-time constant in v1.
- **TimedOut session recovery.** TimedOut sessions are permanently excluded; never marked `implicit_votes_applied=1` by the background tick.
- **Explicit vote decrement / correction for implicit votes.** Existing `decrement_helpful_ids` path handles it; crt-020 adds no new correction logic.

---

## Alignment Status

Vision guardian report: **PASS. No variances.** The feature aligns with the Unimatrix
learning-and-drift mission. Deriving confidence signal from observable session outcomes
without requiring agent cooperation is consistent with the self-improving knowledge engine
vision. The no-implicit-unhelpful constraint (v1 scope) was explicitly accepted as the
safe conservative design; attribution of failure to individual entries is deferred
until a more reliable mechanism is available.
