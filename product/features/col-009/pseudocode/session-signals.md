# Pseudocode: session-signals

## Purpose

Extend `SessionState` with new fields for rework tracking, agent action interception, and the dedup guard. Add new `SessionRegistry` methods: `drain_and_signal_session` (atomic — ADR-003), `sweep_stale_sessions`, `record_rework_event`, `record_agent_action`, and `has_crossed_rework_threshold`.

## Files

- MODIFY `crates/unimatrix-server/src/session.rs`

## New Types (add to `session.rs`)

```rust
use std::collections::{HashMap, HashSet};
use unimatrix_store::{SignalRecord, SignalType, SignalSource};

pub struct ReworkEvent {
    pub tool_name: String,
    pub file_path: Option<String>,
    pub had_failure: bool,
    pub timestamp: u64,
}

pub struct SessionAction {
    pub entry_id: u64,
    pub action: AgentActionType,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentActionType {
    ExplicitUnhelpful,
    ExplicitHelpful,
    Correction,
    Deprecation,
}

pub struct SignalOutput {
    pub session_id: String,
    pub helpful_entry_ids: Vec<u64>,
    pub flagged_entry_ids: Vec<u64>,
    pub final_outcome: SessionOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionOutcome {
    Success,
    Rework,
    Abandoned,
}

const STALE_SESSION_THRESHOLD_SECS: u64 = 4 * 3600;
const REWORK_EDIT_CYCLE_THRESHOLD: usize = 3;
```

## Modified: `SessionState`

Add fields to the existing struct:

```rust
pub struct SessionState {
    // Existing fields (unchanged)
    pub session_id: String,
    pub role: Option<String>,
    pub feature: Option<String>,
    pub injection_history: Vec<InjectionRecord>,
    pub coaccess_seen: HashSet<Vec<u64>>,
    pub compaction_count: u32,

    // New col-009 fields
    pub signaled_entries: HashSet<u64>,   // entries that already got an implicit signal
    pub rework_events: Vec<ReworkEvent>,  // PostToolUse rework observations
    pub agent_actions: Vec<SessionAction>, // explicit MCP actions (Session Intent Registry)
    pub last_activity_at: u64,            // max(registration_ts, last_injection_ts, last_rework_ts)
}
```

## Modified: `SessionRegistry::register_session`

Add initialization for new fields:

```rust
sessions.insert(session_id.to_string(), SessionState {
    // existing fields...
    signaled_entries: HashSet::new(),
    rework_events: Vec::new(),
    agent_actions: Vec::new(),
    last_activity_at: now_secs(),
});
```

## Modified: `SessionRegistry::record_injection`

Update `last_activity_at` after appending injection records:

```rust
// After pushing to injection_history:
state.last_activity_at = state.last_activity_at.max(now);
```

## New: `SessionRegistry::record_rework_event`

```rust
pub fn record_rework_event(&self, session_id: &str, event: ReworkEvent) {
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = sessions.get_mut(session_id) {
        let ts = event.timestamp;
        state.rework_events.push(event);
        state.last_activity_at = state.last_activity_at.max(ts);
    }
    // Unregistered session: silent no-op (FR-03.2)
}
```

## New: `SessionRegistry::record_agent_action`

```rust
pub fn record_agent_action(&self, session_id: &str, action: SessionAction) {
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(state) = sessions.get_mut(session_id) {
        state.agent_actions.push(action);
    }
    // Unregistered session: silent no-op (FR-03.3)
}
```

## New: `SessionRegistry::drain_and_signal_session` (ADR-003: ATOMIC)

```rust
/// Atomic: acquires lock once, generates SignalOutput, removes session.
/// If session already cleared: returns None.
pub fn drain_and_signal_session(
    &self,
    session_id: &str,
    hook_outcome: &str,
) -> Option<SignalOutput> {
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());

    // If session absent, already cleared → no-op (FR-04.2, AC-03)
    let state = sessions.remove(session_id)?;

    // Build signal output from the removed state
    let output = build_signal_output_from_state(state, hook_outcome);

    // Lock released here — session is gone, no race possible (ADR-003)
    Some(output)
}
```

## Internal: `build_signal_output_from_state`

Called from within the locked scope (lock already held via remove). Since sessions.remove already returns the SessionState, this is called after removal:

```rust
fn build_signal_output_from_state(
    state: SessionState,
    hook_outcome: &str,
) -> SignalOutput {
    // Determine outcome (FR-04.4)
    let rework_crossed = has_crossed_rework_threshold(&state);
    let final_outcome = match (hook_outcome, rework_crossed) {
        (_, true) => SessionOutcome::Rework,
        ("success", false) => SessionOutcome::Success,
        _ => SessionOutcome::Abandoned,  // "", "abandoned", None, etc.
    };

    // If abandoned: return empty output with no signals
    if final_outcome == SessionOutcome::Abandoned {
        return SignalOutput {
            session_id: state.session_id.clone(),
            helpful_entry_ids: Vec::new(),
            flagged_entry_ids: Vec::new(),
            final_outcome,
        };
    }

    // Build deduplicated entry set from injection_history (FR-04.3)
    let explicit_unhelpful: HashSet<u64> = state.agent_actions.iter()
        .filter(|a| a.action == AgentActionType::ExplicitUnhelpful)
        .map(|a| a.entry_id)
        .collect();

    let all_injected: HashSet<u64> = state.injection_history.iter()
        .map(|r| r.entry_id)
        .collect();

    // Exclude: already signaled + ExplicitUnhelpful
    let eligible: Vec<u64> = all_injected.into_iter()
        .filter(|id| !state.signaled_entries.contains(id))
        .filter(|id| !explicit_unhelpful.contains(id))
        .collect();

    // Route to helpful or flagged based on outcome
    match final_outcome {
        SessionOutcome::Success => SignalOutput {
            session_id: state.session_id,
            helpful_entry_ids: eligible,
            flagged_entry_ids: Vec::new(),
            final_outcome: SessionOutcome::Success,
        },
        SessionOutcome::Rework => SignalOutput {
            session_id: state.session_id,
            helpful_entry_ids: Vec::new(),
            flagged_entry_ids: eligible,
            final_outcome: SessionOutcome::Rework,
        },
        SessionOutcome::Abandoned => unreachable!(),
    }
}
```

## Internal: `has_crossed_rework_threshold` (ADR-002)

```rust
fn has_crossed_rework_threshold(state: &SessionState) -> bool {
    // Group rework events by file_path (Edit/Write/MultiEdit events only)
    // For each file_path, count edit-fail-edit cycles:
    //   A cycle is: Edit(file) → Bash(fail) → Edit(file)
    //   We need 3 such cycles (REWORK_EDIT_CYCLE_THRESHOLD)
    //
    // Algorithm:
    //   For each unique file_path:
    //     Walk rework_events in order
    //     Track: last_edit_for_this_file = bool
    //            had_failure_since_last_edit = bool
    //            edit_cycle_count = 0
    //     When we see an Edit event for this file:
    //       if last_edit_for_this_file AND had_failure_since_last_edit:
    //         edit_cycle_count += 1
    //       last_edit_for_this_file = true
    //       had_failure_since_last_edit = false
    //     When we see a Bash event with had_failure=true:
    //       had_failure_since_last_edit = true  (for ALL file paths being tracked)
    //     If edit_cycle_count >= REWORK_EDIT_CYCLE_THRESHOLD: return true

    use std::collections::HashMap;

    // Collect unique file paths from Edit/Write/MultiEdit events
    let file_paths: HashSet<&str> = state.rework_events.iter()
        .filter(|e| matches!(e.tool_name.as_str(), "Edit" | "Write" | "MultiEdit"))
        .filter_map(|e| e.file_path.as_deref())
        .collect();

    for path in file_paths {
        let mut last_was_edit = false;
        let mut failure_since_last_edit = false;
        let mut cycle_count = 0usize;

        for event in &state.rework_events {
            match event.tool_name.as_str() {
                "Edit" | "Write" | "MultiEdit"
                    if event.file_path.as_deref() == Some(path) =>
                {
                    if last_was_edit && failure_since_last_edit {
                        cycle_count += 1;
                        if cycle_count >= REWORK_EDIT_CYCLE_THRESHOLD {
                            return true;
                        }
                    }
                    last_was_edit = true;
                    failure_since_last_edit = false;
                }
                "Bash" if event.had_failure => {
                    failure_since_last_edit = true;
                }
                _ => {}
            }
        }
    }

    false
}
```

## New: `SessionRegistry::sweep_stale_sessions`

```rust
/// Single lock acquisition: find stale sessions, generate signals, remove them.
pub fn sweep_stale_sessions(&self) -> Vec<(String, SignalOutput)> {
    let now = now_secs();
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());

    let stale_ids: Vec<String> = sessions.iter()
        .filter(|(_, state)| {
            now.saturating_sub(state.last_activity_at) >= STALE_SESSION_THRESHOLD_SECS
        })
        .map(|(id, _)| id.clone())
        .collect();

    let mut results = Vec::new();
    for session_id in stale_ids {
        if let Some(state) = sessions.remove(&session_id) {
            // Stale sessions default to "success" outcome (orphaned = best effort)
            // If injection_history is empty: return empty signal output, no queue write needed
            if !state.injection_history.is_empty() {
                let output = build_signal_output_from_state(state, "success");
                results.push((session_id, output));
            }
            // else: silent eviction (FR-09.4)
        }
    }

    results
}
```

## Helper: `now_secs`

```rust
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
```

## Error Handling

- All methods: Mutex poison recovery via `.unwrap_or_else(|e| e.into_inner())` (existing pattern)
- `drain_and_signal_session`: if session absent, returns `None` — caller handles
- `record_rework_event` / `record_agent_action`: silent no-op if session not found

## Key Test Scenarios

1. `test_drain_and_signal_session_success` — 3 injected entries, "success" outcome → SignalOutput.helpful_entry_ids == [id1, id2, id3]
2. `test_drain_and_signal_session_idempotent` — call twice same session_id → second returns None
3. `test_drain_and_signal_session_abandoned` — outcome="" → returns Some(output) with empty lists
4. `test_drain_and_signal_rework_override` — 3 edit-fail-edit cycles → outcome overridden to Rework
5. `test_explicit_unhelpful_excluded` — entry_id=42 with ExplicitUnhelpful → excluded from helpful set
6. `test_rework_threshold_two_cycles_not_crossed` — 2 cycles → has_crossed == false
7. `test_rework_threshold_three_cycles_crossed` — 3 cycles → has_crossed == true
8. `test_rework_threshold_no_intervening_failure` — 5 rapid edits, no failures → not crossed
9. `test_rework_threshold_different_files` — 3 cycles but different files (1 per file) → not crossed
10. `test_sweep_stale_sessions_evicts_old` — session with last_activity_at = now - 4h - 1s → swept
11. `test_sweep_stale_sessions_keeps_recent` — session with last_activity_at = now - 3h → not swept
12. `test_last_activity_at_updated_by_rework_event` — record_rework_event updates last_activity_at
13. `test_last_activity_at_updated_by_injection` — record_injection updates last_activity_at
14. `test_sweep_empty_session_silent_eviction` — stale session with no injection_history → no signal output
