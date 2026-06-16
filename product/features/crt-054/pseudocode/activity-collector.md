# Component 5 — activity collector (`activity_snapshots_for_feature`)

**File**: `crates/unimatrix-server/src/infra/session.rs` (modify) — on `SessionRegistry`.
**Mirrors**: `take_transcripts_for_feature` (`:469`, verified body) — same two-phase lock discipline, same registered ∪ held selection, same dedup-by-`Arc`.
**ADRs**: ADR-004 (late-bind attribution, never fabricate a zero), ADR-006 (read-before-purge survival).
**Consumer**: crt-055 calls this at review, before `purge_cycle_transcripts`.

## Purpose

The registry-level collector that returns one `ActivitySnapshot` per session in a cycle, covering both registered and held buffers (deduped by `Arc` identity). It is the Surface-B read seam for crt-055. It NEVER fabricates an entry for a session that produced no buffer — coverage = cycle-declaration coverage (FR-B10, AC-12).

## Function

```
// On SessionRegistry, modeled exactly on take_transcripts_for_feature.
fn activity_snapshots_for_feature(&self, feature_cycle: &str) -> Vec<(String, ActivitySnapshot)>

    // ── Phase 1 — registry lock: scan + Arc-clone only (microsecond-class). ──
    let arcs: Vec<(String, Arc<Mutex<TranscriptBuffer>>)> = {
        let sessions = self.sessions.lock() (poison-recover: unwrap_or_else into_inner)
        let mut arcs = sessions.values()
            .filter(|s| s.feature.as_deref() == Some(feature_cycle))   // None never matches → undeclared excluded
            .map(|s| (s.session_id.clone(), Arc::clone(&s.transcript)))
            .collect()

        // Held-scan branch (Wave B, SEVERABLE — same as take_transcripts_for_feature).
        // Reached only through the optional HeldBufferScan handle; dedup by Arc identity
        // so a buffer that is BOTH registered and held is snapshotted once.
        if let Some(hold) = self.transcript_hold.as_ref():
            for (sid, arc) in hold.held_arcs_for_feature(feature_cycle):
                if not arcs.iter().any(|(_, a)| Arc::ptr_eq(a, &arc)):
                    arcs.push((sid, arc))
        arcs
    }   // registry lock RELEASED before any buffer lock

    // ── Phase 2 — per-buffer lock: counters copy via activity_snapshot() only. ──
    let mut out = Vec::with_capacity(arcs.len())
    for (sid, arc) in arcs:
        let snap = {
            let buf = lock_buffer(&arc)        // poison→empty policy (#4764), same helper
            buf.activity_snapshot()            // Component 4 — Copy, no content
        }   // buffer lock RELEASED
        out.push((sid, snap))
    return out
```

This is `take_transcripts_for_feature` with exactly one substitution: `buf.snapshot()` → `buf.activity_snapshot()`, and the return element type `TranscriptSnapshot` → `ActivitySnapshot`. Keep the structure identical so the two seams cannot diverge in routing/dedup behavior (the held-route coverage that protects against the believable-zero trap is inherited verbatim).

## Late-bind attribution / no fabricated zero (ADR-004, AC-12)

- The Phase-1 `filter(s.feature == Some(feature_cycle))` means **undeclared sessions (feature == None) contribute no entry** — they are absent from the result, not present-with-zero. An undeclared session that purged at drain (its buffer gone) is simply not in `sessions` and not in the hold's `held_arcs_for_feature` set, so it contributes nothing.
- crt-054 returns only `(session_id, snapshot)` for sessions that have a live (registered or held) buffer in the cycle. Absence is conveyed by the session NOT appearing in the Vec — crt-055 surfaces "unavailable" via its own `raw_signals_available`-style flag (Binding constraint, not crt-054's job). crt-054 emits no zero on a missing session's behalf.
- A session present with a genuinely zero-byte buffer (deltas were all clipped, say) DOES appear with a zero snapshot — that is a measured zero, distinct from absence. Both are honest.

## Read-before-purge (ADR-006, AC-07)

crt-055 calls this collector BEFORE `purge_cycle_transcripts` zeroes/drops the buffers. crt-054 owns no purge ordering — it only guarantees the snapshot is accurate while the buffer is held. The read-before-purge ordering test (AC-07) exercises: collect → assert non-zero → purge → assert buffer zeroed.

## Error handling

- Infallible. Lock poison degrades to empty per buffer (#4764) — never drops a session, never panics.
- No `Result` — returns the Vec directly (matches `take_transcripts_for_feature`).

## Key test scenarios (hints)

- Registered ∪ held union with `Arc` dedup: a session both registered and held appears exactly once (mirror the existing dedup test at `session.rs:3300+`).
- Undeclared session (feature == None) contributes NO entry (AC-12) — assert absent, not zero.
- Held-only session (drained but held) appears with its folded non-zero snapshot (AC-06 path at the collector level).
- Read-before-purge ordering with the real crt-052/vnc-025 fixtures (AC-07).
- Element type is `(String, ActivitySnapshot)`; widths match crt-055's contract (AC-08 conformance).
- Two-phase lock: no buffer lock held while the registry lock is held (review + the inherited structure).
