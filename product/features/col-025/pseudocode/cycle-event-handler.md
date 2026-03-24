# Component: cycle-event-handler

**Crate**: `unimatrix-server`
**File**: `src/uds/listener.rs` (fn `handle_cycle_event`)

---

## Purpose

Extract `goal` from `CYCLE_START_EVENT` payloads, apply the UDS byte guard
(truncation at UTF-8 char boundary), set `current_goal` synchronously on
`SessionState`, and pass the goal into the fire-and-forget `insert_cycle_event`
spawn. `PhaseEnd` and `Stop` events do not read or modify `current_goal`.

---

## Modified Function: `handle_cycle_event`

Current signature (unchanged):
```
fn handle_cycle_event(
    event: &unimatrix_engine::wire::ImplantEvent,
    lifecycle: CycleLifecycle,
    session_registry: &SessionRegistry,
    store: &Arc<Store>,
)
```

### Changes within the SYNCHRONOUS SECTION

After the existing Step 3 (`set_current_phase` / `set_current_phase(None)`)
and before the `=== END OF SYNCHRONOUS SECTION ===` comment, insert Step 3b:

```
// Step 3b: Extract goal and set current_goal (col-025, Start only).
// Placed in the synchronous section to guarantee visibility before
// any spawn; mirrors the set_current_phase placement invariant.
if lifecycle == CycleLifecycle::Start {
    let raw_goal: Option<String> = event
        .payload
        .get("goal")
        .and_then(|v| v.as_str())
        // UDS path: no whitespace or empty-string normalization.
        // Whatever arrives is used (after truncation). Empty string stored verbatim.
        // (MCP path normalizes empty strings to None at the handler; UDS does not.)
        .map(|s| s.to_string());

    // UDS byte guard (ADR-005): truncate at nearest valid UTF-8 char boundary
    // at or below MAX_GOAL_BYTES. The MCP path hard-rejects; UDS path truncates.
    let goal: Option<String> = raw_goal.map(|g| {
        if g.len() > MAX_GOAL_BYTES {
            // Find the largest valid char boundary <= MAX_GOAL_BYTES
            let truncated = truncate_at_utf8_boundary(&g, MAX_GOAL_BYTES);
            tracing::warn!(
                session_id = %event.session_id,
                original_bytes = g.len(),
                truncated_bytes = truncated.len(),
                "col-025: UDS goal exceeds MAX_GOAL_BYTES; truncated at char boundary"
            );
            truncated
        } else {
            g
        }
    });

    // Set current_goal synchronously in the registry.
    // If session not registered yet, set_current_goal is a silent no-op.
    session_registry.set_current_goal(&event.session_id, goal.clone());

    // goal is captured by the spawn closure below (Step 5 modification).
    // Store it for use in the spawn section.
    // (Use a local variable that is moved into the spawn closure.)
}
```

Note on `truncate_at_utf8_boundary`: this is a private helper function to add
to `listener.rs`:

```
/// Truncate a string to at most `max_bytes` bytes at a valid UTF-8 char boundary.
///
/// Returns the largest prefix whose byte length is <= max_bytes and whose
/// end position is a valid char boundary. Never panics.
fn truncate_at_utf8_boundary(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    // Walk backward from max_bytes until we find a valid char boundary.
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}
```

### Changes within Step 4 (fire-and-forget CYCLE_EVENTS INSERT)

The existing spawn in Step 5 calls `store.insert_cycle_event(...)` with 7
arguments. Update it to pass `goal` as the 8th argument:

```
// Step 5: Fire-and-forget CYCLE_EVENTS INSERT (crt-025, updated col-025).
if !feature_cycle.is_empty() {
    let phase_val = event.payload.get("phase")...;
    let outcome_val = event.payload.get("outcome")...;
    let next_phase_for_db = event.payload.get("next_phase")...;
    let event_type_str = event.event_type.clone();
    let cycle_id = feature_cycle.clone();
    let timestamp = unix_now_secs() as i64;
    let store_clone = Arc::clone(store);

    // col-025: capture goal for the spawn.
    // For Start events, goal was computed above.
    // For PhaseEnd and Stop events, goal is always None.
    let goal_for_db: Option<String> = if lifecycle == CycleLifecycle::Start {
        // goal was computed in Step 3b; re-extract from registry for the spawn.
        // Alternatively, capture goal from the local variable computed in Step 3b.
        // Implementation note: restructure the goal computation into a local variable
        // at the top of the Start-only block and capture it here.
        goal_value_from_step_3b   // the computed Option<String> from Step 3b
    } else {
        None
    };

    let _ = tokio::spawn(async move {
        let seq = store_clone.get_next_cycle_seq(&cycle_id).await;
        if let Err(e) = store_clone
            .insert_cycle_event(
                &cycle_id,
                seq,
                &event_type_str,
                phase_val.as_deref(),
                outcome_val.as_deref(),
                next_phase_for_db.as_deref(),
                timestamp,
                goal_for_db.as_deref(),   // NEW 8th argument
            )
            .await
        {
            tracing::warn!(error = %e, cycle_id = %cycle_id, "crt-025: insert_cycle_event failed");
        }
    });
}
```

### Implementation note on goal variable lifecycle

The cleanest restructuring is:

1. Compute `goal: Option<String>` in Step 3b (synchronous section) for Start
   events; set to `None` for all other lifecycle variants.
2. Call `session_registry.set_current_goal` in Step 3b.
3. In Step 5, capture `goal` by value into the spawn closure.

This avoids re-reading from the registry inside the spawn.

---

## Constants

`MAX_GOAL_BYTES` must be visible in `listener.rs`. It is defined in `hook.rs`
adjacent to `MAX_INJECTION_BYTES` and `MAX_PRECOMPACT_BYTES`. Either:
- Move `MAX_GOAL_BYTES` to a shared `constants.rs` module and import it in
  both `hook.rs` and `listener.rs`, or
- Re-declare it in `listener.rs` with the same value (duplication risk).

Recommendation: define `MAX_GOAL_BYTES` in `hook.rs` (consistent with other
byte-budget constants) and `pub(crate)` it for import in `listener.rs`.

---

## Data Flow

Input:
- `event.payload["goal"]` — from the `ImplantEvent` serialized by the MCP
  handler when it emits the `CYCLE_START_EVENT` to the UDS listener

Output:
- `session_registry.current_goal` — set synchronously before function returns
- `store.cycle_events.goal` — set asynchronously via fire-and-forget spawn

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| Goal absent from payload | `None` — session proceeds with no goal |
| Goal empty string in payload | Stored verbatim as `Some("")` on UDS path — no empty-string normalization (ADR-005, FR-11 scope is MCP-path only) |
| Goal > MAX_GOAL_BYTES | Truncated at UTF-8 char boundary + `tracing::warn!` |
| `set_current_goal` session not registered | Silent no-op (consistent with `set_current_phase`) |
| `insert_cycle_event` spawn fails | `tracing::warn!` in spawn — does not affect session state |
| `truncate_at_utf8_boundary` with zero-len string | Returns empty string; safe |

---

## Key Test Scenarios

### T-CEH-01: Goal stored and current_goal set on CYCLE_START_EVENT (AC-01)
```
setup: register session, prepare event with payload goal = "feature intent"
act:   call handle_cycle_event with lifecycle = Start
assert: session_registry.get_state(session_id).current_goal == Some("feature intent")
```

### T-CEH-02: Goal not written on CYCLE_PHASE_END_EVENT (FR-01)
```
setup: register session with goal already set
act:   call handle_cycle_event with lifecycle = PhaseEnd
assert: session_registry.get_state(session_id).current_goal unchanged (not cleared)
assert: insert_cycle_event called with goal = None
```

### T-CEH-03: Goal not written on CYCLE_STOP_EVENT (FR-01)
```
similar to T-CEH-02 but with lifecycle = Stop
```

### T-CEH-04: Absent goal in payload → current_goal = None (AC-02)
```
setup: register session; payload has no "goal" key
act:   call handle_cycle_event with lifecycle = Start
assert: session_registry.get_state(session_id).current_goal == None
assert: insert_cycle_event called with goal = None
```

### T-CEH-05: UDS byte guard truncation — UTF-8 char boundary (R-07)
```
setup: goal = string of MAX_GOAL_BYTES + 1 bytes where last byte is mid-CJK char
act:   call handle_cycle_event with lifecycle = Start
assert: no panic
assert: current_goal is Some(s) where s.len() <= MAX_GOAL_BYTES
assert: s is valid UTF-8 (s.is_char_boundary(s.len()))
assert: tracing::warn! emitted with "col-025: UDS goal exceeds MAX_GOAL_BYTES"
```

### T-CEH-06: UDS goal exactly MAX_GOAL_BYTES — no truncation (R-07)
```
setup: goal = string of exactly MAX_GOAL_BYTES bytes of ASCII
act:   call handle_cycle_event with lifecycle = Start
assert: current_goal == Some(original_goal) — not truncated
assert: no tracing::warn! about truncation
```

### T-CEH-07: UDS truncate-then-overwrite retry (R-13)
```
setup: write truncated goal via first UDS cycle_start event
act:   write corrected goal via second UDS cycle_start event (same cycle_id)
assert: insert_cycle_event called twice; second call's goal overwrites first
assert: current_goal == Some(corrected_goal)
```

### T-CEH-08: insert_cycle_event goal column position (R-08)
```
See schema-migration-v16.md T-V16-03 — same scenario covers this risk.
```
