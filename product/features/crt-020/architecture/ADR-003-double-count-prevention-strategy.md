## ADR-003: Double-Count Prevention Strategy

### Context

crt-020 introduces a second vote application path (background tick) that writes to the same
`helpful_count` / `unhelpful_count` columns as the existing real-time path. The real-time path
fires on the Stop hook:

```
Stop hook → session_registry.drain_and_signal_session()
          → write_signals_to_queue() [inserts SignalRecord into signal_queue]
          → run_confidence_consumer() [drains signal_queue, calls record_usage_with_confidence]
```

The `SignalOutput` from `drain_and_signal_session` contains `helpful_entry_ids` and
`flagged_entry_ids`. These are built from the in-memory `SessionState.injection_history` — the
set of entries registered via `record_injection()` during the session.

SR-04 from the risk assessment flags that both paths could vote on the same (session, entry) pair:
- Real-time path: votes on `helpful_entry_ids` from `SessionState` (in-memory registry)
- Background tick: votes on entries from `injection_log` (persistent store) for sessions with
  `implicit_votes_applied = 0`

Without disjoint enforcement, a session that received real-time votes would also receive
background votes on the next tick.

**Vote source analysis (proving disjointness)**:

The real-time path operates on the in-memory `SessionRegistry`. When `drain_and_signal_session`
is called, the session is *removed* from the registry. The Stop hook then:
1. Persists the session record to `sessions` table with `status = Completed` and an `outcome`
   string.
2. Writes the signal to `signal_queue`.
3. Runs `run_confidence_consumer` to drain and apply votes.

The `sessions` table write (step 1) is where `implicit_votes_applied` is set. The decision
is: the Stop hook sets `implicit_votes_applied = 1` at this write, meaning the background tick's
filter (`WHERE implicit_votes_applied = 0`) will never see sessions the real-time path handled.

The background tick processes sessions where:
- `implicit_votes_applied = 0` — real-time path has NOT marked them
- `status = Completed AND outcome IS NOT NULL` — session has closed and resolved

These are sessions that the real-time path missed entirely, which happens when:
- The session closed without a Stop hook (orphaned — swept by `sweep_stale_sessions`)
- The Stop hook fired but the server crashed after writing the session but before
  `run_confidence_consumer` completed (rare, signal_queue-drained-but-flag-not-set race, see
  below)

**Race condition analysis**:

The Stop hook persists the session (sets `implicit_votes_applied = 1`) and then writes to
`signal_queue` and drains it. If the server crashes between the session write and the
`run_confidence_consumer` drain, the `implicit_votes_applied = 1` flag is already set, so the
background tick will not retry. The signal in `signal_queue` will be drained on the next
`run_confidence_consumer` invocation. This is an existing concern with the real-time path — not
a new failure mode introduced by crt-020.

If the server crashes after writing to `signal_queue` but before `run_confidence_consumer` drains
it, the signal remains in `signal_queue` and will be consumed on the next Stop hook or tick that
calls `run_confidence_consumer`. The `implicit_votes_applied = 1` flag prevents the background
tick from double-voting.

**Conclusion**: The two paths are disjoint at the session level. The `implicit_votes_applied` flag
is the boundary mechanism.

### Decision

The double-count prevention strategy is:

1. **`implicit_votes_applied` column on `sessions`** (schema v13): Added via migration, defaults
   to `0`. Queries from the background tick filter `WHERE implicit_votes_applied = 0`.

2. **Stop hook sets the flag at session close**: When `persist_session_close` writes a `SessionRecord`
   to the `sessions` table at Stop hook time, it sets `implicit_votes_applied = 1`. This is
   the authoritative write that marks the session as handled by the real-time path.

3. **No per-entry-per-session dedup table needed**: The `implicit_votes_applied` flag operates at
   session granularity. Within a session, per-entry dedup is handled in Rust via `HashSet` over
   the injection_log rows (distinct entry_ids per session). A separate
   `implicit_vote_log(session_id, entry_id)` table (SCOPE Option A) would only be needed if the
   background tick could re-process an already-processed session — which the flag prevents.

4. **Orphaned session path** (swept sessions, sessions without Stop hooks): These sessions have
   `implicit_votes_applied = 0` and valid `outcome` from the sweep logic. The background tick
   processes them and sets the flag. The real-time `run_confidence_consumer` does not process
   these because `signal_queue` contains no signal for them (only Stop-hook-triggered sessions
   generate signals).

5. **GC interaction**: `gc_sessions` deletes sessions older than 30 days, including their
   `implicit_votes_applied` flag. This is correct: deleted sessions have no injection_log rows
   and no pending votes to apply. The implicit vote step runs after GC in each tick, so GC
   never deletes sessions mid-processing.

### Consequences

**Easier**:
- Single, auditable flag per session: the `sessions` table shows which sessions have been
  processed. No secondary dedup table to maintain.
- GC deletes both the session record and the flag atomically — no orphaned dedup rows.
- Testing is straightforward: set up a session with `implicit_votes_applied = 0`, run the tick,
  assert the flag becomes `1`.

**Harder**:
- The Stop hook write path must be updated to include `implicit_votes_applied = 1` in the
  `INSERT OR REPLACE` / `UPDATE sessions` call. This is a one-line change but must be verified
  against all code paths that write to `sessions` at session close time.
- The flag is session-level, not entry-level. If an entry appears in both a real-time Stop hook
  session AND a background-tick session (different sessions, same entry), it will receive votes
  from both — which is correct and intended (different session = different signal instance).
