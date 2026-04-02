# Agent Report: crt-039-agent-2-spec

**Agent**: crt-039-agent-2-spec  
**Role**: Specification Writer  
**Feature**: crt-039 — Tick Decomposition: Decouple Structural Graph Inference from NLI Gate  
**Output**: `product/features/crt-039/specification/SPECIFICATION.md`

---

## Summary

SPECIFICATION.md produced and ready for downstream agents (architect, pseudocode, tester, risk
strategist). All SCOPE.md acceptance criteria are present and carry their original AC-IDs. All
four scope risks flagged by the Scope Risk Assessment (SR-01, SR-02, SR-04, SR-05) are resolved
as named acceptance criteria and test cases.

---

## Key Decisions Made

**D-01 (Mutual exclusion mechanism — SR-02/SR-03)**: Spec requires explicit Phase 4 candidate
set subtraction from Phase 4b candidates. This is not disjoint by construction — the cosine
ranges `[nli_informs_cosine_floor=0.5, supports_candidate_threshold=0.65]` overlap, so pairs
with cosine in `[0.5, 0.65]` qualify for both phases. The spec names this as an explicit
exclusion invariant (FR-06, AC-13, TC-07) rather than leaving it implied.

**D-02 (SR-05 split — two tests, not one)**: The old `test_run_graph_inference_tick_nli_not_ready_no_op`
is named for removal (TR-01). It is replaced by exactly two tests: TC-01 (Phase 4b CAN write
Informs when NLI not ready) and TC-02 (Phase 8 does NOT write Supports when NLI not ready).
Both are positive assertions; TC-01 in particular requires at least one Informs edge written,
not merely no failure.

**D-03 (Phase 8b write path — left open for architect)**: The exact control flow restructuring
for Option B (Phase 8b iterating `informs_metadata` directly rather than `merged_pairs`) is
deferred to the architect as OQ-02. The spec describes the intent and the constraint (Phase 8b
must not depend on NLI scores) but does not prescribe the loop structure.

**D-04 (Observability as AC, not just recommendation)**: SR-06 (no signal to distinguish floor
too high vs all-deduped vs cap applying) is promoted from a risk recommendation to a mandatory
functional requirement (FR-14) and acceptance criterion (AC-17). The four log fields are named
explicitly.

**D-05 (config parameter retention — open question)**: After removing guards 4 and 5 from
`apply_informs_composite_guard`, the `config: &InferenceConfig` parameter is unused. Spec
surfaces this as OQ-01 for the architect to decide before pseudocode — the spec does not
prescribe removing it.

---

## Self-Check

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-11 present with SCOPE.md traceability)
- [x] Every functional requirement is testable (FRs reference specific verification methods or test cases)
- [x] Non-functional requirements include measurable targets (NFR-02 throughput bound, NFR-05 MRR gate)
- [x] Domain Models section defines key terms and before/after signatures for apply_informs_composite_guard
- [x] NOT in scope section is explicit (9 exclusions listed)
- [x] Output file is in `product/features/crt-039/specification/` only
- [x] No placeholder or TBD sections — four open questions flagged for architect
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 18 entries returned. Most load-bearing: entry #3937 (pattern describing the neutral score tap for Informs — the exact mechanism being removed), #3949 (testing pattern confirming independent negative tests per composite guard predicate), #3971 (bugfix-473 decisions on independent Informs/Supports caps), #3713 (lesson: threshold tuning is blind without log coverage — drove FR-14 to mandatory status), #3957 (lesson: cross-feature guard conflating concerns — directly relevant to guard 3 semantics).
