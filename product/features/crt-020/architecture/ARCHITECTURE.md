# crt-020: Implicit Helpfulness from Outcome Signals — Architecture

## System Overview

crt-020 closes the confidence feedback loop for automated delivery pipelines that never
produce explicit `helpful: true` votes. It joins `injection_log` with resolved session
outcomes at background tick time, deriving implicit helpful votes from observable session
results for success sessions only.

The feature adds one new sub-step to the existing 15-minute maintenance tick in
`background.rs` and one new column to the `sessions` table (schema v12 → v13). No new
crates, no new MCP tools, no new tables beyond the schema migration.

Dependencies: crt-018b (session outcome infrastructure stable), crt-019 (confidence formula
calibrated to use helpful_count meaningfully).

---

## Component Breakdown

### 1. Schema Migration (unimatrix-store)

- Migration v12 → v13 adds `implicit_votes_applied INTEGER NOT NULL DEFAULT 0` to `sessions`.
- Index on `(implicit_votes_applied, status)` supports the sweep query.
- All existing rows default to 0 (cold-start eligible).
- File: `crates/unimatrix-store/src/migration.rs`

### 2. Store Primitives (unimatrix-store)

Three new synchronous functions exposed as public API:

| Function | Purpose |
|----------|---------|
| `scan_implicit_vote_candidates(conn, limit) -> Vec<SessionId>` | Returns up to `limit` session IDs with `status=1, implicit_votes_applied=0, outcome IS NOT NULL`, oldest-first |
| `get_injection_entry_ids(conn, session_ids) -> HashMap<SessionId, Vec<EntryId>>` | Joins `injection_log` → distinct entry IDs per session |
| `mark_implicit_votes_applied(conn, session_ids)` | Sets `implicit_votes_applied=1` within the same transaction as the vote writes |

These are synchronous (`&Connection`) functions consistent with the store crate's API
contract (ADR-004, Unimatrix #61). They contain no async code and no server-crate imports.

`record_usage_with_confidence` (existing) is the write target for `helpful_count` increments
and inline confidence recomputation.

### 3. Implicit Vote Sweep (unimatrix-server, background.rs)

`apply_implicit_votes` — free `async fn` in `background.rs`, called from `maintenance_tick`
after `gc_sessions` and before confidence refresh (see ADR-005, Unimatrix #1639).

Responsibilities:
- Snapshot `alpha0`/`beta0` from `ConfidenceStateHandle` on the async thread (C-08).
- Delegate synchronous SQLite work to `spawn_blocking`.
- Inside `spawn_blocking`: scan candidates, join injection_log, deduplicate entry IDs per
  session via `HashSet`, call `record_usage_with_confidence` with `helpful_ids`, mark
  sessions processed, log sweep stats.
- Return `ImplicitVoteSweepStats` (sessions_processed, votes_applied, sessions_skipped).

### 4. Stop Hook Flag-Set (unimatrix-server, listener.rs)

`process_session_close` sets `implicit_votes_applied=1` on the session record at session
close time (FR-10). This prevents the background tick from re-processing real-time sessions.
The Stop hook does NOT apply implicit votes; it only sets the deduplication flag.

---

## Component Interactions

```
maintenance_tick (background.rs)
  └── gc_sessions            [existing, step runs first — C-07]
  └── apply_implicit_votes   [new, crt-020]
       ├── read alpha0/beta0 from ConfidenceStateHandle (async thread)
       └── spawn_blocking
            ├── scan_implicit_vote_candidates (store)
            ├── get_injection_entry_ids (store)
            ├── record_usage_with_confidence (store, existing)
            └── mark_implicit_votes_applied (store)
  └── confidence_refresh     [existing, runs after]

process_session_close (listener.rs)
  └── update_session: sets implicit_votes_applied=1 [new field write, crt-020]
```

---

## Technology Decisions

| Decision | Choice | ADR |
|----------|--------|-----|
| Deduplication guard granularity | Session-level flag on `sessions` table | ADR-003 (#1614) |
| Batch cap default and ordering | 500 sessions/tick, oldest-first | ADR-002 (#1613) |
| Confidence recomputation strategy | Inline via `record_usage_with_confidence` confidence_fn | ADR-004 (#1615) |
| `apply_implicit_votes` function location | Free async fn in `background.rs` (server crate) | ADR-005 (#1639) |

Note: ADR-001 (#1612, pair accumulation counter) is superseded by the scope change to
success-only implicit votes. It is retained in Unimatrix for historical reference.

---

## Integration Points

### Existing APIs consumed (no changes to their signatures)

| Interface | Crate | Notes |
|-----------|-------|-------|
| `record_usage_with_confidence(conn, helpful_ids, unhelpful_ids, confidence_fn)` | unimatrix-store | Write target; unhelpful_ids always empty in this feature |
| `gc_sessions(store)` | unimatrix-store (called from background.rs) | Must complete before apply_implicit_votes runs (C-07) |
| `ConfidenceStateHandle::alpha0() / beta0()` | unimatrix-server | Snapshotted before spawn_blocking (C-08) |
| `maintenance_tick(...)` | unimatrix-server/background.rs | apply_implicit_votes added as sub-step |
| `process_session_close(...)` | unimatrix-server/listener.rs | Gains one new field write: implicit_votes_applied=1 |

### New store primitives (public surface, synchronous)

| Function | Signature |
|----------|-----------|
| `scan_implicit_vote_candidates` | `fn(conn: &Connection, limit: usize) -> Result<Vec<u64>, rusqlite::Error>` |
| `get_injection_entry_ids` | `fn(conn: &Connection, session_ids: &[u64]) -> Result<HashMap<u64, Vec<u64>>, rusqlite::Error>` |
| `mark_implicit_votes_applied` | `fn(conn: &Connection, session_ids: &[u64]) -> Result<(), rusqlite::Error>` |

### New server-layer type

| Type | Location | Fields |
|------|----------|--------|
| `ImplicitVoteSweepStats` | `background.rs` or `services/` | `sessions_processed: u32`, `votes_applied: u32`, `sessions_skipped: u32` |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `apply_implicit_votes` | `async fn(&Store, &ConfidenceStateHandle) -> Result<ImplicitVoteSweepStats, ServiceError>` | background.rs (new) |
| `scan_implicit_vote_candidates` | `fn(&Connection, usize) -> Result<Vec<u64>, rusqlite::Error>` | unimatrix-store (new) |
| `get_injection_entry_ids` | `fn(&Connection, &[u64]) -> Result<HashMap<u64, Vec<u64>>, rusqlite::Error>` | unimatrix-store (new) |
| `mark_implicit_votes_applied` | `fn(&Connection, &[u64]) -> Result<(), rusqlite::Error>` | unimatrix-store (new) |
| `sessions.implicit_votes_applied` | `INTEGER NOT NULL DEFAULT 0` | schema v13 migration |
| `IMPLICIT_VOTE_BATCH_LIMIT` | `const usize = 500` | background.rs (new constant) |

---

## Architectural Decisions (Summary)

| ADR | Title | Unimatrix ID |
|-----|-------|--------------|
| ADR-001 | Pair Accumulation Counter Location (superseded) | #1612 |
| ADR-002 | Cold-Start Batch Cap 500, Oldest-First Ordering | #1613 |
| ADR-003 | Double-Count Prevention via implicit_votes_applied Flag | #1614 |
| ADR-004 | Inline Confidence Recomputation in Implicit Vote Step | #1615 |
| ADR-005 | apply_implicit_votes Function Location — background.rs Free Function | #1639 |

---

## Open Questions

None. All open questions from the specification have been resolved:

- OQ-01 (batch limit default): 500 confirmed — ADR-002 (#1613).
- OQ-02 (env var vs. compile-time): Compile-time constant `IMPLICIT_VOTE_BATCH_LIMIT = 500`.
  Runtime configurability deferred; the pattern (`parse_auto_quarantine_cycles`) exists if
  needed but is not justified for this feature's expected scale.
- OQ-03 (Stop hook flag-set timing): Same `update_session` call — adding `implicit_votes_applied=1`
  to the existing session update closure. Single write, no separate round-trip.
- OQ-04 (ordering within maintenance_tick): After `gc_sessions`, before confidence refresh.
  Ordering relative to co-access cleanup and graph compaction is either-before — after GC
  is the only hard constraint (C-07). Chosen position: immediately after GC, before
  co-access cleanup, so vote counts are stable when co-access boost is computed.
- OQ (apply_implicit_votes location): Resolved by ADR-005 (#1639) — free async fn in
  `background.rs`, server crate.
