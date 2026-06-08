# Agent Report — C10 Consumer Guidance (crt-052)

Agent: crt-052-agent-3-consumer-guidance
Component: C10 — consumer guidance (documentation; no Rust)
Wave: A. No reference to `transcript_hold.rs`.

## Files Modified
- `.claude/skills/uni-retro/SKILL.md`
- `.claude/protocols/uni/uni-agent-routing.md`

Committed: `f2893d61` (scoped to the two doc files; other agents' in-flight Rust
changes on the shared `feature/crt-052` checkout were left unstaged).

## What Was Added
- New `## Consuming transcript_candidates` section in uni-retro SKILL.md, linked
  from Phase 1 step 1 (where `context_cycle_review` is called).
- Phase 1 step 1 now flags the section and the absent-section pass-through (AC-04).
- Phase 2 architect briefing gained a `TRANSCRIPT CANDIDATES` block; a new task 4.E
  instructs the extraction subagent to act on candidates.
- uni-agent-routing.md retro Phase 1 step references the new guidance.

## AC-13 Checklist Items Covered
- [x] Four marker families (Decision, Rework, Lesson, PhaseGate) + hints ADVISORY
      (rules select, agent extracts — Constraint 6). Mapping table to store types.
- [x] Q8 folds: Warning-hotspot timestamp-adjacent join (rework-why), gate-failure
      units, human-intervention ledger from USER blocks, phase narration.
- [x] Call-time-vs-cached note (OQ-4 / AC-05): explicit — candidates distilled fresh
      at call time, may differ from memoized RetrospectiveReport on a cache hit.
- [x] Feature-attributed `context_store` extraction — only path to the KB (AC-09).
- [x] Provenance weighting (ADR-007): Reconstructed weighted lower (0.81 floor);
      elided_bytes > 0 clips head / early Decision content (ass-070 Q5); has_holes note.
- [x] Loss visibility: how to read SessionLossInfo (elided_bytes, has_holes,
      provenance Primary/Reconstructed, dropped_candidates cap-drop, AC-08).
- [x] Dependency-posture review gate referenced (cargo audit / regex-class only,
      AC-13 / NFR-6) as a review-gate check, not extraction work.

## Notes
- "Cycle-review protocol step" resolved to two homes: the uni-retro SKILL.md Phase 1
  (the only place `context_cycle_review` is invoked) and the retro step in
  uni-agent-routing.md. Both updated. No new top-level docs created.
- `cargo audit` shell check (Stage 3c) NOT run here — crt-052 Rust code is not yet
  merged on this branch; running audit against an unrelated tree proves nothing. The
  guidance documents it as the review gate per the C10 test plan.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced crt-052 ADRs (4850 ADR-004,
  4856 ADR-007, 4847 ADR-001, 4851 ADR-005) already in the brief; no new gotchas.
- Stored: nothing novel to store — this is a documentation deliverable that restates
  ADR-007/OQ-4/AC-13 guidance already captured in the crt-052 ADRs; no reusable
  implementation pattern or failure-mode lesson emerged.
