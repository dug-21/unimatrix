# C8 — Held-Buffer Store (Option B)

**Target source:** `unimatrix-server/src/infra/transcript_hold.rs` (NEW) + MINIMAL diffs to
`infra/session.rs` (`drain_and_signal_session`, `sweep_stale_sessions`) and `listener.rs` (delta route)
**Wave:** **B — the ONLY component that references / is referenced for `transcript_hold.rs`.**
**ADRs:** ADR-008 (held store), ADR-009 (audit move + staging), ADR-001 §4 (seam scans held).
**Risks:** R-01, R-02, R-03, R-05, R-16, R-17. **AC:** AC-11 (PRE-MERGE PRIMARY-PATH PROOF — hard gate).
**Constraints:** 11 (no wire change), 13 (cite, don't rework vnc-030 §2). **Patterns:** #4799, #981.
**Sequencing:** LAST. Layered on a Wave A that is correct without it.

## Purpose

A bounded server-only structure that keeps multi-turn buffers alive across the per-turn drain (#4799),
so the primary distillation path is non-empty. Buffers survive `drain_and_signal_session`, keep merging
deltas, re-adopt on re-registration (loud on mismatch), evict on cap/TTL, and purge at review/sweep.
This is the dominant risk surface (R-01/R-02/R-03/R-05). No wire change (Constraint 11).

## State & Types

```
struct HeldBuffer {
    arc:              Arc<Mutex<TranscriptBuffer>>
    feature_cycle:    String          // the contract-attributed cycle this buffer is bound to (SR-02)
    last_activity_at: Instant         // updated on delta merge; basis for TTL + cap-eviction ordering
}
// metadata-only Debug (R-19): print feature_cycle, last_activity_at, NEVER the buffer bytes.

struct TranscriptHold {
    held: Mutex<HashMap<SessionId, HeldBuffer>>   // O(1) keyed by session_id (R-17 — no linear scan on hot path)
    max_sessions: usize                           // from cfg.transcript_hold_max_sessions (C9)
}
```

Owned by the `SessionRegistry`. The registry exposes it to C1 via the optional/severable handle
(OVERVIEW Wave A/B map).

## State Machine (per session)

```
        SessionRegister
   ┌─────────────────────────► REGISTERED (live buffer in registry)
   │                                  │ Stop -> SessionClose -> drain_and_signal_session
   │                                  ▼
   │   readopt (cycle MATCH)     hold_on_drain
   │◄──────────────────────────  HELD (in TranscriptHold; keeps merging deltas)
   │                                  │
   │   readopt (cycle MISMATCH/NULL)  │ ── cap-hit eviction (oldest last_activity first) ─┐
   │   => FAIL LOUD: drop held,       │ ── TTL sweep (last_activity > ttl) ───────────────┤
   │      diagnostic, treat fresh     │ ── purge_held_for_feature (cycle review, post-distill)
   │                                  ▼                                                    │
   │                              PURGED  ◄──────────────────────────────────────────────┘
   │                            (audit fires EXACTLY ONCE here — R-03)
```

## Functions (ARCH §4 — binding signatures)

### `hold_on_drain` — called from `drain_and_signal_session` (minimal diff, Constraint 13)

```
fn hold_on_drain(&self, session_id: &str, arc: Arc<Mutex<TranscriptBuffer>>, feature_cycle: &str):
    // 3-arg form RATIFIED at Gate 3a (ADR-008 + ARCH §4 updated). feature_cycle is REQUIRED for SR-02
    //   loud re-adoption; the binding ARCH §4 row now carries it.
    guard = self.held.lock()
    if feature_cycle is None/empty:
        // a buffer with no attributed cycle can never be loud-re-adopted safely (#981) — do NOT hold it;
        // let the existing drain free it (degrades to Wave A fallback for that session).
        return
    guard.insert(session_id, HeldBuffer { arc, feature_cycle, last_activity_at: now() })
    enforce_cap(&mut guard)        // cap-hit eviction (below) — runs on every insert
```

`drain_and_signal_session` diff (cite vnc-030 ADR-007 §2 #4819, do NOT rework precedence): instead of
freeing the buffer, hand its `Arc` to `hold_on_drain(session_id, arc, feature_cycle)`. Everything else
in drain keeps its vnc-030-shipped shape. The per-close `transcript_session_purged` audit MOVES off
drain (ADR-009) — drain no longer emits it for held buffers.

### `readopt` — called on `SessionRegister` (R-01 / SR-02 — LOUD)

```
fn readopt(&self, session_id: &str, registering_feature_cycle: &str)
    -> Option<Arc<Mutex<TranscriptBuffer>>>:
    // 2-arg form RATIFIED at Gate 3a (ADR-008 + ARCH §4 updated; supersedes the earlier 1-arg
    //   readopt(session_id)). SR-02 requires the cycle to match, so the caller MUST pass the
    //   re-registering cycle.
    guard = self.held.lock()
    held = guard.get(session_id)?
    if held.feature_cycle == registering_feature_cycle:
        arc = guard.remove(session_id).arc          // re-adopt: hand the live buffer back to the registry
        return Some(arc)                            // registry rebinds it; deltas now route to registry again
    else:
        // MISMATCH or NULL/None cycle => FAIL LOUD (R-01, cite #981):
        guard.remove(session_id)                    // drop the held buffer; treat re-register as FRESH
        emit_diagnostic_metadata_only(session_id, held.feature_cycle, registering_feature_cycle)
        // NO content re-adopted under the wrong cycle. The dropped buffer's audit fires here as a
        //   terminal purge (trigger=readopt_mismatch or reuse stale_sweep semantics — see audit below).
        return None
```

Re-adoption rebinds ONLY on `feature_cycle` match (contract-attributed, vnc-030 §2 makes a declared
cycle un-flippable). Mismatch / NULL never silently re-adopts (the #981 failure mode). Diagnostic is
metadata-only (no content — R-04).

### `sweep_expired` — called from `sweep_stale_sessions` (independent TTL — R-02)

```
fn sweep_expired(&self, ttl: Duration) -> Vec<TranscriptPurgeRecord>:
    guard = self.held.lock()
    expired = guard.entries where now() - last_activity_at > ttl
    records = []
    for (session_id, held) in expired:
        guard.remove(session_id)
        records.push(audit_record(session_id, held, trigger = stale_sweep))   // exactly-once at sweep
    return records
```

Reclamation does NOT depend on cycle review ever firing (SR-01). The existing `sweep_stale_sessions`
path calls this with `cfg.transcript_hold_ttl_secs`.

### `purge_held_for_feature` — called post-distill at cycle review (R-03 / R-13)

```
fn purge_held_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord>:
    guard = self.held.lock()
    matching = guard.entries where feature_cycle == feature_cycle
    records = []
    for (session_id, held) in matching:
        guard.remove(session_id)
        records.push(audit_record(session_id, held, trigger = cycle_review))   // exactly-once at review
    return records
```

Called from `purge_cycle_transcripts` (C7) AFTER distill, alongside the existing
`clear_transcripts_for_feature` (registered buffers). Congruent with C1's registered ∪ held scan (R-13).

### `arcs_for_feature` — read-only scan for the seam (C1 phase 1, ADR-001 §4)

```
fn arcs_for_feature(&self, feature_cycle: &str) -> Vec<(String, Arc<Mutex<TranscriptBuffer>>)>:
    guard = self.held.lock()
    return guard.entries where feature_cycle == feature_cycle, mapped to (session_id, Arc::clone(arc))
    // Arc-clone only, under the hold lock; no buffer lock, no parse (lock discipline).
```

### `enforce_cap` — cap-hit eviction (R-02 / R-16, never silent)

```
fn enforce_cap(&self, guard: &mut HashMap):
    while guard.len() > self.max_sessions:
        victim = entry with OLDEST last_activity_at        // oldest-last-activity-first
        held = guard.remove(victim.session_id)
        emit_audit(victim.session_id, held, trigger = cap_evict)   // EVERY eviction emits audit (R-16)
```

Memory bound = `buffer_cap × max_sessions` regardless of review cadence (SR-01). Both mechanisms (cap,
TTL) are independent — either alone bounds memory; both run.

## Delta routing while held (R-17 — hot-path lock discipline)

`listener.rs` delta route (minimal diff): on `apply_transcript_delta`, if the session is not in the
registry but IS in the hold, route the delta to the held `Arc`'s buffer. Lookup MUST be O(1) keyed by
`session_id` (the hold's `HashMap`), NOT a linear scan, and the merge happens under the BUFFER lock only
(vnc-025 ADR-001 discipline), updating `last_activity_at`. The batch filter (`listener.rs:1238`,
two-pipe boundary) is UNCHANGED — held deltas are still transcript deltas, still kept out of
`insert_observations_batch` (AC-09).

## Audit Shape (ADR-009 / R-03 / SR-03)

- Event SHAPE unchanged: `transcript_session_purged`, content-free
  `detail = "bytes=<n> trigger=<cycle_review|stale_sweep|cap_evict|readopt_mismatch>"`, never content
  (R-04).
- Fires EXACTLY ONCE per held session, at its terminal purge (review / sweep / evict / readopt-mismatch
  drop). The per-turn `session_close` trigger goes away for held buffers (they no longer purge at drain).
- **PREREQUISITE GATE (ADR-009 / brief / Coverage Gap 1):** before this audit move lands, the named
  no-consumer survey must be recorded clean — `gc_audit_log` (crt-036), retention/analytics readers,
  per-close-emission tests confirm NO downstream consumer keys on per-close cadence. The move does not
  merge until the survey is clean. Flagged in report.

## Wave A/B Boundary (R-11)

This is the ONLY file Wave A must not reference. C1's held-scan branch and the drain/listener delta
diffs are the integration points, all behind the optional hold handle. Reverting C8 + those diffs leaves
Wave A compiling and shipping degraded to the C5 fallback (ADR-009 safe-revert target).

## Data Flow

- **In:** drained `Arc<Mutex<TranscriptBuffer>>` + `feature_cycle` (hold_on_drain); re-register cycle
  (readopt); deltas (listener route); TTL + max_sessions (C9).
- **Out:** re-adopted `Arc` (readopt); `Vec<TranscriptPurgeRecord>` (sweep/purge); audit records.
- **Consumers:** C1 (`arcs_for_feature`), C7/`purge_cycle_transcripts` (`purge_held_for_feature`),
  registry register/drain/sweep paths.

## Error Handling

Poisoned hold lock → recover (`into_inner` + `clear_poison`, #4764). Poisoned buffer lock on a held
buffer during snapshot is handled by C1 (treat-as-empty, surfaced as lossy). No panic on any path.

## Key Test Scenarios (AC-11 — named `continuity_simulated_lifecycle`, HARD MERGE GATE, R-05)

The ONE pre-merge proof of the primary path. Must be the FAITHFUL per-turn-drain simulation, NOT a
single-turn happy path:

```
register → deltas → drain(Stop→SessionClose) → deltas → drain → deltas → drain → re-register → cycle review
   (≥ 3 drain cycles; deltas applied BETWEEN each drain to prove merge-while-held)
```

Asserts:
- (a) the review snapshot contains content streamed across ALL turns, not just the last (merge-while-held).
- (b) re-adoption rebinds to the same `feature_cycle`; FAILS LOUD on key mismatch and on NULL cycle
  (R-01, cite #981) — buffer dropped, metadata-only diagnostic, no content re-adopted under wrong cycle.
- (c) held-count stays within `transcript_hold_max_sessions`; eviction observable (oldest-first) when
  exceeded; eviction emits audit (R-02/R-16).
- (d) TTL stale sweep reclaims a held buffer NEVER re-registered/reviewed — independent of cycle review
  (R-02). Disable cycle review entirely → memory still bounded by cap × TTL alone.
- (e) `transcript_session_purged` fires EXACTLY ONCE per held session at its terminal purge
  (review/sweep/evict), not at the per-turn drains, even across multiple hold→re-adopt rounds (R-03).

Additional:
- R-17: held-buffer delta lookup is O(1) keyed; merge under buffer lock only; `apply_transcript_delta`
  lock-hold class unchanged with the hold active.
- R-13: a held + registered session same cycle → both snapshotted and both purged; Arc-identity dedup.
- Negative (R-05): a single-turn-only test must NOT be accepted as AC-11 evidence.
