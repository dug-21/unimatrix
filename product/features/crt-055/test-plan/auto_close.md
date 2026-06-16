# Test Plan — auto_close handler arm (#593)

**Component**: `unimatrix-server/src/mcp/tools.rs` — `auto_close: bool` param; write `cycle_stop` synchronously before the pipeline when absent
**Risks**: R-14 (auto_close ordering / duplication / second writer)
**ACs**: AC-15 (folded #593)

> ADR-010: when `auto_close == true` and no `cycle_stop` exists, write the stop via the EXISTING `cycle_events` event writer (NOT a second `store_cycle_review`), at the TOP of the pipeline so the timeline closes before rank-1 reads it. Idempotent.

## Unit / handler tests

### Three auto_close paths (R-14, AC-15)
- `test_auto_close_true_no_stop_writes_before_pipeline` (AC-15a) — `auto_close=true`, no prior `cycle_stop` → a `cycle_stop` is written synchronously at the TOP, before rank-1 reads the timeline → the final phase closes (NOT a false never-closed #556).
- `test_auto_close_true_stop_exists_idempotent` (AC-15b) — `auto_close=true`, a `cycle_stop` already exists → NO duplicate written (idempotent).
- `test_auto_close_false_no_stop_written` (AC-15c) — `auto_close=false` → no stop; an open final phase correctly surfaces as never-closed (#556 fail-loud, NOT an error).
- `test_auto_close_default_is_false` — the param defaults to `false` when omitted.

### Ordering / no-second-writer (R-14, AC-15)
- `test_auto_close_writes_via_event_writer_not_store_cycle_review` (R-14) — the stop is written through the existing `cycle_events` event writer, NOT a second `store_cycle_review` (Constraint 2 — single-writer invariant holds; cross-ref store_cycle_review.md AC-17).
- `test_auto_close_precedes_rank1_reckoning` (R-14) — assert the stop write site precedes the rank-1 timeline read (couples with aggregate_reckoning.md — a stop written AFTER rank-1 would mis-count the final phase as never-closed).

## Integration tests (MCP harness)
- `test_cycle_review_auto_close_writes_stop_before_pipeline` (tools suite, AC-15a) — `auto_close=true`, no prior stop, open final phase → after review the final phase is closed, not a false never-closed.
- `test_cycle_review_auto_close_idempotent_when_stop_exists` (tools suite, AC-15b) — stop exists → no duplicate.
- `test_cycle_review_auto_close_false_open_phase_never_closed` (tools suite, AC-15c) — `auto_close=false`, open final phase → honest never-closed surfaced (correct, not a bug).

## Edge cases (from RISK-TEST-STRATEGY §Edge Cases)
- Re-review with `auto_close=true` when a stop already exists → idempotent no-op (no duplicate stop, no second writer).
- `auto_close=true` on a cycle with NO phases at all → no error; nothing to close.
- `auto_close` writes a record event only — it never controls execution (NFR-07 informs-not-controls; assert no orchestration side effect).

## Expected behaviors / assertions summary
- Three paths: true+no-stop writes-before-pipeline; true+stop idempotent; false no-stop (honest never-closed).
- Stop via event writer at the TOP, before rank-1 — never a second `store_cycle_review`.
- Default `false`; informs-not-controls.
