# Component Test Plan: auto-quarantine-guard

**Source**: `crates/unimatrix-server/src/background.rs` (modified — auto-quarantine section of maintenance_tick)
**Risk coverage**: R-03 (Critical), R-11 (Medium), R-12 (Low), R-13 (Critical)

---

## Unit Test Expectations

All tests in `#[cfg(test)] mod tests` within `background.rs`. Extend the existing test module for background.rs (or create a submodule `mod quarantine_tests`).

### AC-10 / AC-11 — Auto-Quarantine Trigger Condition

**Test**: `test_auto_quarantine_fires_at_threshold`
- Setup: `EffectivenessState` with entry A having `consecutive_bad_cycles = 3`, category = Ineffective
- Setup: `AUTO_QUARANTINE_CYCLES = 3`
- Invoke the auto-quarantine scan logic (extracted helper or inline)
- Assert `store.quarantine_entry(entry_a_id, ...)` is called exactly once
- Assert the reason string contains "3 consecutive" and "Ineffective"

**Test**: `test_auto_quarantine_does_not_fire_below_threshold`
- Entry A: `consecutive_bad_cycles = 2`, category = Ineffective, threshold = 3
- Assert `quarantine_entry` is NOT called

**Test**: `test_auto_quarantine_fires_at_threshold_1`
- Entry A: `consecutive_bad_cycles = 1`, category = Noisy, threshold = 1
- Assert `quarantine_entry` IS called
- Documents AC-11: threshold = 1 means quarantine on first consecutive bad tick

**Test**: `test_auto_quarantine_threshold_env_default_is_3`
- When `UNIMATRIX_AUTO_QUARANTINE_CYCLES` env var is not set: threshold == 3
- Assert default behavior without any env var configuration

**Test**: `test_auto_quarantine_threshold_env_override`
- Set `UNIMATRIX_AUTO_QUARANTINE_CYCLES = "2"` in env
- Assert threshold reads as 2
- Assert quarantine fires at counter = 2, not at counter = 1

### AC-12 — Auto-Quarantine Disabled When Threshold = 0

**Test**: `test_auto_quarantine_disabled_when_threshold_zero`
- Entry A: `consecutive_bad_cycles = 100`, category = Ineffective
- `AUTO_QUARANTINE_CYCLES = 0`
- Assert `quarantine_entry` is NOT called (AC-12)
- This is the disable path guard

**Test**: `test_auto_quarantine_env_zero_disables`
- Set `UNIMATRIX_AUTO_QUARANTINE_CYCLES = "0"` in env
- Assert threshold = 0
- Assert no quarantine fires for any counter value

### AC-12 / Constraint 14 — Env Var Validation at Startup

**Test**: `test_auto_quarantine_cycles_validation_rejects_over_1000`
- Set `UNIMATRIX_AUTO_QUARANTINE_CYCLES = "1001"` in env
- Assert startup configuration parsing returns an error (not a panic, not silent acceptance)
- Assert the error message references the implausible value

**Test**: `test_auto_quarantine_cycles_validation_accepts_boundary_1000`
- Set `UNIMATRIX_AUTO_QUARANTINE_CYCLES = "1000"` in env
- Assert this is the maximum accepted value (boundary: 1000 is accepted, 1001 is not)

**Test**: `test_auto_quarantine_cycles_validation_rejects_non_integer`
- Set `UNIMATRIX_AUTO_QUARANTINE_CYCLES = "abc"` in env
- Assert startup configuration parsing returns an error

### AC-14 / R-11 — Category Restriction: Only Ineffective and Noisy

**Test**: `test_auto_quarantine_does_not_fire_for_settled`
- Entry A: `consecutive_bad_cycles = 10`, category = Settled, threshold = 3
- Assert `quarantine_entry` is NOT called (AC-14)

**Test**: `test_auto_quarantine_does_not_fire_for_unmatched`
- Entry A: `consecutive_bad_cycles = 10`, category = Unmatched, threshold = 3
- Assert `quarantine_entry` is NOT called (AC-14)

**Test**: `test_auto_quarantine_does_not_fire_for_effective`
- Entry A: `consecutive_bad_cycles = 10`, category = Effective, threshold = 3
- Assert `quarantine_entry` is NOT called

### AC-15 — Already-Quarantined Entry Not Incremented

**Test**: `test_already_quarantined_entry_absent_from_tick_report`
- Entry A was quarantined between ticks; it no longer appears in `load_entry_classification_meta` output
- Simulate tick with report that does NOT include entry A
- Assert entry A's key is removed from `consecutive_bad_cycles`
- Assert `quarantine_entry` is NOT called for entry A (already quarantined)

### R-03 — Bulk Quarantine Loop Isolation (per-entry error isolation)

**Test**: `test_bulk_quarantine_continues_on_single_entry_error`
- Setup: entries A, B, C all at threshold (consecutive_bad_cycles = 3, category = Ineffective)
- Mock: `quarantine_entry(A)` succeeds, `quarantine_entry(B)` returns error ("already quarantined"), `quarantine_entry(C)` succeeds
- Assert: entries A and C are quarantined; B's failure does not abort the loop
- Assert: audit events emitted for A and C; warning logged for B

**Test**: `test_bulk_quarantine_counter_reset_only_on_success`
- Entry A: quarantine succeeds → counter reset to 0
- Entry B: quarantine fails → counter is NOT reset (remains at threshold value)
- This is the ordering invariant: reset counter only after confirmed success

**Test**: `test_bulk_quarantine_five_entries_all_succeed`
- Seed 5 entries all at threshold, all Ineffective
- Trigger auto-quarantine scan
- Assert all 5 `quarantine_entry` calls are made
- Assert all 5 counters reset to 0
- Assert 5 audit events emitted
- Validates the R-03 scenario: bulk quarantine does not short-circuit after first entry

### NFR-02 / R-13 — Write Lock Released Before Quarantine Loop

**Test**: `test_write_lock_released_before_quarantine_scan`
- Verify by code structure that the quarantine scan executes after the write guard scope ends
- Practical test: construct mock where the `quarantine_entry` callback asserts the `EffectivenessState` read lock can be acquired (if the write lock were still held, this would deadlock or return a would-block error)
- Use `try_read()` on the handle inside the mock to assert non-blocking acquisition

### R-12 — auto_quarantined_this_cycle Populated

**Test**: `test_auto_quarantined_this_cycle_populated_on_quarantine`
- Trigger quarantine for entry A
- Assert `EffectivenessReport.auto_quarantined_this_cycle` contains entry A's ID
- This verifies FR-14: the field is populated during the same tick that triggers quarantine

**Test**: `test_auto_quarantined_this_cycle_empty_when_no_quarantine`
- Tick fires, no entries at threshold
- Assert `auto_quarantined_this_cycle` is empty Vec

### Counter Reset After Quarantine

**Test**: `test_consecutive_bad_cycles_reset_after_quarantine`
- Entry A: counter = 3, threshold = 3 → quarantine fires
- After quarantine: assert `consecutive_bad_cycles[entry_a_id] == 0`
- On next tick: entry A absent from report → key removed from map

---

## Integration Test Expectations

The AC-17 item 3 integration test covers end-to-end auto-quarantine through the MCP interface. As noted in OVERVIEW.md, this test may need to be filed as a known gap if the background tick interval cannot be shortened for tests.

**Minimum integration coverage**: `test_store_quarantine_restore_search_finds` in `test_lifecycle.py` (existing L-08) confirms the manual quarantine/restore path works. The auto-quarantine path is additive: it invokes the same `quarantine_entry` store method. The unit tests above confirm the trigger logic; the existing integration test confirms the store method works correctly.

**AC-13 integration test** (audit event verification): after auto-quarantine fires, `context_status` response must include audit entries with `operation = "auto_quarantine"`. If the harness cannot drive background ticks, this assertion is deferred to the audit component test plan (auto-quarantine-audit.md) using a unit-level audit capture.

---

## Edge Cases

| Scenario | Expected | Test Type |
|----------|----------|-----------|
| `AUTO_QUARANTINE_CYCLES = 1` | Quarantine on first bad tick, no persistence required | Unit (AC-11 boundary) |
| Server restart mid-accumulation | Counter resets to 0; no retroactive quarantine | Integration (restart test) |
| Entry deprecated between tick write and quarantine scan | `quarantine_entry` on Deprecated entry → log error, skip; entry stays Deprecated | Unit (R-03 error isolation) |
| All 5,000 entries simultaneously at threshold | Bulk loop completes within spawn_blocking budget; no OOM | Load/benchmark (out of scope for Stage 3c unless CI time allows) |
