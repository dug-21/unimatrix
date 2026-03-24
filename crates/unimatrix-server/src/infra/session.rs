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

// -- New types (col-017) --

/// Accumulator for topic signal votes within a session (ADR-017-002).
///
/// Tracks how many times a topic was seen and when it was last observed,
/// enabling majority vote resolution on SessionClose.
#[derive(Clone, Debug)]
pub struct TopicTally {
    /// Number of times this topic signal was observed.
    pub count: u32,
    /// Unix timestamp of the most recent observation.
    pub last_seen: u64,
}

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
#[derive(Debug)]
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

// -- col-022: Force-set attribution result --

/// Result of a `set_feature_force` operation (col-022, ADR-002).
///
/// Indicates what happened when an explicit cycle_start event
/// attempted to set the session's feature_cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetFeatureResult {
    /// Feature was None, now set.
    Set,
    /// Feature was already set to the same value.
    AlreadyMatches,
    /// Feature was set to a different value, now overwritten.
    Overridden { previous: String },
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
    pub signaled_entries: HashSet<u64>, // entries that already got an implicit signal
    pub rework_events: Vec<ReworkEvent>, // PostToolUse rework observations
    pub agent_actions: Vec<SessionAction>, // explicit MCP actions (Session Intent Registry)
    pub last_activity_at: u64,          // tracks staleness for sweep
    // col-017 fields
    pub topic_signals: HashMap<String, TopicTally>, // accumulated topic signals for majority vote
    // crt-025 fields
    pub current_phase: Option<String>, // active workflow phase; None until first phase signal
    // crt-026 fields
    /// Per-session category histogram for WA-2 histogram affinity boost.
    /// Incremented by record_category_store on each successful non-duplicate context_store.
    /// Read by get_category_histogram before context_search scoring.
    /// In-memory only: never persisted, reset on register_session (reconnection).
    pub category_counts: HashMap<String, u32>,
    // col-025 fields
    /// Goal of the active feature cycle, cached in-memory.
    ///
    /// None  — no goal provided, or pre-v16 cycle, or DB error on resume.
    /// Some  — set from context_cycle(start) payload via handle_cycle_event,
    ///         or reconstructed from cycle_events on session resume.
    pub current_goal: Option<String>,
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
                topic_signals: HashMap::new(),
                current_phase: None,
                category_counts: HashMap::new(), // crt-026: empty histogram on session start
                current_goal: None, // col-025: initialized None; populated by handle_cycle_event or resume
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

    /// Increment the category histogram counter for a session (crt-026, WA-2).
    ///
    /// Increments `category_counts[category]` by 1. Silent no-op for unregistered
    /// sessions. Lock held for one HashMap entry + integer increment (microseconds).
    /// No I/O, no spawn_blocking, no await — same lock contract as `record_injection`.
    ///
    /// Callers MUST only invoke this after a non-duplicate store succeeds
    /// (`duplicate_of.is_none()`). The duplicate guard lives in `context_store` handler.
    pub fn record_category_store(&self, session_id: &str, category: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            let count = state
                .category_counts
                .entry(category.to_string())
                .or_insert(0);
            *count += 1;
        }
        // Unregistered session: silent no-op (matches record_injection contract)
    }

    /// Return a clone of the session's category histogram (crt-026, WA-2).
    ///
    /// Returns `HashMap::new()` when the session is not registered. The caller is
    /// responsible for mapping an empty return to `None` before storing in
    /// `ServiceSearchParams.category_histogram`. Lock held for one lookup + clone
    /// (microseconds); no I/O, no spawn_blocking, no await (NFR-01).
    pub fn get_category_histogram(&self, session_id: &str) -> HashMap<String, u32> {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        match sessions.get(session_id) {
            Some(state) => state.category_counts.clone(),
            None => HashMap::new(),
        }
    }

    /// Remove all state for a session (called on SessionClose when no signals needed).
    pub fn clear_session(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.remove(session_id);
    }

    /// Set `feature` on a session if it is currently `None` (#198, Part 1).
    ///
    /// Returns `true` if the feature was set (was absent), `false` if already set
    /// or session not registered. Enables early attribution from event payloads.
    pub fn set_feature_if_absent(&self, session_id: &str, feature: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            if state.feature.is_none() {
                state.feature = Some(feature.to_string());
                return true;
            }
        }
        false
    }

    /// Unconditionally set the session's feature_cycle (col-022, ADR-002).
    ///
    /// Unlike `set_feature_if_absent`, this overwrites any existing value.
    /// Used exclusively by `cycle_start` events. All heuristic paths continue
    /// using `set_feature_if_absent`.
    pub fn set_feature_force(&self, session_id: &str, feature: &str) -> SetFeatureResult {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());

        match sessions.get_mut(session_id) {
            None => {
                // Session not registered. Return Set as no-op indicator.
                // The event is still persisted as observation by the caller.
                tracing::debug!(session_id, "set_feature_force: session not in registry");
                SetFeatureResult::Set
            }
            Some(state) => match &state.feature {
                None => {
                    state.feature = Some(feature.to_string());
                    SetFeatureResult::Set
                }
                Some(existing) if existing == feature => SetFeatureResult::AlreadyMatches,
                Some(existing) => {
                    let previous = existing.clone();
                    state.feature = Some(feature.to_string());
                    SetFeatureResult::Overridden { previous }
                }
            },
        }
    }

    /// Set the active workflow phase for a session (crt-025, ADR-001 / SR-01).
    ///
    /// Called SYNCHRONOUSLY in the UDS listener before any `spawn_blocking` DB write.
    /// Passing `None` clears the phase (used on `cycle_stop`).
    /// Silent no-op if the session is not registered.
    /// Mutex lock poisoning is recovered via `unwrap_or_else(|e| e.into_inner())`.
    pub fn set_current_phase(&self, session_id: &str, phase: Option<String>) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            state.current_phase = phase;
        }
        // Unregistered session: silent no-op
    }

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
    /// Passing `None` resets to "no goal" (equivalent to pre-col-025 state).
    pub fn set_current_goal(&self, session_id: &str, goal: Option<String>) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            state.current_goal = goal;
        }
        // Unregistered session: silent no-op (consistent with set_current_phase pattern)
    }

    /// Check if a session's leading topic signal meets the eager attribution threshold (#198, Part 2).
    ///
    /// Returns `Some(winner)` if:
    /// - The session has no `feature` set yet
    /// - The leading candidate has count >= 3
    /// - The leading candidate has > 60% share of total signal count
    ///
    /// This is a threshold-based check, not a full majority vote.
    pub fn check_eager_attribution(&self, session_id: &str) -> Option<String> {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let state = sessions.get(session_id)?;

        // Only run if feature is not yet resolved
        if state.feature.is_some() {
            return None;
        }

        if state.topic_signals.is_empty() {
            return None;
        }

        let total_count: u32 = state.topic_signals.values().map(|t| t.count).sum();

        // Find the leader
        let (leader_topic, leader_tally) =
            state.topic_signals.iter().max_by_key(|(_, t)| t.count)?;

        // Threshold: 3+ count AND >60% share
        if leader_tally.count >= 3 && (leader_tally.count as f64 / total_count as f64) > 0.6 {
            Some(leader_topic.clone())
        } else {
            None
        }
    }

    /// Record a topic signal for majority vote resolution on SessionClose (col-017).
    ///
    /// Increments the count for the topic and updates `last_seen` if the timestamp
    /// is newer. O(1) per signal. Silently ignored for unregistered sessions.
    pub fn record_topic_signal(&self, session_id: &str, signal: String, timestamp: u64) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            let tally = state.topic_signals.entry(signal).or_insert(TopicTally {
                count: 0,
                last_seen: 0,
            });
            tally.count += 1;
            if timestamp > tally.last_seen {
                tally.last_seen = timestamp;
            }
            state.last_activity_at = state.last_activity_at.max(timestamp);
        }
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
    ///
    /// (#198, Part 3): Before eviction, runs majority vote on topic_signals
    /// to resolve feature_cycle. Returns the resolved feature alongside the
    /// signal output so callers can persist it.
    pub fn sweep_stale_sessions(&self) -> Vec<SweepResult> {
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
                // (#198): Resolve feature_cycle via majority vote before eviction
                let resolved_feature =
                    majority_vote_internal(&state.topic_signals).or_else(|| state.feature.clone());

                // Stale sessions default to "success" outcome (orphaned — best effort)
                // If injection_history is empty: silent eviction (FR-09.4)
                if !state.injection_history.is_empty() {
                    let output = build_signal_output_from_state(state, "success");
                    results.push(SweepResult {
                        session_id,
                        output,
                        resolved_feature,
                    });
                }
            }
        }

        results
    }

    /// Return the number of currently tracked sessions (used in tests).
    #[cfg(any(test, feature = "test-support"))]
    pub fn session_count(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

/// Result of sweeping a single stale session (#198).
///
/// Includes the resolved feature_cycle so callers can persist it.
#[derive(Debug)]
pub struct SweepResult {
    pub session_id: String,
    pub output: SignalOutput,
    pub resolved_feature: Option<String>,
}

// -- Internal helpers --

/// Internal majority vote over topic signals (#198).
///
/// Same algorithm as listener.rs `majority_vote` but usable from session.rs.
/// Resolution rules:
/// 1. Empty → None
/// 2. Single winner by count → return it
/// 3. Tie → highest last_seen. Still tied → lexicographic smallest.
fn majority_vote_internal(signals: &HashMap<String, TopicTally>) -> Option<String> {
    if signals.is_empty() {
        return None;
    }

    let max_count = signals.values().map(|t| t.count).max().unwrap_or(0);
    let candidates: Vec<&String> = signals
        .iter()
        .filter(|(_, t)| t.count == max_count)
        .map(|(k, _)| k)
        .collect();

    if candidates.len() == 1 {
        return Some(candidates[0].clone());
    }

    // Tie: break by most recent last_seen
    let max_last_seen = candidates
        .iter()
        .map(|k| signals[*k].last_seen)
        .max()
        .unwrap_or(0);
    let recency_candidates: Vec<&String> = candidates
        .into_iter()
        .filter(|k| signals[*k].last_seen == max_last_seen)
        .collect();

    if recency_candidates.len() == 1 {
        return Some(recency_candidates[0].clone());
    }

    // Still tied: lexicographic smallest
    recency_candidates.into_iter().min().cloned()
}

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
                "Edit" | "Write" | "MultiEdit" if event.file_path.as_deref() == Some(path) => {
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
            topic_signals: HashMap::new(),
            current_phase: None,
            category_counts: HashMap::new(),
            current_goal: None,
        }
    }

    #[test]
    fn rework_threshold_not_crossed_zero_cycles() {
        let state = make_state_with_rework(vec![("Edit", Some("/foo.rs"), false)]);
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
        assert_eq!(results[0].session_id, "s1");
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
                state.last_activity_at =
                    now_secs().saturating_sub(STALE_SESSION_THRESHOLD_SECS + 1);
            }
        }
        let results = reg.sweep_stale_sessions();
        // No result because injection_history is empty (FR-09.4)
        assert!(results.is_empty());
        // Session was still removed
        assert!(reg.get_state("s1").is_none());
    }

    // -- Atomicity test (R-01): concurrent drain_and_signal + sweep --

    #[test]
    fn concurrent_drain_and_sweep_each_session_appears_in_exactly_one() {
        use std::sync::Arc;

        let reg = Arc::new(make_registry());

        // Register "s1" as stale (will be swept) with injections
        reg.register_session("s1", None, None);
        {
            let mut sessions = reg.sessions.lock().unwrap();
            if let Some(state) = sessions.get_mut("s1") {
                state.last_activity_at =
                    now_secs().saturating_sub(STALE_SESSION_THRESHOLD_SECS + 1);
                state.injection_history.push(InjectionRecord {
                    entry_id: 1,
                    confidence: 0.9,
                    timestamp: 0,
                });
            }
        }

        // Register "s2" as the closing session (won't be swept — recent)
        reg.register_session("s2", None, None);
        reg.record_injection("s2", &[(2, 0.8)]);

        // Sweep: s1 should be swept (stale)
        let swept = reg.sweep_stale_sessions();
        // Drain: s2 should be drained
        let drained = reg.drain_and_signal_session("s2", "success");

        // s1 in sweep exactly once
        assert_eq!(swept.len(), 1);
        assert_eq!(swept[0].session_id, "s1");

        // s2 in drain exactly once
        assert!(drained.is_some());
        assert_eq!(drained.unwrap().session_id, "s2");

        // Both sessions are gone
        assert!(reg.get_state("s1").is_none());
        assert!(reg.get_state("s2").is_none());

        // Neither session appears in the opposite output
        assert!(swept.iter().all(|r| r.session_id != "s2"));
    }

    // -- Empty session no signal test (R-13, AC-05) --

    #[test]
    fn empty_injection_history_success_produces_no_entry_ids() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // No injections — empty injection_history
        let out = reg.drain_and_signal_session("s1", "success").unwrap();
        assert_eq!(out.final_outcome, SessionOutcome::Success);
        assert!(out.helpful_entry_ids.is_empty());
    }

    // -- col-017: Topic signal accumulation tests (T-06) --

    #[test]
    fn record_topic_signal_single() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_topic_signal("s1", "col-017".to_string(), 1000);
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.topic_signals.len(), 1);
        let tally = &state.topic_signals["col-017"];
        assert_eq!(tally.count, 1);
        assert_eq!(tally.last_seen, 1000);
    }

    #[test]
    fn record_topic_signal_same_twice() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_topic_signal("s1", "col-017".to_string(), 1000);
        reg.record_topic_signal("s1", "col-017".to_string(), 2000);
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.topic_signals.len(), 1);
        let tally = &state.topic_signals["col-017"];
        assert_eq!(tally.count, 2);
        assert_eq!(tally.last_seen, 2000);
    }

    #[test]
    fn record_topic_signal_different_signals() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_topic_signal("s1", "col-017".to_string(), 1000);
        reg.record_topic_signal("s1", "nxs-001".to_string(), 2000);
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.topic_signals.len(), 2);
    }

    #[test]
    fn record_topic_signal_memory_bounded() {
        // 100 signals for same topic -> still 1 HashMap entry (R2)
        let reg = make_registry();
        reg.register_session("s1", None, None);
        for i in 0..100 {
            reg.record_topic_signal("s1", "col-017".to_string(), i);
        }
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.topic_signals.len(), 1);
        assert_eq!(state.topic_signals["col-017"].count, 100);
    }

    #[test]
    fn record_topic_signal_non_monotonic_timestamp() {
        // SR-5: out-of-order timestamps — last_seen stays at max
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_topic_signal("s1", "col-017".to_string(), 200);
        reg.record_topic_signal("s1", "col-017".to_string(), 100);
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.topic_signals["col-017"].last_seen, 200);
    }

    #[test]
    fn record_topic_signal_unregistered_noop() {
        let reg = make_registry();
        reg.record_topic_signal("unknown", "col-017".to_string(), 1000);
        assert!(reg.get_state("unknown").is_none());
    }

    #[test]
    fn record_topic_signal_updates_last_activity() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        let before = reg.get_state("s1").unwrap().last_activity_at;
        reg.record_topic_signal("s1", "col-017".to_string(), before + 100);
        let after = reg.get_state("s1").unwrap().last_activity_at;
        assert_eq!(after, before + 100);
    }

    // -- #198: set_feature_if_absent tests --

    #[test]
    fn test_set_feature_if_absent_sets_when_absent() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        assert!(reg.set_feature_if_absent("s1", "col-020"));
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.feature.as_deref(), Some("col-020"));
    }

    #[test]
    fn test_set_feature_if_absent_returns_false_when_already_set() {
        let reg = make_registry();
        reg.register_session("s1", None, Some("col-017".to_string()));
        assert!(!reg.set_feature_if_absent("s1", "col-020"));
        // Original feature preserved
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.feature.as_deref(), Some("col-017"));
    }

    #[test]
    fn test_set_feature_if_absent_unregistered_returns_false() {
        let reg = make_registry();
        assert!(!reg.set_feature_if_absent("unknown", "col-020"));
    }

    #[test]
    fn test_set_feature_if_absent_idempotent() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        assert!(reg.set_feature_if_absent("s1", "col-020"));
        // Second call: feature already set
        assert!(!reg.set_feature_if_absent("s1", "col-021"));
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.feature.as_deref(), Some("col-020"));
    }

    // -- col-022: set_feature_force tests --

    #[test]
    fn test_set_feature_force_sets_when_absent() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        let result = reg.set_feature_force("s1", "col-022");
        assert_eq!(result, SetFeatureResult::Set);
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    #[test]
    fn test_set_feature_force_already_matches() {
        let reg = make_registry();
        reg.register_session("s1", None, Some("col-022".to_string()));
        let result = reg.set_feature_force("s1", "col-022");
        assert_eq!(result, SetFeatureResult::AlreadyMatches);
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    #[test]
    fn test_set_feature_force_overrides_existing() {
        let reg = make_registry();
        reg.register_session("s1", None, Some("col-017".to_string()));
        let result = reg.set_feature_force("s1", "col-022");
        assert_eq!(
            result,
            SetFeatureResult::Overridden {
                previous: "col-017".to_string()
            }
        );
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    #[test]
    fn test_set_feature_force_unregistered_session() {
        let reg = make_registry();
        let result = reg.set_feature_force("unknown", "col-022");
        assert_eq!(result, SetFeatureResult::Set);
    }

    #[test]
    fn test_set_feature_force_sequential_different_topics() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.set_feature_force("s1", "col-017");
        let result = reg.set_feature_force("s1", "col-022");
        assert_eq!(
            result,
            SetFeatureResult::Overridden {
                previous: "col-017".to_string()
            }
        );
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    #[test]
    fn test_set_feature_force_preserves_heuristic_path() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // Heuristic sets feature
        assert!(reg.set_feature_if_absent("s1", "col-017"));
        // Explicit force overrides
        let result = reg.set_feature_force("s1", "col-022");
        assert_eq!(
            result,
            SetFeatureResult::Overridden {
                previous: "col-017".to_string()
            }
        );
        // Subsequent heuristic cannot override explicit
        assert!(!reg.set_feature_if_absent("s1", "col-099"));
        let state = reg.get_state("s1").unwrap();
        assert_eq!(state.feature.as_deref(), Some("col-022"));
    }

    // -- #198: check_eager_attribution tests --

    #[test]
    fn test_eager_attribution_returns_none_below_count_threshold() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // Only 2 signals (need 3)
        reg.record_topic_signal("s1", "col-020".to_string(), 100);
        reg.record_topic_signal("s1", "col-020".to_string(), 200);
        assert!(reg.check_eager_attribution("s1").is_none());
    }

    #[test]
    fn test_eager_attribution_returns_none_below_share_threshold() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // 3 signals for col-020, but 2 for col-021 = 60% share (not >60%)
        for i in 0..3 {
            reg.record_topic_signal("s1", "col-020".to_string(), i);
        }
        for i in 0..2 {
            reg.record_topic_signal("s1", "col-021".to_string(), 100 + i);
        }
        // 3/5 = 60%, need >60%
        assert!(reg.check_eager_attribution("s1").is_none());
    }

    #[test]
    fn test_eager_attribution_returns_winner_above_threshold() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // 4 signals for col-020, 1 for col-021 = 80% share
        for i in 0..4 {
            reg.record_topic_signal("s1", "col-020".to_string(), i);
        }
        reg.record_topic_signal("s1", "col-021".to_string(), 100);
        let result = reg.check_eager_attribution("s1");
        assert_eq!(result, Some("col-020".to_string()));
    }

    #[test]
    fn test_eager_attribution_returns_none_when_feature_already_set() {
        let reg = make_registry();
        reg.register_session("s1", None, Some("col-017".to_string()));
        // Even with enough signals, should return None because feature is set
        for i in 0..5 {
            reg.record_topic_signal("s1", "col-020".to_string(), i);
        }
        assert!(reg.check_eager_attribution("s1").is_none());
    }

    #[test]
    fn test_eager_attribution_returns_none_for_unregistered() {
        let reg = make_registry();
        assert!(reg.check_eager_attribution("unknown").is_none());
    }

    #[test]
    fn test_eager_attribution_returns_none_for_empty_signals() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        assert!(reg.check_eager_attribution("s1").is_none());
    }

    // -- #198: sweep_stale_sessions with majority vote --

    #[test]
    fn sweep_stale_sessions_resolves_feature_via_majority_vote() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // Backdate + add injections + topic signals
        {
            let mut sessions = reg.sessions.lock().unwrap();
            if let Some(state) = sessions.get_mut("s1") {
                let stale_time = now_secs().saturating_sub(STALE_SESSION_THRESHOLD_SECS + 1);
                state.last_activity_at = stale_time;
                state.injection_history.push(InjectionRecord {
                    entry_id: 10,
                    confidence: 0.9,
                    timestamp: stale_time,
                });
                state.topic_signals.insert(
                    "col-020".to_string(),
                    TopicTally {
                        count: 5,
                        last_seen: 1000,
                    },
                );
                state.topic_signals.insert(
                    "nxs-001".to_string(),
                    TopicTally {
                        count: 2,
                        last_seen: 900,
                    },
                );
            }
        }
        let results = reg.sweep_stale_sessions();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "s1");
        assert_eq!(results[0].resolved_feature, Some("col-020".to_string()));
    }

    #[test]
    fn sweep_stale_sessions_falls_back_to_registered_feature() {
        let reg = make_registry();
        reg.register_session("s1", None, Some("col-017".to_string()));
        {
            let mut sessions = reg.sessions.lock().unwrap();
            if let Some(state) = sessions.get_mut("s1") {
                let stale_time = now_secs().saturating_sub(STALE_SESSION_THRESHOLD_SECS + 1);
                state.last_activity_at = stale_time;
                state.injection_history.push(InjectionRecord {
                    entry_id: 10,
                    confidence: 0.9,
                    timestamp: stale_time,
                });
                // No topic signals — should fall back to registered feature
            }
        }
        let results = reg.sweep_stale_sessions();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].resolved_feature, Some("col-017".to_string()));
    }

    #[test]
    fn sweep_stale_sessions_none_feature_when_no_signals_or_registration() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        {
            let mut sessions = reg.sessions.lock().unwrap();
            if let Some(state) = sessions.get_mut("s1") {
                let stale_time = now_secs().saturating_sub(STALE_SESSION_THRESHOLD_SECS + 1);
                state.last_activity_at = stale_time;
                state.injection_history.push(InjectionRecord {
                    entry_id: 10,
                    confidence: 0.9,
                    timestamp: stale_time,
                });
            }
        }
        let results = reg.sweep_stale_sessions();
        assert_eq!(results.len(), 1);
        assert!(results[0].resolved_feature.is_none());
    }

    // -- #198: majority_vote_internal tests --

    #[test]
    fn test_majority_vote_internal_empty() {
        assert!(majority_vote_internal(&HashMap::new()).is_none());
    }

    #[test]
    fn test_majority_vote_internal_single() {
        let mut signals = HashMap::new();
        signals.insert(
            "col-020".to_string(),
            TopicTally {
                count: 3,
                last_seen: 100,
            },
        );
        assert_eq!(
            majority_vote_internal(&signals),
            Some("col-020".to_string())
        );
    }

    #[test]
    fn test_majority_vote_internal_clear_winner() {
        let mut signals = HashMap::new();
        signals.insert(
            "col-020".to_string(),
            TopicTally {
                count: 5,
                last_seen: 100,
            },
        );
        signals.insert(
            "nxs-001".to_string(),
            TopicTally {
                count: 2,
                last_seen: 200,
            },
        );
        assert_eq!(
            majority_vote_internal(&signals),
            Some("col-020".to_string())
        );
    }

    // -- crt-026: Category histogram tests --

    // T-SS-01: register_session initializes category_counts to empty (AC-01, R-04 baseline)
    #[test]
    fn test_register_session_category_counts_empty() {
        let reg = make_registry();
        reg.register_session("s1", None, None);

        let state = reg.get_state("s1").unwrap();
        assert!(state.category_counts.is_empty());
        assert_eq!(state.category_counts.len(), 0);
    }

    // T-SS-02: record_category_store increments count for registered session (AC-02, R-03)
    #[test]
    fn test_record_category_store_increments_count() {
        let reg = make_registry();
        reg.register_session("s1", None, None);

        reg.record_category_store("s1", "decision");

        let histogram = reg.get_category_histogram("s1");
        assert_eq!(histogram.get("decision"), Some(&1));
        assert_eq!(histogram.len(), 1);
    }

    // T-SS-03: multiple categories and repeated calls accumulate correctly (AC-02, R-01 fixture)
    #[test]
    fn test_record_category_store_multiple_categories() {
        let reg = make_registry();
        reg.register_session("s1", None, None);

        reg.record_category_store("s1", "decision");
        reg.record_category_store("s1", "decision");
        reg.record_category_store("s1", "decision");
        reg.record_category_store("s1", "pattern");
        reg.record_category_store("s1", "pattern");

        let histogram = reg.get_category_histogram("s1");
        assert_eq!(histogram.get("decision"), Some(&3));
        assert_eq!(histogram.get("pattern"), Some(&2));
        assert_eq!(histogram.len(), 2);
        let total: u32 = histogram.values().sum();
        assert_eq!(total, 5);
    }

    // T-SS-04: unregistered session is silent no-op — GATE BLOCKER (AC-03, R-04)
    #[test]
    fn test_record_category_store_unregistered_session_is_noop() {
        let reg = make_registry(); // no register_session called

        // Must not panic
        reg.record_category_store("nonexistent-session", "decision");

        // State unchanged — session still absent
        assert!(reg.get_state("nonexistent-session").is_none());

        // get_category_histogram returns empty for unregistered session
        let empty_map = reg.get_category_histogram("nonexistent-session");
        assert!(empty_map.is_empty());
    }

    // T-SS-05: get_category_histogram on unregistered session returns empty (AC-03, R-04)
    #[test]
    fn test_get_category_histogram_unregistered_returns_empty() {
        let reg = make_registry();

        let h = reg.get_category_histogram("no-such-session");
        assert!(h.is_empty());
    }

    // T-SS-06: histogram is isolated between sessions (AC-02, R-04)
    #[test]
    fn test_record_category_store_isolated_between_sessions() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.register_session("s2", None, None);

        reg.record_category_store("s1", "decision");
        reg.record_category_store("s1", "pattern");

        let h1 = reg.get_category_histogram("s1");
        assert_eq!(h1.get("decision"), Some(&1));
        assert_eq!(h1.get("pattern"), Some(&1));

        let h2 = reg.get_category_histogram("s2");
        assert!(h2.is_empty(), "stores for s1 must not leak into s2");
    }

    // T-SS-07: re-registration resets category_counts (AC-01, R-03 re-registration)
    #[test]
    fn test_register_session_resets_category_counts() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_category_store("s1", "decision");

        // Re-register the same session_id
        reg.register_session("s1", None, None);

        assert!(
            reg.get_category_histogram("s1").is_empty(),
            "re-registration must discard accumulated histogram"
        );
    }

    // -- col-025: current_goal tests --

    /// T-SSE-01 / AC-11: register_session initializes current_goal to None.
    #[test]
    fn test_register_session_initializes_current_goal_to_none() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        let state = reg.get_state("s1").expect("session must be registered");
        assert_eq!(state.current_goal, None);
    }

    /// T-SSE-02 (R-06 coverage): SessionState struct with current_goal: Some(...) compiles and round-trips.
    #[test]
    fn test_session_state_current_goal_field_exists() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.set_current_goal("s1", Some("implement feature goal signal".to_string()));
        let state = reg.get_state("s1").expect("session must be registered");
        assert_eq!(
            state.current_goal,
            Some("implement feature goal signal".to_string())
        );
    }

    /// T-SSE-03 / test_set_current_goal_sets_and_overwrites: set, overwrite, clear.
    #[test]
    fn test_set_current_goal_sets_and_overwrites() {
        let reg = make_registry();
        reg.register_session("s1", None, None);

        // Initial state: None
        assert_eq!(reg.get_state("s1").unwrap().current_goal, None);

        // Set to Some
        reg.set_current_goal("s1", Some("goal A".to_string()));
        assert_eq!(
            reg.get_state("s1").unwrap().current_goal,
            Some("goal A".to_string())
        );

        // Overwrite with different value
        reg.set_current_goal("s1", Some("goal B".to_string()));
        assert_eq!(
            reg.get_state("s1").unwrap().current_goal,
            Some("goal B".to_string())
        );

        // Clear back to None
        reg.set_current_goal("s1", None);
        assert_eq!(reg.get_state("s1").unwrap().current_goal, None);
    }

    /// T-SSE-04: set_current_goal on unregistered session is a silent no-op.
    #[test]
    fn test_set_current_goal_unknown_session_is_noop() {
        let reg = make_registry();
        // Must not panic
        reg.set_current_goal("nonexistent-session", Some("goal".to_string()));
        // Session was never registered — get_state returns None
        assert!(reg.get_state("nonexistent-session").is_none());
    }

    /// T-SSE-05: set_current_goal is idempotent when called twice with the same value.
    #[test]
    fn test_set_current_goal_idempotent_same_value() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.set_current_goal("s1", Some("my goal".to_string()));
        reg.set_current_goal("s1", Some("my goal".to_string()));
        assert_eq!(
            reg.get_state("s1").unwrap().current_goal,
            Some("my goal".to_string())
        );
    }
}
