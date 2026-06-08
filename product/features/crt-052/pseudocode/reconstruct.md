# C5 — Reconstruction Fallback

**Target source:** `unimatrix-observe/src/distill/reconstruct.rs`
**Wave:** A — **NO reference to `transcript_hold.rs`; pure, no I/O, no lock.**
**ADRs:** ADR-006 (trigger + topic_source), ADR-007 (provenance). **Risks:** R-09, R-14.
**AC:** AC-07, AC-08. **Constraints:** 8 (fidelity floor), 9 (tail-window-equivalence).
**Sequencing:** after C4; independently fixture-testable.

## Purpose

When a session's snapshot is empty/hole-ridden past threshold (whole-session either/or — OQ-2), build
distillation input from the session's already-loaded `ObservationRecord`s. Labeled `Reconstructed`
provenance. This is a 0.81-ceiling fidelity FLOOR (DEC-weakest, ass-070 Q6), NOT parity — provenance
labeling is mandatory (Constraint 8). NEVER writes the byte buffer, NEVER produces observation rows.

## Fallback Trigger (ADR-006 — lives logically here, invoked by C6)

Defined against ADR-002 tail-window-equivalence semantics, NOT assumed losslessness (R-09, SR-08). A
session falls back (whole-session) when, from its `TranscriptSnapshot`:

```
fn fallback_triggered(snap: &TranscriptSnapshot, hole_fraction_threshold: f64) -> bool:
    // 1. empty after JSONL filtering yields no user/assistant blocks
    if snap.bytes.is_empty():  return true
    //    (C6 may also treat "parsed but zero candidate-eligible blocks" as empty — see C6)
    // 2. ring-tail clipping present
    if snap.elided_bytes > 0:  return true
    // 3. holes cover more than the configured fraction of the span
    span = snap.high_water.saturating_sub(snap.base_offset)
    hole_bytes = sum(h.end - h.start for h in snap.holes)
    if span > 0 and (hole_bytes as f64 / span as f64) > hole_fraction_threshold:  return true
    return false
```

This is per-session and whole-session: a session is either Primary or Reconstructed, never a byte-level
mix (OQ-2). The SAME predicate result drives both the fallback choice AND the `provenance` label in
`SessionLossInfo` (ADR-007 — no re-computation). Boundary-tested at the 4 MiB cap edge and under
ring-tail overflow (cite #4764 active).

> Placement note: the predicate is small and shared. Put it in `reconstruct.rs` (or `distill/mod.rs`)
> and have C6 call it once per session, using the result for BOTH the path choice and the provenance
> label. Do not inline a second copy in C6.

## Reconstruction (ARCH §4 — binding signature)

```
fn reconstruct_from_observations(session_id: &str, obs: &[ObservationRecord], session_cap: usize)
    -> Vec<TranscriptCandidate>:

    // 1. scope to this session's observations (caller passes the cycle's obs; filter by session_id).
    rows = obs.filter(|o| o.session_id == session_id)

    // 2. SOFT topic_source preference (FR-9 / SR-06 / R-14): STABLE-SORT to ORDER declared/registry-fill
    //    rows ahead of vote/extracted/NULL — NEVER a filter. No row dropped, no session excluded.
    rows = stable_sort_by(rows, key = topic_source_rank(o.topic_source))
    //    topic_source_rank: declared=0, registry-fill=1, extracted=2, vote=3, NULL=4 (ordering only)

    // 3. build degraded distillation input from each observation's fields (tool, input,
    //    response_snippet <= 500 chars). Compose a synthetic block of text per observation.
    candidates = []
    for o in rows:
        text = compose_reconstructed_text(o.tool, o.input, o.response_snippet)
        hints = match_families(&text)        // reuse C3 markers.rs; advisory hints over reconstructed text
        if hints.is_empty():
            hints = default_family_hint_for(o)   // ensure family_hints non-empty (C4 invariant);
                                                 //   e.g. infer a coarse family from event_type, advisory
        candidates.push(TranscriptCandidate {
            session_id: session_id.to_string(),
            byte_offset: 0,                  // no buffer offset; reconstructed input has no stream offset
            ts: o.ts.to_string_opt(),        // observation timestamp = ordering key
            family_hints: hints,
            text: text,
        })

    // 4. order chronologically by ts; per-session cap (keep-earliest), same as C3.
    candidates.sort_by_key(|c| (c.ts.clone(), c.byte_offset))
    capped = keep_earliest_within(candidates, session_cap)
    return capped
    //    provenance = Reconstructed is assigned by C6 (per-session, into SessionLossInfo).
```

## Hard Invariants (AC-07)

- NEVER writes the byte buffer.
- NEVER produces / inserts `ObservationRecord` rows (read-only over the already-loaded `obs`).
- Output is distillation-INPUT only, labeled `Reconstructed` (per-session, via C6/`SessionLossInfo`).
- `topic_source` is ORDER-only (stable sort key), NEVER a filter (R-14): all feature-matched
  observations contribute; no session is excluded by `topic_source`. crt-052 reads the already-loaded
  `topic_source` column; never persists or re-derives it.

## `byte_offset` for reconstructed candidates

There is no buffer stream position, so `byte_offset = 0` (the ordering key is `ts`). Documented so the
consumer does not treat reconstructed offsets as stream positions.

## Data Flow

- **Input:** `session_id`, `&[ObservationRecord]` (already loaded by `load_cycle_observations`),
  `session_cap` (C9). Reuses C3 `markers.rs` for advisory hints.
- **Output:** `Vec<TranscriptCandidate>` (Reconstructed, assigned by C6), possibly empty.
- **Consumer:** C6 (aggregates; marks the session `Reconstructed`, emits `SessionLossInfo`).

## Error Handling

No `Result`, no panic. A session with zero matching observations → empty Vec; C6 still emits a
`SessionLossInfo` row with `provenance: Reconstructed` (loss not invisible, ADR-007).

## Fidelity (Constraint 8)

0.81 ceiling (DEC-weakest, 5 of 8 lost — ass-070 Q6). The degraded label is load-bearing for the
consumer (C10 weights `Reconstructed` candidates differently) and for future quality measurement. Not
parity — made discriminable by provenance, not by content quality.

## Key Test Scenarios

- AC-07(i): empty buffer + observations present → reconstructed candidates labeled degraded.
- AC-07(ii): hole-ridden buffer at/above threshold → fallback fires whole-session.
- AC-07(iii): assert no buffer write and no observation-row insert on the fallback path.
- AC-07(iv) / R-14: mixed `topic_source` → reconstruction REORDERS (declared/registry-fill first) but
  drops NO observation and excludes NO feature-matched session; all-`vote` rows still reconstruct.
- R-09 boundary: trigger at 4 MiB cap edge and under ring-tail overflow; `elided_bytes > 0` fires;
  holes below fraction → primary, above → fallback; whole-session either/or (no byte-level mix).
