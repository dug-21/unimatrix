# Test Plan: session-state-extension

**Crate**: `unimatrix-server`
**File modified**: `src/infra/session.rs`

**Risks covered**: R-06
**ACs covered**: AC-11 (compile-time coverage)

---

## Overview

Component 2 adds `current_goal: Option<String>` to `SessionState` and
`set_current_goal(&self, session_id, goal)` to `SessionRegistry`. This is
primarily a compile-time risk: any `SessionState { .. }` struct literal that
does not use `..Default::default()` will fail to compile after the field is added.

The test coverage goal is:
1. Zero compile errors after field addition (R-06 primary coverage).
2. At least one test constructs `SessionState` with `current_goal: Some("goal")`.
3. `set_current_goal` is tested for idempotency and concurrent-safety.

---

## Pre-Delivery Audit (R-06)

Before adding the field, enumerate all construction sites:

```bash
grep -rn "SessionState {" crates/unimatrix-server/src/
grep -rn "make_session_state" crates/unimatrix-server/src/
```

Identified sites from current codebase:
- `src/infra/session.rs`: `register_session` builds `SessionState { .. }` struct literal.
- `src/services/index_briefing.rs`: `make_session_state` test helper.
- Possibly `src/uds/listener.rs` test section.

Each site must either:
- Add `current_goal: None` explicitly, or
- Adopt `..Default::default()` (which requires `SessionState: Default`).

---

## Tests

All tests in this component live in `src/infra/session.rs` (`#[cfg(test)] mod tests`).

### Test: `test_session_state_current_goal_field_default_is_none` (R-06 / AC-11)

```
#[test] fn test_session_state_current_goal_field_default_is_none()
```

Construct a `SessionState` via `SessionRegistry::register_session`.
Assert `state.current_goal == None` immediately after registration.
Verifies initialization to `None` (IMPLEMENTATION-BRIEF.md §Data Structures).

### Test: `test_session_state_current_goal_field_exists`

```
#[test] fn test_session_state_current_goal_field_exists()
```

Construct a `SessionState` with `current_goal: Some("test goal")` explicitly.
Read back the field. Assert `== Some("test goal")`.
This test exercises the field with a non-None value (R-06 coverage requirement).

### Test: `test_set_current_goal_sets_and_overwrites`

```
#[test] fn test_set_current_goal_sets_and_overwrites()
```

- Register a session. Assert `current_goal == None`.
- Call `set_current_goal(session_id, Some("goal A"))`. Assert `current_goal == Some("goal A")`.
- Call `set_current_goal(session_id, Some("goal B"))`. Assert `current_goal == Some("goal B")`.
- Call `set_current_goal(session_id, None)`. Assert `current_goal == None`.
Verifies idempotency and overwrite behavior.

### Test: `test_set_current_goal_unknown_session_is_noop`

```
#[test] fn test_set_current_goal_unknown_session_is_noop()
```

Call `set_current_goal("nonexistent-session", Some("goal"))` without registering
the session first. Assert no panic. This is the "silently ignored" contract
(matching the pattern of `record_injection` for unregistered sessions).

### Test: `test_set_current_goal_idempotent_same_value`

```
#[test] fn test_set_current_goal_idempotent_same_value()
```

Call `set_current_goal(session_id, Some("goal"))` twice with the same value.
Assert `current_goal == Some("goal")` after both calls. No error, no state
corruption.

### Test: `test_register_session_initializes_current_goal_to_none` (AC-11)

```
#[test] fn test_register_session_initializes_current_goal_to_none()
```

After `register_session`, get the session state. Assert `current_goal == None`.
Explicit test for the initialization contract in `register_session` (not
implied — must be stated per IMPLEMENTATION-BRIEF §Data Structures).

---

## `make_session_state` Helper Update (R-06 / R-05)

The `make_session_state` test helper in `src/services/index_briefing.rs` builds
`SessionState` directly. After adding `current_goal`, this helper must be
updated. The test plan for `briefing-query-derivation.md` depends on this.

The helper signature must be extended to accept `current_goal: Option<&str>` as
a parameter (or use `..Default::default()` for existing call sites):

```rust
fn make_session_state(
    feature: Option<&str>,
    signals: Vec<(&str, u32)>,
    current_goal: Option<&str>,  // NEW
) -> SessionState { ... }
```

All existing call sites in `index_briefing.rs` tests must be updated to pass
`None` for `current_goal` (backward-compatible update). New tests pass
`Some("goal text")`.

---

## Compile-Time Gate

After adding `current_goal` to `SessionState`:

```bash
cargo build --workspace 2>&1 | tail -20
```

Must produce zero errors. Any `missing field 'current_goal'` errors identify
sites that were missed during the pre-delivery audit. Fix them before proceeding.
