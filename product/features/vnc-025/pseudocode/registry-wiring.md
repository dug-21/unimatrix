# Pseudocode: registry-wiring (`infra/session.rs` — MODIFIED, thin wiring only)

ADRs: ADR-001 (field shape + lock discipline), ADR-004 (purge returns), ADR-006 (cap injection),
ADR-007 (key seam), ADR-008 (poison recovery). FRs: FR-03, FR-04, FR-12, FR-15, NFR-02, NFR-03.

## Purpose

Thread the `TranscriptBuffer` through `SessionState`/`SessionRegistry`: new field, new ctor,
ingest method, feature-scoped clear, and purge-record returns from drain/sweep. `session.rs`
is over the 500-line cap — every addition here is thin wiring; all buffer logic lives in
`session_transcript.rs`.

## Imports / Module Registration

- `infra/mod.rs`: register `pub mod session_transcript;`
- `session.rs`: `use crate::infra::session_transcript::{session_key, TranscriptBuffer, TranscriptPurgeRecord};`
- `use std::sync::Arc;` (Mutex already imported).

## Shared Poison-Recovery Helper (ADR-008 Layer 2 — used at EVERY buffer lock site)

One private helper in `session.rs` (dispatch-wiring's PreCompact site replicates the same
pattern or calls a shared `pub(crate)` version — implementer's choice, but the behavior is
pinned):

```
fn lock_buffer(arc: &Arc<Mutex<TranscriptBuffer>>) -> MutexGuard<TranscriptBuffer> {
    match arc.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            // ADR-008 Layer 2: a panic mid-mutation may have left data/holes/base_offset
            // mutually inconsistent; empty is the only state with guaranteed invariants.
            let mut guard = poisoned.into_inner();
            let _ = guard.clear();          // treat-as-empty; drop the possibly-corrupt bytes
            guard
        }
    }
}
// NEVER lock().unwrap() on a buffer mutex (grep-able review gate).
```

Caveat: callers that need `bytes_purged` from a poisoned buffer must capture `clear()`'s
return inside the recovery arm instead of discarding it — see `purge_record_for` below.

## Struct Changes

### `SessionState` (field added; `derive(Clone, Debug)` stays — ADR-001)

```
pub struct SessionState {
    ... existing 15 fields unchanged ...
    // vnc-025: per-session in-memory transcript buffer (ADR-001). Arc so SessionState
    // clones (get_state, hot paths) copy 8 bytes + refcount, never transcript bytes
    // (AC-10). Debug derives fine: TranscriptBuffer has a manual metadata-only Debug.
    pub transcript: Arc<Mutex<TranscriptBuffer>>,
}
```

### `SessionRegistry` (cap stored once, injected per buffer — ADR-006)

```
pub struct SessionRegistry {
    sessions: Mutex<HashMap<String, SessionState>>,
    transcript_cap: usize,          // immutable per registry lifetime
}
```

## Functions

### Constructors

```
pub fn new() -> Self {
    // keeps the 4 MiB default — zero churn across existing test call sites (ADR-006)
    Self::with_transcript_cap(DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES)
}

pub fn with_transcript_cap(max_bytes: usize) -> Self {
    SessionRegistry { sessions: Mutex::new(HashMap::new()), transcript_cap: max_bytes }
}
```

### `register_session` (existing — one line added)

In the `SessionState { ... }` literal:

```
transcript: Arc::new(Mutex::new(TranscriptBuffer::new(self.transcript_cap))),
```

Note: `register_session` overwrites on reconnection — the old Arc is replaced, the old buffer
frees on last drop, the session gets a FRESH EMPTY buffer (pins the "re-registration after
drain: fresh empty buffer, no ghost content" edge case). See OVERVIEW open question on
mid-session re-registration (cycle_start path).

### `pub fn apply_transcript_delta(&self, session_id: &str, offset: u64, bytes: &[u8])` (NEW)

Silent no-op for unregistered sessions (FR-04 — no auto-registration, no slot, NO allocation
before the registry check). No return value (always-Ack is dispatch's job).

```
// Phase 1 — registry lock: lookup + Arc clone + activity bump ONLY (microseconds, ADR-001)
let arc = {
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());   // registry
                                                            // mutex keeps its existing idiom
    let key = session_key("default", "", session_id);        // ADR-007 seam
    match sessions.get_mut(&key) {
        None => return,                                       // silent no-op (FR-04, AC-03)
        Some(state) => {
            state.last_activity_at = now_secs();
            Arc::clone(&state.transcript)
        }
    }
};  // registry lock RELEASED here

// Phase 2 — buffer lock: the memcpy (≤1 MiB frame ceiling) happens here, never under
// the registry lock (ADR-001 / NFR-03)
let mut buf = lock_buffer(&arc);
buf.apply_delta(offset, bytes);
```

### `pub fn clear_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord>` (NEW — the named crt-052 seam)

Sessions stay registered; buffers cleared in place. Counts-only today, deliberately
(ADR-004 — crt-052 makes it take-shaped). Arcs cloned under the registry lock, cleared after
release (no deadlock with concurrent delta streams — R-06.3).

```
// Phase 1 — registry lock: linear scan (no feature→session index exists; fine at OSS scale)
let handles: Vec<(String, Arc<Mutex<TranscriptBuffer>>)> = {
    let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
    sessions.values()
        .filter(|s| s.feature.as_deref() == Some(feature_cycle))   // None never matches (R-10.1)
        .map(|s| (s.session_id.clone(), Arc::clone(&s.transcript)))
        .collect()
};  // registry lock RELEASED

// Phase 2 — per-buffer clear
let mut records = Vec::new();
for (sid, arc) in handles {
    let purged = { let mut buf = lock_buffer(&arc); buf.clear() };
    if purged > 0 {
        records.push(TranscriptPurgeRecord {
            session_id: session_key("default", "", &sid),
            bytes_purged: purged,
        });
    }
}
records          // caller (cycle-review-purge) emits audit — never this method
```

### `drain_and_signal_session` (MODIFIED signature)

```
pub fn drain_and_signal_session(&self, session_id: &str, hook_outcome: &str)
    -> Option<(SignalOutput, Option<TranscriptPurgeRecord>)>
{
    let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
    let state = sessions.remove(session_id)?;            // existing key handling unchanged

    // vnc-025: snapshot purge metadata BEFORE state is consumed. Buffer lock taken while
    // holding the registry lock — permitted order (registry → buffer), bounded work.
    let purge = purge_record_for(&state);

    let output = build_signal_output_from_state(state, hook_outcome);   // unchanged
    Some((output, purge))
    // SignalOutput shape UNTOUCHED (feeds the persisted signal queue — ADR-004 firm)
}
```

### private `fn purge_record_for(state: &SessionState) -> Option<TranscriptPurgeRecord>`

```
let arc = Arc::clone(&state.transcript);
let purged = match arc.lock() {
    Ok(mut buf) => buf.clear(),
    Err(poisoned) => poisoned.into_inner().clear(),   // best-effort bytes_purged (ADR-008)
};
// clear() (not just len()) so a racing reader/second purge point sees 0 — guarantees
// "at most one non-zero audit per buffer content" (edge case: sweep × cycle-review race)
if purged == 0 { None } else {
    Some(TranscriptPurgeRecord {
        session_id: session_key("default", "", &state.session_id),
        bytes_purged: purged,
    })
}
```

### `sweep_stale_sessions` (MODIFIED signature)

```
pub fn sweep_stale_sessions(&self) -> (Vec<SweepResult>, Vec<TranscriptPurgeRecord>) {
    ... existing stale-id collection unchanged ...
    let mut results = Vec::new();
    let mut purges = Vec::new();
    for session_id in stale_ids {
        if let Some(state) = sessions.remove(&session_id) {
            // vnc-025: purge record for EVERY evicted session — INCLUDING silently-evicted
            // ones (empty injection_history, no SweepResult) — or AC-08 has a hole
            // (ADR-004 / R-08.1 named mandatory case).
            if let Some(rec) = purge_record_for(&state) { purges.push(rec); }

            ... existing majority-vote + SweepResult logic unchanged
                (the `if !state.injection_history.is_empty()` branch stays as-is) ...
        }
    }
    (results, purges)
}
```

## Error Handling

- Registry mutex: keep the existing `unwrap_or_else(|e| e.into_inner())` idiom (registry state
  is scalar bookkeeping; existing recovery posture unchanged).
- Buffer mutex: poison → `into_inner()` + `clear()` everywhere (ADR-008 Layer 2). Merge after
  recovery applies against an empty buffer; purge after recovery reports best-effort count.
- No method here returns a `Result` and none can carry transcript bytes.

## Key Test Scenarios (R-06, R-08, R-10, AC-03, AC-10)

1. `apply_transcript_delta` for unregistered session: registry size unchanged, other buffers
   unchanged, no allocation (AC-03).
2. Clone-cost guard (AC-10): `get_state()` clone shares the buffer (Arc::ptr_eq) — structural
   proof the transcript never rides wholesale clones.
3. Concurrency smoke: N delta-streaming tasks + M `get_state()`/tail readers on one session —
   no deadlock (registry→buffer order only).
4. Poisoned buffer mutex: merge resumes empty, purge reports best-effort, no panic propagates.
5. Drain shapes at unit level: `Some((out, Some(rec)))` / `Some((out, None))` (empty buffer) /
   `None` (unknown session).
6. Silently-evicted session (MANDATORY, R-08.1): deltas streamed, never injected, idle past
   4 h → no `SweepResult` but a `TranscriptPurgeRecord` present.
7. `clear_transcripts_for_feature` matrix: `Some(cycle)` / `Some(other)` / `None` features —
   only the first clears; all stay registered; counts match (R-10.1).
8. `clear_transcripts_for_feature` under concurrent delta stream: no deadlock; post-clear
   merges apply (R-06.3).
9. Orphaned-Arc merge (delta racing drain key removal): lands in orphaned buffer, freed on
   drop, re-registered same-id session unaffected (R-06.4).
10. Re-registration after drain: fresh empty buffer.
11. Sweep × cycle-review race on the same session: at most one non-zero purge record total.
