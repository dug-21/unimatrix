# Agent Report — crt-052-researcher (design restart)

## Task
Re-research the crt-052 (#689) problem space and rewrite SCOPE.md. The prior scope was written before
the F4 predecessors shipped and before the six Open Questions were decided. Re-verify every code claim
against the post-vnc-030 tree, fold in the binding decisions, re-home stale cross-references, and
elevate two new first-class constraints.

## Output
- `product/features/crt-052/SCOPE.md` — rewritten in place. 13-AC structure preserved; ACs touched by
  the resolved OQs updated; new Constraint 4 (#700 single-reader seam) and Constraint 9 (tail-window
  buffer semantics) elevated to first-class; Delivery-Ordering section removed; Open Questions reduced
  from 6 (all resolved) to 3 genuine design-phase items.

## Substantive changes from the prior scope

**Folded in (resolved OQs, binding human decisions #689 2026-06-08):**
- OQ-1 → Option B (server-only transcript hold). Goal 8 and AC-11 are now concrete Option B, not "shape
  per OQ-1". Option A (close-reason wire field) removed from Non-Goals as a routing contingency — it is
  off the table, routes to no F4 feature.
- OQ-2 → whole-session either/or per session (AC-07, Goal 5).
- OQ-3 → keep 24 KB/session default AND add a per-cycle aggregate cap. New config knob added to Goal 2,
  AC-02, and the approach.
- OQ-4 → distill whatever is present at call time. AC-05 strengthened with cache-hit semantics; Goal 7
  / AC-13 add the call-time-vs-cached consumer note.
- OQ-5 → lean `unimatrix-observe` (architecture finalizes). Reflected in the approach.
- OQ-6 → small synthetic corpus authored independently of the ported regex set. AC-03 now requires
  fixture-authorship independence (anchors-before-porting or different author) to prevent
  self-fulfilment.

**Corrected (stale cross-references, per #689 vnc-030 cross-feature notice):**
- "Delivery Ordering vs vnc-027" section (whole) → removed; replaced with a short "predecessors
  shipped — build on the delivered interfaces" framing in the header and Tracking.
- vnc-027 cited for attribution + close/sweep adjacency → re-homed to vnc-030 (#699).
- "vnc-027 OQ-4" marker tier → re-homed to #700.
- "AC-19" offset-delete reference → would be vnc-027 AC-10; the accidental-re-stream-heals-buffers
  rationale is dropped entirely (Option B supersedes it).
- Constraint 12 "vnc-027 designs in parallel" → replaced with Constraint 13 "cite vnc-030's precedence
  interface".

**Added (new first-class constraints + interface):**
- Constraint 4 — the #700 single-reader snapshot-seam invariant, promoted from interaction warning to a
  load-bearing Constraint that shapes the seam's return type/contract. Mirrored in Goal 1, Approach 2,
  Non-Goals, AC tie-ins, and Open Question 2.
- Constraint 9 — tail-window-equivalence buffer semantics (vnc-025 ADR-002 #4740 / ADR-008 #4746);
  distillation window and fallback trigger designed against hole/elision state, not a lossless buffer.
  Mirrored in Goal 5 and AC-07.
- Constraint 13 — cite vnc-030 ADR-007 §2's minimal-diff close/sweep precedence interface; do not
  rework precedence.
- vnc-030 `observations.topic_source` assessed as an optional sharpening input for fallback selection
  (Background + Open Question 1) — not a dependency.

**Removed:** the Delivery Ordering section; the OQ-1 Option A routing language; the FR-16
accidental-re-stream rationale.

**Preserved (re-verified, still holds):** ass-070 extraction architecture (rules select / agent
extracts); four-success-return purge topology (#4750); memoization-trap AC (AC-06); two-pipe boundary
(AC-09); secrets posture (#4721); reconstruction fidelity floor (0.81, DEC-weakest); 4 MiB cap;
exhaustive `TranscriptRetention` match (AC-10).

## Code claims re-verified against current tree (post-vnc-030)
All verified; several line numbers had shifted and are corrected in SCOPE:
- `clear_transcripts_for_feature` → `session.rs:299` (was :262).
- `drain_and_signal_session` → defined `session.rs:651`; invoked from `process_session_close`
  (`listener.rs:2069`) at `listener.rs:2133`. (Prior SCOPE cited `listener.rs:1935` for drain — the
  invocation moved; the definition is in session.rs.)
- `sweep_stale_sessions` → `session.rs:687` (vnc-030's note said :628 — moved post-merge).
- `purge_cycle_transcripts` def `server.rs:541`; four call sites `tools.rs:2110/2236/2925/3027`
  (unchanged).
- Single content reader: PreCompact `contiguous_tail(...)` → `extract_transcript_block_from_bytes`
  at `listener.rs:1834-1838` (vnc-030's note said ~:1646 — moved post-merge). All other
  `contiguous_tail` callers are tests. **Confirmed: only one production content reader today.**
- Review-time attribution: `load_cycle_observations` `services/observation.rs:308` — window-based over
  `cycle_events`, reads no transcript content. **Confirmed exactly as vnc-030 described.**
- Two-pipe filter: delta apply `listener.rs:953`/`:1206`, filter `listener.rs:1238`, insert
  `listener.rs:1248` (was :999-1025/:1009 — stale).
- `clear()` preserves `high_water`/`elided_bytes`: `session_transcript.rs:199-208`. Confirmed.
- `TranscriptRetention` exhaustive match: `PurgeOnCycleClose` `server.rs:543`, `RetainDays(_)` `:551`.
- `RetrospectiveReport` `types.rs:381`; `synthesize_narratives` `synthesis.rs:15`;
  `build_phase_narrative` `phase_narrative.rs:21`; `ObservationRecord` `observation.rs:21`.
- Stop/TaskCompleted→SessionClose mapping `build-request.js:59-62` — intact; per-turn drain reality
  holds, Option B remains necessary.
- `observations.topic_source` shipped; enum origins `session.rs:112-124`.
- Config knob pattern `infra/config.rs:1561-1576`.

**No code claims failed verification.** Every claim in the prior SCOPE that survives is true in the
current tree; the only discrepancies were line-number drift (corrected) and references made obsolete by
the F4 merges (re-homed or removed).

Note on a knowledge entry: vnc-030 ADR-007 (#4819) and ADR-008 (#4746) show `status: deprecated` in
Unimatrix, but ADR-007 carries a later update timestamp (corrected, not retired) and #689's standing
note (2026-06-08 15:28Z) cites ADR-007 §2 as the authoritative interface — I treated both as the
binding current decisions. Worth a leader confirmation that these statuses are stale-label, not actual
retraction.

## Genuinely-still-open questions for the human (few — most decided)
1. **`topic_source` filtering of fallback selection** — optional sharpening input; design-phase call,
   not blocking. (Leader/architect, not human-blocking.)
2. **Snapshot-seam return shape for #700 reuse** — design decision; the constraint is binding, the
   exact return type is for architecture.
3. **Option B audit-shape change** — moving the purge audit off per-turn-close changes vnc-025's
   shipped audit timing; confirm no downstream consumer keys on per-close audits.

None of these block scope approval. The scope is decided; these are architecture-phase items.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search/context_get — surfaced and incorporated
  #4742 (named seam / purge points), #4740 (ADR-002 tail-window semantics), #4746 (ADR-008
  poison-recovery / "crt-052 reconstructs"), #4750 (four success returns), #4739 (Arc<Mutex> shape),
  #4799 (per-turn drain pattern), #4816 (vnc-030 ADR-004 FeatureSource/topic_source), #4819 (vnc-030
  ADR-007 §2 close/sweep interface + §4 marker-recovery single-reader pin), #3793 (memoization persist).
- Stored: nothing novel to store — the per-turn drain pattern (#4799) and the single-reader invariant
  are already captured in Unimatrix (#4799, #4819 §4); this restart re-verified and re-homed existing
  knowledge rather than discovering new generalizable patterns. Feature-specific scope lives in
  SCOPE.md.
