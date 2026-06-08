# crt-052 Architect Ratification Report — Gate 3a Contract Additions

> Agent: crt-052-agent-arch-ratify | Feature: crt-052 (GH #689)
> Scope: RATIFICATION of the 4 Gate-3a-flagged contract additions (SOUND, awaiting one-line architect
> confirmation). Not a redesign. Each decision recorded in the binding docs + Unimatrix.

## The Four Ratification Decisions

1. **`SessionLossInfo.dropped_candidates: u64` — CONFIRMED (add).** A content-free count of candidates a
   session lost to the per-session OR per-cycle aggregate cap. Required to satisfy AC-08 no-silent-cap-drop;
   the original struct could not surface aggregate-cap drops. Rides the same response-transient,
   never-persisted path (AC-06). C6 populates it.

2. **`readopt(session_id, registering_feature_cycle)` 2-arg — CONFIRMED (supersedes 1-arg);
   `hold_on_drain(session_id, arc, feature_cycle)` 3-arg — CONFIRMED.** SR-02/AC-11(b)/R-01 loud
   re-adoption on cycle MATCH is impossible without the re-registering cycle as a caller-supplied input.
   The earlier ADR-008 / ARCH §4 1-arg `readopt` form is replaced. The 3-arg `hold_on_drain` (already in
   ADR-008 §Decision) is now also reflected in the binding ARCH §4 row.

3. **`transcript_fallback_hole_fraction: f64` (default 0.5) — CONFIRMED as a CONFIG KNOB, not a
   compile-time constant.** ADR-006 already names the threshold a "configured fraction" / "tuning
   parameter that must be boundary-tested." Making it a `RetentionConfig` knob (merge + `validate()`
   range-check [0.0,1.0], same pattern as the other knobs) lets dogfooding calibrate it against real
   ring-tail overflow without a rebuild and keeps all distillation tuning on one config surface.

4. **`TranscriptCandidate` Debug — RULED: Debug MAY show `text`.** `text` IS the response content the
   agent consumes; it structurally cannot reach a persisted/log surface (ADR-004 — the AC-06 leak gate
   tests SQL/log/audit/persisted surfaces, where candidates never land). The R-19 metadata-only-Debug
   rule targets the SNAPSHOT / held-buffer types only (`TranscriptSnapshot`/`HoleInfo` per ADR-002,
   `HeldBuffer` per ADR-008). `TranscriptCandidate` MAY `derive(Debug)`. This closes the
   response-types.md pseudocode↔test-plan contradiction in favor of the test-plan position
   (`test_candidate_debug_present_text_is_intentional`). Will not re-open at Gate 3b.

## Files Updated

- `product/features/crt-052/architecture/ARCHITECTURE.md` — §4 Candidate row (Debug-may-show-text note),
  SessionLossInfo row (+dropped_candidates), Held store row (readopt 2-arg / hold_on_drain 3-arg); §2 C9
  row (+transcript_fallback_hole_fraction).
- `product/features/crt-052/architecture/ADR-007-loss-visibility-provenance.md` — dropped_candidates field
  + population rule + TranscriptCandidate Debug ruling.
- `product/features/crt-052/architecture/ADR-008-option-b-held-buffer-store.md` — readopt 2-arg signature
  + ratification line.
- `product/features/crt-052/architecture/ADR-006-fallback-trigger-and-topic-source.md` — hole-fraction
  config-knob ratification line.
- `product/features/crt-052/pseudocode/response-types.md` — Debug section rewritten to ratified ruling;
  dropped_candidates contract-note marked RATIFIED.
- `product/features/crt-052/pseudocode/held-buffer-store.md` — readopt / hold_on_drain notes marked
  RATIFIED.
- `product/features/crt-052/pseudocode/config-knobs.md` — transcript_fallback_hole_fraction promoted to a
  ratified Wave A field + default fn + validate() range check.

## Unimatrix Corrections (provenance preserved via context_correct)

| ADR | Old ID | New ID | Change |
|-----|--------|--------|--------|
| ADR-006 | #4852 | **#4858** | hole-fraction = config knob (not const) |
| ADR-007 | #4853 | **#4856** | +dropped_candidates; TranscriptCandidate Debug may show text |
| ADR-008 | #4854 | **#4857** | readopt 2-arg (supersedes 1-arg); hold_on_drain 3-arg confirmed |

ADRs 001–005 / 009 (#4847–4851, #4855) unchanged.

## Open Questions

None introduced. The pre-existing ARCH §7 open questions (hold-cap/cycle-cap default tuning, audit
consumer survey, fixture independence) are unaffected by this ratification and remain spec/delivery
obligations.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_lookup(topic=crt-052,category=decision) + context_get(#4852,#4853,#4854) -- retrieved the live crt-052 ADR set (#4847-4855) and confirmed the three ADRs needing correction; no conflicting prior decisions in adjacent features (vnc-025 #4740/#4742, crt-028 #3335 reviewed, no supersession).
- Stored: corrected entries #4856 (ADR-007), #4857 (ADR-008), #4858 (ADR-006) via context_correct (chained from #4853/#4854/#4852) -- nothing additionally novel/cross-feature to store; these ratifications are crt-052-specific contract refinements already captured in the chained ADRs.
