//! Per-session state management for the cortical implant.
//!
//! SessionRegistry replaces col-007's CoAccessDedup with a unified container
//! for all session-scoped server-side state: injection history, co-access dedup,
//! session metadata, and compaction count. See ADR-001.
//!
//! col-009 extends SessionState with rework tracking, agent action recording,
//! and implicit signal generation on session close (drain_and_signal_session).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::infra::session_transcript::{
    DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES, TranscriptBuffer, TranscriptPurgeRecord, session_key,
};

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
    // col-028 fields
    /// Entry IDs explicitly retrieved by the agent this session.
    ///
    /// Populated by `context_get` (always) and `context_lookup` (single-ID
    /// requests only — request-side cardinality, not result-set cardinality).
    /// Not populated by briefing, search, write, or mutation tools.
    /// In-memory only; reset on register_session; never persisted.
    /// First consumer: Thompson Sampling (future feature).
    pub confirmed_entries: HashSet<u64>,
    // vnc-025 fields
    /// Per-session in-memory transcript buffer (ADR-001). Arc so SessionState
    /// clones (get_state, hot paths) copy 8 bytes + refcount, never transcript
    /// bytes (AC-10). Debug derives fine: TranscriptBuffer has a manual
    /// metadata-only Debug. Lock order: registry → buffer, NEVER reverse.
    pub transcript: Arc<Mutex<TranscriptBuffer>>,
}

/// Thread-safe registry for per-session state.
///
/// Wraps `HashMap<String, SessionState>` behind a `Mutex`. Contention is
/// minimal -- lock is held for microseconds per operation, and hook events
/// are serialized per-session by Claude Code.
pub struct SessionRegistry {
    sessions: Mutex<HashMap<String, SessionState>>,
    /// Per-session transcript buffer cap in bytes (vnc-025, ADR-006).
    /// Immutable for the registry lifetime; injected into each new buffer.
    transcript_cap: usize,
}

impl SessionRegistry {
    pub fn new() -> Self {
        // Keeps the 4 MiB default — zero churn across existing test call sites (ADR-006).
        Self::with_transcript_cap(DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES)
    }

    /// Construct with an explicit per-session transcript buffer cap (vnc-025, ADR-006).
    ///
    /// Production path: `with_transcript_cap(cfg.retention.transcript_buffer_max_bytes)`.
    pub fn with_transcript_cap(max_bytes: usize) -> Self {
        SessionRegistry {
            sessions: Mutex::new(HashMap::new()),
            transcript_cap: max_bytes,
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
                confirmed_entries: HashSet::new(), // col-028: empty on session start
                // vnc-025: fresh empty buffer. Re-registration replaces the old Arc;
                // the old buffer frees on last drop — no ghost content (ADR-001).
                transcript: Arc::new(Mutex::new(TranscriptBuffer::new(self.transcript_cap))),
            },
        );
    }

    /// Merge a transcript delta into the session's buffer (vnc-025, FR-03).
    ///
    /// Silent no-op for unregistered sessions (FR-04, AC-03): no auto-registration,
    /// no slot, no allocation before the registry check. No return value —
    /// always-Ack is dispatch's job (ADR-003).
    ///
    /// Lock discipline (ADR-001 / NFR-03): registry lock does lookup + Arc clone +
    /// activity bump only; the memcpy (≤1 MiB frame ceiling) happens under the
    /// per-session buffer lock after the registry lock is released.
    pub fn apply_transcript_delta(&self, session_id: &str, offset: u64, bytes: &[u8]) {
        // Phase 1 — registry lock: lookup + Arc clone + scalar bump ONLY.
        let arc = {
            let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
            let key = session_key("default", "", session_id); // ADR-007 seam
            match sessions.get_mut(&key) {
                None => return, // silent no-op (FR-04, AC-03)
                Some(state) => {
                    state.last_activity_at = state.last_activity_at.max(now_secs());
                    Arc::clone(&state.transcript)
                }
            }
        }; // registry lock RELEASED here

        // Phase 2 — buffer lock: the memcpy happens here, never under the registry lock.
        let mut buf = lock_buffer(&arc);
        buf.apply_delta(offset, bytes);
    }

    /// Clear transcript buffers for every session attributed to `feature_cycle`
    /// (vnc-025, FR-12 — the named crt-052 seam, ADR-004).
    ///
    /// Sessions stay registered; buffers are cleared in place. Counts-only today,
    /// deliberately (crt-052 makes it take-shaped). Arcs are cloned under the
    /// registry lock and cleared after release — no deadlock with concurrent
    /// delta streams (R-06.3). Zero-byte purges produce no record (ADR-004).
    /// The caller (cycle-review-purge) emits audit — never this method.
    pub fn clear_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord> {
        // Phase 1 — registry lock: linear scan (no feature→session index; fine at OSS scale).
        let handles: Vec<(String, Arc<Mutex<TranscriptBuffer>>)> = {
            let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
            sessions
                .values()
                .filter(|s| s.feature.as_deref() == Some(feature_cycle)) // None never matches (R-10.1)
                .map(|s| (s.session_id.clone(), Arc::clone(&s.transcript)))
                .collect()
        }; // registry lock RELEASED

        // Phase 2 — per-buffer clear.
        let mut records = Vec::new();
        for (sid, arc) in handles {
            let purged = {
                let mut buf = lock_buffer(&arc);
                buf.clear()
            };
            if purged > 0 {
                records.push(TranscriptPurgeRecord {
                    session_id: session_key("default", "", &sid),
                    bytes_purged: purged,
                });
            }
        }
        records
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
            *count = count.saturating_add(1);
        }
        // Unregistered session: silent no-op (matches record_injection contract)
    }

    /// Record an entry ID as explicitly retrieved by the agent this session (col-028).
    ///
    /// Called after a successful `context_get` (always) or `context_lookup` with a
    /// single target ID (request-side cardinality). No I/O, no spawn_blocking, no
    /// await — same lock-and-mutate contract as `record_category_store`.
    /// Silent no-op for unregistered sessions. HashSet insert is idempotent.
    pub fn record_confirmed_entry(&self, session_id: &str, entry_id: u64) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(state) = sessions.get_mut(session_id) {
            state.confirmed_entries.insert(entry_id);
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
                // Returns Set even when the session is absent from the registry (no-op case).
                // Callers that need to distinguish "set on live session" from "session not
                // registered" MUST check get_state().is_none() before calling — Set here
                // is indistinguishable from a successful write.
                // GH #519: handle_cycle_event pre-registers evicted sessions before calling
                // this function so that the None arm is never reached for cycle_start events.
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

        let total_count: u32 = state
            .topic_signals
            .values()
            .map(|t| t.count)
            .fold(0u32, |acc, v| acc.saturating_add(v));

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
            tally.count = tally.count.saturating_add(1);
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
    ///
    /// vnc-025: also purges the session's transcript buffer and returns a
    /// counts-only `TranscriptPurgeRecord` when bytes were purged (ADR-004).
    /// The `SignalOutput` shape is UNTOUCHED — it feeds the persisted signal
    /// queue (Wave 0 baseline pins it).
    pub fn drain_and_signal_session(
        &self,
        session_id: &str,
        hook_outcome: &str,
    ) -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)> {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());

        // If session absent, already cleared — no-op (FR-04.2, AC-03)
        let state = sessions.remove(session_id)?;

        // vnc-025: snapshot purge metadata BEFORE state is consumed. Buffer lock
        // taken while holding the registry lock — permitted order (registry →
        // buffer), bounded work (ADR-001).
        let purge = purge_record_for(&state);

        // Build signal output from the removed state (lock still held — ADR-003)
        let output = build_signal_output_from_state(state, hook_outcome);

        // Lock released here — session is gone, no race possible (ADR-003)
        Some((output, purge))
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
    ///
    /// vnc-025: also purges transcript buffers and returns counts-only
    /// `TranscriptPurgeRecord`s for EVERY evicted session — INCLUDING
    /// silently-evicted ones (empty injection_history, no SweepResult) — or
    /// AC-08 has a hole (ADR-004 / R-08.1 named mandatory case).
    pub fn sweep_stale_sessions(&self) -> (Vec<SweepResult>, Vec<TranscriptPurgeRecord>) {
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
        let mut purges = Vec::new();
        for session_id in stale_ids {
            if let Some(state) = sessions.remove(&session_id) {
                // vnc-025: purge record for every evicted session (R-08.1).
                if let Some(rec) = purge_record_for(&state) {
                    purges.push(rec);
                }

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

        (results, purges)
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

/// Lock a session's transcript buffer with ADR-008 Layer 2 poison recovery
/// (vnc-025). A panic mid-mutation may have left `data`/`holes`/`base_offset`
/// mutually inconsistent; empty is the only state with guaranteed invariants —
/// so recovery clears the buffer (treat-as-empty) and continues.
///
/// NEVER `lock().unwrap()` on a buffer mutex (grep-able review gate).
/// Callers that need `bytes_purged` from a poisoned buffer must capture
/// `clear()`'s return inside their own recovery arm — see `purge_record_for`.
pub(crate) fn lock_buffer(arc: &Arc<Mutex<TranscriptBuffer>>) -> MutexGuard<'_, TranscriptBuffer> {
    match arc.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            let mut guard = poisoned.into_inner();
            let _ = guard.clear(); // treat-as-empty; drop the possibly-corrupt bytes
            // Un-poison so recovery happens ONCE: without this, every later
            // lock re-enters this arm and re-clears — "subsequent deltas
            // accumulate" (ADR-008 / R-06.2) would never hold.
            arc.clear_poison();
            guard
        }
    }
}

/// Snapshot-and-clear a session's transcript buffer into a counts-only purge
/// record (vnc-025, ADR-004). `clear()` (not just `len()`) so a racing reader
/// or second purge point sees 0 — guarantees "at most one non-zero audit per
/// buffer content" (sweep × cycle-review race). Returns None for empty buffers
/// (zero-byte purges emit nothing).
fn purge_record_for(state: &SessionState) -> Option<TranscriptPurgeRecord> {
    let purged = match state.transcript.lock() {
        Ok(mut buf) => buf.clear(),
        // ADR-008 Layer 2: best-effort bytes_purged from a poisoned buffer.
        Err(poisoned) => {
            let purged = poisoned.into_inner().clear();
            state.transcript.clear_poison(); // one-shot recovery (see lock_buffer)
            purged
        }
    };
    if purged == 0 {
        None
    } else {
        Some(TranscriptPurgeRecord {
            session_id: session_key("default", "", &state.session_id),
            bytes_purged: purged,
        })
    }
}

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

        let (out, _purge) = reg.drain_and_signal_session("s1", "success").unwrap();
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

        let (out, _purge) = reg.drain_and_signal_session("s1", "abandoned").unwrap();
        assert_eq!(out.final_outcome, SessionOutcome::Abandoned);
        assert!(out.helpful_entry_ids.is_empty());
        assert!(out.flagged_entry_ids.is_empty());
    }

    #[test]
    fn drain_and_signal_session_empty_outcome_abandoned() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9)]);

        let (out, _purge) = reg.drain_and_signal_session("s1", "").unwrap();
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

        let (out, _purge) = reg.drain_and_signal_session("s1", "success").unwrap();
        assert!(!out.helpful_entry_ids.contains(&42));
        assert!(out.helpful_entry_ids.contains(&1));
        assert!(out.helpful_entry_ids.contains(&2));
    }

    // -- vnc-025 Wave 0: pre-change SignalOutput baseline (ADR-004 firm constraint) --

    /// HARD GATE (registry-wiring §2, Gate 3a W3/OQ-5): `SignalOutput` content
    /// and its persisted-queue serialization must be byte-identical to the
    /// committed pre-vnc-025 baseline across the Stage 3b drain signature
    /// change (`Option<SignalOutput>` → tuple with `TranscriptPurgeRecord`).
    ///
    /// Pins, per committed fixture:
    /// - `signal_output_drain.txt`: Debug of drained `SignalOutput` for the
    ///   success / rework / abandoned outcomes (entry ids are sorted by
    ///   `build_signal_output_from_state` — deterministic).
    /// - `signal_record_wire.json`: serde-JSON of the `SignalRecord`s produced
    ///   by the exact `write_signals_to_queue` field mapping (listener.rs),
    ///   with `created_at` pinned — the shape that feeds the persisted
    ///   SIGNAL_QUEUE (ADR-004: must not change).
    #[test]
    fn test_signal_output_shape_unchanged() {
        let reg = make_registry();

        // Success outcome: three injected entries, no exclusions.
        reg.register_session("vnc025-signal-success", None, None);
        reg.record_injection("vnc025-signal-success", &[(30, 0.7), (10, 0.9), (20, 0.8)]);
        let (success, _purge) = reg
            .drain_and_signal_session("vnc025-signal-success", "success")
            .expect("success drain");

        // Rework outcome: injections + 3 edit-fail-edit cycles on one file.
        reg.register_session("vnc025-signal-rework", None, None);
        reg.record_injection("vnc025-signal-rework", &[(7, 0.9), (5, 0.8)]);
        let cycle_events: Vec<(&str, Option<&str>, bool)> = vec![
            ("Edit", Some("/foo.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/foo.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/foo.rs"), false),
            ("Bash", None, true),
            ("Edit", Some("/foo.rs"), false),
        ];
        for (tool, file, failed) in cycle_events {
            reg.record_rework_event(
                "vnc025-signal-rework",
                make_rework_event(tool, file, failed),
            );
        }
        let (rework, _purge) = reg
            .drain_and_signal_session("vnc025-signal-rework", "success")
            .expect("rework drain");
        assert_eq!(rework.final_outcome, SessionOutcome::Rework);

        // Abandoned outcome: injections present but outcome not success.
        reg.register_session("vnc025-signal-abandoned", None, None);
        reg.record_injection("vnc025-signal-abandoned", &[(1, 0.9)]);
        let (abandoned, _purge) = reg
            .drain_and_signal_session("vnc025-signal-abandoned", "abandoned")
            .expect("abandoned drain");

        let debug_doc =
            format!("success: {success:?}\nrework: {rework:?}\nabandoned: {abandoned:?}\n");
        crate::test_support::assert_matches_committed_baseline(
            "signal_output_drain.txt",
            &debug_doc,
        );

        // Persisted-queue serialization: replicate the write_signals_to_queue
        // (listener.rs) SignalOutput → SignalRecord field mapping exactly,
        // with created_at pinned for determinism (live code uses now()).
        const PINNED_CREATED_AT: u64 = 1_700_000_000;
        let map_to_record = |output: &SignalOutput| -> Option<unimatrix_store::SignalRecord> {
            let (entry_ids, signal_type, signal_source) = match output.final_outcome {
                SessionOutcome::Success if !output.helpful_entry_ids.is_empty() => (
                    output.helpful_entry_ids.clone(),
                    unimatrix_store::SignalType::Helpful,
                    unimatrix_store::SignalSource::ImplicitOutcome,
                ),
                SessionOutcome::Rework if !output.flagged_entry_ids.is_empty() => (
                    output.flagged_entry_ids.clone(),
                    unimatrix_store::SignalType::Flagged,
                    unimatrix_store::SignalSource::ImplicitRework,
                ),
                _ => return None,
            };
            Some(unimatrix_store::SignalRecord {
                signal_id: 0, // Allocated by insert_signal
                session_id: output.session_id.clone(),
                created_at: PINNED_CREATED_AT,
                entry_ids,
                signal_type,
                signal_source,
            })
        };

        let success_record = map_to_record(&success).expect("success maps to Helpful record");
        let rework_record = map_to_record(&rework).expect("rework maps to Flagged record");
        assert!(
            map_to_record(&abandoned).is_none(),
            "abandoned must produce no signal record"
        );

        let wire_doc = format!(
            "{}\n{}\n",
            serde_json::to_string(&success_record).expect("serialize success record"),
            serde_json::to_string(&rework_record).expect("serialize rework record"),
        );
        crate::test_support::assert_matches_committed_baseline(
            "signal_record_wire.json",
            &wire_doc,
        );
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
            confirmed_entries: HashSet::new(), // col-028
            transcript: Arc::new(Mutex::new(TranscriptBuffer::new(
                DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES,
            ))), // vnc-025
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
        let (out, _purge) = reg.drain_and_signal_session("s1", "success").unwrap();
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
        let (results, _purges) = reg.sweep_stale_sessions();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session_id, "s1");
        assert!(reg.get_state("s1").is_none());
    }

    #[test]
    fn sweep_stale_sessions_keeps_recent() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // last_activity_at is now (just registered) — not stale
        let (results, _purges) = reg.sweep_stale_sessions();
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
        let (results, _purges) = reg.sweep_stale_sessions();
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
        let (swept, _purges) = reg.sweep_stale_sessions();
        // Drain: s2 should be drained
        let drained = reg.drain_and_signal_session("s2", "success");

        // s1 in sweep exactly once
        assert_eq!(swept.len(), 1);
        assert_eq!(swept[0].session_id, "s1");

        // s2 in drain exactly once
        assert!(drained.is_some());
        assert_eq!(drained.unwrap().0.session_id, "s2");

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
        let (out, _purge) = reg.drain_and_signal_session("s1", "success").unwrap();
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
        let (results, _purges) = reg.sweep_stale_sessions();
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
        let (results, _purges) = reg.sweep_stale_sessions();
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
        let (results, _purges) = reg.sweep_stale_sessions();
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

    // -- col-028: confirmed_entries tests --

    /// AC-08: register_session initialises confirmed_entries as empty.
    #[test]
    fn test_register_session_confirmed_entries_starts_empty() {
        // Arrange
        let registry = SessionRegistry::new();
        // Act
        registry.register_session("sess-001", None, None);
        let state = registry.get_state("sess-001").expect("state");
        // Assert
        assert!(
            state.confirmed_entries.is_empty(),
            "confirmed_entries must be empty after register_session"
        );
    }

    /// AC-08 variant: re-registration resets confirmed_entries.
    #[test]
    fn test_re_register_session_resets_confirmed_entries() {
        // Arrange: register and populate confirmed_entries.
        let registry = SessionRegistry::new();
        registry.register_session("sess-002", None, None);
        registry.record_confirmed_entry("sess-002", 42_u64);
        // Sanity check: entry is present.
        let state = registry.get_state("sess-002").expect("state");
        assert!(state.confirmed_entries.contains(&42_u64));

        // Act: re-register same session_id.
        registry.register_session("sess-002", None, None);

        // Assert: confirmed_entries is reset to empty.
        let state = registry.get_state("sess-002").expect("state");
        assert!(
            state.confirmed_entries.is_empty(),
            "confirmed_entries must reset on re-registration"
        );
    }

    /// AC-09 / AC-10 positive arm: record_confirmed_entry inserts the entry_id.
    #[test]
    fn test_record_confirmed_entry_single_id_is_stored() {
        // Arrange
        let registry = SessionRegistry::new();
        registry.register_session("sess-003", None, None);

        // Act: record_confirmed_entry is called when context_lookup target_ids.len() == 1
        registry.record_confirmed_entry("sess-003", 100_u64);

        // Assert
        let state = registry.get_state("sess-003").expect("state");
        assert!(
            state.confirmed_entries.contains(&100_u64),
            "confirmed_entries must contain entry 100 after record_confirmed_entry"
        );
    }

    /// AC-10 negative arm: confirmed_entries not modified without explicit record call.
    #[test]
    fn test_confirmed_entries_not_modified_without_record_call() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-004", None, None);
        // No calls to record_confirmed_entry
        let state = registry.get_state("sess-004").expect("state");
        assert!(state.confirmed_entries.is_empty());
        // The multi-target guard is tested in tools-read-side.md AC-10 negative arm
    }

    /// AC-10: multiple calls accumulate; same entry is idempotent (HashSet).
    #[test]
    fn test_record_confirmed_entry_multiple_entries_accumulate() {
        let registry = SessionRegistry::new();
        registry.register_session("sess-005", None, None);
        registry.record_confirmed_entry("sess-005", 10_u64);
        registry.record_confirmed_entry("sess-005", 20_u64);
        registry.record_confirmed_entry("sess-005", 10_u64); // duplicate

        let state = registry.get_state("sess-005").expect("state");
        assert!(state.confirmed_entries.contains(&10_u64));
        assert!(state.confirmed_entries.contains(&20_u64));
        // HashSet deduplicates: len is 2, not 3
        assert_eq!(state.confirmed_entries.len(), 2);
    }

    /// EC-03: record_confirmed_entry for non-existent session is a silent no-op.
    #[test]
    fn test_record_confirmed_entry_unknown_session_is_noop() {
        let registry = SessionRegistry::new();
        // No registration for "unknown-sess"
        // Must not panic
        registry.record_confirmed_entry("unknown-sess", 99_u64);
        // No assertion needed — the test passes if no panic occurs
    }

    /// AC-20: make_state_with_rework compiles (confirmed_entries field present in helper).
    /// Verified by this test compiling and passing without "missing field" error.
    #[test]
    fn test_make_state_with_rework_includes_confirmed_entries() {
        let state = make_state_with_rework(vec![]);
        assert!(
            state.confirmed_entries.is_empty(),
            "make_state_with_rework must initialise confirmed_entries as empty HashSet"
        );
    }

    // GH #345: saturating_add prevents counter wrap at u32::MAX
    #[test]
    fn test_category_counter_saturates_at_u32_max() {
        let reg = make_registry();
        reg.register_session("s1", None, None);

        // Manually pre-set the category counter to u32::MAX via repeated stores would be
        // impractical; instead directly mutate the state via the lock.
        {
            let mut sessions = reg.sessions.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(state) = sessions.get_mut("s1") {
                state
                    .category_counts
                    .insert("overflow-cat".to_string(), u32::MAX);
            }
        }

        // One more store must NOT wrap to 0
        reg.record_category_store("s1", "overflow-cat");

        let histogram = reg.get_category_histogram("s1");
        assert_eq!(
            histogram.get("overflow-cat"),
            Some(&u32::MAX),
            "counter must saturate at u32::MAX, not wrap to 0"
        );
    }

    // -- vnc-025: registry-wiring tests (test-plan/registry-wiring.md) --

    /// Backdate a session's last_activity_at past the stale threshold.
    fn backdate_session(reg: &SessionRegistry, session_id: &str) {
        let mut sessions = reg.sessions.lock().unwrap();
        if let Some(state) = sessions.get_mut(session_id) {
            state.last_activity_at = now_secs().saturating_sub(STALE_SESSION_THRESHOLD_SECS + 1);
        }
    }

    /// Poison a session's transcript buffer mutex by panicking while holding it.
    fn poison_buffer(arc: &Arc<Mutex<TranscriptBuffer>>) {
        let thread_arc = Arc::clone(arc);
        let handle = std::thread::spawn(move || {
            let _guard = thread_arc.lock().expect("not yet poisoned");
            panic!("intentional poison (vnc-025 ADR-008 Layer 2 test)");
        });
        assert!(handle.join().is_err(), "poison thread must panic");
        assert!(arc.is_poisoned(), "buffer mutex must be poisoned");
    }

    // §1 apply_transcript_delta

    #[test]
    fn test_apply_transcript_delta_registered_merges() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        let before = reg.get_state("s1").unwrap().last_activity_at;

        reg.apply_transcript_delta("s1", 0, b"hello transcript");

        let state = reg.get_state("s1").unwrap();
        let tail = state
            .transcript
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contiguous_tail(1024);
        assert_eq!(tail.as_deref(), Some(b"hello transcript".as_slice()));
        assert!(
            state.last_activity_at >= before,
            "last_activity_at must be bumped (monotonic)"
        );
    }

    /// AC-03: unknown session — no panic, no slot created, no other session's
    /// buffer affected. Structural half (no allocation before the registry
    /// check): the byte slice is borrowed until after lookup — review gate.
    #[test]
    fn test_apply_transcript_delta_unregistered_silent_noop() {
        let reg = make_registry();
        reg.register_session("other", None, None);
        reg.apply_transcript_delta("other", 0, b"other-bytes");
        assert_eq!(reg.session_count(), 1);

        reg.apply_transcript_delta("unknown", 0, b"dropped");

        // No slot created (no auto-registration).
        assert_eq!(reg.session_count(), 1);
        assert!(reg.get_state("unknown").is_none());
        // Other session's buffer unaffected.
        let other = reg.get_state("other").unwrap();
        let tail = other
            .transcript
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contiguous_tail(1024);
        assert_eq!(tail.as_deref(), Some(b"other-bytes".as_slice()));
    }

    // test_apply_transcript_delta_no_memcpy_under_registry_lock (NFR-03) is a
    // structural review gate, not a runtime assertion: the registry lock scope
    // in apply_transcript_delta contains lookup + Arc clone + scalar bump only;
    // buf.apply_delta runs after the registry guard is dropped (ADR-001).

    // §2 Drain / sweep signature changes — R-08

    #[test]
    fn test_drain_returns_signal_and_purge_record() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9)]);
        reg.apply_transcript_delta("s1", 0, b"0123456789"); // 10 bytes

        let (output, purge) = reg.drain_and_signal_session("s1", "success").unwrap();
        assert_eq!(output.session_id, "s1");
        let rec = purge.expect("non-empty buffer must yield a purge record");
        assert_eq!(rec.session_id, "s1");
        assert_eq!(rec.bytes_purged, 10);
        // Key removed.
        assert!(reg.get_state("s1").is_none());
    }

    #[test]
    fn test_drain_empty_buffer_returns_none_record() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.record_injection("s1", &[(1, 0.9)]);

        let (output, purge) = reg.drain_and_signal_session("s1", "success").unwrap();
        assert_eq!(output.session_id, "s1");
        assert!(
            purge.is_none(),
            "zero-byte purge must emit nothing (ADR-004)"
        );
    }

    #[test]
    fn test_drain_unknown_session_returns_none() {
        let reg = make_registry();
        assert!(reg.drain_and_signal_session("unknown", "success").is_none());
    }

    #[test]
    fn test_sweep_returns_purge_records_for_stale() {
        let reg = make_registry();
        reg.register_session("stale", None, None);
        reg.record_injection("stale", &[(1, 0.9)]);
        reg.apply_transcript_delta("stale", 0, b"stale-bytes"); // 11 bytes
        backdate_session(&reg, "stale");

        reg.register_session("fresh", None, None);
        reg.apply_transcript_delta("fresh", 0, b"fresh-bytes");

        let (results, purges) = reg.sweep_stale_sessions();
        assert_eq!(results.len(), 1);
        assert_eq!(purges.len(), 1);
        assert_eq!(purges[0].session_id, "stale");
        assert_eq!(purges[0].bytes_purged, 11);

        // Fresh session untouched: still registered, buffer intact.
        let fresh = reg.get_state("fresh").unwrap();
        let tail = fresh
            .transcript
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contiguous_tail(1024);
        assert_eq!(tail.as_deref(), Some(b"fresh-bytes".as_slice()));
    }

    /// MANDATORY (R-08.1, #4140): deltas streamed, never injected (empty
    /// injection_history), idle past threshold → swept with NO SweepResult
    /// but WITH a TranscriptPurgeRecord. The audit-row half is purge-audit §1.
    #[test]
    fn test_sweep_silently_evicted_session_yields_purge_record() {
        let reg = make_registry();
        reg.register_session("silent", None, None);
        reg.apply_transcript_delta("silent", 0, b"never injected"); // 14 bytes
        backdate_session(&reg, "silent");

        let (results, purges) = reg.sweep_stale_sessions();
        assert!(
            results.is_empty(),
            "empty injection_history → silent eviction, no SweepResult (FR-09.4)"
        );
        assert_eq!(
            purges.len(),
            1,
            "silently-evicted session still purged (AC-08)"
        );
        assert_eq!(purges[0].session_id, "silent");
        assert_eq!(purges[0].bytes_purged, 14);
        assert!(reg.get_state("silent").is_none());
    }

    #[test]
    fn test_sweep_empty_buffer_session_no_purge_record() {
        let reg = make_registry();
        reg.register_session("empty", None, None);
        backdate_session(&reg, "empty");

        let (results, purges) = reg.sweep_stale_sessions();
        assert!(results.is_empty());
        assert!(
            purges.is_empty(),
            "zero-byte purges produce no record (ADR-004 suppression feeds from here)"
        );
        assert!(reg.get_state("empty").is_none());
    }

    // §3 Lock discipline + poison recovery — R-06, NFR-09 Layer 2

    #[test]
    fn test_concurrent_deltas_and_state_reads_no_deadlock() {
        let reg = Arc::new(make_registry());
        reg.register_session("hot", None, None);

        let mut handles = Vec::new();
        // N delta-streaming threads.
        for t in 0..4u64 {
            let reg = Arc::clone(&reg);
            handles.push(std::thread::spawn(move || {
                for i in 0..200u64 {
                    let offset = (t * 200 + i) * 4;
                    reg.apply_transcript_delta("hot", offset, b"abcd");
                }
            }));
        }
        // M reader threads: get_state + contiguous_tail (registry→buffer order only).
        for _ in 0..2 {
            let reg = Arc::clone(&reg);
            handles.push(std::thread::spawn(move || {
                for _ in 0..200 {
                    if let Some(state) = reg.get_state("hot") {
                        let _ = state
                            .transcript
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .contiguous_tail(4096);
                    }
                }
            }));
        }
        for h in handles {
            h.join().expect("no thread may panic or deadlock");
        }

        let state = reg.get_state("hot").unwrap();
        let len = state
            .transcript
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len();
        assert_eq!(len, 4 * 200 * 4, "all deltas merged");
    }

    /// MANDATORY (R-06.2, ADR-008 Layer 2): poison recovery at every lock-site
    /// class — merge resumes against a cleared buffer, read degrades to the
    /// empty-buffer result, purge reports best-effort bytes without panicking.
    #[test]
    fn test_poisoned_buffer_mutex_recovery() {
        // -- merge + read site --
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.apply_transcript_delta("s1", 0, b"pre-poison-content");
        let arc = Arc::clone(&reg.get_state("s1").unwrap().transcript);
        poison_buffer(&arc);

        // Read after poison: lock_buffer recovers by clearing — empty-buffer result.
        {
            let buf = lock_buffer(&arc);
            assert!(
                buf.contiguous_tail(4096).is_none(),
                "PreCompact degrades to empty"
            );
            assert_eq!(buf.len(), 0);
        }

        // Merge after poison: succeeds against the cleared buffer; subsequent
        // deltas accumulate. Post-clear floor is high_water (18) — resume there.
        reg.apply_transcript_delta("s1", 18, b"after");
        reg.apply_transcript_delta("s1", 23, b"-poison");
        let tail = lock_buffer(&arc).contiguous_tail(4096);
        assert_eq!(tail.as_deref(), Some(b"after-poison".as_slice()));

        // -- purge site (drain): best-effort bytes_purged, no panic --
        let reg2 = make_registry();
        reg2.register_session("s2", None, None);
        reg2.apply_transcript_delta("s2", 0, b"0123456789"); // 10 bytes
        let arc2 = Arc::clone(&reg2.get_state("s2").unwrap().transcript);
        poison_buffer(&arc2);
        let (_out, purge) = reg2.drain_and_signal_session("s2", "success").unwrap();
        let rec = purge.expect("best-effort purge record from poisoned buffer");
        assert_eq!(rec.bytes_purged, 10);

        // -- purge site (sweep): same contract --
        let reg3 = make_registry();
        reg3.register_session("s3", None, None);
        reg3.apply_transcript_delta("s3", 0, b"abc");
        let arc3 = Arc::clone(&reg3.get_state("s3").unwrap().transcript);
        poison_buffer(&arc3);
        backdate_session(&reg3, "s3");
        let (_results, purges) = reg3.sweep_stale_sessions();
        assert_eq!(purges.len(), 1);
        assert_eq!(purges[0].bytes_purged, 3);
    }

    #[test]
    fn test_clear_transcripts_for_feature_under_concurrent_stream() {
        let reg = Arc::new(make_registry());
        reg.register_session("hot", None, Some("vnc-025".to_string()));

        let mut handles = Vec::new();
        for t in 0..4u64 {
            let reg = Arc::clone(&reg);
            handles.push(std::thread::spawn(move || {
                for i in 0..100u64 {
                    let offset = (t * 100 + i) * 4;
                    reg.apply_transcript_delta("hot", offset, b"wxyz");
                }
            }));
        }
        // Clear while delta threads run: Arcs cloned under registry lock,
        // cleared after release — no deadlock (R-06.3).
        for _ in 0..10 {
            let _ = reg.clear_transcripts_for_feature("vnc-025");
        }
        for h in handles {
            h.join().expect("no deadlock under clear + stream");
        }

        // Post-clear merges still apply: stream past high_water (1600 sent).
        reg.apply_transcript_delta("hot", 1600, b"post-clear");
        let state = reg.get_state("hot").unwrap();
        assert!(reg.get_state("hot").is_some(), "session stays registered");
        let tail = state
            .transcript
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contiguous_tail(10);
        assert_eq!(tail.as_deref(), Some(b"post-clear".as_slice()));
    }

    /// R-10.1 matrix: Some(cycle) / Some(other) / None — only the first clears;
    /// all stay registered; counts match.
    #[test]
    fn test_clear_transcripts_for_feature_matrix() {
        let reg = make_registry();
        reg.register_session("match", None, Some("vnc-025".to_string()));
        reg.register_session("other", None, Some("col-099".to_string()));
        reg.register_session("none", None, None);
        reg.apply_transcript_delta("match", 0, b"match-bytes"); // 11
        reg.apply_transcript_delta("other", 0, b"other-bytes");
        reg.apply_transcript_delta("none", 0, b"none-bytes");

        let records = reg.clear_transcripts_for_feature("vnc-025");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id, "match");
        assert_eq!(records[0].bytes_purged, 11);

        // All stay registered (the crt-052 seam: clear in place).
        assert_eq!(reg.session_count(), 3);
        // Matched buffer is empty; others retain content.
        let matched = reg.get_state("match").unwrap();
        assert_eq!(
            matched
                .transcript
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .len(),
            0
        );
        for sid in ["other", "none"] {
            let state = reg.get_state(sid).unwrap();
            assert!(
                state
                    .transcript
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .len()
                    > 0,
                "{sid} buffer must be untouched"
            );
        }

        // Second clear of the same cycle: nothing left to purge.
        assert!(reg.clear_transcripts_for_feature("vnc-025").is_empty());
    }

    /// R-06.4: delta racing drain key removal lands in the orphaned buffer,
    /// freed on drop; re-registered same-id session gets a fresh buffer.
    #[test]
    fn test_orphaned_arc_merge_harmless() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        reg.apply_transcript_delta("s1", 0, b"ghost");
        let orphan = Arc::clone(&reg.get_state("s1").unwrap().transcript);

        // Drain removes the key.
        assert!(reg.drain_and_signal_session("s1", "success").is_some());

        // Merge into the orphan handle directly: no panic, content lands there.
        lock_buffer(&orphan).apply_delta(5, b"-late");

        // Registry-path merge for the removed id: silent no-op.
        reg.apply_transcript_delta("s1", 10, b"dropped");
        assert!(reg.get_state("s1").is_none());

        // Re-registration after drain: fresh empty buffer, no ghost content.
        reg.register_session("s1", None, None);
        let fresh = reg.get_state("s1").unwrap();
        assert!(
            !Arc::ptr_eq(&fresh.transcript, &orphan),
            "re-registration must allocate a fresh buffer"
        );
        assert_eq!(
            fresh
                .transcript
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .len(),
            0
        );
    }

    /// Edge case (pseudocode #11): sweep × cycle-review race — at most one
    /// non-zero purge record total per buffer content (clear-then-report).
    #[test]
    fn test_sweep_after_cycle_review_clear_yields_no_second_record() {
        let reg = make_registry();
        reg.register_session("s1", None, Some("vnc-025".to_string()));
        reg.apply_transcript_delta("s1", 0, b"contents");

        let first = reg.clear_transcripts_for_feature("vnc-025");
        assert_eq!(first.len(), 1);

        backdate_session(&reg, "s1");
        let (_results, purges) = reg.sweep_stale_sessions();
        assert!(
            purges.is_empty(),
            "already-cleared buffer must not produce a second non-zero record"
        );
    }

    // §4 Clone cost — AC-10, NFR-02

    #[test]
    fn test_get_state_does_not_deep_copy_transcript() {
        let reg = make_registry();
        reg.register_session("s1", None, None);
        // Fill with a large payload (1 MiB frame-ceiling sized).
        let payload = vec![b'x'; 1_048_576];
        reg.apply_transcript_delta("s1", 0, &payload);

        let live = Arc::clone(&reg.get_state("s1").unwrap().transcript);
        let count_before = Arc::strong_count(&live);
        let snapshot = reg.get_state("s1").unwrap();

        // Structural proof (ADR-001): the clone shares the buffer.
        assert!(Arc::ptr_eq(&snapshot.transcript, &live));
        assert_eq!(Arc::strong_count(&live), count_before + 1);
    }

    // §5 Constructor

    #[test]
    fn test_with_transcript_cap_propagates_to_new_sessions() {
        let cap = 131_072; // 128 KiB
        let reg = SessionRegistry::with_transcript_cap(cap);
        reg.register_session("s1", None, None);

        // Overflow at 128 KiB, not 4 MiB: send cap + 100 bytes.
        let payload = vec![b'y'; cap + 100];
        reg.apply_transcript_delta("s1", 0, &payload);

        let state = reg.get_state("s1").unwrap();
        let buf = state.transcript.lock().unwrap_or_else(|p| p.into_inner());
        assert_eq!(buf.len(), cap, "ring-tail must enforce the injected cap");
        assert_eq!(buf.elided_bytes(), 100);
    }

    #[test]
    fn test_new_defaults_to_4mib() {
        let reg = SessionRegistry::new();
        assert_eq!(reg.transcript_cap, DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES);
        assert_eq!(reg.transcript_cap, 4_194_304);
    }
}
