# C1 — Snapshot Seam `take_transcripts_for_feature`

**Target source:** `unimatrix-server/src/infra/session.rs` (sibling to `clear_transcripts_for_feature` @ `:299`)
**Wave:** A — the held-scan branch is the ONLY seam point touching Wave B, kept severable (see below).
**ADRs:** ADR-001 (seam), ADR-002 (return type), ADR-008 §4 (held scan), ADR-009 (rollback boundary).
**Risks:** R-08, R-13, R-16. **AC:** AC-01, AC-V-SEAM. **Patterns:** #3753 (use snapshot, never relock), #4799.
**Sequencing:** after C2.

## Purpose

Add a sibling method that reads buffer CONTENT before purge under two-phase lock discipline, returning
owned raw `TranscriptSnapshot`s. Leaves `clear_transcripts_for_feature` (counts-only, used by
session_close/stale_sweep) UNCHANGED (Constraint 13, minimal diff vs vnc-030 ADR-007 §2). Snapshot
reads; purge clears; they do not merge.

## New Function (ARCH §4 — binding signature)

```
fn take_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<(String, TranscriptSnapshot)>
```

Two-phase, identical discipline to the existing counts-only method:

```
fn take_transcripts_for_feature(self, feature_cycle):
    // ---- PHASE 1: registry lock — Arc-clone only, microsecond-class (NFR-1, AC-01) ----
    arcs: Vec<(String, Arc<Mutex<TranscriptBuffer>>)> = []
    {
        guard = self.registry.lock()                       // registry lock
        for (session_id, state) in guard.sessions:
            if state.feature.as_deref() == Some(feature_cycle):   // None never matches (vnc-030 §2)
                arcs.push( (session_id.clone(), Arc::clone(&state.transcript_buffer)) )

        // ---- Wave B held-scan branch (SEVERABLE — ADR-008 §4 / R-11) ----
        // Present ONLY when the hold handle is wired (Wave B). With Wave B reverted this whole block
        // is gone and C1 scans registered buffers only — Wave A still compiles and ships degraded.
        if let Some(hold) = self.transcript_hold:          // optional/injected handle; Wave A => None
            for (session_id, arc) in hold.arcs_for_feature(feature_cycle):
                // R-13: avoid double-snapshot of a buffer that is both registered AND held.
                // Dedup by Arc identity (Arc::ptr_eq) against the registered set.
                if not arcs.any(|(_, a)| Arc::ptr_eq(a, &arc)):
                    arcs.push( (session_id, arc) )
        // ---- end Wave B branch ----
    }   // registry lock RELEASED here — before any buffer lock, before any parse

    // ---- PHASE 2: per-buffer lock — byte copy + metadata read only (no parse, no I/O) ----
    out: Vec<(String, TranscriptSnapshot)> = []
    for (session_id, arc) in arcs:
        guard = arc.lock().unwrap_or_else(|p| p.into_inner())   // poison recovery #4764
        snap  = guard.snapshot()                                // C2 — byte copy + metadata
        // on poison: snapshot of recovered (treat-as-empty) buffer; clear_poison so future locks are clean.
        if lock_was_poisoned: arc.clear_poison()
        drop(guard)                                             // buffer lock RELEASED
        out.push( (session_id, snap) )

    return out
    // The buffer is NOT cleared here. Purge is the separate purge_cycle_transcripts /
    // clear_transcripts_for_feature / purge_held_for_feature step that fires AFTER distill (ADR-005).
```

## Lock Discipline (Constraint 1 / NFR-1 / R-08, pattern #3753)

- Registry lock held only for the scan + Arc-clone(s). Released before any buffer lock.
- Each buffer lock held only for `snapshot()`'s byte copy + metadata read. No JSONL parse, no marker
  match, no `tracing`, no I/O under either lock.
- All parsing happens in C6→C3/C5, strictly after `take_transcripts_for_feature` returns its owned Vec.
- No step downstream re-acquires a buffer lock — consumers read the owned `TranscriptSnapshot` (#3753).
- Concurrency (AC-01b/R-08): delta-merge writers may take a buffer lock concurrently; the two-phase
  release ordering (registry then buffer, never held together for parsing) prevents deadlock; the byte
  copy under the buffer lock yields a consistent (untorn) snapshot of that buffer.

## R-13 — Snapshot/Purge Set Congruence

The seam scans **registered ∪ held** for `feature_cycle`; the post-distill purge (C6 step 6) must clear
the SAME set via `clear_transcripts_for_feature` (registered) + `purge_held_for_feature` (held, C8).
Dedup by `Arc::ptr_eq` guarantees a buffer that is both registered and held is snapshotted once and
purged once. A session registered between the snapshot scan and the purge scan is either in both or
neither for this review (no partial leak).

## Data Flow

- **Input:** `feature_cycle: &str`; the registry; (Wave B) the optional hold handle.
- **Output:** `Vec<(String, TranscriptSnapshot)>` — owned, no buffer borrows held.
- **Consumer:** C6 `distill_before_purge`.

## Error Handling

Infallible return (`Vec`, possibly empty). Poisoned buffer lock → recovered, treat-as-empty, surfaced as
lossy in `SessionLossInfo` by C6 (R-16) — never silently absent. No `feature_cycle` match → empty Vec
(C6 returns `None`, section absent — AC-04). No panic path.

## Wave A / B Notes (R-11)

The held-scan branch reads the hold ONLY through an optional handle (`self.transcript_hold: Option<…>`).
In Wave A that handle is `None` (or the field/branch is absent), so `session.rs`'s seam has **zero**
hard compile dependency on `transcript_hold.rs` types beyond the optional handle injected by the
registry constructor. Reverting Wave B removes the handle + branch; C1 compiles and runs scanning
registered buffers only. This is the documented safe-revert target (ADR-009).

## Key Test Scenarios

- AC-01a structural: no parse/marker-match call inside any lock-guard scope (source assertion).
- AC-01b concurrency/stress: stream deltas during a review scanning the same buffers → no deadlock, no
  torn read, consistent snapshot (loom or stress loop).
- Two-phase: assert Arc-clone under registry lock (phase 1), byte copy under buffer lock (phase 2),
  registry lock released before any buffer lock.
- R-13: a held + a registered session, same cycle → both returned; a buffer both registered and held →
  returned once (Arc identity dedup).
- R-16: poisoned buffer lock → treat-as-empty snapshot returned (not dropped), `clear_poison` called.
- Wave A only (Wave B absent): seam returns registered buffers only, pipeline degrades to fallback.
