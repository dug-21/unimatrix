# Component Test Plan: background-tick-writer

**Source**: `crates/unimatrix-server/src/background.rs` (modified)
**Risk coverage**: R-04 (High), R-08 (Medium), R-13 (Critical)

---

## Unit Test Expectations

All tests in `#[cfg(test)] mod tests` within `background.rs`, or in a dedicated test module `background_tests.rs` if the existing background.rs test module is too large. Extend the existing structure.

### FR-03 / AC-01 — EffectivenessState Written Only by Background Tick

The structural enforcement here is at the type system level (StatusService does not hold a write-capable handle). The unit test confirms the data flow:

**Test**: `test_tick_write_updates_categories_from_report`
- Construct `EffectivenessStateHandle` with known initial state (empty)
- Construct a mock `EffectivenessReport` with 3 entries: one Effective, one Ineffective, one Settled
- Invoke the tick write logic (extracted helper function or inline simulation)
- After write: acquire read lock, assert `categories[id_effective] == EffectivenessCategory::Effective`
- Assert `categories[id_ineffective] == EffectivenessCategory::Ineffective`
- Assert `categories[id_settled] == EffectivenessCategory::Settled`

**Test**: `test_tick_write_increments_generation`
- Before write: `generation == 0`
- After one tick write: `generation == 1`
- After second tick write: `generation == 2`

### FR-09 / AC-09 — Consecutive Bad Cycle Counter Semantics

These are the most important state-machine tests for this component.

**Test**: `test_consecutive_bad_cycles_increment_for_ineffective`
- Entry A starts with counter = 0
- Tick 1: category = Ineffective → assert counter = 1
- Tick 2: category = Ineffective → assert counter = 2

**Test**: `test_consecutive_bad_cycles_increment_for_noisy`
- Entry A starts with counter = 0
- Tick 1: category = Noisy → assert counter = 1

**Test**: `test_consecutive_bad_cycles_reset_on_recovery`
- Entry A: counter = 2 (was Ineffective for 2 ticks)
- Tick 3: category = Effective → assert counter = 0 (removed or set to 0)

**Test**: `test_consecutive_bad_cycles_reset_on_settled`
- Entry A: counter = 3
- Tick: category = Settled → assert counter = 0

**Test**: `test_consecutive_bad_cycles_reset_on_unmatched`
- Entry A: counter = 1
- Tick: category = Unmatched → assert counter = 0

**Test**: `test_consecutive_bad_cycles_remove_absent_entry`
- Entry A has counter = 2 in `consecutive_bad_cycles`
- Next tick report does NOT include entry A (it was quarantined externally)
- After tick write: assert entry A's counter key is removed from `consecutive_bad_cycles`

**Test**: `test_consecutive_bad_cycles_three_tick_sequence_no_quarantine`
- Simulates the edge case: tick 1=Ineffective (counter=1), tick 2=Effective (counter=0), tick 3=Ineffective (counter=1)
- Assert counter = 1 after tick 3, not 3 (reset on recovery resets cleanly)
- This is the critical guard: the three-tick sequence with an interruption must NOT trigger auto-quarantine at threshold=2

### ADR-002 / R-08 — Hold-on-Error Semantics (tick_skipped event)

**Test**: `test_tick_error_holds_consecutive_counters`
- Entry A: counter = 2
- Simulate `compute_report()` error
- Assert entry A's counter remains 2 (not incremented, not reset)
- Assert `EffectivenessState.categories` is unchanged from pre-error state

**Test**: `test_tick_error_does_not_modify_generation`
- `generation == 5` before error
- Simulate `compute_report()` error
- Assert `generation == 5` after (no generation increment on failed tick)

**Test**: `test_tick_skipped_audit_event_emitted`
- Configure a mock audit writer or capture audit events
- Simulate `compute_report()` error with message "db locked"
- Assert one audit event was emitted with:
  - `operation == "tick_skipped"`
  - `agent_id == "system"`
  - `reason` contains "db locked"
  - `outcome == Failure`

### NFR-02 / R-13 — Write Lock Released Before SQL Write

**Test**: `test_write_lock_released_before_quarantine_call` (structural)
- This is a code-structure test: assert that the write guard on `EffectivenessStateHandle` is created in a scoped block that ends before any call to `store.quarantine_entry()`
- Implementer must structure the code with an explicit inner `{ ... }` block or `drop(write_guard)` before the quarantine loop
- The test verifies the invariant by constructing a mock that asserts the lock is NOT held when `quarantine_entry` is called
- Practical approach: use a `AtomicBool` in test to track whether write lock was dropped; mock `quarantine_entry` checks the bool

**Test**: `test_search_not_blocked_during_auto_quarantine` (#[tokio::test])
- Spawn two tasks: one simulates the tick write + auto-quarantine (holds write lock briefly, then calls mock quarantine), one simulates a concurrent `search()` read-lock acquisition
- Assert the search read-lock acquisition completes within 10ms of being attempted even while mock quarantine is "running" (simulated delay)
- This is the concurrency test for R-13

---

## Integration Test Expectations

Integration testing for the write path is primarily via the AC-17 scenario in test_lifecycle.py. See OVERVIEW.md §New Integration Tests Required.

**Specific integration assertion**: After a background tick fires (observable via non-empty `effectiveness` section in `context_status` response), call `context_search` and verify the response completes without timeout or error. This confirms the write lock is released and search is not blocked.

---

## Edge Cases

| Scenario | Expected | Test Type |
|----------|----------|-----------|
| Empty `EffectivenessReport` (no entries) | `categories` becomes empty; `consecutive_bad_cycles` entries all removed | Unit |
| `compute_report()` returns `Ok` with empty report | See Integration Risk #3 in RISK-TEST-STRATEGY.md — should log warning, not silently clear state; verify test distinguishes Ok(empty) from Err | Unit |
| Multiple consecutive errors | Counter held at pre-error value for N failed ticks | Unit (extend tick_error_holds test with 3 errors) |
| Tick write with no entries that need quarantine | Auto-quarantine loop is a no-op; no audit events emitted | Unit |
