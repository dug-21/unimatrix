# Test Plan: background_tick_insertion

## Component

**File modified:** `crates/unimatrix-server/src/background.rs`

**Changes:**
- Add `PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5` constant
- Add call to `run_co_access_promotion_tick(store, inference_config, current_tick).await`
  between orphaned-edge compaction and `TypedGraphState::rebuild()`
- Add ORDERING INVARIANT anchor comment block (ADR-005)
- Update `run_single_tick` call site to pass `current_tick`

**Risks covered:** R-05 (tick ordering violation), R-07 (config field used correctly)

---

## Primary Verification Method: Code Review (AC-05)

AC-05 is verified by static code review, not a unit test. The test plan describes what the
reviewer must confirm, and what structural evidence Stage 3c must capture.

### Code Review Checklist for `background.rs`

1. **Anchor comment present** — The ORDERING INVARIANT block must appear immediately before
   the `run_co_access_promotion_tick` call. Expected form:

   ```rust
   // ORDERING INVARIANT: co_access promotion MUST run after orphaned-edge compaction
   // and BEFORE TypedGraphState::rebuild(). Inserting steps between this block and
   // rebuild() will cause newly promoted edges to be invisible to PPR for one tick.
   // See ADR-005 (crt-034) and SR-06.
   ```

2. **Call site position** — The call appears:
   - AFTER: orphaned-edge compaction (`DELETE FROM graph_edges WHERE ...` for dead entries)
   - BEFORE: `TypedGraphState::rebuild()` (or equivalent)
   - NOT after any `rebuild()` call

3. **Function signature** — Call passes `current_tick: u32`. The existing `run_single_tick`
   must have `current_tick` threaded through to the promotion call.

4. **No `nli_enabled` guard** — The call is unconditional; there is no `if inference_config.nli_enabled` or
   similar gate. FR-07 requires the tick to run unconditionally.

5. **`PROMOTION_EARLY_RUN_WARN_TICKS` constant** — Defined in `background.rs`, not in
   `co_access_promotion_tick.rs` (function receives `current_tick`, the constant lives at
   the call site).

6. **`mod.rs` registration** — `services/mod.rs` contains
   `pub(crate) mod co_access_promotion_tick;`

---

## Unit Test Expectations

### Test: `test_promotion_early_run_warn_ticks_constant_value`

**Covers:** ADR-005 constant compliance

**Arrange:**
- Reference `PROMOTION_EARLY_RUN_WARN_TICKS` constant from `background.rs`

**Act:**
- Access the constant value

**Assert:**
- `assert_eq!(PROMOTION_EARLY_RUN_WARN_TICKS, 5u32)`

**Location:** `crates/unimatrix-server/src/background.rs` `#[cfg(test)]` block (or in
`co_access_promotion_tick.rs` if the constant is imported there).

**Note:** The constant may be `pub(crate)` to allow testing from the tick module. If it
is file-private to `background.rs`, the test lives in `background.rs`.

---

### Test: `test_services_mod_registers_co_access_promotion_tick`

**Covers:** Module registration (structural)

**Verification method:** This is a compile-time test — if `co_access_promotion_tick` is not
registered in `services/mod.rs`, the project does not compile. No explicit test function
is needed; cargo build success is the assertion.

---

## Integration Test Expectations

### Existing Lifecycle Suite

Run `pytest suites/test_lifecycle.py` after Stage 3b to confirm the background tick loop
still operates without crash after the new tick step is inserted.

Specifically, the `test_tick_liveness` test (under `availability` marker in the lifecycle
suite) validates that a tick fires and the server remains responsive. This is the closest
existing test to an ordering integration test for R-05.

### Optional New Integration Test

See OVERVIEW.md — a new `test_co_access_promotion_tick_no_crash_after_tick` in
`suites/test_lifecycle.py` is optional at Stage 3c. Its purpose is to confirm no crash
when the tick fires after co-access state has been accumulated. It does not assert PPR
graph content (not inspectable via MCP).

If added, use the `server` fixture (fresh DB), run a few MCP calls to accumulate co-access,
then call `context_status` (maintain=false) to confirm liveness. Do not use `availability`
marker — keep it in the regular lifecycle suite to avoid requiring the 20-minute run.

---

## AC-05 Verification Evidence Required at Gate 3c

The Stage 3c tester must capture the following as evidence in RISK-COVERAGE-REPORT.md:

1. The actual lines from `background.rs` showing the anchor comment and call site
2. Confirmation that `TypedGraphState::rebuild()` appears AFTER the promotion call

This is the only AC verified by code review rather than test output.

---

## Acceptance Criteria Mapped

| AC-ID | Verification Method | Expected Result |
|-------|--------------------|--------------------|
| AC-05 | Code review of `background.rs` | Promotion call between compaction and rebuild, with ORDERING INVARIANT anchor comment |
| AC-05 (constant) | `test_promotion_early_run_warn_ticks_constant_value` | `PROMOTION_EARLY_RUN_WARN_TICKS == 5` |
| AC-05 (unconditional) | Code review | No `nli_enabled` guard wrapping the call |
| Module registration | Compile success | `services/mod.rs` includes `pub(crate) mod co_access_promotion_tick;` |
