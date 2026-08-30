## ADR-002: Cold-Start Batch Cap Default and Session Ordering

### Context

The first tick after a v13 upgrade encounters a cold-start backlog: all sessions accumulated since
the server was deployed (up to the 30-day GC retention window) are unprocessed
(`implicit_votes_applied = 0`). The batch cap `IMPLICIT_VOTE_BATCH_LIMIT` bounds how many sessions
are processed per tick, preventing a single tick from exceeding `TICK_TIMEOUT = 120s`.

Two design axes must be decided:

**Axis 1: Batch cap default**

The cap must be small enough that worst-case per-tick duration fits within `TICK_TIMEOUT` while
the tick also runs `run_maintenance` (which includes confidence refresh, GC, graph compaction, etc.).
`run_maintenance` has a 200ms confidence refresh guard and takes typically 2–10s. This leaves
110–118s for the implicit vote step.

Performance estimate at cap = 500 sessions (5 average injections per session):
- Session query: ~1ms (indexed)
- Injection log scan: 2,500 rows across 10 chunks: ~10–20ms
- Helpful vote transaction (up to 2,500 entry updates): ~250ms
- Pair accumulation RMW + unhelpful transaction: ~100ms
- Mark applied: ~5ms
- Total: ~400–600ms

At cap = 500 this is comfortably within budget. At cap = 5,000 the estimate grows to 4–6s —
still within budget but leaving less margin for variance. The real risk is confidence
recomputation being disabled on very long sessions with many entries (see ADR-004).

Cap = 500 is the established precedent for batch operations in this codebase
(`MAX_CONFIDENCE_REFRESH_BATCH` is also 500 as of crt-019).

**Axis 2: Session ordering (oldest-first vs newest-first)**

Two orderings were considered:

*Newest-first (`ended_at DESC`)* — prioritizes recent sessions. During cold-start, the most
recently closed sessions get votes first. Older sessions drain last.
- Pro: Recent confidence signal reaches entries quickly.
- Con: During the cold-start drain, entries receive votes from recent sessions while their votes
  from historical sessions are still pending. Confidence scores from recent sessions arrive
  first, which may be inconsistent with the historical signal that will arrive later. No ordering
  guarantee during drain.

*Oldest-first (`ended_at ASC`)* — processes sessions in chronological close order.
- Pro: Confidence signal accumulates in the same temporal order as actual usage, preserving
  monotonic vote progression per entry. When a cold-start drain completes, the entry's vote
  history is consistent with how it was used over time.
- Pro: Each tick makes progress from the oldest unprocessed session forward. The watermark
  advances monotonically. Debugging is straightforward: "all sessions before timestamp T have
  been processed."
- Con: During cold-start, very recent sessions wait until older ones drain. In practice this
  means newly-closed sessions from the last day or two are processed last. The delay is bounded
  by the number of ticks needed to drain the backlog (typically 1–5 ticks at 500/tick).

### Decision

**Batch cap**: `IMPLICIT_VOTE_BATCH_LIMIT = 500` sessions per tick.

**Ordering**: Oldest-first (`ORDER BY ended_at ASC NULLS LAST`). Sessions with `ended_at IS NULL`
are excluded by the `outcome IS NOT NULL` filter, so `NULLS LAST` is a safety guard only.

Rationale: oldest-first preserves temporal consistency of the vote history and makes the drain
progress predictable. The delay for newly-closed sessions during cold-start is bounded and
acceptable — these sessions will be processed within `ceil(backlog_size / 500)` ticks at most.

**Cold-start drain time estimate**: At typical load (50–200 sessions per 30-day window), cold-start
requires 1 tick. At high load (5,000 sessions), cold-start requires 10 ticks = 150 minutes. The
`IMPLICIT_VOTE_BATCH_LIMIT` constant is named explicitly so it can be tuned without code changes.

**SQL**:
```sql
SELECT {SESSION_COLUMNS}
FROM sessions
WHERE implicit_votes_applied = 0
  AND status = 1    -- Completed only (TimedOut excluded, see Open Questions in ARCHITECTURE.md)
  AND outcome IS NOT NULL
ORDER BY ended_at ASC
LIMIT ?1
```

The index `idx_sessions_pending_votes ON sessions(implicit_votes_applied, status)` makes this
query efficient: it filters to the candidate set first, then sorts.

### Consequences

**Easier**:
- Progress is deterministic and traceable: "all sessions with `ended_at < T` are processed."
- Confidence scores reflect the historical usage arc of each entry.
- During steady-state (after cold-start), each tick processes only sessions from the last 15
  minutes — typically 1–10 sessions, well within budget.

**Harder**:
- During cold-start, newly-closed sessions wait behind the historical backlog. This is a
  one-time cost on upgrade.
- If `ended_at` is NULL for some completed sessions (edge case: session closed via sweep before
  `ended_at` was written), those sessions appear last in the ordering and are processed after all
  timestamped sessions. This is acceptable behavior.
