# Test Plan: Hook Path (Component 3)

File: `crates/unimatrix-server/src/uds/hook.rs`
Risks: R-09, AC-16

---

## Unit Test Expectations

All inline `#[cfg(test)]` functions. Focus: hook event parsing, `phase-end` handling, and
fallthrough on validation failure.

### Hook `phase-end` Handling (AC-16, FR-03.7, R-09)

**`test_hook_phase_end_valid_phase_emits_cycle_phase_end`** (AC-16 happy path)
- Arrange: simulate a hook event with `tool_input` containing
  `{"type": "phase-end", "topic": "crt-025", "phase": "scope", "next_phase": "design"}`
- Act: call the hook parsing/dispatch function
- Assert: emitted event has `event_type = "cycle_phase_end"`
- Assert: `phase = "scope"`, `next_phase = "design"` in the emitted payload
- Assert: no error returned to transport

**`test_hook_phase_end_invalid_phase_space_falls_through`** (AC-16 error path, R-09)
- Arrange: hook event with `phase = "scope review"` (contains space — invalid)
- Act: call the hook parsing function
- Assert: returns `Ok(...)` (falls through — no error to transport)
- Assert: emitted action is the generic observation path, not `cycle_phase_end`
- Note: warning logging is not directly assertable in a unit test; the key assertion is
  that no `Err(...)` is returned that would propagate to the transport

**`test_hook_phase_end_empty_phase_falls_through`** (R-09)
- Arrange: hook event with `phase = ""`
- Act: call hook parsing
- Assert: `Ok(...)` — falls through, no transport error

**`test_hook_phase_end_no_phase_field_accepted`** (R-09 edge — phase is optional)
- Arrange: hook event with `type = "phase-end"` but no `phase` key in `tool_input`
- Assert: succeeds; `phase = None` is valid per FR-02.5

**`test_hook_phase_end_phase_normalized`** (R-06, shared validation)
- Arrange: hook event with `phase = "Scope"` (mixed case)
- Assert: emitted payload carries `phase = "scope"` (lowercase normalized)

**`test_hook_start_type_extracted`** (regression — existing behavior preserved)
- Arrange: hook event with `type = "start"`, `topic = "t"`, `next_phase = "scope"`
- Assert: `event_type` maps to `"cycle_start"`, `next_phase` carried through

**`test_hook_stop_type_extracted`** (regression)
- Arrange: hook event with `type = "stop"`, `topic = "t"`
- Assert: `event_type = "cycle_stop"`

**`test_hook_keywords_not_extracted`** (regression — keywords removal, FR-03.5)
- Arrange: hook event with `keywords = ["k1", "k2"]`
- Assert: no `keywords` field in the emitted hook request payload
- Assert: no error from unknown `keywords` field in input

---

## Integration Test Expectations

The hook path is exercised indirectly through the UDS listener and the full server stack.
There is no separate integration test file for `hook.rs`; correctness is verified through:

1. The server-level integration tests for R-01 (which require the UDS path to fire)
2. The infra-001 `lifecycle` suite's phase-tag lifecycle flow test

The fallthrough behavior (R-09 AC-16) cannot be verified through the MCP protocol because
it only manifests when an invalid phase arrives via the hook (pre-tool-use) path, not via
direct MCP tool call. Unit tests are the primary coverage mechanism.

---

## Assertions

- No `HookError` or similar error type returned from the hook path for `phase-end` validation failures (FR-03.7)
- `CYCLE_PHASE_END_EVENT` constant used in event emission (not a hard-coded string)
- `keywords` extraction code is absent from the updated hook handler
