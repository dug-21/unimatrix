# Component: session-resume

**Crate**: `unimatrix-server`
**File**: `src/uds/listener.rs` (fn `dispatch_request`, `HookRequest::SessionRegister` arm)

---

## Purpose

After a server restart, `SessionState` is cleared. When a new agent request
arrives with a `session_id` tied to an existing feature cycle, the
`SessionRegister` arm reconstructs `current_goal` from `cycle_events` via a
single indexed DB lookup. Any DB failure degrades gracefully to
`current_goal = None` (ADR-004).

---

## Modified Code Location

In `dispatch_request`, inside the `HookRequest::SessionRegister { ... }` arm.
The existing arm (line 488+) registers the session and persists a
`SessionRecord`. The goal resume lookup must occur AFTER `register_session` is
called (so the session exists in the registry) and before the arm returns
`HookResponse::Ack`.

### Current structure of SessionRegister arm (simplified):

```
HookRequest::SessionRegister { session_id, cwd, agent_role, feature } => {
    // 1. capability check
    // 2. sanitize session_id
    // 3. sanitize role + feature
    // 4. session_registry.register_session(...)
    // 5. persist SessionRecord to SESSIONS table
    // 6. warm_embedding_model
    return HookResponse::Ack
}
```

### New structure (goal resume lookup added after step 4):

```
HookRequest::SessionRegister { session_id, cwd, agent_role, feature } => {
    // 1. capability check (unchanged)
    // 2. sanitize session_id (unchanged)
    // 3. sanitize role + feature (unchanged)

    // 4. register session in registry (unchanged)
    session_registry.register_session(&session_id, clean_role.clone(), clean_feature.clone());

    // 4b. col-025: Goal resume lookup (ADR-004).
    // Only attempt if the session has a feature_cycle set.
    // If DB error occurs, degrade to None + warn (never block session usability).
    if let Some(ref fc) = clean_feature {
        if !fc.is_empty() {
            let goal = store
                .get_cycle_start_goal(fc)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        error = %e,
                        cycle_id = %fc,
                        "col-025: goal resume lookup failed, degrading to None"
                    );
                    None
                });

            // Always call set_current_goal even when goal is None.
            // This makes the invariant explicit: current_goal is deterministically
            // None after a failed or missing lookup, not just the None from
            // register_session initialization (ADR-004 §Decision).
            session_registry.set_current_goal(&session_id, goal);
        }
    }

    // 5. persist SessionRecord to SESSIONS table (unchanged)
    // 6. warm_embedding_model (unchanged)
    return HookResponse::Ack
}
```

---

## Sequencing Invariant

`register_session` MUST be called before `set_current_goal`. The session must
exist in the registry for `set_current_goal` to update it (silent no-op
otherwise). This ordering is already established: step 4 before step 4b.

The `store.get_cycle_start_goal(...).await` is the only async/fallible
operation introduced by col-025 (ARCHITECTURE.md §Component 5). It runs
inside the already-async `dispatch_request` function, so no additional
`tokio::spawn` is needed here.

---

## Data Flow

Input:
- `clean_feature: Option<String>` — sanitized feature_cycle from the
  `SessionRegister` payload; this is the same value set on the session

Output:
- `session_registry.current_goal` — set to `Some(goal)` on success,
  `None` on DB error or absent row

DB query executed:
```
SELECT goal FROM cycle_events
WHERE cycle_id = ?1 AND event_type = 'cycle_start'
LIMIT 1
```
Served by `idx_cycle_events_cycle_id` (pattern #3383).

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| `feature` is `None` or empty | Skip lookup entirely; `current_goal` remains `None` from `register_session` |
| No `cycle_start` row for `cycle_id` | `get_cycle_start_goal` returns `Ok(None)`; `set_current_goal(None)` called |
| `cycle_start` row exists, `goal IS NULL` | `get_cycle_start_goal` returns `Ok(None)`; `set_current_goal(None)` called |
| `cycle_start` row exists, goal present | `get_cycle_start_goal` returns `Ok(Some(g))`; `set_current_goal(Some(g))` called |
| DB infrastructure error | `get_cycle_start_goal` returns `Err(e)`; `unwrap_or_else` logs warn and returns `None`; `set_current_goal(None)` called; `HookResponse::Ack` still returned |

The session registration ALWAYS completes with `HookResponse::Ack`. A DB
error on goal lookup never causes `HookResponse::Error` (ADR-004).

---

## Key Test Scenarios

### T-SR-01: Goal reconstructed from DB on session resume (AC-03)
```
setup: insert cycle_start row for cycle_id "col-025" with goal = "feature intent"
act:   call SessionRegister with feature = "col-025"
assert: session_registry.get_state(session_id).current_goal == Some("feature intent")
assert: returns HookResponse::Ack
```

### T-SR-02: Pre-v16 cycle — no goal row — current_goal = None (AC-14)
```
setup: cycle_events has no row for cycle_id (or has row with goal = NULL)
act:   call SessionRegister with feature = "col-099"
assert: session_registry.get_state(session_id).current_goal == None
assert: returns HookResponse::Ack
```

### T-SR-03: No feature_cycle — skip lookup (AC-02)
```
act:   call SessionRegister with feature = None
assert: no DB query for get_cycle_start_goal
assert: session_registry.get_state(session_id).current_goal == None
assert: returns HookResponse::Ack
```

### T-SR-04: DB error on resume — None + warn + Ack (AC-15)
```
setup: inject DB error for get_cycle_start_goal
act:   call SessionRegister with feature = "col-025"
assert: session_registry.get_state(session_id).current_goal == None
assert: tracing::warn! emitted with "col-025: goal resume lookup failed"
assert: returns HookResponse::Ack (session registration succeeds)
```

### T-SR-05: goal IS NULL in DB — current_goal = None (AC-14 variant)
```
setup: insert cycle_start row for cycle_id with goal = NULL
       (caller omitted goal when starting the cycle)
act:   call SessionRegister with feature = same cycle_id
assert: session_registry.get_state(session_id).current_goal == None
assert: returns HookResponse::Ack
```

### T-SR-06: Concurrent SessionRegister idempotency (edge case)
```
// set_current_goal uses Mutex; concurrent calls are safe.
// Code review sufficient; no automated race test required.
// The set_current_goal Mutex pattern matches set_current_phase (established pattern).
```
