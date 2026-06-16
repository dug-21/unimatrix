# Component 6 — `compaction_events` writer (at `handle_compact_payload`)

**File**: `crates/unimatrix-server/src/uds/listener.rs` (modify) — at `:1854`, immediately after `session_registry.increment_compaction(session_id)`.
**ADRs**: ADR-007 (durable table, insert-only at the seam, single autocommit INSERT, NO lock across it, named failure counter), ADR-004 (written regardless of declaration).
**Depends on**: Component 7 (table), Component 8 (`insert_compaction_event` helper + counter).

## Purpose

Write exactly one durable `compaction_events` row per compaction event, co-located with the existing `increment_compaction` call, holding only the DB connection (no registry/session/buffer lock across the INSERT). On INSERT failure: increment the named counter `compaction_events_insert_failed`, log ids/counts only, and let the compaction ACK proceed (non-blocking). Written regardless of whether the session declared a `feature_cycle`.

## Seam context (verified)

`handle_compact_payload` (`:1737`) is `async`. By `:1854` the briefing build (`:1804` region) and the transcript-tail read (under the buffer lock, guard dropped at the end of its closure ~`:1833-1835`) are already done. The line at `:1854` is `session_registry.increment_compaction(session_id);` (which takes+releases the `sessions` lock internally and returns). After that line, NO registry/session/buffer lock is held.

The handler already holds `session_state` (an `Arc`-sharing clone from `get_state` at `:1753`), so the buffer `Arc` is reachable for the `high_water` read without a registry lookup.

## Pseudocode (inserted after `:1854`)

```
session_registry.increment_compaction(session_id);          // existing :1854 — returns, no lock held after

// ── NEW (Surface A). No registry/session/buffer lock held across the INSERT. ──

// 1. Capture high_water under the buffer lock, then DROP the guard before the INSERT
//    (pattern #3753 — use the captured snapshot, never hold/re-acquire the lock across a new step).
let high_water: u64 = match session_state.as_ref() {
    Some(s) => {
        let hw = { lock_buffer(&s.transcript).high_water() };   // guard dropped at end of this block
        hw
    }
    None => 0,                                                  // absent session → no buffer → default 0
};

// 2. Timestamp in Unix SECONDS, server wall clock — consistent with now_secs() (.as_secs()).
//    NOT millis. The ts/1000 gate normalization is crt-055's, not here (ADR-007 §4, AC-01a).
let compacted_at_secs: i64 = now_secs() as i64;                // now_secs(): SystemTime → UNIX_EPOCH.as_secs()

// 3. Single autocommit INSERT via the store_ops helper (Component 8). NO explicit transaction.
//    On error: the helper increments the named counter + returns Err; we log ids/counts only and proceed.
match services.store_ops.insert_compaction_event(session_id, compacted_at_secs, high_water as i64).await {
    Ok(()) => { /* durable row landed */ }
    Err(e) => {
        // Named counter already incremented inside the helper (ADR-007 §6, R-15, AC-04a).
        // Log IDS/COUNTS ONLY — never transcript bytes, never payload (ADR-005, content-free).
        tracing::warn!(session_id = %session_id, error = %e, "compaction_events INSERT failed");
        // FALL THROUGH — the compaction ACK is NEVER blocked by an INSERT failure.
    }
}

// ── existing continuation: token_count, HookResponse::BriefingContent { ... } ──
```

### Width / cast note

`compacted_at_secs` and `high_water` are passed as `i64` because the column type is `INTEGER` and the helper binds `i64`. This is the Surface-A INSERT boundary, NOT the Surface-B producer width contract (AC-14 applies to `activity_snapshot()`'s `u64`/`u32` counters, which this writer does not touch). `now_secs()` returns `u64` seconds; `high_water()` returns `u64`. The `as i64` here is the Surface-A persist cast (seconds fit in i64 for millennia; `high_water` is bounded by buffer size) — acceptable and distinct from the Surface-B no-cast rule. If a width-purist test flags it, use a `try_into`/saturating form, but a plain `as i64` is correct for these bounded values.

## Lock ordering (ADR-007, NFR-4, AC-04, R-03/R-09)

- The INSERT runs after `increment_compaction` returns; no registry/session/buffer lock is held across it.
- The `high_water` read takes the buffer lock in a tight block and drops the guard BEFORE the INSERT — the DB write never runs under the buffer lock (R-09).
- The only lock during the INSERT is the store's own connection/pool lock (owned by `store_ops`), which is independent of the handler's registry/session locks — no deadlock cycle.

## Declaration independence (ADR-004, FR-A5, AC-03)

The row is written for the `session_id` regardless of whether the session declared a `feature_cycle`. No `feature_cycle` is read or written here. Attribution to a cycle is crt-055's at review via the session→`feature_cycle` chain.

## Multi-compaction (FR-A2, R-14, AC-02)

Insert-only: each compaction event calls this code once and lands one new row. A session compacting N times produces N rows with monotonically later `compacted_at`. No UPDATE/UPSERT/DELETE path exists.

## Error handling

- INSERT failure: named counter (in helper) + warn log (ids/counts only) + fall through. Never panics, never blocks the ACK (ADR-007 §5/§6).
- Absent `session_state`: `high_water` defaults to 0; the row is still written (session-keyed).

## Key test scenarios (hints)

- One row per compaction with correct `session_id`, `compacted_at` (seconds, within tolerance of now), `high_water` == buffer's `high_water()` at compaction (AC-02).
- Second compaction on the same session adds a second row, later `compacted_at` (AC-02, R-14).
- Undeclared session: row still written, session-keyed, no `feature_cycle`/content column (AC-03).
- **Forced INSERT failure (AC-04a, R-15)**: inject a store failure; assert the named counter `compaction_events_insert_failed` increments by exactly 1 (a log assertion alone does NOT satisfy this), the ACK completes, no panic, no row lands, no content in the metric/log.
- Concurrency/lock-ordering: drive compaction under registry/session contention + background store writes; no deadlock/timeout (AC-04, R-03).
- The buffer guard for the `high_water` read is dropped before the INSERT (review + concurrency, R-09).
