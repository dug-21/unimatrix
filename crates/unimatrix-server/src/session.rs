//! Per-session state management for the cortical implant.
//!
//! SessionRegistry replaces col-007's CoAccessDedup with a unified container
//! for all session-scoped server-side state: injection history, co-access dedup,
//! session metadata, and compaction count. See ADR-001.
//!
//! col-009 extends SessionState with rework tracking, agent action recording,
//! and implicit signal generation on session close (drain_and_signal_session).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};


// -- Constants (ADR-002, ADR-003) --

const STALE_SESSION_THRESHOLD_SECS: u64 = 4 * 3600;
const REWORK_EDIT_CYCLE_THRESHOLD: usize = 3;

// -- New types (col-009) --

/// A single tool-use event recorded for rework threshold analysis (ADR-002).
#[derive(Clone, Debug)]
pub struct ReworkEvent {
    pub tool_name: String,
    pub file_path: Option<String>,
    pub had_failure: bool,
    pub timestamp: u64,
}

/// An explicit agent action recorded from MCP tool calls (Session Intent Registry).
#[derive(Clone, Debug)]
pub struct SessionAction {
    pub entry_id: u64,
    pub action: AgentActionType,
    pub timestamp: u64,
}

/// The type of explicit agent action.
#[derive(Clone, Debug, PartialEq)]
pub enum AgentActionType {
    ExplicitUnhelpful,
    ExplicitHelpful,
    Correction,
    Deprecation,
}

/// The computed output from a drain-and-signal operation.
///
/// Caller writes SignalRecords to the queue for each non-empty list.
pub struct SignalOutput {
    pub session_id: String,
    pub helpful_entry_ids: Vec<u64>,
    pub flagged_entry_ids: Vec<u64>,
    pub final_outcome: SessionOutcome,
}

/// The resolved outcome for a session.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionOutcome {
    Success,
    Rework,
    Abandoned,
}

// -- Existing type (extended with new fields) --

/// A single injection event recorded during ContextSearch.
#[derive(Clone, Debug)]
pub struct InjectionRecord {
    pub entry_id: u64,
    pub confidence: f64,
    pub timestamp: u64,
}

/// Per-session state container.
///
/// Tracks everything the server knows about a session: metadata from
/// SessionRegister, injection history from ContextSearch calls, co-access
/// dedup sets (absorbed from CoAccessDedup), compaction count, rework events,
/// and explicit agent actions.
#[derive(Clone, Debug)]
pub struct SessionState {
    // Existing fields
    pub session_id: String,
    pub role: Option<String>,
    pub feature: Option<String>,
    pub injection_history: Vec<InjectionRecord>,
    pub coaccess_seen: HashSet<Vec<u64>>,
    pub compaction_count: u32,
    // col-009 fields
    pub signaled_entries: HashSet<u64>,     // entries that already got an implicit signal
    pub rework_events: Vec<ReworkEvent>,    // PostToolUse rework observations
    pub agent_actions: Vec<SessionAction>,  // explicit MCP actions (Session Intent Registry)
    pub last_activity_at: u64,             // tracks staleness for sweep
}

/// Thread-safe registry for per-session state.
///
/// Wraps `HashMap<String, SessionState>` behind a `Mutex`. Contention is
/// minimal -- lock is held for microseconds per operation, and hook events
/// are serialized per-session by Claude Code.
pub struct SessionRegistry {
    sessions: Mutex<HashMap<String, SessionState>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        SessionRegistry {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Create or overwrite session state. Handles reconnection (FR-02.4).
    pub fn register_session(
        &self,
        session_id: &str,
        role: Option<String>,
        feature: Option<String>,
    ) {
        let now = now_secs();
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
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
            },
        );
    }

    /// Record injected entries from a ContextSearch response.
    ///
    /// Appends InjectionRecords with the current timestamp. Duplicate entry_ids
    /// across calls are allowed (preserves chronological history -- FR-02.5).
    /// Silently ignored if session is not registered (FR-02.10).
    pub fn record_injection(&self, session_id: &str, entries: &[(u64, f64)]) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            let now = now_secs();
            for &(entry_id, confidence) in entries {
                state.injection_history.push(InjectionRecord {
                    entry_id,
                    confidence,
                    timestamp: now,
                });
            }
            state.last_activity_at = state.last_activity_at.max(now);
        }
        // Unregistered session: silent no-op (FR-02.10)
    }

    /// Return a clone of the session state, or None if not registered.
    pub fn get_state(&self, session_id: &str) -> Option<SessionState> {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.get(session_id).cloned()
    }

    /// Co-access dedup: returns `true` if the entry set is NEW for this session.
    ///
    /// Absorbs CoAccessDedup behavior (col-007). Canonicalizes entry order
    /// before comparison. Returns `false` for unregistered sessions.
    pub fn check_and_insert_coaccess(&self, session_id: &str, entry_ids: &[u64]) -> bool {
        let mut canonical = entry_ids.to_vec();
        canonical.sort_unstable();

        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            state.coaccess_seen.insert(canonical)
        } else {
            false
        }
    }

    /// Increment the compaction count for a session.
    pub fn increment_compaction(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            state.compaction_count += 1;
        }
    }

    /// Remove all state for a session (called on SessionClose when no signals needed).
    pub fn clear_session(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.remove(session_id);
    }

    /// Record a tool-use event for rework threshold analysis (col-009, FR-03.1).
    ///
    /// Silently ignored if session is not registered (FR-03.2).
    pub fn record_rework_event(&self, session_id: &str, event: ReworkEvent) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            let ts = event.timestamp;
            state.rework_events.push(event);
            state.last_activity_at = state.last_activity_at.max(ts);
        }
        // Unregistered session: silent no-op (FR-03.2)
    }

    /// Record an explicit agent action for dedup exclusion (col-009, FR-03.3).
    ///
    /// Silently ignored if session is not registered (FR-03.3).
    pub fn record_agent_action(&self, session_id: &str, action: SessionAction) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            state.agent_actions.push(action);
        }
        // Unregistered session: silent no-op (FR-03.3)
    }

    /// Atomic drain-and-signal: acquires lock once, generates SignalOutput, removes session.
    ///
    /// If session is already cleared, returns None — caller handles (FR-04.2, AC-03).
    /// ADR-003: single lock acquisition for atomicity.
    pub fn drain_and_signal_session(
        &self,
        session_id: &str,
        hook_outcome: &str,
    ) -> Option<SignalOutput> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());

        // If session absent, already cleared — no-op (FR-04.2, AC-03)
        let state = sessions.remove(session_id)?;

        // Build signal output from the removed state (lock still held — ADR-003)
        let output = build_signal_output_from_state(state, hook_outcome);

        // Lock released here — session is gone, no race possible (ADR-003)
        Some(output)
    }

    /// Sweep stale sessions and generate signals for non-empty ones.
    ///
    /// Single lock acquisition. Sessions with last_activity_at older than
    /// STALE_SESSION_THRESHOLD_SECS are removed. Stale sessions with empty
    /// injection_history are silently evicted (FR-09.4).
    pub fn sweep_stale_sessions(&self) -> Vec<(String, SignalOutput)> {
        let now = now_secs();
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());

        let stale_ids: Vec<String> = sessions
            .iter()
            .filter(|(_, state)| {
                now.saturating_sub(state.last_activity_at) >= STALE_SESSION_THRESHOLD_SECS
            })
            .map(|(id, _)| id.clone())
            .collect();

        let mut results = Vec::new();
        for session_id in stale_ids {
            if let Some(state) = sessions.remove(&session_id) {
                // Stale sessions default to "success" outcome (orphaned — best effort)
                // If injection_history is empty: silent eviction (FR-09.4)
                if !state.injection_history.is_empty() {
                    let output = build_signal_output_from_state(state, "success");
                    results.push((session_id, output));
                }
            }
        }

        results
    }

    /// Return the number of currently tracked sessions (used in tests).
    #[cfg(any(test, feature = "test-support"))]
    pub fn session_count(&self) -> usize {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

// -- Internal helpers --

/// Build a SignalOutput from a removed SessionState.
///
/// Determines the final outcome, then collects eligible entry IDs.
/// Called while the session lock is held (ADR-003).
fn build_signal_output_from_state(state: SessionState, hook_outcome: &str) -> SignalOutput {
    // Determine outcome (FR-04.4)
    let rework_crossed = has_crossed_rework_threshold(&state);
    let final_outcome = match (hook_outcome, rework_crossed) {
        (_, true) => SessionOutcome::Rework,
        ("success", false) => SessionOutcome::Success,
        _ => SessionOutcome::Abandoned, // "", "abandoned", None, etc.
    };

    // Abandoned: no signals
    if final_outcome == SessionOutcome::Abandoned {
        return SignalOutput {
            session_id: state.session_id,
            helpful_entry_ids: Vec::new(),
            flagged_entry_ids: Vec::new(),
            final_outcome,
        };
    }

    // Build explicit-unhelpful exclusion set from agent_actions
    let explicit_unhelpful: HashSet<u64> = state
        .agent_actions
        .iter()
        .filter(|a| a.action == AgentActionType::ExplicitUnhelpful)
        .map(|a| a.entry_id)
        .collect();

    // Deduplicated injected entries
    let all_injected: HashSet<u64> = state.injection_history.iter().map(|r| r.entry_id).collect();

    // Eligible: not already signaled, not explicitly marked unhelpful
    let mut eligible: Vec<u64> = all_injected
        .into_iter()
        .filter(|id| !state.signaled_entries.contains(id))
        .filter(|id| !explicit_unhelpful.contains(id))
        .collect();
    eligible.sort_unstable(); // deterministic ordering

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

/// Check whether the session has crossed the rework threshold (ADR-002).
///
/// An edit-fail-edit cycle for a file_path is:
///   Edit(file) → Bash(had_failure=true) → Edit(file)
/// 3 such cycles for any single file_path → rework threshold crossed.
fn has_crossed_rework_threshold(state: &SessionState) -> bool {
    // Collect unique file paths from Edit/Write/MultiEdit events
    let file_paths: HashSet<&str> = state
        .rework_events
        .iter()
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

/// Current Unix timestamp in seconds.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}


#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> SessionRegistry {
        SessionRegistry::new()
    }

    fn make_rework_event(tool: &str, file: Option<&str>, failed: bool) -> ReworkEvent {
        ReworkEvent {
            tool_name: tool.to_string(),
            file_path: file.map(|s| s.to_string()),
            had_failure: failed,
            timestamp: now_secs(),
        }
    }

    // -- Session lifecycle tests --

    #[test]
    fn register_and_get_state() {
        let reg = make_registry();
        reg.register_session("s1", Some("dev".to_string()), Some("col-008".to_string()));

        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.session_id, "s1");
        assert_eq!(state.role.as_deref(), Some("dev"));
        assert_eq!(state.feature.as_deref(), Some("col-008"));
        assert!(state.injection_history.is_empty());
        assert!(state.coaccess_seen.is_empty());
        assert_eq!(state.compaction_count, 0);
        // New fields initialized
        assert!(state.signaled_entries.is_empty());
        assert!(state.rework_events.is_empty());
        assert!(state.agent_actions.is_empty());
        assert!(state.last_activity_at > 0);
    }

    #[test]
    fn register_overwrites_existing() {
        let reg = make_registry();
        reg.register_session("s1", Some("dev".to_string()), None);
        reg.record_injection("s1", &[(1, 0.8)]);

        // Overwrite: fresh state
        reg.register_session("s1", Some("architect".to_string()), None);
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.role.as_deref(), Some("architect"));
        assert!(state.injection_history.is_empty());
    }

    #[test]
    fn get_state_unknown_session() {
        let reg = make_registry();
        assert!(reg.get_state("unknown").is_none());
    }

    #[test]
    fn clear_session_removes_state() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.clear_session("s1");
        assert!(reg.get_state("s1").is_none());
    }

    #[test]
    fn clear_session_unknown_noop() {
        let reg = make_registry();
        reg.clear_session("unknown"); // Should not panic
    }

    #[test]
    fn clear_session_only_affects_target() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.register_session("s2", None, None);
        reg.clear_session("s1");
        assert!(reg.get_state("s1").is_none());
        assert!(reg.get_state("s2").is_some());
    }

    // -- Injection history tests --

    #[test]
    fn record_injection_appends() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.8), (2, 0.6)]);

        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.injection_history.len(), 2);
        assert_eq!(state.injection_history[0].entry_id, 1);
        assert!((state.injection_history[0].confidence - 0.8).abs() < f64::EPSILON);
        assert_eq!(state.injection_history[1].entry_id, 2);
    }

    #[test]
    fn record_injection_accumulates() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.8)]);
        reg.record_injection("s1", &[(2, 0.6)]);

        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.injection_history.len(), 2);
    }

    #[test]
    fn record_injection_allows_duplicates() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.8)]);
        reg.record_injection("s1", &[(1, 0.9)]);

        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.injection_history.len(), 2);
        assert_eq!(state.injection_history[0].entry_id, 1);
        assert_eq!(state.injection_history[1].entry_id, 1);
    }

    #[test]
    fn record_injection_unregistered_session_noop() {
        let reg = make_registry();
        reg.record_injection("unknown", &[(1, 0.8)]);
        assert!(reg.get_state("unknown").is_none());
    }

    #[test]
    fn record_injection_sets_timestamp() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.8)]);

        let state = reg.get_state("s1").unwrap();
        assert!(state.injection_history[0].timestamp > 0);
    }

    #[test]
    fn last_activity_at_updated_by_injection() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        let before = reg.get_state("s1").unwrap().last_activity_at;
        // last_activity_at = max(registration, injection)
        reg.record_injection("s1", &[(1, 0.8)]);
        let after = reg.get_state("s1").unwrap().last_activity_at;
        assert!(after >= before);
    }

    // -- Co-access dedup tests (replicate CoAccessDedup behavior) --

    #[test]
    fn coaccess_new_set_returns_true() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_duplicate_returns_false() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2, 3]));
        assert!(!reg.check_and_insert_coaccess("s1", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_different_set_returns_true() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2, 3]));
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2, 4]));
    }

    #[test]
    fn coaccess_different_session_returns_true() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.register_session("s2", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2, 3]));
        assert!(reg.check_and_insert_coaccess("s2", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_canonical_ordering() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[3, 1, 2]));
        // Same set in different order should be a duplicate
        assert!(!reg.check_and_insert_coaccess("s1", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_clear_resets() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2, 3]));
        reg.clear_session("s1");
        reg.register_session("s1", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2, 3]));
    }

    #[test]
    fn coaccess_clear_only_affects_target() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.register_session("s2", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2]));
        assert!(reg.check_and_insert_coaccess("s2", &[1, 2]));
        reg.clear_session("s1");
        reg.register_session("s1", None, None);
        assert!(reg.check_and_insert_coaccess("s1", &[1, 2])); // new for s1
        assert!(!reg.check_and_insert_coaccess("s2", &[1, 2])); // still dup for s2
    }

    #[test]
    fn coaccess_unregistered_session_returns_false() {
        let reg = make_registry();
        assert!(!reg.check_and_insert_coaccess("unknown", &[1, 2, 3]));
    }

    // -- Compaction count tests --

    #[test]
    fn increment_compaction() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.increment_compaction("s1");
        assert_eq!(reg.get_state("s1").unwrap().compaction_count, 1);
    }

    #[test]
    fn increment_compaction_accumulates() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.increment_compaction("s1");
        reg.increment_compaction("s1");
        assert_eq!(reg.get_state("s1").unwrap().compaction_count, 2);
    }

    #[test]
    fn increment_compaction_unregistered_noop() {
        let reg = make_registry();
        reg.increment_compaction("unknown"); // Should not panic
    }

    // -- Rework event tests --

    #[test]
    fn record_rework_event_appends() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_rework_event("s1", make_rework_event("Edit", Some("/foo.rs"), false));
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.rework_events.len(), 1);
        assert_eq!(state.rework_events[0].tool_name, "Edit");
    }

    #[test]
    fn record_rework_event_unregistered_noop() {
        let reg = make_registry();
        reg.record_rework_event("unknown", make_rework_event("Edit", None, false));
        assert!(reg.get_state("unknown").is_none());
    }

    #[test]
    fn last_activity_at_updated_by_rework_event() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        let before = reg.get_state("s1").unwrap().last_activity_at;
        let event = ReworkEvent {
            tool_name: "Edit".to_string(),
            file_path: Some("/foo.rs".to_string()),
            had_failure: false,
            timestamp: before + 100,
        };
        reg.record_rework_event("s1", event);
        let after = reg.get_state("s1").unwrap().last_activity_at;
        assert_eq!(after, before + 100);
    }

    // -- Agent action tests --

    #[test]
    fn record_agent_action_appends() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_agent_action(
            "s1",
            SessionAction {
                entry_id: 42,
                action: AgentActionType::ExplicitUnhelpful,
                timestamp: now_secs(),
            },
        );
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.agent_actions.len(), 1);
        assert_eq!(state.agent_actions[0].entry_id, 42);
    }

    #[test]
    fn record_agent_action_unregistered_noop() {
        let reg = make_registry();
        reg.record_agent_action(
            "unknown",
            SessionAction {
                entry_id: 1,
                action: AgentActionType::Correction,
                timestamp: 0,
            },
        );
        assert!(reg.get_state("unknown").is_none());
    }

    // -- drain_and_signal_session tests --

    #[test]
    fn drain_and_signal_session_success_basic() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9), (2, 0.8), (3, 0.7)]);

        let out = reg.drain_and_signal_session("s1", "success").unwrap();
        assert_eq!(out.final_outcome, SessionOutcome::Success);
        let mut ids = out.helpful_entry_ids.clone();
        ids.sort_unstable();
        assert_eq!(ids, vec![1, 2, 3]);
        assert!(out.flagged_entry_ids.is_empty());
        // Session is gone
        assert!(reg.get_state("s1").is_none());
    }

    #[test]
    fn drain_and_signal_session_idempotent() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9)]);

        let first = reg.drain_and_signal_session("s1", "success");
        assert!(first.is_some());
        // Second call: session already removed
        let second = reg.drain_and_signal_session("s1", "success");
        assert!(second.is_none());
    }

    #[test]
    fn drain_and_signal_session_abandoned() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9)]);

        let out = reg.drain_and_signal_session("s1", "abandoned").unwrap();
        assert_eq!(out.final_outcome, SessionOutcome::Abandoned);
        assert!(out.helpful_entry_ids.is_empty());
        assert!(out.flagged_entry_ids.is_empty());
    }

    #[test]
    fn drain_and_signal_session_empty_outcome_abandoned() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9)]);

        let out = reg.drain_and_signal_session("s1", "").unwrap();
        assert_eq!(out.final_outcome, SessionOutcome::Abandoned);
    }

    #[test]
    fn drain_and_signal_unknown_session_returns_none() {
        let reg = make_registry();
        assert!(reg.drain_and_signal_session("unknown", "success").is_none());
    }

    #[test]
    fn explicit_unhelpful_excluded_from_helpful() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9), (2, 0.8), (42, 0.7)]);
        reg.record_agent_action(
            "s1",
            SessionAction {
                entry_id: 42,
                action: AgentActionType::ExplicitUnhelpful,
                timestamp: now_secs(),
            },
        );

        let out = reg.drain_and_signal_session("s1", "success").unwrap();
        assert!(!out.helpful_entry_ids.contains(&42));
        assert!(out.helpful_entry_ids.contains(&1));
        assert!(out.helpful_entry_ids.contains(&2));
    }

    // -- Rework threshold tests --

    fn make_state_with_rework(events: Vec<(&str, Option<&str>, bool)>) -> SessionState {
        SessionState {
            session_id: "test".to_string(),
            role: None,
            feature: None,
            injection_history: Vec::new(),
            coaccess_seen: HashSet::new(),
            compaction_count: 0,
            signaled_entries: HashSet::new(),
            rework_events: events
                .into_iter()
                .map(|(tool, file, failed)| ReworkEvent {
                    tool_name: tool.to_string(),
                    file_path: file.map(|s| s.to_string()),
                    had_failure: failed,
                    timestamp: 0,
                })
                .collect(),
            agent_actions: Vec::new(),
            last_activity_at: 0,
        }
    }

    #[test]
    fn rework_threshold_not_crossed_zero_cycles() {
        let state = make_state_with_rework(vec![
            ("Edit", Some("/foo.rs"), false),
        ]);
        assert!(!has_crossed_rework_threshold(&state));
    }

    #[test]
    fn rework_threshold_two_cycles_not_crossed() {
        // Edit → Bash(fail) → Edit → Bash(fail) → Edit = 2 cycles
        let state = make_state_with_rework(vec![
            ("Edit", Some("/foo.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/foo.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/foo.rs"), false),
        ]);
        assert!(!has_crossed_rework_threshold(&state));
    }

    #[test]
    fn rework_threshold_three_cycles_crossed() {
        // 3 cycles = threshold
        let state = make_state_with_rework(vec![
            ("Edit", Some("/foo.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/foo.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/foo.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/foo.rs"), false),
        ]);
        assert!(has_crossed_rework_threshold(&state));
    }

    #[test]
    fn rework_threshold_no_intervening_failure() {
        // 5 edits but no failure between them — no cycles
        let state = make_state_with_rework(vec![
            ("Edit", Some("/foo.rs"), false),
            ("Edit", Some("/foo.rs"), false),
            ("Edit", Some("/foo.rs"), false),
            ("Edit", Some("/foo.rs"), false),
            ("Edit", Some("/foo.rs"), false),
        ]);
        assert!(!has_crossed_rework_threshold(&state));
    }

    #[test]
    fn rework_threshold_different_files_not_crossed() {
        // 1 cycle each for 3 different files — none individually crosses threshold
        let state = make_state_with_rework(vec![
            ("Edit", Some("/a.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/a.rs"), false),
            ("Edit", Some("/b.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/b.rs"), false),
            ("Edit", Some("/c.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/c.rs"), false),
        ]);
        assert!(!has_crossed_rework_threshold(&state));
    }

    #[test]
    fn drain_and_signal_rework_overrides_success() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9), (2, 0.8)]);

        // Add 3 edit-fail-edit cycles
        for _ in 0..3 {
            reg.record_rework_event("s1", make_rework_event("Edit", Some("/foo.rs"), false));
            reg.record_rework_event("s1", make_rework_event("Bash", None, true));
        }
        reg.record_rework_event("s1", make_rework_event("Edit", Some("/foo.rs"), false));

        // hook_outcome="success" but rework threshold crossed → Rework
        let out = reg.drain_and_signal_session("s1", "success").unwrap();
        assert_eq!(out.final_outcome, SessionOutcome::Rework);
        assert!(out.helpful_entry_ids.is_empty());
        assert!(!out.flagged_entry_ids.is_empty());
    }

    // -- sweep_stale_sessions tests --

    #[test]
    fn sweep_stale_sessions_evicts_old() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // Backdate last_activity_at to 4h+1s ago
        {
            let mut sessions = reg.sessions.lock().unwrap();
            if let Some(state) = sessions.get_mut("s1") {
                let stale_time = now_secs().saturating_sub(STALE_SESSION_THRESHOLD_SECS + 1);
                state.last_activity_at = stale_time;
                // Add an injection so it produces a signal
                state.injection_history.push(InjectionRecord {
                    entry_id: 10,
                    confidence: 0.9,
                    timestamp: stale_time,
                });
            }
        }
        let results = reg.sweep_stale_sessions();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "s1");
        assert!(reg.get_state("s1").is_none());
    }

    #[test]
    fn sweep_stale_sessions_keeps_recent() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // last_activity_at is now (just registered) — not stale
        let results = reg.sweep_stale_sessions();
        assert!(results.is_empty());
        assert!(reg.get_state("s1").is_some());
    }

    #[test]
    fn sweep_empty_session_silent_eviction() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // Backdate but leave injection_history empty
        {
            let mut sessions = reg.sessions.lock().unwrap();
            if let Some(state) = sessions.get_mut("s1") {
                state.last_activity_at = now_secs().saturating_sub(STALE_SESSION_THRESHOLD_SECS + 1);
            }
        }
        let results = reg.sweep_stale_sessions();
        // No result because injection_history is empty (FR-09.4)
        assert!(results.is_empty());
        // Session was still removed
        assert!(reg.get_state("s1").is_none());
    }
}
