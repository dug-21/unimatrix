# Pseudocode: C5 Usage Dedup

## File: crates/unimatrix-server/src/usage_dedup.rs (NEW)

### VoteAction Enum

```
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
```

### DedupState (Private)

```
struct DedupState {
    /// (agent_id, entry_id) pairs where access_count has been incremented.
    access_counted: HashSet<(String, u64)>,
    /// (agent_id, entry_id) -> last vote value (true=helpful, false=unhelpful).
    vote_recorded: HashMap<(String, u64), bool>,
}
```

### UsageDedup Struct

```
pub struct UsageDedup {
    inner: Mutex<DedupState>,
}

impl UsageDedup {
    pub fn new() -> Self {
        UsageDedup {
            inner: Mutex::new(DedupState {
                access_counted: HashSet::new(),
                vote_recorded: HashMap::new(),
            }),
        }
    }

    /// Returns entry IDs not yet access-counted for this agent.
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

    /// Determine the vote action for each entry ID given a new vote value.
    /// Returns a Vec of (entry_id, VoteAction) pairs.
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
                    // First vote for this pair
                    state.vote_recorded.insert(key, helpful);
                    result.push((id, VoteAction::NewVote));
                }
                Some(&prior) if prior == helpful => {
                    // Same vote value -- no-op
                    result.push((id, VoteAction::NoOp));
                }
                Some(_) => {
                    // Different vote value -- correction
                    state.vote_recorded.insert(key, helpful);
                    result.push((id, VoteAction::CorrectedVote));
                }
            }
        }

        result
    }
}
```

Notes:
- Mutex::lock unwrap_or_else handles poison recovery (FM-03)
- filter_access atomically checks-and-marks in a single lock acquisition
- check_votes atomically checks-and-updates in a single lock acquisition

## File: crates/unimatrix-server/src/lib.rs

Add module declaration:
```
pub mod usage_dedup;
```
