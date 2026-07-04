# Agent Report — crt-057-synthesizer-v3

**Role:** Synthesizer (re-compile) · **Task:** regenerate IMPLEMENTATION-BRIEF.md + ACCEPTANCE-MAP.md for the ass-091 redesign; update GH #894.

## Deliverables

- `product/features/crt-057/IMPLEMENTATION-BRIEF.md` — regenerated for the non-destructive `transcript{}` contract.
- `product/features/crt-057/ACCEPTANCE-MAP.md` — SPEC AC-01..AC-19 (authoritative) mapped to verifying risk scenario(s) + Scope-AC trace column.
- GH Issue #894 body — replaced boolean-contract framing with the new `transcript{}` contract / no purge verb: https://github.com/dug-21/unimatrix/issues/894

## AC coverage

- **19 SPEC ACs.** 18 have a directly-verifying risk scenario.
- **AC-19 (ownership boundary — negative, NG-5) flagged: no dedicated risk scenario.** The risk register has no SR/R risk for NG-5; SR-06→R-18 covers the *NG-7* line (distilling INTO the summary), a different negative. R-18 sc.1 is adjacent only. Recommend the tester add a schema-shape + no-cross-GH-block-synthesis negative scenario. Low severity (fences scope out).

## Delivery-critical facts carried into the brief

Orphan deletion of `purge_cycle_transcripts` + `clear_transcripts_for_feature` + `purge_held_for_feature` (R-06/SR-10, anti-stub) with exhaustive `TranscriptRetention` re-home; four-site seam now gates only the fold read with the ×4 purge-count / attach-before-purge assertions deliberately removed with rationale (R-07/R-11); silent-false-negative top Critical (R-01); clock normalization (R-05); scoped-filter correctness (R-09); residency/no-new-persistence (R-03); 5-site atomic unit + two-protocol lifecycle blast radius (R-02/R-04); pre-flight: SR-12 rebase, OQ-4 vestigial-name decision, `"summary"` DROP consumer sweep (R-12).

## Residual gaps / contradictions across the artifact set

1. **AC-19 risk-coverage gap** (above) — the only substantive gap.
2. **ADR-003/ADR-004 file slugs are boolean-era** ("warn-and-proceed-on-incomplete-extraction", "flag-gating-lockstep") though content is reworked. Cosmetic; flagged in the brief so downstream agents don't read the filenames as the design. Optional rename.
3. **No cross-artifact contradiction found.** SCOPE / SPEC / ARCHITECTURE / ADR-001..006 / RISK-TEST-STRATEGY / ALIGNMENT-REPORT agree on the three axes, ±120 s/±3-block window default, four-site fold-only gating, both-protocols merge→close→retro, and the 1:1 SCOPE↔SPEC AC-01..17 mapping. ALIGNMENT verdict PASS 5 / WARN 1 (WARN = `"summary"` DROP breaking change, delivery sweep required).
