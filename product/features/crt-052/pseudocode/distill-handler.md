# C6 — Distill Helper / Handler Glue

**Target source:** `unimatrix-server/src/mcp/distill_handler.rs` (NEW, thin) + thin wiring in
`mcp/tools.rs` at the four `result.is_ok()` returns (`:2110`, `:2236`, `:2925`, `:3027`)
**Wave:** A — **NO reference to `transcript_hold.rs`** (the held scan lives behind C1's seam).
**ADRs:** ADR-005 (one helper / four returns / gate), ADR-004 (assembly-level attach), ADR-001/002
(snapshot), ADR-003 (select), ADR-006 (fallback trigger), ADR-007 (loss). **Risks:** R-04, R-07, R-13,
R-15, R-16. **AC:** AC-05, AC-06, AC-08, AC-10. **Constraints:** 3 (four returns), 10 (500-line).
**Sequencing:** LAST of Wave A — depends on C1, C3, C4, C5, C7, C9.

## Purpose

One shared helper called at ALL FOUR `result.is_ok()` success returns, immediately BEFORE
`purge_cycle_transcripts`. Orchestrates C7 gate → C1 snapshot → per-session C3/C5 → per-cycle cap →
C4 section. The handler attaches the returned section at RESPONSE-ASSEMBLY level (outside the memoized
`RetrospectiveReport`), then purges.

## Helper (ARCH §4 — binding signature)

```
fn distill_before_purge(
    registry:     &SessionRegistry,
    feature_cycle:&str,
    observations: &[ObservationRecord],   // already loaded by load_cycle_observations — do NOT re-query
    cfg:          &RetentionConfig,
) -> Option<TranscriptCandidatesSection>:

    // (1) EXHAUSTIVE retention gate (C7 / ADR-005 / AC-10) — no wildcard arm
    match cfg.transcript_retention {
        PurgeOnCycleClose => {}            // proceed
        RetainDays(_)     => return None,  // OSS-unreachable; neither distill nor purge
    }

    // (2) snapshot off-lock (C1 / ADR-001). Returns registered ∪ held (Wave B) for the cycle.
    snapshots: Vec<(String, TranscriptSnapshot)> = registry.take_transcripts_for_feature(feature_cycle)
    //     ALL parsing happens AFTER this returns. No lock held here.

    all_candidates: Vec<TranscriptCandidate> = []
    loss:           Vec<SessionLossInfo>      = []

    // (3) per-session: choose Primary (C3) or Reconstructed (C5) via the SHARED fallback predicate
    for (session_id, snap) in snapshots:
        is_fallback = fallback_triggered(&snap, cfg.hole_fraction_threshold)   // C5/ADR-006 predicate

        if not is_fallback:
            primary = select_candidates(&snap.bytes, &session_id, snap.base_offset,
                                        cfg.transcript_candidate_session_cap_bytes)   // C3
            // R-09 corner: snapshot non-empty bytes but ZERO candidate-eligible blocks is still Primary
            //   with zero candidates — NOT a fallback (trigger keys on empty bytes / loss, not on
            //   "no markers matched"). Only the predicate decides fallback.
            provenance = Primary
            session_cands = primary
            // AC-08 per-session cap-drop count (see C3 contract note: derive here)
            dropped = count_dropped_by_session_cap(&snap, &session_id,
                                                   cfg.transcript_candidate_session_cap_bytes)
        else:
            recon = reconstruct_from_observations(&session_id, observations,
                                                  cfg.transcript_candidate_session_cap_bytes)   // C5
            provenance = Reconstructed
            session_cands = recon
            dropped = 0   // session-cap drop accounting for reconstructed input handled in C5 similarly

        all_candidates.extend(session_cands)

        // (4a) build per-session SessionLossInfo (ADR-007 / AC-08). Same predicate result -> provenance.
        has_holes = not snap.holes.is_empty()
        if snap.elided_bytes > 0 or has_holes or provenance == Reconstructed or dropped > 0:
            loss.push(SessionLossInfo {
                session_id, elided_bytes: snap.elided_bytes, has_holes,
                provenance, dropped_candidates: dropped,
            })
        // clean Primary session with no loss/no drop => omitted (silence == nothing to report)

    // (4b) order the UNION deterministically (R-15) by (ts, session_id, byte_offset)
    all_candidates.sort_by_key(|c| (c.ts.clone(), c.session_id.clone(), c.byte_offset))

    // (4c) PER-CYCLE aggregate cap (cfg.transcript_candidate_cycle_cap_bytes, ADR-005 §4 / FR-4).
    //   Deterministic chronological KEEP-EARLIEST (brief "truncation order" pin). Repeatable (R-15).
    (kept, cycle_dropped_by_session) = keep_earliest_within_cycle(
        all_candidates, cfg.transcript_candidate_cycle_cap_bytes)
    all_candidates = kept
    // AC-08: surface aggregate-cap drops — fold cycle_dropped_by_session into the loss rows so no
    //   aggregate-cap drop is silent. For each session that lost candidates to the cycle cap, ensure a
    //   loss row exists and add to its dropped_candidates.
    merge_cycle_drops_into_loss(&mut loss, cycle_dropped_by_session)

    // (5) absent-when-empty (AC-04): no candidates AND no loss to report -> None
    if all_candidates.is_empty() and loss.is_empty():
        return None
    return Some(TranscriptCandidatesSection { candidates: all_candidates, loss })
```

## Wiring at the Four Returns (Constraint 3 / ADR-005 / AC-05 / R-07, pattern #4750)

`tools.rs` gains FOUR thin call lines (no logic — Constraint 10). Each of the four `result.is_ok()`
success returns, in order:

```
// purged-signals (:2110), cached-MetricVector (:2236), memoization-hit (:2925), full-pipeline (:3027)
let section = distill_before_purge(registry, feature_cycle, &observations, cfg);   // BEFORE purge
attach_to_response_assembly(&mut response, section);                               // ADR-004 — see below
purge_cycle_transcripts(...);                                                      // existing, AFTER distill
```

- All four sites call the SAME helper with the SAME gating and ordering (distill → attach → purge).
- An EXHAUSTIVENESS regression test fails if a fifth success return is added without wiring the helper
  (R-07 / SR-05) — modeled on vnc-025's purge exhaustiveness test.
- Memoization-hit path (`:2925`, #3800): candidates are distilled FRESH from call-time buffer content
  and attached to the response; they MAY differ from the cached `RetrospectiveReport` (acceptable,
  documented — OQ-4 / AC-05). The cached report is unchanged.
- Error paths: NOT wired — they keep transcripts and produce no candidates (existing behavior).

## Assembly-Level Attach (ADR-004 / AC-06 / R-04 — secrets-critical)

`attach_to_response_assembly` sets the additive `transcript_candidates: Option<...>` field on the
RESPONSE struct AFTER the `RetrospectiveReport` is computed and memoized via `store_cycle_review()`
(#3793). It NEVER writes the section onto `RetrospectiveReport` (the persisted type has no such field —
ADR-004 makes the leak structurally impossible). `None` → field omitted (AC-04). No candidate or buffer
content touches any SQL write, file write, or log line (AC-06 content-leak gate).

## R-13 / R-16 corners

- R-13: the snapshot set (registered ∪ held) and the purge set must be congruent. C6 snapshots, then
  `purge_cycle_transcripts` (C7) clears registered + held for the SAME `feature_cycle`. No session
  snapshotted-but-not-purged or vice versa.
- R-16: a poison-recovered (treat-as-empty) snapshot (C1) yields empty `bytes` → fallback predicate
  fires → Reconstructed path → the session still surfaces in `loss` (not silently absent).

## Data Flow

- **Input:** registry, `feature_cycle`, already-loaded `&[ObservationRecord]`, `&RetentionConfig`.
- **Output:** `Option<TranscriptCandidatesSection>` (None when nothing to report → field absent).
- **Bridges:** server registry types (C1) ↔ pure observe module (C3/C5) ↔ response types (C4). Threads
  both the registry and the already-loaded observations without re-querying (ADR-005).

## Error Handling

`distill_before_purge` never panics: C1 is infallible, C3/C5 never `Err`/panic on untrusted input
(R-10). A fully-corrupt snapshot → C3 yields zero candidates → session may be omitted or surface as
Primary-with-loss; handler returns a normal response (AC-V-FUZZ handler-level: never panics).

## Key Test Scenarios

- AC-05 / R-07: per-path test at each of the four returns asserting (distill → attach → purge) order;
  exhaustiveness test fails on a fifth unwired return; error path retains transcripts, no candidates.
- AC-05 memoization-hit: fresh call-time candidates attached, cached report unchanged, divergence ok.
- AC-06 / R-04: re-review of stored record returns no candidates; content-leak gate over the helper
  path; section attached at assembly, never onto `RetrospectiveReport`.
- AC-08: per-session AND per-cycle aggregate cap drops surface in `loss.dropped_candidates` (no silent
  drop); poison-recovered session surfaces in `loss`.
- AC-04: zero attributed sessions / no candidates+no loss → `None` → field absent.
- R-13: registered + held session same cycle → both snapshotted and both purged; no double-count.
- R-15: per-cycle cap truncation deterministic (chronological keep-earliest), repeatable.
