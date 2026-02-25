# Pseudocode: C2 -- Session Dedup Extension

## Crate: unimatrix-server

### usage_dedup.rs changes

```rust
struct DedupState {
    access_counted: HashSet<(String, u64)>,
    vote_recorded: HashMap<(String, u64), bool>,
    co_access_recorded: HashSet<(u64, u64)>,  // NEW: ordered pairs, agent-independent
}

impl UsageDedup {
    pub fn new() -> Self {
        UsageDedup {
            inner: Mutex::new(DedupState {
                access_counted: HashSet::new(),
                vote_recorded: HashMap::new(),
                co_access_recorded: HashSet::new(),  // NEW
            }),
        }
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
}
```

Key design notes:
- Co-access dedup is agent-independent (SCOPE non-goal: no per-agent profiles)
- Pairs must be pre-ordered (min, max) before passing to filter
- Same Mutex pattern as existing filter_access and check_votes
- Poison recovery via unwrap_or_else
