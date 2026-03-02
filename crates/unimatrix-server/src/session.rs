//! Per-session state management for the cortical implant.
//!
//! SessionRegistry replaces col-007's CoAccessDedup with a unified container
//! for all session-scoped server-side state: injection history, co-access dedup,
//! session metadata, and compaction count. See ADR-001.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

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
/// dedup sets (absorbed from CoAccessDedup), and compaction count.
#[derive(Clone, Debug)]
pub struct SessionState {
    pub session_id: String,
    pub role: Option<String>,
    pub feature: Option<String>,
    pub injection_history: Vec<InjectionRecord>,
    pub coaccess_seen: HashSet<Vec<u64>>,
    pub compaction_count: u32,
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
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            for &(entry_id, confidence) in entries {
                state.injection_history.push(InjectionRecord {
                    entry_id,
                    confidence,
                    timestamp: now,
                });
            }
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

    /// Remove all state for a session (called on SessionClose).
    pub fn clear_session(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.remove(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_registry() -> SessionRegistry {
        SessionRegistry::new()
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
}
