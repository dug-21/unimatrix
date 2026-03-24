## ADR-004: Session Resume Goal Lookup Degrades to None on Any Failure

### Context

This ADR addresses SR-05 (session resume failure contract).

The session start path is synchronous and zero-cost for goal: the goal arrives
in the `CYCLE_START_EVENT` payload and is set directly on `SessionState` without
any DB read.

The session resume path (when an existing session_id arrives after a server
restart) must reconstruct `current_goal` from the database:

```sql
SELECT goal FROM cycle_events
WHERE cycle_id = ?1 AND event_type = 'cycle_start'
LIMIT 1
```

This is the only async/fallible operation introduced by col-025. Three failure
modes exist:

1. **Row not found** (cycle started before v16, or cycle_id not yet written):
   returns `None`. Correct; no goal was ever stored.
2. **Row found, goal = NULL** (v16 cycle started without a `goal` param):
   returns `None`. Correct; caller did not provide a goal.
3. **DB error** (timeout, pool exhaustion, corruption): query fails with an error.

For case 3, two response strategies are possible:

**Option A**: Fail the `SessionRegister` operation with an error. This would
prevent session reconstruction if the DB is temporarily unavailable, which is a
worse outcome than simply missing the goal. Session registration is fire-and-forget
from the hook side; the session proceeding with `current_goal = None` is
functionally equivalent to a pre-col-025 session.

**Option B**: Log the error, set `current_goal = None`, and continue session
registration. This is consistent with the graceful degradation pattern throughout
the codebase (entry #3301: "Graceful degradation via empty fallback, not early
return, when post-error side effects must run") and with SCOPE.md §Non-Goals
(backward compatibility).

Option B was chosen. The lesson learned at entry #324 (session gaps cause expensive
re-reads) and entry #3027 (phase-snapshot pattern) confirm that state reconstruction
on resume is a historically problematic path — it should not block session usability.

Additionally, the resume path query must handle v15 databases where the `goal`
column does not exist. The migration runs before the server starts accepting
connections, so a connected server always has a v16 schema. However, the
`get_cycle_start_goal` helper should be written defensively (return `None` on
any SQL error) to handle unexpected column-absence during development or test.

### Decision

`Store::get_cycle_start_goal(cycle_id: &str) -> Result<Option<String>>` returns:
- `Ok(Some(goal))` if a `cycle_start` row exists with a non-NULL goal.
- `Ok(None)` if the row exists but goal is NULL, or if no `cycle_start` row exists.
- `Err(...)` only on DB infrastructure failures.

In the `SessionRegister` arm of `dispatch_request`, the call is:

```rust
let goal = store.get_cycle_start_goal(&feature_cycle)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, cycle_id = %feature_cycle,
            "col-025: goal resume lookup failed, degrading to None");
        None
    });
session_registry.set_current_goal(&session_id, goal);
```

The `set_current_goal` call always runs — even on DB error — to ensure
`current_goal` is deterministically set (to `None`) rather than left at
whatever value `register_session` initialized it to (also `None`, but this
makes the invariant explicit).

The degradation is observable via a `tracing::warn!` log line. No error is
returned to the hook client; `HookResponse::Ack` is still emitted.

### Consequences

- Session resume never fails due to goal lookup. Worst case: goal-powered
  briefing degrades to topic-ID fallback, which is the pre-col-025 behavior.
- A tracing warn is emitted on DB errors so operators can diagnose persistent
  failures without disrupting agent workflows.
- Spec writer must include a test: resume with a pre-v16 NULL row returns
  `current_goal = None` and session registration succeeds (AC-03).
- The `get_cycle_start_goal` function should have a unit test that verifies:
  - Returns `Ok(None)` for unknown cycle_id.
  - Returns `Ok(None)` for cycle_id with NULL goal column.
  - Returns `Ok(Some(goal))` for cycle_id with stored goal.
