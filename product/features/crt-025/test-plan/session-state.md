# Test Plan: SessionState (Component 4)

File: `crates/unimatrix-server/src/infra/session.rs`
Risks: R-01 (Critical — partial), AC-06, FR-05

---

## Unit Test Expectations

All inline `#[cfg(test)]` functions. Focus: `current_phase` field initialization, mutation rules
per event type, and `set_current_phase` method behavior.

### Initialization (FR-05.1)

**`test_session_state_current_phase_initialized_to_none`**
- Arrange: register a new session via `SessionRegistry::register_session`
- Act: read `session_state.current_phase` (via `get_session_state` or equivalent accessor)
- Assert: `current_phase == None`

### `set_current_phase` Method (FR-05.2, FR-05.3, FR-05.4)

**`test_set_current_phase_some_value`**
- Arrange: session registered with `current_phase = None`
- Act: `registry.set_current_phase(session_id, Some("scope".to_string()))`
- Assert: `current_phase == Some("scope")`

**`test_set_current_phase_none_clears_value`** (FR-05.4, stop event)
- Arrange: session has `current_phase = Some("implementation")`
- Act: `registry.set_current_phase(session_id, None)`
- Assert: `current_phase == None`

**`test_set_current_phase_overwrites_existing`**
- Arrange: `current_phase = Some("scope")`
- Act: `set_current_phase(session_id, Some("design".to_string()))`
- Assert: `current_phase == Some("design")`

**`test_set_current_phase_unknown_session_no_panic`** (Failure Mode from Risk Strategy)
- Arrange: call `set_current_phase` with a session_id that was never registered
- Assert: does not panic; returns `()` (no-op or logged error — must not crash)

### Phase Transition Rules (AC-06, FR-05.2, FR-05.3)

**`test_phase_end_with_next_phase_updates_current_phase`** (AC-06 happy path)
- Arrange: `current_phase = Some("scope")`
- Simulate: `phase-end` event with `next_phase = Some("design")`
- Act: `set_current_phase(session_id, Some("design".to_string()))`
- Assert: `current_phase == Some("design")`

**`test_phase_end_without_next_phase_leaves_current_phase_unchanged`** (AC-06 edge)
- Arrange: `current_phase = Some("scope")`
- Simulate: `phase-end` event with `next_phase = None`
- Act: listener does NOT call `set_current_phase` (per phase transition logic)
- Assert: `current_phase` remains `Some("scope")`

**`test_start_with_next_phase_sets_current_phase`** (FR-05.2)
- Arrange: fresh session, `current_phase = None`
- Simulate: `start` event with `next_phase = Some("scope")`
- Act: `set_current_phase(session_id, Some("scope".to_string()))`
- Assert: `current_phase == Some("scope")`

**`test_start_without_next_phase_leaves_current_phase_none`** (FR-05.2 edge)
- Arrange: fresh session, `current_phase = None`
- Simulate: `start` event without `next_phase`
- Act: listener does NOT call `set_current_phase`
- Assert: `current_phase` remains `None`

**`test_stop_event_clears_current_phase`** (FR-05.4)
- Arrange: `current_phase = Some("testing")`
- Simulate: `stop` event
- Act: `set_current_phase(session_id, None)`
- Assert: `current_phase == None`

### R-01 Causal Guarantee (NFR-02)

R-01 cannot be fully verified by unit tests alone, but the following unit test documents the
behavioral contract:

**`test_set_current_phase_is_synchronous_within_session_lock`**
- Arrange: two threads; thread A holds session lock and calls `set_current_phase`
- Act: thread B attempts to read `current_phase` while A holds the lock
- Assert: thread B sees either the old value (before lock) or the new value (after lock) —
  never an intermediate state
- This is a documentation test — the `Mutex<HashMap>` pattern ensures atomicity

The actual causal test (R-01 must verify that `context_store` called after `phase-end` reads
the updated phase) is covered in the server-level integration tests in `mcp-tool-handler.md`
and `uds-listener.md`.

---

## Integration Test Expectations

`SessionState` internals are not observable through the MCP protocol directly. The state is
verified indirectly through `context_store` phase tagging results in the following tests:

- `test_context_cycle_start_with_next_phase_then_store` (in server-level integration)
- `test_context_cycle_stop_then_store_phase_null` (in server-level integration)
- infra-001: `test_phase_tag_store_cycle_review_flow` (end-to-end)

---

## Assertions

- `current_phase` field exists on `SessionState` struct (compile-time)
- `set_current_phase(session_id, phase)` is a public method on `SessionRegistry`
- `set_current_phase` with an unrecognized `session_id` does not panic
- `current_phase` is initialized to `None` on session registration — not `Some("")`
