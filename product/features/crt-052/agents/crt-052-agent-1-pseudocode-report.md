# Agent Report — crt-052-agent-1-pseudocode

## Deliverables

Per-component pseudocode for C1..C10, OVERVIEW.md + one file per component, all in
`product/features/crt-052/pseudocode/`. Matches ARCH §4 / brief "Data Structures" + "Function
Signatures" verbatim. Naming pin honored: `TranscriptSnapshot` (never `SessionTranscriptSnapshot`).

## Components Covered → Target Source

| C | File | Target source | Wave |
|---|------|---------------|------|
| C1 | snapshot-seam.md | infra/session.rs `take_transcripts_for_feature` | A |
| C2 | snapshot-types.md | infra/session_transcript.rs `TranscriptSnapshot`/`HoleInfo`/`snapshot()` | A |
| C3 | selection-module.md | unimatrix-observe/src/distill/{mod,jsonl,markers,select}.rs | A |
| C4 | response-types.md | unimatrix-observe/src/types.rs candidate/section + response field | A |
| C5 | reconstruct.md | unimatrix-observe/src/distill/reconstruct.rs | A |
| C6 | distill-handler.md | mcp/distill_handler.rs + thin wiring in mcp/tools.rs (4 returns) | A |
| C7 | retention-gate.md | server.rs `purge_cycle_transcripts` exhaustive match | A |
| C8 | held-buffer-store.md | infra/transcript_hold.rs + minimal diffs session.rs/listener.rs | B |
| C9 | config-knobs.md | infra/config.rs `RetentionConfig` | A/B |
| C10 | consumer-guidance.md | .claude/skills/uni-retro + protocol step | A |

## Critical constraints encoded

Lock discipline (Arc-clone under registry lock, byte copy under buffer lock, all parse after release,
#3753 no-relock); Wave A/B dependency-direction map + per-file reference status (R-11); pure C3 module
(skip-with-count, never panic — R-10/AC-V-FUZZ); candidates outside memoized struct at assembly level
(AC-06); one helper at four returns gated on exhaustive TranscriptRetention match (AC-05/AC-10); logical
byte_offset (R-12); snapshot() as 2nd-and-last reader (AC-V-SEAM); reconstruction 0.81 floor with
mandatory provenance (AC-07); AC-11 faithful ≥3-drain simulation as the sole primary-path proof.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get (#3753 pre-cloned snapshot
  never-relock; #4799 per-turn drain starvation; #4750 four success returns; ADRs #4847-#4854).
  Findings folded into C1 lock discipline, C6 four-return wiring, C8 continuity rationale.
- Deviations from established patterns: none. Pseudocode follows #4750 (one gated helper at four
  returns), #3753 (read the owned snapshot, never re-acquire a lock), #4764 (treat-as-empty poison
  recovery), #3793/ADR-004 (attach outside the memoized struct).

## Open Questions / Contract Ambiguities (for leader/architect)

1. **`SessionLossInfo.dropped_candidates` (added field).** AC-08 requires cap-forced truncation
   (per-session AND per-cycle) to be surfaced, never silent. ARCH §4's `SessionLossInfo` lists only
   `{session_id, elided_bytes, has_holes, provenance}` with no drop count. C4/C6 ADD a content-free
   `dropped_candidates: u64`. Confirm this field or specify an alternate AC-08 surfacing.

2. **`select_candidates` signature vs per-session cap-drop count.** ARCH §4 pins
   `-> Vec<TranscriptCandidate>` (no count out-param), but AC-08 needs the per-session cap-drop count.
   C3/C6 assume C6 re-derives the drop from pre-cap vs post-cap accounting to preserve the pinned
   signature. Confirm, or widen the return.

3. **`hold_on_drain` / `readopt` arity.** ARCH §4 main table lists `hold_on_drain(session_id, arc)` and
   `readopt(session_id) -> Option<Arc<…>>`, but ADR-008 §decision and SR-02 (loud re-adopt on cycle
   match) REQUIRE the `feature_cycle`. Pseudocode uses the 3-arg `hold_on_drain(session_id, arc,
   feature_cycle)` and `readopt(session_id, registering_feature_cycle)` (ADR-008 binding form).
   Confirm the ADR-008 form supersedes the §4 short form.

4. **Fallback hole-fraction threshold knob.** ADR-006 names a configurable hole-fraction threshold for
   the fallback trigger, but the brief's knob table (4 knobs) does not list it. C9 proposes adding
   `transcript_fallback_hole_fraction: f64`. Confirm whether it is a config knob or a compile-time
   constant.

5. **`SessionTranscriptSnapshot` field-name fork (resolved, noted).** SPEC §Domain Models names a
   `hole_info` field and a `session_id` field on the snapshot; ARCH §4 / brief use `holes: Vec<HoleInfo>`
   and no `session_id` on `TranscriptSnapshot` (session_id is the tuple key in the seam's return). C2
   follows ARCH §4. No action needed — flagging the spec/arch field-set divergence for awareness.

6. **PREREQUISITE GATE (carry-forward, not a pseudocode gap).** ADR-009's no-consumer audit survey
   (gc_audit_log/crt-036, retention readers, per-close-emission tests) must be recorded clean BEFORE the
   Wave B audit-shape move merges. Encoded as a gate in held-buffer-store.md; the survey itself is a
   delivery task, not pseudocode.
