# Pseudocode: session-registry

## Purpose

New module `session.rs` providing unified per-session state management. Replaces col-007's CoAccessDedup with a richer SessionRegistry that also tracks injection history, session metadata, and compaction count.

## New File: crates/unimatrix-server/src/session.rs

### Data Structures

```
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct InjectionRecord {
    pub entry_id: u64,
    pub confidence: f64,
    pub timestamp: u64,
}

pub(crate) struct SessionState {
    pub session_id: String,
    pub role: Option<String>,
    pub feature: Option<String>,
    pub injection_history: Vec<InjectionRecord>,
    pub coaccess_seen: HashSet<Vec<u64>>,
    pub compaction_count: u32,
}

pub(crate) struct SessionRegistry {
    sessions: Mutex<HashMap<String, SessionState>>,
}
```

### SessionRegistry Methods

```
impl SessionRegistry {
    pub fn new() -> Self:
        SessionRegistry { sessions: Mutex::new(HashMap::new()) }

    pub fn register_session(session_id, role: Option<String>, feature: Option<String>):
        lock sessions
        // Overwrite if exists (handles reconnection -- FR-02.4)
        insert new SessionState {
            session_id: session_id.to_string(),
            role,
            feature,
            injection_history: Vec::new(),
            coaccess_seen: HashSet::new(),
            compaction_count: 0,
        }

    pub fn record_injection(session_id, entries: &[(u64, f64)]):
        lock sessions
        IF session exists for session_id:
            let now = current_unix_timestamp()
            FOR (entry_id, confidence) in entries:
                push InjectionRecord { entry_id, confidence, timestamp: now }
        ELSE:
            // Silently ignore -- FR-02.10
            return

    pub fn get_state(session_id) -> Option<SessionState>:
        lock sessions
        IF session exists:
            return Some(clone of SessionState)
        ELSE:
            return None

    pub fn check_and_insert_coaccess(session_id, entry_ids: &[u64]) -> bool:
        // Same behavior as CoAccessDedup::check_and_insert
        let mut canonical = entry_ids.to_vec()
        canonical.sort_unstable()
        lock sessions
        IF session exists:
            return session.coaccess_seen.insert(canonical)
        ELSE:
            // No session registered -- return false (no recording)
            return false

    pub fn increment_compaction(session_id):
        lock sessions
        IF session exists:
            session.compaction_count += 1

    pub fn clear_session(session_id):
        lock sessions
        remove session_id from map
}
```

### Clone Implementation for SessionState

SessionState needs Clone for `get_state()` to return a snapshot. InjectionRecord needs Clone. coaccess_seen (HashSet<Vec<u64>>) is Clone. All fields are Clone-able.

```
#[derive(Clone)]
struct InjectionRecord { ... }

#[derive(Clone)]
struct SessionState { ... }
```

### Mutex Poison Recovery

All lock() calls use `.unwrap_or_else(|e| e.into_inner())` pattern (consistent with CategoryAllowlist in vnc-004, CoAccessDedup in col-007).

### lib.rs Change

Add `pub mod session;` to crates/unimatrix-server/src/lib.rs.

## Error Handling

- No Result return types -- all operations are infallible
- Unregistered session in record_injection: silent no-op (FR-02.10)
- Unregistered session in check_and_insert_coaccess: return false
- Unregistered session in increment_compaction: no-op
- Unregistered session in clear_session: no-op (HashMap::remove on missing key)
- Mutex poisoning: recover via into_inner()

## Key Test Scenarios

1. register_session creates state, get_state returns it
2. register_session overwrites existing session (reconnection)
3. record_injection appends to history
4. record_injection on unregistered session is no-op
5. get_state returns None for unknown session
6. check_and_insert_coaccess: new set returns true, duplicate returns false
7. check_and_insert_coaccess: canonical ordering (3,1,2 == 1,2,3)
8. check_and_insert_coaccess: different sessions are independent
9. check_and_insert_coaccess on unregistered session returns false
10. clear_session removes all state
11. clear_session only affects target session
12. increment_compaction increments count
13. Multiple record_injection calls accumulate history
