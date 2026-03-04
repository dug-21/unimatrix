//! In-memory session deduplication for usage tracking.
//!
//! Prevents the same agent from inflating counters by repeatedly
//! retrieving the same entry within a session. Vote tracking uses
//! last-vote-wins semantics with a HashMap to enable vote correction.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// The action to take for a vote on a specific entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteAction {
    /// First vote for this (agent, entry) pair. Increment the appropriate counter.
    NewVote,
    /// Agent is changing their vote. Decrement the old counter, increment the new one.
    CorrectedVote,
    /// Same vote value as before. No-op.
    NoOp,
}

/// Internal dedup state protected by a Mutex.
struct DedupState {
    /// (agent_id, entry_id) pairs where access_count has been incremented.
    access_counted: HashSet<(String, u64)>,
    /// (agent_id, entry_id) -> last vote value (true=helpful, false=unhelpful).
    /// Tracks the most recent vote per agent per entry. Enables last-vote-wins correction.
    vote_recorded: HashMap<(String, u64), bool>,
    /// Ordered co-access pairs recorded this session (agent-independent).
    co_access_recorded: HashSet<(u64, u64)>,
}

/// Session-scoped deduplication for usage tracking.
///
/// Tracks (agent_id, entry_id) pairs to ensure:
/// - access_count increments at most once per agent per entry per session
/// - helpful/unhelpful votes use last-vote-wins: an agent can change its vote,
///   and the old counter is decremented while the new counter is incremented
///
/// In-memory only. Cleared on server restart. Not persisted.
pub struct UsageDedup {
    inner: Mutex<DedupState>,
}

impl UsageDedup {
    /// Create a new empty dedup tracker.
    pub fn new() -> Self {
        UsageDedup {
            inner: Mutex::new(DedupState {
                access_counted: HashSet::new(),
                vote_recorded: HashMap::new(),
                co_access_recorded: HashSet::new(),
            }),
        }
    }

    /// Check which entry IDs should have access_count incremented.
    /// Returns the subset of `entry_ids` not yet counted for this agent.
    /// Marks all returned IDs as counted.
    pub fn filter_access(&self, agent_id: &str, entry_ids: &[u64]) -> Vec<u64> {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut result = Vec::new();
        for &id in entry_ids {
            let key = (agent_id.to_string(), id);
            if state.access_counted.insert(key) {
                // insert returns true if the value was NOT already present
                result.push(id);
            }
        }
        result
    }

    /// Filter co-access pairs to only those not yet recorded this session.
    /// Agent-independent: co-access is global, not per-agent.
    /// Input pairs must already be ordered (min, max).
    /// Returns subset not yet seen. Marks returned pairs as recorded.
    pub fn filter_co_access_pairs(&self, pairs: &[(u64, u64)]) -> Vec<(u64, u64)> {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut result = Vec::new();
        for &pair in pairs {
            if state.co_access_recorded.insert(pair) {
                // insert returns true if the value was NOT already present
                result.push(pair);
            }
        }
        result
    }

    /// Determine the vote action for each entry ID given a new vote value.
    /// Returns a Vec of (entry_id, VoteAction) pairs.
    ///
    /// For each entry_id:
    /// - No prior vote: returns NewVote, records the vote
    /// - Prior vote with same value: returns NoOp
    /// - Prior vote with different value: returns CorrectedVote, updates the recorded vote
    pub fn check_votes(
        &self,
        agent_id: &str,
        entry_ids: &[u64],
        helpful: bool,
    ) -> Vec<(u64, VoteAction)> {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut result = Vec::with_capacity(entry_ids.len());

        for &id in entry_ids {
            let key = (agent_id.to_string(), id);
            match state.vote_recorded.get(&key) {
                None => {
                    state.vote_recorded.insert(key, helpful);
                    result.push((id, VoteAction::NewVote));
                }
                Some(&prior) if prior == helpful => {
                    result.push((id, VoteAction::NoOp));
                }
                Some(_) => {
                    state.vote_recorded.insert(key, helpful);
                    result.push((id, VoteAction::CorrectedVote));
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- R-03: Dedup Bypass --

    #[test]
    fn test_filter_access_first_call() {
        let dedup = UsageDedup::new();
        let result = dedup.filter_access("agent-1", &[1, 2, 3]);
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn test_filter_access_second_call_empty() {
        let dedup = UsageDedup::new();
        dedup.filter_access("agent-1", &[1, 2, 3]);
        let result = dedup.filter_access("agent-1", &[1, 2, 3]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_access_per_agent() {
        let dedup = UsageDedup::new();
        let r1 = dedup.filter_access("agent-1", &[42]);
        let r2 = dedup.filter_access("agent-2", &[42]);
        assert_eq!(r1, vec![42]);
        assert_eq!(r2, vec![42]);
    }

    #[test]
    fn test_filter_access_mixed_new_and_old() {
        let dedup = UsageDedup::new();
        dedup.filter_access("agent-1", &[1, 2]);
        let result = dedup.filter_access("agent-1", &[2, 3]);
        assert_eq!(result, vec![3]);
    }

    #[test]
    fn test_filter_access_large_batch() {
        let dedup = UsageDedup::new();
        let ids: Vec<u64> = (1..=100).collect();
        let r1 = dedup.filter_access("agent-1", &ids);
        assert_eq!(r1.len(), 100);
        let r2 = dedup.filter_access("agent-1", &ids);
        assert!(r2.is_empty());
    }

    #[test]
    fn test_check_votes_first_call_new_vote() {
        let dedup = UsageDedup::new();
        let result = dedup.check_votes("agent-1", &[1, 2], true);
        assert_eq!(result, vec![(1, VoteAction::NewVote), (2, VoteAction::NewVote)]);
    }

    #[test]
    fn test_check_votes_same_value_noop() {
        let dedup = UsageDedup::new();
        dedup.check_votes("agent-1", &[1], true);
        let result = dedup.check_votes("agent-1", &[1], true);
        assert_eq!(result, vec![(1, VoteAction::NoOp)]);
    }

    #[test]
    fn test_check_votes_per_agent() {
        let dedup = UsageDedup::new();
        let r1 = dedup.check_votes("agent-1", &[42], true);
        let r2 = dedup.check_votes("agent-2", &[42], true);
        assert_eq!(r1, vec![(42, VoteAction::NewVote)]);
        assert_eq!(r2, vec![(42, VoteAction::NewVote)]);
    }

    #[test]
    fn test_filter_access_and_votes_independent() {
        let dedup = UsageDedup::new();
        let access = dedup.filter_access("agent-1", &[42]);
        let votes = dedup.check_votes("agent-1", &[42], true);
        assert_eq!(access, vec![42]);
        assert_eq!(votes, vec![(42, VoteAction::NewVote)]);
    }

    // -- R-16: Vote Correction --

    #[test]
    fn test_vote_correction_unhelpful_to_helpful() {
        let dedup = UsageDedup::new();
        let r1 = dedup.check_votes("agent-1", &[42], false);
        assert_eq!(r1, vec![(42, VoteAction::NewVote)]);

        let r2 = dedup.check_votes("agent-1", &[42], true);
        assert_eq!(r2, vec![(42, VoteAction::CorrectedVote)]);
    }

    #[test]
    fn test_vote_correction_helpful_to_unhelpful() {
        let dedup = UsageDedup::new();
        dedup.check_votes("agent-1", &[42], true);
        let result = dedup.check_votes("agent-1", &[42], false);
        assert_eq!(result, vec![(42, VoteAction::CorrectedVote)]);
    }

    #[test]
    fn test_batch_correction_mixed() {
        let dedup = UsageDedup::new();
        // Vote helpful on 5 entries
        dedup.check_votes("agent-1", &[1, 2, 3, 4, 5], true);
        // Change vote on 3 of them
        let result = dedup.check_votes("agent-1", &[1, 2, 3, 4, 5], false);
        // All 5 should be CorrectedVote (changing from true to false)
        for (_, action) in &result {
            assert_eq!(*action, VoteAction::CorrectedVote);
        }
    }

    // -- Co-access dedup (R-05, crt-004) --

    #[test]
    fn test_filter_co_access_first_call() {
        let dedup = UsageDedup::new();
        let result = dedup.filter_co_access_pairs(&[(1, 2), (3, 4)]);
        assert_eq!(result, vec![(1, 2), (3, 4)]);
    }

    #[test]
    fn test_filter_co_access_second_call_filters() {
        let dedup = UsageDedup::new();
        dedup.filter_co_access_pairs(&[(1, 2), (3, 4)]);
        let result = dedup.filter_co_access_pairs(&[(1, 2), (5, 6)]);
        assert_eq!(result, vec![(5, 6)]);
    }

    #[test]
    fn test_filter_co_access_empty_input() {
        let dedup = UsageDedup::new();
        let result = dedup.filter_co_access_pairs(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_co_access_agent_independent() {
        let dedup = UsageDedup::new();
        // Agent A accesses entries -- recorded via filter_access (per-agent)
        dedup.filter_access("agent-a", &[1, 2]);
        // Co-access pairs recorded
        dedup.filter_co_access_pairs(&[(1, 2)]);
        // Agent B tries the same co-access pair -- should be filtered (agent-independent)
        let result = dedup.filter_co_access_pairs(&[(1, 2)]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_co_access_independent_of_access_dedup() {
        let dedup = UsageDedup::new();
        // Access dedup for agent-a
        dedup.filter_access("agent-a", &[1, 2]);
        // Co-access dedup is independent
        let result = dedup.filter_co_access_pairs(&[(1, 2)]);
        assert_eq!(result, vec![(1, 2)]);
        // And access dedup still works
        let access = dedup.filter_access("agent-a", &[1, 2]);
        assert!(access.is_empty());
    }

    #[test]
    fn test_filter_co_access_concurrent() {
        use std::sync::Arc;

        let dedup = Arc::new(UsageDedup::new());
        let dedup1 = Arc::clone(&dedup);
        let dedup2 = Arc::clone(&dedup);

        let h1 = std::thread::spawn(move || dedup1.filter_co_access_pairs(&[(1, 2)]));
        let h2 = std::thread::spawn(move || dedup2.filter_co_access_pairs(&[(1, 2)]));

        let r1 = h1.join().unwrap();
        let r2 = h2.join().unwrap();

        // Exactly one thread should get the pair, the other gets empty
        let total_pairs = r1.len() + r2.len();
        assert_eq!(total_pairs, 1);
    }

    #[test]
    fn test_batch_correction_partial() {
        let dedup = UsageDedup::new();
        // Vote helpful on entries 1,2,3
        dedup.check_votes("agent-1", &[1, 2, 3], true);
        // Now vote unhelpful on entries 2,3,4 (4 is new)
        let result = dedup.check_votes("agent-1", &[2, 3, 4], false);
        assert_eq!(result, vec![
            (2, VoteAction::CorrectedVote),
            (3, VoteAction::CorrectedVote),
            (4, VoteAction::NewVote),
        ]);
    }
}
