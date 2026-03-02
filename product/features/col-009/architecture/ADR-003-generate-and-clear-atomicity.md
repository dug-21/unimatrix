## ADR-003: generate_signals + clear_session Atomicity via Single Lock Acquisition

### Context

SR-07 (Scope Risk Assessment) identified a race condition risk: if `generate_signals()` and `clear_session()` are separate lock acquisitions on `SessionRegistry`, a concurrent stale session sweep could clear a session between signal generation and the session's deletion, causing lost signals or double-processing.

The scenario:
1. Thread A (SessionClose): `generate_signals("s1")` → acquires lock, reads injection_history, releases lock
2. Thread B (stale sweep): `sweep_stale_sessions()` → acquires lock, finds "s1" expired, generates signals for it, removes it
3. Thread A: `clear_session("s1")` → no-op (already cleared), but signals have now been generated twice

The `signaled_entries: HashSet<u64>` dedup set prevents double-writing Helpful signals per entry per session, but only if it survives to be consulted by both calls. If clear_session runs between them, the dedup set is lost.

Three designs were considered:
- **Option A**: Two separate lock acquisitions (generate, then clear). Race window exists. Dedup prevents duplicate signals only if session is still in registry.
- **Option B**: Single `drain_and_clear_session(session_id)` method that holds the lock for the entire generate + clear sequence. No race window. Simpler to reason about.
- **Option C**: Optimistic approach — mark session as "signaling in progress" flag, release lock, generate, reacquire, clear. More complex, no benefit over Option B given that lock hold time is microseconds.

### Decision

`SessionRegistry` exposes a single atomic method `drain_and_signal_session(session_id, hook_outcome) -> Option<SignalOutput>` that:
1. Acquires the `Mutex<HashMap<String, SessionState>>` lock once
2. Looks up the session — if absent (already cleared by sweep), returns `None`
3. Evaluates rework threshold, applies dedup, constructs `SignalOutput`
4. Marks `signaled_entries` in the session as complete (sets a `signals_generated: bool` flag)
5. Removes the session from the map
6. Releases the lock
7. Returns `SignalOutput`

The stale sweep method `sweep_stale_sessions()` uses the same single-lock pattern: acquires lock once, collects all stale sessions, generates their `SignalOutput`s, removes them from the map, releases lock.

Because both operations remove the session from the registry in the same lock scope as signal generation, there is no window for double-processing.

### Consequences

- The Mutex is held for slightly longer per operation (a few microseconds for the threshold evaluation and dedup set construction). This is acceptable — hook events arrive serially from Claude Code's perspective.
- Easier: the race condition is impossible by construction. No test can exercise it because the window no longer exists.
- Harder: the `SessionRegistry` Mutex does slightly more work per acquisition. For a session with 100 injected entries, the dedup HashSet construction is O(100) while holding the lock — still sub-millisecond.
- The `signals_generated: bool` flag on `SessionState` is redundant (session is deleted) but preserved as documentation of design intent in comments.
