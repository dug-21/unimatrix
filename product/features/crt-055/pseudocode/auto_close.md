# Component 8 — auto_close handler arm (#593)

**Crate**: `unimatrix-server`
**Files**: `tools.rs` — `context_cycle_review` params (`:380`) + handler (`:1943`); reuses the existing `cycle_events` event writer (e.g. `insert_cycle_event`, used at `cycle_review_index.rs` tests and the handler)
**ADRs**: ADR-010 (#5045) | **Risks**: R-14 (ordering/duplication) | **Wave**: 2/3 (rides; must precede rank-1)

## Purpose

Add `auto_close: bool` (default `false`). When `true` and the cycle has no `cycle_stop` row, write `cycle_stop` SYNCHRONOUSLY at the TOP of the full-pipeline block — before rank-1 reads the timeline — using the existing `cycle_events` writer (NOT a second `store_cycle_review`). Idempotent.

## Constraints honored

- Writes a `cycle_events` row, NOT a `cycle_review_index` row → does not violate the single `store_cycle_review` writer (Constraint 1 / ADR-002).
- Ordered before rank-1 reckoning (Component 3) so the final phase closes and is not a false #556 never-closed (R-14/R-15).
- Idempotent: no-op if a `cycle_stop` already exists.
- Informs/closes a record; never controls execution (RQ-8 edge).

## 8a. Parameter

```
ADD to the context_cycle_review params struct (tools.rs:380):
    #[serde(default)] auto_close: bool       // default false
```

## 8b. Handler arm (top of the full-pipeline block, before rank-1)

```
// Runs at the TOP of the full-pipeline block (Component 9 step 1), BEFORE rank-1
// reads cycle_events. NOT on memo-hit / purged-retain / force+purged returns.
fn maybe_auto_close(store, feature_cycle, auto_close):
    if not auto_close:
        return    // default — leave timeline as-is; an open final phase surfaces
                  // as never-closed per #556 (correct fail-loud, not an error)
    // Idempotency check: does a cycle_stop already exist for this cycle?
    has_stop = EXISTS(SELECT 1 FROM cycle_events
                      WHERE cycle_id = feature_cycle AND event_type = 'cycle_stop')
    if has_stop:
        return    // idempotent no-op (re-review with auto_close=true writes no duplicate)
    // Write cycle_stop synchronously via the EXISTING cycle_events writer.
    now = now_unix_secs()
    store.insert_cycle_event(
        feature_cycle, /*seq*/ <next-seq-or-0>, "cycle_stop",
        /*phase*/ None, /*outcome*/ None, /*detail*/ None, now, /*goal*/ None)
    // After this returns, rank-1 (Component 3) reads a timeline that includes cycle_stop,
    // so still-open phases are closed at `now` and are NOT counted never-closed.
```

Confirm the exact `insert_cycle_event` signature / a synchronous `cycle_stop`-write helper at implementation (the handler already calls `insert_cycle_event` in tests with `(cycle_id, seq, event_type, phase, outcome, detail, timestamp, goal)`). Reuse it; do NOT introduce a new writer (ADR-010, Architecture §10 Q2).

## Data flow

- IN: `auto_close: bool`, `feature_cycle`, store handle.
- OUT: at most one `cycle_stop` row in `cycle_events` (side effect) BEFORE rank-1 reads the timeline.

## Error handling

- `insert_cycle_event` Err → log; the review can still proceed (the timeline simply stays open and the final phase surfaces never-closed — honest, not silently wrong). Do not abort the whole review on the auto-close write failing.
- The existence check Err → treat as "unknown" and skip the write (safer than a possible duplicate); log.

## Key test scenarios (AC-15, R-14)

- `auto_close=true`, no prior stop → `cycle_stop` written synchronously at the TOP, before rank-1 reads the timeline; final phase closes (not a false never-closed).
- `auto_close=true`, stop already exists → no duplicate written (idempotent).
- `auto_close=false` (default) → no stop; an open final phase correctly surfaces as never-closed (#556 fail-loud).
- The write goes to `cycle_events` via the existing event writer, not a second `store_cycle_review` (no second cycle_review_index writer).
- Re-review with `auto_close=true` after a stop exists → idempotent no-op.
