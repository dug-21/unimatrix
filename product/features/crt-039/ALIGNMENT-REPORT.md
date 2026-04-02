# Alignment Report: crt-039

> Reviewed: 2026-04-02
> Artifacts reviewed:
>   - product/features/crt-039/architecture/ARCHITECTURE.md
>   - product/features/crt-039/specification/SPECIFICATION.md
>   - product/features/crt-039/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-039/SCOPE.md
> Risk source: product/features/crt-039/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly enables W1-1 graph edge accumulation; prerequisite for Group 3 as roadmapped |
| Milestone Fit | PASS | Wave 1 infrastructure work; typed relationship graph (W1-1) depends on Informs edges accumulating |
| Scope Gaps | PASS | AC-13 explicit subtraction confirmed as intended by human (stronger than SCOPE.md wording, resolves SR-03 cleanly). OQ-01 resolved: `config` dropped from signature. |
| Scope Additions | PASS | No items in source docs extend beyond SCOPE.md intent |
| Architecture Consistency | PASS | `apply_informs_composite_guard` signature inconsistency resolved: spec updated to drop `config` parameter, matching ARCHITECTURE.md integration surface table. OQ-01 closed. |
| Risk Completeness | PASS | All 7 SCOPE-RISK-ASSESSMENT risks addressed with traceability table; 12 risks registered with test scenarios and coverage requirements |

**Overall status: PASS** — All checks pass. Two WARNs from initial review resolved post-synthesis: OQ-01 closed (spec updated), AC-13 confirmed as intended. No VARIANCE or FAIL.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `config` parameter in `apply_informs_composite_guard` | SCOPE.md AC-03 specifies `nli_scores` removal only. Spec (OQ-01) asks architect to decide on `config` parameter. ARCHITECTURE.md integration table shows the parameter removed ("config removed — remaining guards need no config fields") but the spec's "after" code block (page 300) still shows `config: &InferenceConfig` in the function signature. This is a minor internal inconsistency across architecture and spec, not a scope gap. |
| Simplification | Pre-deployment corpus scan (SR-02) explicitly downgraded from blocking requirement | SCOPE-RISK-ASSESSMENT recommends a corpus pre-condition gate at 0.45 vs 0.50. Spec's "NOT In Scope" section states this is "recommended by SR-02 but not a blocking requirement." This is an acceptable simplification because FR-14 observability log and AC-11 eval gate provide quantitative backstops. Rationale is documented. |
| Addition | FR-14 observability log (four structured fields) | Not present in SCOPE.md Goals or Acceptance Criteria. Added by spec writer in response to SR-06. This is a beneficial addition directly addressing a scope risk recommendation — not a scope creep concern. |
| Addition | AC-12 (dedup-before-cap ordering as explicit AC) | SCOPE.md does not name this as an acceptance criterion. Added in response to SR-01. Consistent with SCOPE.md Goals and with the `MAX_INFORMS_PER_TICK` constraint. |
| Addition | AC-13 (explicit Phase 4 set subtraction for mutual exclusion) | SCOPE.md §"Change 2" states mutual exclusion is "handled by candidate set separation" without requiring explicit subtraction. Spec writer upgraded this to a mandatory explicit subtraction with test (TC-07). This strengthens the implementation guarantee beyond what SCOPE.md specified and resolves SR-03. |

---

## Variances Requiring Approval

None. All checks resolve to PASS or WARN. The WARN items are noted below for human awareness but do not require blocking approval.

---

## Detailed Findings

### Vision Alignment

The vision describes a "typed knowledge graph [that] formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency." It also identifies "intelligence pipeline is additive boosts, not a learned function" as a High-severity gap, with W3-1 (GNN) as the roadmapped resolution. W3-1 depends on Informs edges being present in the graph.

crt-039 directly enables this: structural Informs inference via HNSW cosine (Phase 4b) has been silently disabled in every production deployment because `nli_enabled` defaults to `false`. The feature removes the misplaced NLI gate, allowing the typed graph to begin accumulating Informs relationship edges from tick 1. This is prerequisite work for Group 3 graph enrichment and ultimately for W3-1 training data quality.

The vision's W1-1 milestone ("Typed Relationship Graph") is marked COMPLETE in the vision document, but the accompanying note confirms the bootstrap edges carry `bootstrap_only=1` and require NLI confirmation or refutation. crt-039's structural Informs path writes non-bootstrap Informs edges grounded in cosine similarity and domain guards — this is consistent with the W1-1 design intent, not a contradiction of it.

**Finding**: PASS. All three source documents are aligned with the vision's graph intelligence direction. No shortcut that contradicts strategic direction was detected.

---

### Milestone Fit

crt-039 is tagged as Cortical phase (learning/drift). It is prerequisite work for Group 3 (graph enrichment), which is Wave 1 infrastructure. The feature does not reach into Wave 2 deployment concerns, Wave 3 GNN concerns, or behavioral signal infrastructure (Groups 5/6). All three source documents explicitly list Group 3 items in their Non-Goals or exclusion sections.

The SCOPE.md §"Non-Goals" list is faithfully reproduced in the spec's "NOT In Scope" section. The architecture and risk strategy do not introduce any future-milestone capability.

**Finding**: PASS. The feature targets the correct milestone footprint. No future-milestone scope addition detected.

---

### Architecture Review

The architecture document is well-structured and resolves all four SCOPE.md design decisions (D-01 through D-04):

- D-01 (Option A vs B): Correctly selects Option B with clear rationale. The control flow diagram (Phase 4b → Path A / Path B gate) is unambiguous and matches the spec's FR-02/FR-03 requirements.
- D-02 (Guards 4 and 5 removal): Correctly removes both, with SR-03 addressed by explicit candidate set subtraction described in the Phase 4b section.
- D-03 (Rayon pool floor): Correctly retains pool floor at 4 when `nli_enabled=false`. NFR-01 matches.
- D-04 (Module rename deferred): Consistent with SCOPE.md and spec FR-12.

**One internal inconsistency (WARN)**:

The ARCHITECTURE.md Integration Surface table (line 276) shows `apply_informs_composite_guard` with signature `(candidate: &InformsCandidate) -> bool` — both `nli_scores` and `config` removed. However, the spec's "After crt-039" code block (SPECIFICATION.md line 319-327) retains `config: &InferenceConfig` in the function signature. The spec's narrative text (line 335-337) acknowledges this as an open question: "config parameter may be retained for future extensibility or dropped if unused." OQ-01 explicitly asks the architect to decide before pseudocode, but the architecture's integration table treats it as already decided (removed).

This creates a minor ambiguity for the implementor: the architecture says `config` is removed; the spec says `config` is present but possibly unused. The implementor must choose. Neither choice introduces risk, but the disagreement between architecture and spec on the function signature could cause the implementor to assert against the wrong signature in a test.

**Finding**: WARN. Architecture and spec disagree on whether `config` is present in the `apply_informs_composite_guard` signature after the refactor. The architect should clarify before implementation begins. Recommendation: the architecture's integration table is the more authoritative source for signatures (it is the ADR-002 consequence); spec should be updated to match.

---

### Specification Review

The specification is thorough and directly traceable to SCOPE.md. All 11 SCOPE.md acceptance criteria (AC-01 through AC-11) are reproduced verbatim or with additional verification detail in SPECIFICATION.md (AC-01 through AC-11). Seven additional ACs (AC-12 through AC-18) are added to address scope risks SR-01 through SR-06, all with clear rationale.

The functional requirements (FR-01 through FR-14) cover every SCOPE.md goal. FR-14 (observability log) is an addition not in SCOPE.md but recommended by SR-06 and clearly justified. The non-functional requirements (NFR-01 through NFR-07) are consistent with SCOPE.md constraints.

The test specification is complete: 7 new tests (TC-01 through TC-07), 3 updated tests (TC-U01 through TC-U03), and 3 removed tests (TR-01 through TR-03) are all named with precise setup/assert/verify language. Traceability from each test to AC and SR identifiers is present throughout.

**One scope alignment note (WARN)**:

AC-13 (explicit Phase 4 Supports-set subtraction) goes beyond what SCOPE.md specifies. SCOPE.md §"Change 2" states mutual exclusion is "handled by candidate set separation between Phase 4 and Phase 4b" without prescribing explicit subtraction. The spec upgrades this to a mandatory explicit subtraction with a unit test (TC-07). This is stronger than SCOPE.md intended and resolves SR-03 more rigorously. This is an acceptable and beneficial scope refinement, not a violation — but it is noted because it changes the implementation obligation from "rely on threshold arithmetic" to "explicitly subtract the Phase 4 candidate set."

If the implementor relies on the SCOPE.md wording rather than AC-13, the implementation would be accepted by SCOPE.md criteria but would fail AC-13's TC-07. The spec's position (explicit subtraction required) is technically correct and should be the definitive standard.

**Finding**: WARN (minor, non-blocking). The explicit subtraction requirement (AC-13 / FR-06) is stronger than SCOPE.md specified, and this divergence is not flagged in the spec itself. It should be called out so the human can confirm the stricter guarantee is intended.

---

### Risk Strategy Review

The risk-test strategy is thorough. The 7 SCOPE-RISK-ASSESSMENT items (SR-01 through SR-07) all appear in the scope risk traceability table at the end of the document, with explicit resolution status for each. All 4 Critical risks (R-01 through R-04) have named integration tests, structural review requirements, and gate conditions. The 4 High risks have eval gates or compiler enforcement. The 4 Medium risks have existing-test coverage or code inspection checks.

The knowledge stewardship block in the risk document shows evidence of Unimatrix queries producing directly applicable prior lessons (#3579, #2758, #2577, #3937, #3675, #3949, #3723, #3437, #3441). The risk document correctly applied these lessons to severity and coverage decisions.

No risk is under-scoped relative to the feature's scope. No test scenario claims to cover a risk that is not registered. The failure modes section is especially strong — it covers the expected production case (`get_provider()` returns `Err`) as the primary success path, correctly distinguishing it from an error state.

**Finding**: PASS. Risk coverage is complete, appropriately prioritized, and consistent with architecture and specification. The traceability table closes every SR item with a definitive resolution.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns -- Found #3742 (optional future branch architecture must match scope intent — WARN if architecture and risk diverge from scope deferral), #3337 (architecture diagram informal headers diverge from spec — testers assert against wrong strings), #2298 (config key semantic divergence between vision example and implementation). Applied: #3742 pattern informed the `apply_informs_composite_guard` config-parameter inconsistency finding; #3337 confirmed the importance of flagging architecture/spec format divergences even when subtle.
- Stored: nothing novel to store — the `apply_informs_composite_guard` signature inconsistency (architecture removes `config`, spec retains it as open question) is a feature-specific OQ that was not definitively resolved before handoff. This pattern (architect leaves OQ open that affects the integration surface table) would require confirmation across a second feature before warranting a stored pattern.
