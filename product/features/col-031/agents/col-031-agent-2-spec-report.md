# Agent Report: col-031-agent-2-spec

Agent: col-031-agent-2-spec
Feature: col-031
Role: Specification Writer
Date: 2026-03-27

## Output

`product/features/col-031/specification/SPECIFICATION.md`

## Self-Check

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-16)
- [x] AC-17 added for SR-06 (phase_affinity_score doc comment, verifiable)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (500 lines, bit-for-bit score identity, f32 range)
- [x] Domain Models section defines: PhaseFreqTable, PhaseFreqTableHandle, PhaseFreqRow, Phase, Category, Rank Score, Cold-Start, w_phase_explicit
- [x] NOT in scope section is explicit (14 exclusions)
- [x] Output file in `product/features/col-031/specification/` only
- [x] No TBD sections; unknowns are flagged as open questions (OQ-01 through OQ-04)
- [x] Knowledge Stewardship report block included

## Key Requirements Summary

1. **PhaseFreqTable** — new in-memory struct in `services/phase_freq_table.rs` (≤ 500
   lines) keyed by `(phase, category)`, rank-normalized, rebuilt each tick from
   `query_log`.
2. **Two cold-start behaviors**: fused scoring contributes `0.0` via `use_fallback`
   guard; PPR caller receives `1.0` via `phase_affinity_score` directly.
3. **SR-01 encoded**: `PhaseFreqTableHandle` is a required non-optional constructor
   parameter; all `SearchService::new` sites must be grepped before declaring wiring
   complete.
4. **SR-03 encoded as NFR-05 and CON-10**: AC-12 PASS requires AC-16 in the same or
   preceding wave with verified non-null `current_phase` in scenario output — hard gate
   ordering constraint.
5. **SR-06 encoded as AC-17**: `phase_affinity_score` doc comment must name both callers
   (PPR and fused scoring) and their respective cold-start contracts — verifiable in
   code review.
6. **w_phase_explicit raised from 0.0 to 0.05** — additive outside six-weight constraint;
   sum-check comment updated to `0.95 + 0.02 + 0.05 = 1.02`.
7. **AC-16 in scope** — eval harness extract.rs fix is a prerequisite for a non-vacuous
   AC-12 regression gate.

## Open Questions

- **OQ-01**: TypedGraphStateHandle pattern drift — confirm current `run_single_tick`
  lock pattern matches SCOPE.md template before background.rs integration.
- **OQ-02**: SearchService construction sites — grep required before wiring declared
  complete (SR-01 mitigation).
- **OQ-03**: col-030 baseline validity with `w_phase_explicit=0.05` — re-confirm thresholds.
- **OQ-04**: lookback_days per-environment override mechanism — confirm `InferenceConfig`
  TOML deserialization is the intended path.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 15 entries returned; key relevant:
  #3679, #3683 (col-031 ADRs), #3677 (affinity absent=1.0 pattern), #3555 (eval
  harness gap), #3565 (phase soft vocabulary ADR), #3519 (col-028 phase column ADR).
  All consistent with SCOPE.md; no contradictions.
