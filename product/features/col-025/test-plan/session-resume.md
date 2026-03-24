# Test Plan: session-resume

**Crate**: `unimatrix-server`
**File modified**: `src/uds/listener.rs` (`SessionRegister` arm of `dispatch_request`)

**Risks covered**: R-03, R-10
**ACs covered**: AC-03, AC-14, AC-15

---

## Overview

Component 5 handles the `SessionRegister` arm of `dispatch_request`. When a
session has a `feature_cycle` already set (server restart scenario), it calls:

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

The four key behaviors under test:
1. Successful lookup with non-null goal → `current_goal = Some(goal)`.
2. Successful lookup, goal IS NULL (pre-v16 row or absent goal) → `current_goal = None`.
3. No `cycle_start` row exists → `current_goal = None`.
4. DB error → `current_goal = None` + warn log emitted + registration succeeds.

---

## Unit Tests

### Test: `test_resume_loads_goal_from_cycle_events` (AC-03)

```
#[tokio::test] async fn test_resume_loads_goal_from_cycle_events()
```

Arrange: v16 SqlxStore. Insert a `cycle_start` row with `goal = "Resume goal text."`.
Act: dispatch a `SessionRegister` hook with `feature_cycle` pointing to that row.
Assert: `session_registry.get_state(session_id)?.current_goal == Some("Resume goal text.")`.
Assert: `HookResponse::Ack` returned (registration succeeds).

This covers AC-03: "After a server restart, a session... loads `current_goal`
from `cycle_events` on resume."

### Test: `test_resume_no_cycle_start_row_sets_none` (AC-14)

```
#[tokio::test] async fn test_resume_no_cycle_start_row_sets_none()
```

Arrange: v16 SqlxStore, no `cycle_events` rows for the test `cycle_id`.
Act: dispatch `SessionRegister` with that `feature_cycle`.
Assert: `current_goal == None`.
Assert: `HookResponse::Ack` returned (registration succeeds with no error).

This covers AC-14: "Session resume when `cycle_events` has no matching
`cycle_start` row... sets `current_goal = None` and completes registration
without error."

### Test: `test_resume_null_goal_row_sets_none` (R-03)

```
#[tokio::test] async fn test_resume_null_goal_row_sets_none()
```

Arrange: v16 SqlxStore. Insert a `cycle_start` row with `goal = NULL` (caller
omitted goal on the start event).
Act: dispatch `SessionRegister` with that `feature_cycle`.
Assert: `current_goal == None`.
Assert: `HookResponse::Ack` returned.

Covers R-03 scenario 3: "Resume with v16 cycle where `goal = NULL`".

### Test: `test_resume_db_error_degrades_to_none_with_warn` (R-03 / AC-15 / Gate 3c scenario 7)

```
#[tokio::test] async fn test_resume_db_error_degrades_to_none_with_warn()
```

This is the **non-negotiable Gate 3c scenario 7**.

Arrange: a store configured to return `Err(...)` from `get_cycle_start_goal`
(use a mock or a store with an intentionally corrupt database that causes a
SQL error on the target query).
Act: dispatch `SessionRegister`.
Assert:
- `current_goal == None` (degraded).
- `HookResponse::Ack` returned (registration did NOT fail).
- `tracing::warn!` was emitted containing the message `"col-025: goal resume lookup failed"`.

Log assertion approach: use the `tracing_test` crate or capture logs via a
subscriber installed in the test. If `tracing_test` is not available, at minimum
verify the code calls `tracing::warn!` via code review and document the limitation.

This covers AC-15: "Session resume when the DB lookup returns an error sets
`current_goal = None`, logs the error, and completes registration without
propagating the error."

### Test: `test_resume_no_feature_cycle_skips_goal_lookup` (NFR-01)

```
#[tokio::test] async fn test_resume_no_feature_cycle_skips_goal_lookup()
```

Arrange: `SessionRegister` with no `feature_cycle` set on the hook payload.
Act: dispatch.
Assert: `current_goal == None` (no DB call made for goal lookup).
Assert: `HookResponse::Ack`.

This tests that the goal lookup is only triggered when `feature_cycle` is
present, not on plain session registrations with no feature context.

---

## Store Helper Tests (also in `schema-migration-v16.md`)

The `get_cycle_start_goal` function is the DB-layer helper. Store-level tests
are defined in `schema-migration-v16.md`:
- `test_get_cycle_start_goal_returns_stored_goal` → `Ok(Some(goal))`
- `test_get_cycle_start_goal_returns_none_for_unknown_cycle_id` → `Ok(None)`
- `test_get_cycle_start_goal_returns_none_when_goal_is_null` → `Ok(None)`
- `test_get_cycle_start_goal_multiple_start_rows_returns_first` → LIMIT 1 guard

The session-resume tests (above) test the caller's error handling via
`unwrap_or_else` — the store-level tests confirm the function's return-value contract.

---

## Concurrent Session Registration (R-03 edge case)

### Test: `test_set_current_goal_concurrent_calls_idempotent` (R-03)

```
#[test] fn test_set_current_goal_concurrent_calls_idempotent()
```

Spawn two concurrent tokio tasks both calling
`set_current_goal(session_id, Some("goal"))` on the same `SessionRegistry`.
Assert: no panic, no deadlock; `current_goal == Some("goal")` after both tasks
complete. Verifies `Mutex` safety (R-03 edge case from RISK-TEST-STRATEGY.md
§Edge Cases).

---

## Integration Scenario

The full restart persistence scenario (session registered → server restart →
session resumed with goal intact) is covered at the infra-001 level:
`test_cycle_start_with_goal_persists_across_restart` in `test_lifecycle.py`.

The unit tests here cover the individual sub-cases (goal present, goal absent,
null, DB error) which are harder to exercise end-to-end.
