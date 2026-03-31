# Test Plan: run_maintenance GC Block

**Component:** `crates/unimatrix-server/src/services/status.rs` (step 4 + 4f rewrite)
**Risks Covered:** R-04, R-05, R-06, R-08
**ACs Covered:** AC-04, AC-05, AC-15, AC-16

These tests exercise the orchestration logic in `run_maintenance()` — the loop that
calls `list_purgeable_cycles`, applies the `max_cycles_per_tick` cap, invokes the
gate check and GC methods per cycle, and handles error paths. Tests use an in-memory
SQLite store constructed directly; no MCP or background thread is started.

---

## Unit Test Expectations

### `test_gc_gate_no_review_row` (AC-04 — R-05 None path)

**Arrange:**
- Insert a session and observations attributed to `feature_cycle = "ghost-cycle"`.
- Do NOT insert a `cycle_review_index` row for `"ghost-cycle"`.
- Set up `cycle_review_index` with K+1 other reviewed cycles so `"ghost-cycle"`
  would appear in the purgeable list IF the gate were bypassed (i.e., `ghost-cycle`
  has `computed_at` that would make it purgeable — but since it has no review row,
  it should not appear in `list_purgeable_cycles` at all).

**Act:** Call the GC block logic with K = K.

**Assert:**
- `"ghost-cycle"`'s sessions and observations still exist after GC.
- `list_purgeable_cycles` never returns `"ghost-cycle"` (the SQL only returns cycles
  with `cycle_review_index` rows).

**Note:** This test verifies the gate at the SQL level — the purgeable query is
self-gating because it reads from `cycle_review_index`. A cycle without a review row
cannot appear in the purgeable set.

---

### `test_gc_raw_signals_flag_and_summary_json_preserved` (AC-05 — R-03)

This is a critical non-negotiable test (Gate 3c blocker).

**Arrange:**
- Insert a reviewed cycle C1 with a `cycle_review_index` row containing:
  - `raw_signals_available = 1`
  - `summary_json = <a non-trivial JSON string, e.g., `{"report":"test-content-xyz"}`>`
- Insert sessions and observations for C1 (to be pruned).
- Insert a retained cycle C2 (within K) with `raw_signals_available = 1`.
- Set K = 1 so C1 is purgeable and C2 is retained.

**Act:** Execute the GC block for C1 (gate check → `gc_cycle_activity` → `store_cycle_review`
with struct update `{ raw_signals_available: 0, ..record }`).

**Assert:**
- `cycle_review_index` row for C1: `raw_signals_available == 0`.
- `cycle_review_index` row for C1: `summary_json` is byte-for-byte identical to the
  pre-GC value `{"report":"test-content-xyz"}` (not empty, not null, not a default).
- `cycle_review_index` row for C2: `raw_signals_available == 1` (unchanged).

**Structural assertion:** The `store_cycle_review()` call must use struct update syntax
`{ raw_signals_available: 0, ..record }` where `record` is the `CycleReviewRecord`
returned by `get_cycle_review()` in the gate check. Code inspection confirms:
- `record` is retained in scope across the `gc_cycle_activity()` call.
- No reconstruction of `CycleReviewRecord` from partial fields between gate and update.

---

### `test_gc_tracing_output` (AC-15 — R-05 warn path)

Uses the `tracing-test` crate (`#[traced_test]` or equivalent) to capture log events.

**Arrange:**
- Insert 3 reviewed cycles: C1 (purgeable), C2 (purgeable with sessions), C3 (retained).
- C1 has a `cycle_review_index` row but sessions and observations.
- Artificially manufacture a gate-skip scenario: create a cycle C_skip that appears
  in a manually-constructed purgeable list but has `get_cycle_review()` return `Ok(None)`.
  (Alternatively: test this via a mock/stub of `get_cycle_review` that returns `Ok(None)`
  for a specific cycle ID.)

**Act:** Call GC block with the manufactured input.

**Assert log events:**
- `tracing::info!` with field `purgeable_count` emitted at start of pass.
- `tracing::info!` with fields `observations_deleted` and `cycle_id` for each pruned cycle.
- `tracing::info!` with field `cycles_pruned = 2` (or similar completion summary) at end.
- `tracing::warn!` with the skipped cycle ID emitted when `get_cycle_review` returns
  `Ok(None)`. Message must contain the cycle ID and a reason string.

---

### `test_gc_max_cycles_per_tick_cap` (AC-16 — R-04, R-08)

This test is a non-negotiable Gate 3c blocker and covers two distinct risks.

**Arrange:**
- Insert 20 reviewed cycles, all purgeable (more than K cycles reviewed, K = 0 or K = 1
  with 21 cycles).
- Each purgeable cycle has sessions and observations.
- Assign `computed_at` values with distinct timestamps so ordering is deterministic.
- `RetentionConfig { max_cycles_per_tick: 5, activity_detail_retention_cycles: 1, .. }`.

**Act — tick 1:** Call GC block once.

**Assert tick 1:**
- Exactly 5 cycles pruned (their sessions and observations gone).
- The 5 pruned cycles are the 5 oldest (lowest `computed_at`).
- 15 cycles still have sessions and observations.
- `list_purgeable_cycles` on the next call would return 15 remaining cycles.

**Act — tick 2:** Call GC block again.

**Assert tick 2:** 5 more cycles pruned (10 total). 10 remain purgeable.

**Act — ticks 3 and 4:** Call GC block twice more.

**Assert after tick 4:** All 20 cycles pruned. `list_purgeable_cycles` returns empty.

**Concurrent write sub-assertion (R-04 structural verification):**
Between tick calls (or as a code inspection), assert that `gc_cycle_activity()` acquires
`pool.begin()` inside the per-cycle loop body, not outside. The write connection must
be released between cycles. Verification approach:
- Code inspection: `pool.begin()` call site is inside the `for cycle in purgeable_cycles`
  loop, not in the enclosing function body.
- Integration-level: spawn an async task that performs a write (e.g., `session_insert`)
  concurrently with a GC run of 5 cycles. Assert the write task completes within 5 seconds
  (no deadlock / timeout).

**Idempotency assertion (R-06):**
After all 20 cycles are pruned (tick 4), call GC block a 5th time.
Assert: `list_purgeable_cycles` returns empty; GC block completes with 0 cycles pruned.
No errors. Second run of `gc_cycle_activity` against an already-pruned cycle is not
possible (the cycle no longer appears as purgeable after sessions are gone and
`raw_signals_available = 0`).

---

## Error Path Expectations

### Gate-failure error handling (R-05 Err path)

When `get_cycle_review()` returns `Err(_)` for a purgeable cycle, the GC block must:
- Emit `tracing::warn!` with the cycle ID and error description.
- Skip that cycle (do not call `gc_cycle_activity`).
- Continue processing remaining purgeable cycles in the same tick.
- NOT abort the entire pass.

Verification: inject an error-returning mock for `get_cycle_review` on one cycle
out of 3, verify the other 2 are still processed.

### `gc_cycle_activity` error propagation

When `gc_cycle_activity()` returns `Err(_)`, the GC block must:
- NOT call `store_cycle_review` with `raw_signals_available: 0`.
- Log the error at `warn` level.
- Continue to the next cycle.

Verification: confirmed by `test_gc_max_cycles_per_tick_cap` idempotency sub-test
and code inspection.

---

## Structural Assertions

- `run_maintenance()` signature includes `retention_config: &RetentionConfig` parameter.
- `run_maintenance()` is called from `background.rs` with `Arc<RetentionConfig>` threaded
  through `run_single_tick()` — same pattern as `Arc<InferenceConfig>`.
- `gc_unattributed_activity()` is called unconditionally after the cycle loop (regardless
  of whether `max_cycles_per_tick` cap was reached).
- `gc_audit_log()` is called after `gc_unattributed_activity()` (step 4f ordering).
- Step 5 (`gc_sessions` time-based) remains unchanged; step 4 and step 5 are independent.
