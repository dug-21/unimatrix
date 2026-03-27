# Agent Report: col-031-agent-2-spec

## Output

`product/features/col-031/specification/SPECIFICATION.md` — written.

## Key Decisions Made

1. **AC-11 / NFR-03 cold-start invariant clarification**: SCOPE.md states the invariant as
   "bit-for-bit identical to pre-col-031." This is true only when `current_phase = None`
   (FR-10 maps None → 0.0, which matches the pre-col-031 hardcoded 0.0). When
   `current_phase = Some(phase)` and the table is cold-start, `use_fallback = true` causes
   `phase_affinity_score` to return `1.0`, adding a uniform `0.05` offset to all scores —
   ranking-preserving but not bit-for-bit identical. The spec documents this distinction
   explicitly and raises it as OQ-04 for the architect to resolve (whether neutral should be
   `1.0` or `0.0`).

2. **SR-03 gate coupling formalized**: AC-16 (eval harness fix) and AC-12 (eval regression
   gate) are explicitly marked non-separable. AC-12 cannot be declared passing until AC-16
   is complete. NFR-05 captures this as a measurable non-functional requirement.

3. **SR-01 locked as implementation constraint C-02**: The `json_each` form must be verified
   against a real `query_log` row at implementation time. The spec does not pre-decide the
   form; it requires the architect's store query design to pin it. This is the highest
   implementation-surprise risk.

4. **SR-06 lock ordering as a hard constraint C-04**: Formalized as a named constraint requiring
   the architect to document the full lock acquisition sequence (PhaseFreqTableHandle,
   TypedGraphStateHandle, EffectivenessStateHandle) before implementation begins.

5. **NFR-03 uniform-offset behavior on cold-start with phase set**: Raised as OQ-04 because
   this is a semantic decision (neutral = 1.0 vs. neutral = 0.0) that affects scoring when
   the server has just started and `current_phase` is populated. The spec accepts 1.0 as
   stated in SCOPE.md but flags the non-identical-to-pre-col-031 implication.

## Open Questions for Architect

- OQ-01 (SR-01): Exact `json_each.value` SQL form for integer JSON arrays — verify at design time.
- OQ-02 (SR-02): Does ASS-032 provide numerical grounding for `w_phase_explicit = 0.05`?
- OQ-03 (SR-04): Emit `tracing::debug!` on phase-key miss (silent degradation)?
- OQ-04 (NFR-03): Should cold-start with `current_phase = Some(...)` return `0.0` or `1.0`
  as the neutral affinity score?
- OQ-05 (SR-05): #398 PPR concurrency — check status before delivery wave.

## Self-Check

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-15, plus AC-16)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (NFR-02 tick timing, AC-13 formula)
- [x] Domain Models section defines key terms
- [x] NOT in scope section is explicit
- [x] Output file is in `product/features/col-031/specification/` only
- [x] No placeholder or TBD sections — unknowns raised as open questions
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — Entry #3163 confirmed w_phase_explicit=0.0
  placeholder origin (ADR-003, crt-026). Entry #3175 confirmed additive weight invariant
  (ADR-004). Entry #749 confirmed test fixture extension pattern. Entry #3562 confirmed
  null-phase suppression in eval JSONL (relevant to AC-16).
