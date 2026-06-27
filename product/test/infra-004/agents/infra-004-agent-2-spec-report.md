# Agent Report — infra-004-agent-2-spec

**Role:** Specification writer
**Deliverable:** `product/test/infra-004/specification/SPECIFICATION.md`
**Status:** Complete

## Summary
Produced SPECIFICATION.md from SCOPE.md + SCOPE-RISK-ASSESSMENT.md. All 15 ACs
(AC-01..AC-15) carried forward with explicit verification methods; D-1/D-2/D-3 and
SR-01..SR-09 folded into constraints, NFRs, and open questions.

## Key decisions / interpretations
- Tri-state exit-code mapping modeled as ubiquitous-language domain primitive and
  pinned to `release-gate-lib.sh` (D-1) as single source of truth.
- AC verification methods made concrete: git diff, stub-seam truth-table run,
  `needs:`-graph assertion, the dispatch cold-model run/log, verify-by-name marker grep.
- SR-04 (blast radius), SR-05 (never-green-on-a-tag), SR-06 (byte-identical
  provenance), SR-07 (chronic-INFRA vigilance) elevated to constraints + open questions.
- Change confinement pinned to exactly three files (NFR-5 / AC-15).

## Open questions raised
- OQ-1: harness/setup-step failure classification once lane is blocking (SR-04) → architect.
- OQ-2: GHCR `:latest-<arch>` push from non-default feature branch (SR-09 / D-2) → architect, verify early.
- OQ-3: chronic-INFRA — human-vigilance only vs automated escalation (SR-07) → human.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #5347 (isolation-gate kernel), #5348
  (infra-003 correction lesson), #5161 (N3 capability, partial→proven). Read-only tier;
  no storage.
