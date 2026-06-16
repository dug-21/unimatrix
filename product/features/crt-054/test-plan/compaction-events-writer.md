# Test Plan — `compaction_events` writer (at `handle_compact_payload`)

**Component**: the Surface A INSERT at `uds/listener.rs:1854`, after `increment_compaction` returns and the buffer-tail guard is dropped; captures `high_water` (guard dropped), INSERTs one row via the helper; named failure counter on error; non-blocking. No registry/session/buffer lock held across the INSERT (ADR-007).
**Pseudocode**: `pseudocode/compaction-events-writer.md` · **Layer**: integration (drives the handler seam).
**Anchor ACs**: **AC-02** (one-row/seconds/high_water/multi-row), **AC-03** (undeclared row), **AC-04** (lock graph — Critical), AC-04a (failure counter — shared w/ helper), **AC-16** (seconds-producer half). **Risks**: **R-03 (Critical)**, R-09, R-11, R-13, R-14, R-15.

## One-row contract (AC-02, R-11/R-13/R-14) — integration

`crates/unimatrix-server/src/uds/listener/tests/` (alongside `purge_audit.rs`).

1. `test_compaction_writes_one_row` — Arrange: a session with a non-trivial buffer (`high_water > 0`). Act: drive one compaction through `handle_compact_payload`. Assert: exactly one new `compaction_events` row with the correct `session_id`. (FR-A2/A3, AC-02.)
2. `test_compacted_at_is_seconds_within_tolerance` — Assert `compacted_at` is Unix **seconds** (not millis): within a few seconds of `now_secs()`, and consistent with the clock source crt-055 gates against (`.as_secs()`). A millis value would be ~1000× too large → fails. (R-11, AC-02; the AC-16 producer half — see §AC-16.)
3. `test_high_water_equals_buffer_high_water` — Assert the row's `high_water` equals the buffer's `high_water()` captured at compaction (non-default where the buffer sent bytes). (R-13, AC-02.) Fixture MUST send non-trivial bytes so the assertion is not trivially `0`.
4. `test_second_compaction_adds_monotonic_row` — Act: compact the same session twice. Assert: TWO distinct rows (0..N, insert-only); the second `compacted_at >= first` (monotonic). No UPDATE/DELETE path exists. (R-14, R-11 scenario 2, AC-02.)

## Undeclared-session row (AC-03, R-08) — integration

5. `test_compaction_row_written_for_undeclared_session` — Arrange: a session with NO `feature_cycle`. Act: compact it. Assert: a session-keyed row IS written (Surface A is declaration-independent; written at the handler). (FR-A5, AC-03.)
6. `test_compaction_events_no_feature_cycle_or_content_column` — schema assertion: `compaction_events` has no `feature_cycle` column and no content/payload column. (AC-03; cross-ref migration plan.)

## Lock graph (AC-04, R-03/R-09) — INTEGRATION, CRITICAL

7. `test_compaction_insert_under_lock_contention_no_deadlock` — Arrange: drive compaction through the handler while concurrent delta routing (registry/session lock contention) + background store writes run. Assert: no deadlock, no timeout; the compaction ACK completes within bound. (R-03, AC-04.)
8. `test_high_water_guard_dropped_before_insert` — Assert the `high_water()` read and the INSERT do not overlap in lock scope: the `Arc`-shared `high_water` is captured, the buffer guard released, THEN the INSERT runs (no buffer/registry/session lock held across the DB write). Verified by review (lock-ordering documented in ADR-007) + the contention test (7). Pattern #3753 (use the captured snapshot, never hold/re-acquire across a new step). (R-09, AC-04.)

### Negative-mutation (AC-04)
- An INSERT placed BEFORE `increment_compaction` returns, or under a held buffer/registry lock, must surface in the contention test (7) as a stall/deadlock or be caught by the lock-ordering review (8).

## Failure path (AC-04a, R-15) — shared with compaction-insert-helper.md

9. `test_insert_failure_non_blocking_no_panic` — force the helper's INSERT to fail at the seam; assert the compaction path proceeds (ACK completes), the handler never panics, and no row lands for that event. (R-03 scenario 3, R-15, AC-04a.) The **named-counter** assertion (`compaction_events_insert_failed`) is owned by compaction-insert-helper.md test 1; this test asserts the seam-level non-blocking behavior.

## AC-16 — seconds-producer half (co-owned with crt-055; PHYSICALLY LANDED HERE)

10. `test_compaction_events_seconds_boundary` (in-crate integration here; mirrored in infra-001 `test_lifecycle.py` per OVERVIEW §4.3) — **crt-054 owns and lands the seconds-PRODUCER assertion**: a compaction at a known wall-clock instant writes a row whose `compacted_at` is Unix **seconds** (`.as_secs()`), within tolerance of `now`, monotonic across repeats. This is the producer guarantee crt-055's gate rests on, landed as an INTEGRATION test (driven through the compaction seam), NOT unit-only.
   - **References crt-055 for the normalization half**: crt-055 owns the `ts/1000` (epoch millis → seconds) normalization and the `read_ts_secs > compacted_at` pre/post classification (crt-055 Binding constraint 8). crt-054 does NOT land that half; the full pre/post-boundary classification is crt-055's consumer test consuming these seconds rows.
   - **Neither side lands its half as a unit-only test** (OVERVIEW §5). SM confirms this split at the producer/consumer test-plan handoff.

## Fixtures / dependencies
- Depends on compaction-events-migration.md (table must exist) and compaction-insert-helper.md (the INSERT + counter).
- Reuse the `handle_compact_payload` / `increment_compaction` test harness and `purge_audit.rs` patterns — extend, do not re-scaffold.
