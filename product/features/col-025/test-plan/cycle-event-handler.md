# Test Plan: cycle-event-handler

**Crate**: `unimatrix-server`
**File modified**: `src/uds/listener.rs` (fn `handle_cycle_event`)

**Risks covered**: R-07, R-08, R-13
**ACs covered**: AC-01, AC-02, AC-13b

---

## Overview

Component 3 handles the UDS `CYCLE_START_EVENT` path:

1. Extract `goal` from the `ImplantEvent` payload.
2. Apply the UDS byte guard: if `goal.len() > MAX_GOAL_BYTES`, truncate at the
   nearest valid UTF-8 character boundary and emit `tracing::warn!`.
3. Set `state.current_goal` synchronously via `session_registry.set_current_goal`.
4. Pass `goal` to the fire-and-forget `insert_cycle_event` spawn.

`CYCLE_PHASE_END` and `CYCLE_STOP` arms do not touch `current_goal`.

---

## Unit Tests

Tests live in `src/uds/listener.rs` (`#[cfg(test)] mod tests`) or in a
dedicated test module. Tests for the UDS byte guard can be extracted as pure
unit tests on the truncation logic.

### Test: `test_uds_goal_truncation_at_utf8_char_boundary` (R-07 / AC-13b / Gate 3c scenario 5)

```
#[test] fn test_uds_goal_truncation_at_utf8_char_boundary()
```

This is the **non-negotiable Gate 3c scenario 5**.

Construct a goal string where a 3-byte CJK character (e.g., U+4E00 = `一`,
encoded as `E4 B8 80`) straddles the `MAX_GOAL_BYTES` boundary. For example:

- Fill first `MAX_GOAL_BYTES - 2` bytes with ASCII `'a'`.
- Append a 3-byte CJK character (occupies positions `[MAX_GOAL_BYTES-2 .. MAX_GOAL_BYTES+1]`).
- Total: `MAX_GOAL_BYTES + 1` bytes; but a naive slice at `MAX_GOAL_BYTES` would
  cut the 3-byte character in the middle — an invalid UTF-8 boundary.

Call the truncation helper (extracted function or closure inside `handle_cycle_event`).
Assert:
- Result is valid UTF-8.
- Result byte length ≤ `MAX_GOAL_BYTES`.
- Result byte length = `MAX_GOAL_BYTES - 2` (the CJK character is dropped entirely).
- No panic.

### Test: `test_uds_goal_exact_max_bytes_stored_verbatim` (R-07 / AC-13b)

```
#[test] fn test_uds_goal_exact_max_bytes_stored_verbatim()
```

Construct a goal string of exactly `MAX_GOAL_BYTES` bytes of valid ASCII.
Apply the truncation logic.
Assert:
- Result equals the input verbatim (no truncation).
- No `tracing::warn!` should be emitted (boundary is not exceeded).
- Byte length = `MAX_GOAL_BYTES`.

### Test: `test_uds_goal_over_max_bytes_ascii_truncated` (AC-13b)

```
#[test] fn test_uds_goal_over_max_bytes_ascii_truncated()
```

Construct a goal string of `MAX_GOAL_BYTES + 100` bytes of ASCII `'x'`.
Apply the truncation logic.
Assert:
- Result byte length = `MAX_GOAL_BYTES`.
- Result is exactly the first `MAX_GOAL_BYTES` bytes of input.
- A `tracing::warn!` with the truncation message was emitted (use `tracing_test`
  or assert on the returned truncated value and verify warn is triggered via log
  subscriber).

### Test: `test_uds_goal_within_limit_unchanged` (AC-13b)

```
#[test] fn test_uds_goal_within_limit_unchanged()
```

Construct a goal string of `MAX_GOAL_BYTES / 2` bytes of ASCII.
Apply the truncation logic.
Assert: result equals input; no truncation; no warn.

### Test: `test_uds_cycle_start_sets_current_goal_in_registry` (AC-01)

```
#[tokio::test] async fn test_uds_cycle_start_sets_current_goal_in_registry()
```

Arrange: `SessionRegistry` with a registered session; a mock/stub for the store.
Act: call `handle_cycle_event` (or the relevant dispatch arm) with a
`CYCLE_START_EVENT` payload containing `goal = "feature goal text"`.
Assert: `session_registry.get_state(session_id)?.current_goal == Some("feature goal text")`.

### Test: `test_uds_cycle_start_no_goal_sets_none` (AC-02)

```
#[tokio::test] async fn test_uds_cycle_start_no_goal_sets_none()
```

Same setup, payload without `goal` field.
Assert: `current_goal == None`.

### Test: `test_uds_cycle_phase_end_does_not_modify_current_goal` (FR-01)

```
#[tokio::test] async fn test_uds_cycle_phase_end_does_not_modify_current_goal()
```

Register session with `current_goal = Some("existing goal")`.
Dispatch a `CYCLE_PHASE_END` event.
Assert: `current_goal` still equals `Some("existing goal")` — unchanged.

### Test: `test_uds_cycle_stop_does_not_modify_current_goal` (FR-01)

```
#[tokio::test] async fn test_uds_cycle_stop_does_not_modify_current_goal()
```

Same pattern as above with `CYCLE_STOP` event.

---

## Integration Scenario (R-13 / Gate 3c scenario 9)

### Test: `test_uds_truncate_then_overwrite_last_writer_wins`

```
#[tokio::test] async fn test_uds_truncate_then_overwrite_last_writer_wins()
```

This is the **non-negotiable Gate 3c scenario 9**.

Sequence:
1. Dispatch a UDS `CYCLE_START_EVENT` with an oversized goal (> `MAX_GOAL_BYTES`).
   Assert that `get_cycle_start_goal(cycle_id)` returns the truncated value.
2. Dispatch a second UDS `CYCLE_START_EVENT` for the same `cycle_id` with a
   correct-length goal.
   Assert that `get_cycle_start_goal(cycle_id)` returns the corrected (second) goal.
   Assert that the corrected value is NOT the truncated first value.
3. Assert `session_registry.get_state(session_id)?.current_goal` equals the
   corrected goal (in-memory consistency with DB).

This test validates the "last-writer-wins" semantics required by ADR-005. It
requires a live v16 `SqlxStore` and a real `SessionRegistry`.

Prerequisite: verify at code review that `insert_cycle_event` uses semantics that
allow overwriting an existing `cycle_start` row's `goal` column (not INSERT OR IGNORE).

---

## Edge Cases

| Edge Case | Test Name |
|-----------|-----------|
| 2-byte UTF-8 character (e.g., U+00C0 = `À`) at boundary | `test_uds_goal_two_byte_char_at_boundary` — same pattern as CJK test |
| Empty goal string on UDS path (blank string, no whitespace normalization) | `test_uds_goal_empty_string_stored_verbatim` — UDS does not normalize whitespace; empty string stored as empty |
| `goal` field absent from payload entirely | Same as `test_uds_cycle_start_no_goal_sets_none` |
