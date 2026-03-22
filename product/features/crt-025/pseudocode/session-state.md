# Component 4: SessionState
## File: `crates/unimatrix-server/src/infra/session.rs`

---

## Purpose

Adds `current_phase: Option<String>` to `SessionState` and exposes `SessionRegistry::set_current_phase` for synchronous phase mutation by the UDS listener. This is the in-memory source of truth for which phase is active in a session. WA-2 and WA-4 will read this field directly.

---

## Modified Struct: `SessionState`

```
// BEFORE (existing fields):
pub struct SessionState {
    pub session_id:        String,
    pub role:              Option<String>,
    pub feature:           Option<String>,
    pub injection_history: Vec<InjectionRecord>,
    pub coaccess_seen:     HashSet<Vec<u64>>,
    pub compaction_count:  u32,
    pub signaled_entries:  HashSet<u64>,
    pub rework_events:     Vec<ReworkEvent>,
    pub agent_actions:     Vec<SessionAction>,
    pub last_activity_at:  u64,
    pub topic_signals:     HashMap<String, TopicTally>,
}

// AFTER (add at end):
pub struct SessionState {
    // ... all existing fields unchanged, in original order ...
    pub current_phase: Option<String>,   // NEW: None until first phase signal
}
```

---

## Modified Function: `register_session`

Add `current_phase: None` to the `SessionState` constructor:

```
FUNCTION register_session(session_id, role, feature):
    now = now_secs()
    sessions.insert(session_id, SessionState {
        session_id: session_id.to_string(),
        role,
        feature,
        injection_history: Vec::new(),
        coaccess_seen:     HashSet::new(),
        compaction_count:  0,
        signaled_entries:  HashSet::new(),
        rework_events:     Vec::new(),
        agent_actions:     Vec::new(),
        last_activity_at:  now,
        topic_signals:     HashMap::new(),
        current_phase:     None,         // NEW: initialized to None
    })
```

---

## New Method: `SessionRegistry::set_current_phase`

```
impl SessionRegistry:

    /// Set current_phase for a session.
    ///
    /// Synchronous. Called by the UDS listener handler before any DB spawn_blocking.
    /// If session_id is not registered: silent no-op (consistent with other Registry methods).
    /// phase = None clears the phase (used on "stop" events).
    pub fn set_current_phase(&self, session_id: &str, phase: Option<String>):
        sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner())
        IF let Some(state) = sessions.get_mut(session_id):
            state.current_phase = phase
        // Else: unregistered session → silent no-op
```

---

## Existing Method: `get_state` (unchanged)

`get_state` returns a clone of `SessionState`, which now includes `current_phase`. All callers receive the field automatically. No changes needed to `get_state` itself.

---

## State Transitions

```
Event received by UDS listener → set_current_phase called:

cycle_start with next_phase="scope"  → set_current_phase(sid, Some("scope"))
cycle_start without next_phase       → no call (leave unchanged)
cycle_phase_end with next_phase="X"  → set_current_phase(sid, Some("X"))
cycle_phase_end without next_phase   → no call (leave unchanged)
cycle_stop                           → set_current_phase(sid, None)
```

These rules are enforced by the UDS listener (component 5), not by `set_current_phase` itself. `set_current_phase` is a dumb setter.

---

## Invariants

- `current_phase` begins as `None` for all new sessions.
- `None` means "no phase signal received yet" or "cycle has ended".
- A normalized lowercase phase string (from `validate_cycle_params`) is the only value written here; the session registry does no normalization.
- No maximum length enforcement at this level; `validate_cycle_params` already enforced 64-char limit before the value was put here.

---

## Error Handling

| Condition | Behavior |
|-----------|----------|
| `session_id` not in registry | Silent no-op (matches `record_injection` pattern) |
| Lock poisoned | `unwrap_or_else(|e| e.into_inner())` recovery (existing pattern in this file) |

---

## Key Test Scenarios

1. `register_session` → `get_state().current_phase == None`
2. `set_current_phase(sid, Some("scope"))` → `get_state().current_phase == Some("scope")`
3. `set_current_phase(sid, Some("implementation"))` (second call) → overwrites to `Some("implementation")`
4. `set_current_phase(sid, None)` → `get_state().current_phase == None`
5. `set_current_phase("unknown-session", Some("scope"))` → no panic, registry state unchanged
6. Clone of `SessionState` includes `current_phase` (verify field present in clone)
