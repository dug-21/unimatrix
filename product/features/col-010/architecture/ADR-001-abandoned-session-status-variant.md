# ADR-001: Abandoned Session as a Distinct SessionLifecycleStatus Variant

**Feature**: col-010
**Status**: Accepted
**Date**: 2026-03-02

## Context

`SessionLifecycleStatus` has three variants in the initial design from SCOPE.md: `Active`, `Completed`, `TimedOut`. Abandoned sessions were to be written as `status = Completed, outcome = "abandoned"` — no distinct status variant.

SR-06 from the scope risk assessment identified this as a correctness gap: `scan_sessions_by_feature()` returning `status = Completed` sessions would mix abandoned sessions with genuine successes and rework sessions. The `from_structured_events()` retrospective function uses session outcomes for narrative synthesis — including abandoned sessions inflates metrics with injection events from incomplete/cancelled work.

## Decision

Add `Abandoned` as a fourth `SessionLifecycleStatus` variant.

```rust
pub enum SessionLifecycleStatus {
    Active,
    Completed,
    TimedOut,
    Abandoned,    // session ended without completing meaningful work
}
```

- `SessionClose` with `final_outcome = Abandoned` writes `status = Abandoned`, `outcome = "abandoned"`.
- `SessionClose` with `final_outcome = Success` or `Rework` writes `status = Completed`.
- `scan_sessions_by_feature()` returns all sessions; callers filter by status.
- `from_structured_events()` excludes `Abandoned` sessions from hotspot metric computation.
- GC logic (`gc_sessions`) applies to all statuses uniformly (delete after DELETE_THRESHOLD_SECS regardless of status).

## Rationale

A distinct `Abandoned` variant costs one enum arm. It provides:
1. Precise filtering in `from_structured_events()` — abandoned sessions don't corrupt retrospective metrics.
2. Clean `scan_sessions_by_feature()` — callers can filter without inspecting the `outcome` string field.
3. Explicit semantics — `Completed` means the session ran to conclusion (Success or Rework); `Abandoned` means it was cut short.

The alternative (encoding abandoned state in `outcome` field while keeping `status = Completed`) conflates lifecycle status with outcome string, requiring string comparisons for status-based queries.

## Consequences

- `SessionLifecycleStatus` has 4 variants instead of 3.
- `from_structured_events()` must filter `status != Abandoned`.
- Auto-outcome entries (§4 of ARCHITECTURE.md) are not written for `Abandoned` sessions.
- No impact on GC logic — all statuses treated uniformly for time-based deletion.
- No impact on the SESSIONS table schema — `status` is serialized inside the bincode blob.
