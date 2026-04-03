# Alignment Report: crt-042

> Reviewed: 2026-04-02
> Agent ID: crt-042-vision-guardian
> Artifacts reviewed:
>   - product/features/crt-042/architecture/ARCHITECTURE.md
>   - product/features/crt-042/specification/SPECIFICATION.md
>   - product/features/crt-042/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-042/SCOPE.md
> Scope risk: product/features/crt-042/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances Wave 1A intelligence pipeline goals |
| Milestone Fit | PASS | Correctly targets Wave 1A; no premature Wave 3 scope |
| Scope Gaps | WARN | One scope AC not reflected in the specification (AC-13 validation wording shift) |
| Scope Additions | WARN | Architecture adds a latency ceiling (P95 < 50ms) and open questions not present in SCOPE.md |
| Architecture Consistency | PASS | Internal consistency strong; all ADRs present; combined ceiling documented |
| Risk Completeness | PASS | Risk register covers all SCOPE-RISK-ASSESSMENT items plus additional edge cases |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | AC-13 wording | SCOPE.md AC-13 says "always, regardless of ppr_expander_enabled"; SPECIFICATION.md FR-08 correctly preserves this intent. No divergence. |
| Addition | Latency ceiling (P95 < 50ms) | ARCHITECTURE.md defines "P95 Phase 0 latency addition < 50ms" as a gate before default enablement. SCOPE.md mentions instrumentation and measurement but sets no numeric ceiling. The ceiling is a tightening, not a conflict. |
| Addition | Open questions (OQ-01 to OQ-04) | ARCHITECTURE.md lists four open questions that remain unresolved at design time. SCOPE.md does not frame these as open. Three of the four (OQ-01/SR-03, OQ-02/SR-01, OQ-03/SR-04) have substantive scope risk entries; OQ-04 (eval gate failure owner) is named as SR-05 but SCOPE.md does not explicitly say it must be an open question in the architecture document. |
| Addition | ADR-006 behavioral traversal contract | ARCHITECTURE.md §Component 1 behavioral contract paragraph ("Given seeds {B} and edge A→B…") specifies Outgoing direction surfaces only forward-reachable entries. AC-04 in the spec confirms a backward edge (C→B where C is not otherwise reachable) does NOT surface C. SCOPE.md AC-03 says traversal is "Outgoing direction only" but is silent on the behavioral consequence for backward edges. The spec adds this behavioral precision — consistent with scope intent, but constitutes added specification. |
| Simplification | eval gate failure owner | SCOPE.md SR-05 recommends assigning an owner. ARCHITECTURE.md OQ-04 and SPECIFICATION.md OQ-04 name it but leave it open for the delivery brief. This is an acceptable deferral — both documents explicitly flag it as unresolved. |

---

## Variances Requiring Approval

No FAIL classifications identified. One WARN pair requires human awareness.

### WARN-1: Traversal behavioral contract — forward-edge-only consequence not expressed in SCOPE.md

**What**: SCOPE.md AC-03 states traversal is Outgoing-only. The architecture (Component 1 behavioral contract) and specification (FR-02, AC-04) add a precise behavioral consequence: an entry C that points TO a seed (edge C→B, C is not a seed) is NOT returned by `graph_expand` unless C is also reachable via a forward edge from another seed. This is the correct interpretation of Outgoing-only traversal, but it is a specification-level precision not stated in SCOPE.md.

**Why it matters**: If the S1/S2 Informs edges are single-direction (source_id < target_id), a seed at the higher ID is invisible from the lower-ID entry's perspective — exactly the SR-03 risk. The behavioral contract in the spec makes this failure mode explicit and testable (AC-04). This is good engineering. However, the SCOPE.md left open whether the spec should cover the backward-edge case explicitly. The spec's coverage exceeds the scope's silence.

**Recommendation**: Accept. The behavioral precision is necessary for correct implementation and test coverage. No scope change needed. Document as deliberate elaboration, not scope addition.

---

### WARN-2: Latency ceiling number (P95 < 50ms) appears in ARCHITECTURE.md but not in SCOPE.md

**What**: SCOPE.md says "Architect must wire wall-clock debug instrumentation in Phase 0 and define a latency ceiling as a post-measurement gate condition." The architect has done this — and set the ceiling at P95 < 50ms (ARCHITECTURE.md §Latency Profile). This is the architect acting on the scope's instruction. The number itself is not in SCOPE.md and was not approved by the human.

**Why it matters**: The P95 < 50ms ceiling will govern when `ppr_expander_enabled` can default to `true`. If the measurement comes in at, say, 80ms, this number means the flag stays off. The human should confirm 50ms is an acceptable ceiling before it becomes the gate condition.

**Recommendation**: Human review of the 50ms ceiling before the delivery brief locks it. If the ceiling should be higher or lower given deployment context, amend ARCHITECTURE.md §Latency Profile before delivery begins.

---

## Detailed Findings

### Vision Alignment

The product vision (Wave 1A) states: "The intelligence pipeline cannot learn from usage it cannot observe, cannot predict what agents need without knowing where they are in the cycle, and cannot close the feedback loop without knowing when retrieval fails." The vision also documents a critical gap: "Intelligence pipeline is additive boosts, not a learned function — High — Roadmapped Wave 1A + W3-1."

crt-042 directly addresses this by widening the PPR candidate pool so that cross-category entries connected via the typed graph can receive non-zero personalization mass. The vision's W1-1 (typed relationship graph) and W1-4 (NLI Supports/Contradicts edges) built the graph that crt-042 exploits. The PPR function (crt-030) and graph enrichment (crt-040, crt-041) are the upstream investments; crt-042 is the retrieval unlock that makes them produce measurable P@5 improvement.

The feature is explicitly blocked behind `ppr_expander_enabled = false` by default, with a measurement-gated path to default-on. This aligns with the vision's "Gate condition" philosophy (as used for W1-4 and W2-4).

The feature does not introduce any ML training, GNN, or Wave 3 scope. It operates on the existing in-memory `TypedRelationGraph` using the existing PPR infrastructure. This is correct milestone discipline.

**Finding: PASS.** The feature is a necessary Wave 1A step — it unlocks retrieval improvement from the graph enrichment infrastructure already shipped.

---

### Milestone Fit

Wave 1A is the "Adaptive Intelligence Pipeline" milestone. The wave goals are:
- WA-0: Ranking signal fusion (COMPLETE)
- WA-1: Phase signal (COMPLETE)
- WA-2: Session context enrichment (COMPLETE)
- WA-3: MissedRetrieval signal (DEFERRED)
- WA-4: Proactive delivery (COMPLETE)
- WA-5: PreCompact transcript restoration (COMPLETE)

crt-042 is not listed in the original Wave 1A items — it is a Cortical phase feature (crt-) targeting retrieval improvement via graph expansion. It fits between W1-1 (typed graph) and W3-1 (GNN), specifically enabling the graph-enrichment work to produce measurable retrieval signal before W3-1 replaces the manual scoring formula.

The feature does not pull in any Wave 3 (W3-1 GNN training, session context feature vector) or Wave 2 (container, HTTP, OAuth) work. It adds three `InferenceConfig` fields — backward-compatible and consistent with the config externalization principle from W0-3/dsn-001. No schema migration.

**Finding: PASS.** Milestone fit is correct. The feature is a Cortical-phase retrieval improvement, appropriately scoped to the pre-W3-1 intelligence foundation.

---

### Architecture Review

**Strengths:**
- All six ADRs are present and cross-referenced. The architecture document references them with decision numbers.
- The combined ceiling (270 entries: k=20 + Phase 0 max 200 + Phase 5 max 50) is documented explicitly in §Combined Expansion Ceiling, addressing SCOPE-RISK-ASSESSMENT SR-04.
- The latency profile section (§Latency Profile) wires the P95 < 50ms gate condition and documents the O(N) scan concern with corpus-size context (7k → 70k scaling note).
- The S1/S2 directionality finding is fully investigated in §Integration Points: the architecture confirms crt-041 writes single-direction S1/S2 edges and identifies that the S8 CoAccess edge is covered by the crt-035 promotion tick. This is a substantive advance over SCOPE.md, which flagged it as a delivery prerequisite check.
- The O(1) embedding lookup investigation path is documented (OQ-03 / §Open Questions item 3).
- Lock ordering invariant, async boundary, and `edges_of_type()` SR-01 constraint are all explicitly documented.

**Potential concern — behavioral contract paragraph in Component 1:**
The architecture's behavioral contract states:
> "Given seeds {B} and edge A→B (A Informs B), graph_expand surfaces A in the result."

This statement says a seed B with an INCOMING edge from A (i.e., A→B) surfaces A. But the traversal is Outgoing-only from seeds. An Outgoing traversal from B follows edges B→X, not edges Y→B. For A to be surfaced from seed B, the edge would need to be B→A (outgoing from B), not A→B (incoming to B).

The second line of the contract correctly states: "Given seeds {B} and edge B→C (B Informs C), graph_expand surfaces C." This is correct — B→C is outgoing from seed B.

The first line appears to describe the result of a bidirectional traversal (or the write-time symmetry pattern), but the architecture concludes Outgoing-only. This paragraph is likely attempting to explain that bidirectionality is solved at write time — if A Informs B, both A→B and B→A are written, so traversal from seed B via B→A outgoing reaches A. But the description is expressed as "edge A→B" which is the logical semantic direction, not the traversal direction. This is exactly the direction-semantics ambiguity documented in Unimatrix entry #3754 and SCOPE-RISK-ASSESSMENT SR-06.

The specification correctly avoids this by expressing the behavioral contract in terms of observable outcomes (AC-03 through AC-07) without referencing Direction:: enum values. The RISK-TEST-STRATEGY references entry #3754 and maps SR-06 to no residual testing risk. However, the architecture document itself reproduces the ambiguity in prose that a future architect or delivery agent may read.

**Finding: PASS overall.** The architecture is internally complete and addresses all scope risks. The behavioral contract prose ambiguity (entry #3754 pattern) is noted below as a WARN.

---

### Specification Review

**Strengths:**
- All 25 acceptance criteria are behavioral — no Direction:: enum references anywhere in the ACs. This directly implements the SR-06 mitigation from SCOPE-RISK-ASSESSMENT.
- AC-00 (SR-03 prerequisite gate) is present as a blocking constraint before Phase 0 code is written. The constraint is echoed in C-01.
- The "four coordinated locations" requirement for InferenceConfig fields (struct body, Default, serde default function, validate()) is explicit in FR-07, referencing known problematic patterns (entries #2730, #4044, #3817).
- NFR-01 (latency instrumentation) is mandatory and tied to the flag-off-by-default contract — the flag must not default to true in this feature.
- FR-06 documents the quarantine caller responsibility as an explicit contract, addressing SR-07.
- Open questions OQ-01 through OQ-04 are carried forward from the scope risk assessment, maintaining traceability.

**Minor concern — spec AC-04 vs. architecture behavioral contract:**
SPECIFICATION.md AC-04 states: "Given seed entry B and a graph edge C→B (B is the target, not the source) — entry C does NOT appear in the graph_expand return set unless C is also reachable via a forward edge from another seed."

This is precisely the correct behavioral statement and is consistent with Outgoing-only traversal. However, the architecture's behavioral contract states the opposite for the first example ("edge A→B, graph_expand surfaces A"). The spec is authoritative here; the architecture prose is misleading. If a delivery agent reads ARCHITECTURE.md first, they may implement bidirectional traversal to match the architecture's stated example, then fail AC-04 in the spec.

The risk is not theoretical — entry #3337 (pattern stored in Unimatrix) documents exactly this: testers asserting against wrong architecture strings. In this case, the impact is on implementation rather than tests.

**Finding: PASS overall.** The spec is complete, behavioral, and well-cross-referenced. The architecture prose ambiguity is a WARN that the delivery agent must be directed to AC-04 as the authoritative traversal contract, not the architecture's behavioral contract paragraph.

---

### Risk Strategy Review

**Strengths:**
- All 7 SCOPE-RISK-ASSESSMENT risks (SR-01 through SR-07) map to RISK-TEST-STRATEGY entries. The §Scope Risk Traceability table at the end of RISK-TEST-STRATEGY makes this mapping explicit.
- R-02 (S1/S2 single-direction) is rated Critical/High and maps to the AC-00 prerequisite gate — it is a blocking gate before implementation. This correctly implements the SR-03 escalation from "delivery prerequisite check" to "hard blocking gate."
- R-08 (InferenceConfig hidden test sites) is rated High with four explicit test scenarios. Historical pattern entries #4044, #2730, #4013 are cited.
- R-10 (timing instrumentation wrong level) explicitly cites entry #3935 (gate failure where tracing tests were deferred) and states "do not defer this." This is correct knowledge application.
- R-16 (Phase 0 insertion point wrong) is rated High and has explicit test scenarios asserting expanded entries appear in Phase 1's input — this is the core correctness invariant.
- Non-negotiable tests are listed with a "gate blockers" designation, citing entry #2758 / #3579 patterns.
- The R-06 back-fill race scenario (back-fill migration running during eval) is an addition beyond the scope risk assessment and represents correct risk analysis — the eval snapshot must be taken after the migration completes.

**One gap — R-02 scenario 3 direction:**
RISK-TEST-STRATEGY R-02 scenario 3 states: "Construct a unit test with an S1/S2-style graph (one direction only, A→B, seed=B). Assert `graph_expand` returns empty." This test confirms that with a single-direction edge A→B and seed {B}, B has no outgoing edges to A — so expansion from B returns nothing. This is the correct confirmation of the failure mode. Good.

**One gap — integration ordering (Phase 0 ↔ Step 6c co-access prefetch):**
RISK-TEST-STRATEGY §Integration Risks documents: "Phase 0 ↔ Step 6c co-access prefetch ordering: Per ADR-002, Phase 0 runs before Step 6c." However, looking at the current search pipeline in SCOPE.md: Step 6c (co-access boost map prefetch) is listed after Step 6d in the existing pipeline. The architecture is inserting Phase 0 inside Step 6d. If Phase 0 runs before Step 6c, expanded entries benefit from the co-access boost — which is the stated intent (they should not get `coac_norm = 0.0`). The RISK-TEST-STRATEGY correctly documents the ordering but no test scenario covers it. This is not a blocking issue but represents a coverage gap.

**Finding: PASS.** The risk strategy is the most complete of the three source documents. All scope risks are mapped; all SCOPE-RISK-ASSESSMENT recommendations are implemented. The Phase 0 ↔ Step 6c ordering gap has no test scenario, but is an edge case within a larger integration risk section.

---

## Architecture Behavioral Contract — Clarity Issue (WARN)

**What**: ARCHITECTURE.md §Component 1 behavioral contract contains a prose example that describes the result of traversal using logical semantic direction ("edge A→B, A Informs B") rather than graph traversal direction. The example reads as if backward traversal is occurring (seed B, edge A→B, A surfaces). The actual behavior is that A is reachable from seed B only if a B→A edge exists (from write-time bidirectionality). The spec correctly expresses this in AC-04. The architecture prose is misleading.

**Why it matters**: Entry #3754 (Unimatrix knowledge base) documents that direction-semantics ambiguity in architecture docs has historically caused implementation bugs — the architecture says one thing, the spec says another, and the delivery agent implements the architecture's semantics. Pattern entry #3337 documents a related issue where architecture sample outputs differed from spec definitions.

**Recommendation**: The delivery agent must be directed to treat SPECIFICATION.md AC-04 as the authoritative traversal behavioral contract. Before Phase 0 implementation begins, the architecture behavioral contract paragraph should be amended to match the spec's language — expressing the contract in terms of observable output (entry A appears when B is a seed and edge B→A exists via write-time bidirectionality), not as "edge A→B surfaces A." This is a pre-implementation correction, not a post-ship fix.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision` — found 4 relevant entries. Most applicable: entry #3742 (optional future branch / scope addition pattern), entry #3337 (architecture diagram diverges from spec — testers assert against wrong strings), entry #3754 (direction semantics ambiguity in architecture — referenced throughout this review).
- Stored: nothing novel to store — the architecture behavioral contract ambiguity reproducing the #3754 pattern is feature-specific (crt-042) and already captured by the existing #3337 and #3754 patterns. The combination of "SCOPE.md latency ceiling instruction → architect sets a specific number" is common to multiple features and already captured generically. No new cross-feature pattern emerges from this review.
