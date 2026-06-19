# Alignment Report: crt-056

> Reviewed: 2026-06-19
> Artifacts reviewed:
>   - product/features/crt-056/architecture/ARCHITECTURE.md
>   - product/features/crt-056/specification/SPECIFICATION.md
>   - product/features/crt-056/RISK-TEST-STRATEGY.md
> Scope source: product/features/crt-056/SCOPE.md
> Scope risk source: product/features/crt-056/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Goals queried: #4946 (personal-cloud), #4677 (self-learning)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances #4946 (C5 parity) and #4677 (per-slug self-learning); honors principles 5, 6, 7. |
| Milestone Fit | PASS | C5 parity now; C6 (#785) and Step B explicitly deferred, not pre-built. |
| Scope Gaps | PASS | All 3 SCOPE goals + AC-1..7 + Non-Goals carried into all three docs. |
| Scope Additions | PASS | FR-10 closes OQ-5 (adapt_service/session_capabilities) per scope's own ask; no unrequested feature added. |
| Architecture Consistency | PASS | ADR-001..006 trace to SCOPE OQs/SRs; integration surface matches spec FRs. |
| Risk Completeness | PASS | All 10 SR-XX traced to R-01..12; AC-4 (N=2) load-bearing; no risk accepted without coverage. |

**Status counts:** PASS 6 · WARN 0 · VARIANCE 0 · FAIL 0

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | All three SCOPE goals, AC-1..AC-7, and the four Non-Goals appear in all source docs. |
| Addition | (none) | The only material additions (FR-10 closing `adapt_service`/`session_capabilities`) are resolutions of SCOPE OQ-5, explicitly delegated to the design session by SCOPE L168-169. Not unrequested scope. |
| Simplification | Serial loop over resident slugs | Rationale: SCOPE Non-Goals + Constraints — "correct for modest N," Step B deferred as additive follow-up. Carried consistently (NFR-1, C-1, R-09). |
| Simplification | Global config only (no per-slug overrides) | Rationale: per-slug custom config is #785/C6. Enforced by FR-9, C-7, guarded by R-05.3. |

## Variances Requiring Approval

None. No VARIANCE or FAIL findings.

The design tracks the highest-risk vision-alignment trap for this feature class (prior pattern
#3742: "optional future branch must match scope intent — WARN if architecture/risk treat a
deferred branch as in-scope with test requirements"). crt-056 handles the Step B forward-seam
the *correct* way (the Option-B resolution that closes that WARN): all three documents
consistently state Step B is (1) out of scope, (2) not implemented, (3) zero test scenarios
required for the scheduler, and (4) a recommended follow-up. The `BackgroundJob` seam *itself*
(AC-7) is in scope per the SCOPE north star — it is the shape, not the scheduler — so no WARN
arises.

Three items are flagged in the docs for **human confirmation** (not variances — the design
already states the recommended position; listed here for visibility):

1. **HQ-1 / OQ-3 cadence envelope** (SPEC HQ-1, ARCH OQ-3, A4): serial tick is accepted for
   "modest N." Confirm no near-term large-N OSS deployment expectation. Recommendation in docs:
   accept; revisit Step B priority only if large N is expected. Aligned with #4946 ("extend,
   never re-architect").
2. **OQ-5 parity closure** (ARCH OQ-1-for-human, ADR-006): `session_capabilities` in the parity
   checklist, `adapt_service` per-slug-independent. Recommendation in docs: in for capabilities,
   independent for adapt. Consistent with the C5 "first-class Unimatrix" claim.
3. **A2 interior-mutability re-verification** (ARCH OQ-2-for-human, R-04): confirm shared `Arc`s
   (`nli_handle` etc.) carry no interior-mutable read-path state. This is a correctness/Step-B
   precondition, covered by AC-2 + an audit (R-04). Architectural diligence, not a scope/vision
   deviation.

## Detailed Findings

### Vision Alignment — PASS

The feature is squarely on-vision. Evidence:

- **Goal #4946 (personal-cloud), C5.** Goal success criterion: "Multi-PROJECT, multi-CLIENT is
  the destination — one cloud serves N projects ... each fully isolated (own DB, vector index,
  hash chain, **analytics**)." vnc-034 shipped routing + per-slug stores but, per SCOPE Problem
  Statement, left per-slug **analytics un-maintained** and serving in **test-config mode**.
  crt-056 closes exactly that gap so "a registered slug is a *first-class* Unimatrix, not a
  degraded one" (SCOPE L4-5). This is the literal completion of the goal's isolation criterion.

- **Goal #4677 (self-learning).** Intent: "Every deployment improves its own retrieval quality
  from actual usage." SCOPE L26: "The self-improving half of the product is dead per-slug." Wave
  2 (FR-13, AC-3) makes the tick rebuild each slug's analytics from its own store — restoring the
  self-learning surface per project. Notably the feature adds **no new scoring math** (SCOPE
  Non-Goal, FR-13, C-8), so it advances the goal by *delivering existing* learning per-slug, not
  by expanding it — correctly scoped.

- **Architectural principle 7 (in-memory hot path).** FR-15/NFR-4/C-5: the tick writes the same
  per-slug handles the serving path reads; no DB reads at query time. Explicitly preserved.

- **Architectural principle 6 / single isolation seam (#4946 "one isolation seam across local
  AND cloud").** FR-7/NFR-5/C-6 require the single-project daemon and per-slug servers to
  traverse the **same** parity construction — "no cloud-only code path the local single-project
  install never exercises" (vnc-034 ADR-003). The additive `Option<ServiceLayer>` constructor
  (ADR-001) is designed so `None` is unit-test-only and both real paths converge. This directly
  honors the goal's seam invariant.

- **Architectural principle 5 (graceful degradation).** RISK Failure Modes: "If the tick hasn't
  run yet, serving reads clean-default handles (degraded but correct), never another slug's
  state." Absent maintenance = previous behavior, not broken behavior.

- **Goal #4946 integrity boundary.** RISK Security section correctly elevates AC-4 to a
  cross-tenant data-isolation proof: "a cross-slug handle write would leak slug A's analytics
  into slug B's serving results ... not merely a bug." This aligns the corruption guard with the
  goal's "1 client : 1 project is a knowledge-INTEGRITY boundary" framing — and does so *without*
  over-elevating defensiveness to a new goal (consistent with the standing lesson on not
  overstating defensive structure).

### Milestone Fit — PASS

The feature targets the present capability (C5 parity) and **declines** to build forward:

- Step B (bounded pool, LRU residency/eviction, per-project cadence, concurrent rayon) is OUT in
  all three docs (SCOPE Non-Goals; SPEC NOT-in-Scope, NFR-1, NFR-9, C-1, C-2; ARCH ADR-004/005;
  RISK R-09). `ResourceClass` is explicitly a "declaration only ... NO scheduler reads it"
  (ARCH §6) — a forward hook, not forward machinery.
- Per-slug CUSTOM config (#785 / C6) is OUT (FR-9, C-7, R-05.3).
- The seam that *is* built (AC-7) is the SCOPE-stated north star (SCOPE L42-46), justified as
  what makes the eventual scheduler "contained, additive ... ZERO work-unit changes." Building
  the seam now is the milestone-appropriate decision, not future-building.

This is textbook milestone discipline: build to parity, leave a clean seam, do not pre-build the
scheduler. Prior pattern #3742 is the relevant guardrail and the docs satisfy its Option-B
closure consistently.

### Architecture Review — PASS

- ADR index (ARCH §4) maps every ADR to the SCOPE open questions / scope risks it resolves:
  ADR-001→OQ-4/SR-03, ADR-002→SR-05/06/A2/A3, ADR-003→OQ-1/SR-01/07/08, ADR-004→OQ-2/SR-04/07,
  ADR-005→OQ-3/SR-02/09/A4, ADR-006→OQ-5/SR-05. No ADR introduces capability beyond SCOPE.
- The central pivot — "the per-slug `ServiceLayer` IS the per-project work-unit's state," one
  handle set owned by the `ServiceLayer`, referenced by both serve and tick (ARCH L34-39) —
  collapses SR-01/SR-07/SR-08 into one structural decision exactly as the SCOPE-RISK Design
  Recommendation #1 prescribed.
- Integration Surface (ARCH §6) gives concrete signatures matching SPEC FR-1..FR-18 (e.g.,
  `Option<ServiceLayer>` for FR-7, params-at-end threading for FR-1, `PerSlugTickContext` /
  `BackgroundJob` / `Cadence` / `ResourceClass` for FR-11/16). No drift between architecture and
  spec on names or contracts.
- ARCH explicitly keeps `run_single_tick` / the 9 ops "reused per-slug as-is," re-verifying A1
  (no global store singleton) — consistent with SCOPE Background Research and SPEC NFR-8.

### Specification Review — PASS

- All seven SCOPE acceptance criteria (AC-1..AC-7) are reproduced verbatim in SPEC §Acceptance
  Criteria with verification methods, each cross-referencing FRs and SRs.
- FRs cover both waves and every SCOPE goal; Non-Goals are re-stated as explicit exclusions
  (SPEC NOT-in-Scope) plus constraints C-1..C-10.
- OQ resolutions (OQ-1, OQ-3, OQ-5) are recorded with rationale; OQ-2/OQ-4 correctly left to the
  architect; HQ-1 correctly escalated to the human. This matches the SCOPE Open Questions
  delegation exactly — no question silently resolved or dropped.
- FR-10 (the one substantive design choice beyond raw threading) is a resolution of SCOPE OQ-5,
  which SCOPE itself flagged "for the design session" — so it is *requested* design work, not a
  scope addition. Recorded as PASS under Scope Additions.

### Risk Strategy Review — PASS

- All ten SCOPE-RISK SR-XX map to R-01..R-12 (RISK Scope Risk Traceability table); the doc
  asserts "All ten SR-XX risks trace to an architecture risk (none accepted-without-coverage)."
- The load-bearing contract proof (AC-4 at N=2) is correctly identified as Critical and called
  out as non-substitutable by N=1 — directly honoring SCOPE-RISK SR-07 and the #4974 ceremonial-
  seam precedent. This is the single most important alignment guarantee for the feature and the
  risk strategy treats it as such.
- Step B leakage (R-09) is an *audit/reject* risk with no test scenario that would require
  building the scheduler — the correct posture that keeps the deferred branch out of the test
  surface (the #3742 trap is avoided).
- Security section is proportionate and accurate: no new external input surface; the dominant
  risk is cross-slug analytics bleed, covered by AC-4. Consistent with the integrity framing of
  goal #4946 without over-claiming a security goal.

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns -- found #3742 (optional future
  branch must match scope intent; WARN unless all three docs consistently defer with zero test
  scenarios). Directly applicable to crt-056's Step B forward-seam; the docs satisfy its
  Option-B closure, so no WARN. Also reviewed #3337, #3158, #2298 (less relevant).
- Stored: nothing novel to store -- crt-056 is a clean instance of the already-stored #3742
  pattern (correctly handled), not a new misalignment type. The variances are feature-specific
  (none requiring approval); no generalizable cross-feature pattern emerged.
