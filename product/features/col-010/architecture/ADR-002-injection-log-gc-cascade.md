# ADR-002: INJECTION_LOG GC Cascade in gc_sessions

**Feature**: col-010
**Status**: Accepted
**Date**: 2026-03-02

## Context

SR-04 identified a data integrity gap in the initial GC design: `gc_sessions()` deletes SESSIONS records older than 30 days, but `InjectionLogRecord` entries are keyed by monotonic `u64` with `session_id` stored as a field — not as a foreign key index. Nothing specified cascading deletes for INJECTION_LOG.

At the stated volume of <5,000 injection records/day, after 30 days of steady use, deletion without cascade produces ~150,000 orphaned injection records minimum. The `from_structured_events()` function does a full scan of INJECTION_LOG filtered in-process by `session_id`; orphaned records waste scan time and memory.

## Decision

`gc_sessions(timed_out_threshold_secs, delete_threshold_secs)` performs INJECTION_LOG cascade deletion in the same `WriteTransaction`:

```
Phase 1: collect session_ids to delete
         (sessions where started_at + delete_threshold_secs < now_secs)
Phase 2: full scan INJECTION_LOG; collect log_ids where session_id ∈ deletion set
Phase 3: delete all collected INJECTION_LOG entries
Phase 4: delete all collected SESSIONS entries
Phase 5: mark Active sessions where started_at + timed_out_threshold_secs < now as TimedOut
         (update in-place, no deletion)
```

All five phases run in one `redb::WriteTransaction`. Returns `GcStats { timed_out: u32, deleted: u32, log_entries_deleted: u32 }`.

## Rationale

The cascade is straightforward: collect IDs first (Phase 1+2), then delete (Phase 3+4). Single transaction ensures atomicity — no partial state where session is deleted but orphaned log entries remain.

The INJECTION_LOG scan at GC time (Phase 2) is a full table scan. At 30-day GC cycles and <5K records/day volume, this scan processes ≤150K records — acceptable for a maintenance operation that runs only during `maintain=true` calls. The scan uses redb's table iteration which is O(n) and cache-friendly.

Alternative considered: INJECTION_LOG secondary index by `session_id` for faster cascade lookup. Rejected: premature optimization at current volumes, adds index maintenance cost on every injection write.

## Consequences

- INJECTION_LOG stays bounded: records older than 30 days (via their parent session) are removed.
- `from_structured_events()` scans only records whose parent sessions are within the 30-day window.
- GC operation is slightly heavier (full INJECTION_LOG scan) but runs infrequently (maintenance-only path).
- No orphaned injection records accumulate over time.
- `GcStats` returns `log_entries_deleted` for observability — surfaces in `context_status` response when maintain=true.
