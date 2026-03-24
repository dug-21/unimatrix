# Component: session-state-extension

**Crate**: `unimatrix-server`
**File**: `src/infra/session.rs`

---

## Purpose

Add `current_goal: Option<String>` to `SessionState` and provide
`SessionRegistry::set_current_goal` so the cycle-event-handler and
session-resume components can populate the goal without duplicating
lock acquisition logic.

---

## New / Modified Functions

### `SessionState` struct — add field

Current struct ends with `category_counts`:
```
pub struct SessionState {
    // ... existing fields ...
    pub current_phase: Option<String>,      // crt-025
    pub category_counts: HashMap<String, u32>, // crt-026
}
```

Add after `category_counts`:
```
    // col-025 fields
    /// Goal of the active feature cycle, cached in-memory.
    ///
    /// None  — no goal provided, or pre-v16 cycle, or DB error on resume.
    /// Some  — set from context_cycle(start) payload via handle_cycle_event,
    ///         or reconstructed from cycle_events on session resume.
    pub current_goal: Option<String>,
```

### `register_session` — initialize new field

Current `register_session` constructs `SessionState { ... }` inline.
Add `current_goal: None` to the struct literal:

```
sessions.insert(
    session_id.to_string(),
    SessionState {
        session_id: session_id.to_string(),
        role,
        feature,
        injection_history: Vec::new(),
        coaccess_seen: HashSet::new(),
        compaction_count: 0,
        signaled_entries: HashSet::new(),
        rework_events: Vec::new(),
        agent_actions: Vec::new(),
        last_activity_at: now,
        topic_signals: HashMap::new(),
        current_phase: None,
        category_counts: HashMap::new(),
        current_goal: None,    // col-025: initialized None; populated by handle_cycle_event or resume
    },
);
```

### `set_current_goal` — new method on `SessionRegistry`

```
/// Set the active feature goal for a session (col-025).
///
/// Called synchronously from:
///   - handle_cycle_event (CYCLE_START_EVENT arm) — set from payload goal
///   - SessionRegister arm in dispatch_request — reconstructed from DB on resume
///
/// Idempotent: subsequent calls with the same value are safe.
/// Thread-safe: lock acquired and released per call (microseconds).
/// Silent no-op if the session is not registered (consistent with set_current_phase).
/// Mutex lock poisoning recovered via unwrap_or_else(|e| e.into_inner()).
///
/// goal: None resets to "no goal" (equivalent to pre-col-025 state).
pub fn set_current_goal(&self, session_id: &str, goal: Option<String>) {
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = sessions.get_mut(session_id) {
        state.current_goal = goal;
    }
    // Unregistered session: silent no-op (consistent with set_current_phase pattern)
}
```

---

## State Machine

`SessionState.current_goal` transitions:

```
None  -- register_session ---------> None (initial)
None  -- set_current_goal(Some(g)) -> Some(g)
Some  -- set_current_goal(None) ----> None
Some  -- set_current_goal(Some(g)) -> Some(g) (update, idempotent if same value)
```

Only two callers: `handle_cycle_event` (on CYCLE_START_EVENT) and the
`SessionRegister` arm (on session resume). `PhaseEnd` and `Stop` events do
NOT call `set_current_goal`.

---

## Data Flow

Input to `set_current_goal`:
- `session_id: &str` — identifies the session to update
- `goal: Option<String>` — the new goal value; `None` means no goal

Output: none (mutates in-memory state)

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| Mutex lock poisoning | `unwrap_or_else(|e| e.into_inner())` — consistent with all other SessionRegistry methods |
| session_id not in registry | Silent no-op (same as `set_current_phase`) |

---

## Test Audit — `make_session_state` and `SessionState` struct literals (R-06)

The following construction sites in `src/services/index_briefing.rs` tests
must be updated to include `current_goal: None`:

```
// In make_session_state test helper:
SessionState {
    session_id: "test-session".to_string(),
    role: None,
    feature: feature.map(str::to_string),
    injection_history: vec![],
    coaccess_seen: HashSet::new(),
    compaction_count: 0,
    signaled_entries: HashSet::new(),
    rework_events: vec![],
    agent_actions: vec![],
    last_activity_at: 0,
    topic_signals,
    current_phase: None,
    category_counts: HashMap::new(),
    current_goal: None,    // ADD THIS
}
```

Pre-delivery grep required: search `crates/unimatrix-server/src/` for
`SessionState {` — every literal that does NOT use `..Default::default()`
must be updated.

---

## Key Test Scenarios

### T-SSE-01: register_session initializes current_goal to None
```
act:   call session_registry.register_session("sess-1", None, None)
assert: session_registry.get_state("sess-1").current_goal == None
```

### T-SSE-02: set_current_goal sets a goal
```
setup: register session
act:   session_registry.set_current_goal("sess-1", Some("my goal".to_string()))
assert: session_registry.get_state("sess-1").current_goal == Some("my goal")
```

### T-SSE-03: set_current_goal clears a goal (None)
```
setup: register session, set goal to Some("g")
act:   session_registry.set_current_goal("sess-1", None)
assert: session_registry.get_state("sess-1").current_goal == None
```

### T-SSE-04: set_current_goal on unregistered session is silent no-op
```
act:   session_registry.set_current_goal("not-registered", Some("g"))
assert: no panic, no error
assert: get_state("not-registered") returns None (session never registered)
```

### T-SSE-05: set_current_goal is idempotent under concurrent calls
```
// This is a structural/design guarantee ensured by Mutex, not a named test.
// The implementation uses the same Mutex pattern as set_current_phase.
// Confirm by code review: lock acquired, field set, lock released. No TOCTOU.
```

### T-SSE-06: make_session_state helper compiles with current_goal field (R-06)
```
// This is a compile-time test: ensure index_briefing.rs test helper
// includes current_goal: None in its SessionState struct literal.
// cargo test must pass after field addition.
```
