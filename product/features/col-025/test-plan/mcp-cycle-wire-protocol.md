# Test Plan: mcp-cycle-wire-protocol

**Crate**: `unimatrix-server`
**File modified**: `src/mcp/tools.rs` (struct `CycleParams`, `context_cycle` handler)

**Risks covered**: R-06 (CycleParams compile check)
**ACs covered**: AC-13a, AC-17

---

## Overview

Component 4 extends the MCP wire protocol:

1. Add `goal: Option<String>` to `CycleParams`.
2. On `CycleType::Start`: trim + empty/whitespace normalization to `None`, then
   byte check vs `MAX_GOAL_BYTES`. Reject oversized goals with a structured error.
3. Emit validated goal in the `ImplantEvent` payload for the UDS listener.
4. On `CycleType::PhaseEnd` and `CycleType::Stop`: goal param is silently ignored.

The validation logic (steps 2–3) lives in the `context_cycle` handler.

---

## `CycleParams` Deserialization Tests

Extend the existing `CycleParams` deserialization test section in `mcp/tools.rs`
(`#[cfg(test)] mod tests`). Current tests: ~10 round-trip tests starting at line ~2720.

### Test: `test_cycle_params_goal_field_present`

```
#[test] fn test_cycle_params_goal_field_present()
```

Deserialize `{"type": "start", "topic": "col-025", "goal": "Test the goal field."}`.
Assert `params.goal == Some("Test the goal field.")`.

### Test: `test_cycle_params_goal_field_absent`

```
#[test] fn test_cycle_params_goal_field_absent()
```

Deserialize `{"type": "start", "topic": "col-025"}` (no `goal` key).
Assert `params.goal == None`.
Backward compatibility: old clients omitting `goal` receive `None`.

### Test: `test_cycle_params_goal_null`

```
#[test] fn test_cycle_params_goal_null()
```

Deserialize `{"type": "start", "topic": "col-025", "goal": null}`.
Assert `params.goal == None`.

---

## Validation Tests (in handler or extracted helper)

### Test: `test_cycle_start_goal_exceeds_max_bytes_rejected` (AC-13a)

```
#[test] fn test_cycle_start_goal_exceeds_max_bytes_rejected()
```

Build a goal string of exactly `MAX_GOAL_BYTES + 1` bytes of ASCII `'a'`.
Assert the validation function (or inline check) returns an error with a
descriptive message that contains "MAX_GOAL_BYTES" or "goal" and "1024".
Assert the error is a `CallToolResult::error` (not a panic).

### Test: `test_cycle_start_goal_at_exact_max_bytes_accepted` (AC-13a / R-07)

```
#[test] fn test_cycle_start_goal_at_exact_max_bytes_accepted()
```

Build a goal string of exactly `MAX_GOAL_BYTES` bytes of ASCII.
Assert the validation passes (no error returned).
This is the boundary test: 1024 bytes accepted, 1025 bytes rejected.

### Test: `test_cycle_start_empty_goal_normalized_to_none` (AC-17)

```
#[test] fn test_cycle_start_empty_goal_normalized_to_none()
```

Pass `goal = ""` (empty string) through the MCP validation pass.
Assert the normalized result is `None`.
Assert no byte check error is returned (empty is normalized before the check).

### Test: `test_cycle_start_whitespace_only_goal_normalized_to_none` (AC-17)

```
#[test] fn test_cycle_start_whitespace_only_goal_normalized_to_none()
```

Pass `goal = "   "` (three spaces) through the MCP validation pass.
Assert the normalized result is `None`.

### Test: `test_cycle_start_whitespace_trimmed_goal_within_limit_accepted` (AC-17)

```
#[test] fn test_cycle_start_whitespace_trimmed_goal_within_limit_accepted()
```

Pass `goal = "  a short goal  "` (leading and trailing whitespace).
Assert the normalized result is `Some("a short goal")` — trimmed, non-empty,
within byte limit. No error.

### Test: `test_cycle_phase_end_with_goal_ignores_goal` (FR-01)

```
#[test] fn test_cycle_phase_end_with_goal_ignores_goal()
```

Deserialize a `phase-end` event with `goal` field present.
Assert the handler does not write `goal` to any row via `insert_cycle_event`
on phase-end events (pass `None` for goal to insert). Behavior: goal param is
extracted from the payload but not used for non-start events.

### Test: `test_cycle_stop_with_goal_ignores_goal` (FR-01)

```
#[test] fn test_cycle_stop_with_goal_ignores_goal()
```

Same pattern for `stop` event type.

---

## Integration: MCP → ImplantEvent → UDS round-trip (R-08)

This is covered at the infra-001 integration level, not by unit tests alone.
The integration test `test_cycle_start_with_goal_persists_across_restart` in
`test_lifecycle.py` validates the full MCP → UDS → DB round-trip.

However, a unit test should verify that the handler emits the goal in the
`ImplantEvent` payload:

### Test: `test_cycle_start_emits_goal_in_implant_event`

```
#[test] fn test_cycle_start_emits_goal_in_implant_event()
```

Build the `ImplantEvent` payload from a `context_cycle(start)` call with
`goal = "cycle goal"`. Assert the serialized `ImplantEvent` JSON contains a
`goal` field equal to `"cycle goal"`. This confirms the MCP→UDS pathway
carries the goal across the wire (catching a serialization mismatch that would
cause the UDS listener to receive `None` silently).

---

## Backward Compatibility

All existing `CycleParams` deserialization tests must pass without modification.
No existing test may be changed to accommodate the new `goal` field — the field
is optional with `None` as the missing-field default.

After adding `goal: Option<String>` to `CycleParams`, run:

```bash
cargo test -p unimatrix-server 2>&1 | tail -30
```

All pre-existing `CycleParams` tests must pass.
