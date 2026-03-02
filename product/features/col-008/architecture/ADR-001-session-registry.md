## ADR-001: Unified SessionRegistry Replacing CoAccessDedup

### Context

col-007 introduced `CoAccessDedup` — a standalone struct in `uds_listener.rs` that tracks co-access entry sets per session to prevent redundant pair writes. It uses `Mutex<HashMap<String, HashSet<Vec<u64>>>>` keyed by session_id.

col-008 needs per-session injection history (which entries were injected, with confidence scores and timestamps) for compaction defense. Future features also need per-session state: col-009 needs injection history for confidence signals, col-010 needs session metadata for persistence.

Two approaches: (A) add InjectionTracker alongside CoAccessDedup as another standalone struct, or (B) create a unified SessionRegistry that owns all per-session state.

### Decision

Create a `SessionRegistry` in a new `session.rs` module that replaces `CoAccessDedup`. The registry manages `SessionState` structs keyed by session_id. Each `SessionState` contains:

- `session_id`, `role`, `feature` (session metadata from SessionRegister)
- `injection_history: Vec<InjectionRecord>` (ordered injection log for compaction defense)
- `coaccess_seen: HashSet<Vec<u64>>` (absorbed from CoAccessDedup)
- `compaction_count: u32` (how many times this session has been compacted)

The SessionRegistry provides the same `check_and_insert_coaccess()` and `clear_session()` methods that CoAccessDedup offered, plus new methods for injection tracking and compaction state.

Thread safety: `Mutex<HashMap<String, SessionState>>`. Single lock for all session state. Contention is minimal — lock is held for microseconds per operation (insert/lookup), and hook events are serialized per-session by Claude Code.

### Consequences

**Easier:**
- Single point of session state management — no coordination between separate structs
- col-009 and col-010 extend SessionState naturally (add fields, not new structs)
- SessionClose cleanup is a single `remove()` call — no forgetting to clean up one of multiple structs
- Session metadata (role, feature) is available to all handlers that need it

**Harder:**
- col-007 code must be refactored to use SessionRegistry instead of CoAccessDedup (mechanical change, ~10 lines)
- The SessionRegistry module is a new file, adding to the crate's module structure
- Mutex contention could theoretically increase if future features add high-frequency state updates (extremely unlikely at hook event frequency of ~1/10s)
