# Alignment Report: vnc-015

> Reviewed: 2026-05-15
> Artifacts reviewed:
>   - product/features/vnc-015/architecture/ARCHITECTURE.md
>   - product/features/vnc-015/specification/SPECIFICATION.md
>   - product/features/vnc-015/RISK-TEST-STRATEGY.md
> Supporting artifacts reviewed:
>   - product/features/vnc-015/SCOPE.md
>   - product/features/vnc-015/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly delivers W1B-1 as specified in the roadmap |
| Milestone Fit | PASS | W1B-1 is the correct next item per the dependency graph |
| Scope Gaps | WARN | One SCOPE.md goal not fully addressed in source docs (edge attribution `created_by` ambiguity) |
| Scope Additions | WARN | Architecture adds `context_edge` tool; this is explicitly in SCOPE.md Goal 10 but introduces 4 open questions left to delivery that carry implementation risk |
| Architecture Consistency | PASS | Architecture is internally consistent; ADRs are referenced and closed |
| Risk Completeness | PASS | Risk-test strategy is thorough; 15 risks, 37+ scenarios, Critical/High/Medium/Low tiers |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | `RelatedTo` bidirectionality | SCOPE.md mandates bidirectional Contradicts but leaves `RelatedTo` to the architect. Spec FR-08 and domain model correctly reflect this. Rationale: acceptable discretion per SCOPE.md. |
| Simplification | Confidence floor removed | SCOPE.md Proposed Approach originally included a confidence floor. Design decisions in SCOPE.md and Architecture both confirm this was dropped. Rationale: target validation is the quality gate; confidence floor on a brand-new entry is vacuous. Consistently documented. |
| Simplification | `stale_dependency_edges` counts only `Prerequisite` edges | SCOPE.md Goal 5 says "Prerequisite/depends_on edges" — Architecture counts only `Prerequisite`. The `depends_on` variant does not exist in the codebase; this is a documentation artifact, not a gap. |
| Resolved | Edge `created_by` attribution | ADR-008 is authoritative: `GRAPH_EDGES.created_by = EDGE_SOURCE_AGENT = "agent"`, consistent with all other edge sources. Agent identity is traceable via `ENTRIES.created_by` JOIN. AC-05 and AC-18 updated in spec. OQ-2 closed. |
| WARN | Open questions 1 and 4 remain for delivery | OQ-2, OQ-3, OQ-5 closed. OQ-1 (`default_rules` caller audit — R-10 covers it) and OQ-4 (tool count test location) are delivery discovery items, not design blockers. |

---

## Variances Requiring Approval

No VARIANCE or FAIL classifications. All deviations are documented simplifications with rationale or WARNs. Items below require human awareness before delivery begins.

### RESOLVED 1: Edge `created_by` attribution — closed

**Resolution**: ADR-008 is authoritative. `GRAPH_EDGES.source` and `GRAPH_EDGES.created_by` both store `EDGE_SOURCE_AGENT = "agent"` — this is consistent with how all other edge sources are named (e.g., `EDGE_SOURCE_COSINE_SUPPORTS`). Agent identity attribution for graph edges is via `ENTRIES.created_by` on the source entry (available via JOIN), not via a `GRAPH_EDGES.created_by` field. AC-18 and AC-05 in the specification have been updated to reflect this. No provenance regression — the source entry record is the attribution anchor.

---

### WARN 2: OQ-06 / redirect atomicity — blocking potential

**What**: Architecture OQ-6 and Specification OQ-06 both ask whether "atomic" for `redirect_graph_edge` means within-transaction or within-handler (sequential). RISK-TEST-STRATEGY R-02 elevates this to Critical severity — the wrong implementation (raw BEGIN/COMMIT SQL strings across pool connections) causes silent data loss per lesson #2269. The architecture specifies the correct RAII `pool.begin().await?` pattern in the integration-test scenario but does not close OQ-6 as a decision.

**Why it matters**: R-02 is rated Critical/High in the risk register, citing a confirmed past gate failure (nxs-011). If the delivery agent does not have explicit guidance that `redirect_graph_edge` must use RAII transaction, the risk materializes. The architecture body says "redirect_graph_edge executes an atomic SQLite transaction" but the Open Questions section simultaneously asks whether atomicity means transaction or sequential. The body and the OQ contradict each other.

**Recommendation**: Close OQ-6 by updating the architecture before delivery. The correct answer is already implied by the risk strategy (RAII `pool.begin()` required) and by the ADR-009 note ("redirect is an explicit exception"). Delivery should not be asked to discover this during implementation.

---

## Detailed Findings

### Vision Alignment

vnc-015 is W1B-1 ("Typed Edge Write Path") from the product roadmap. The vision document states:

> "W1B-1: Typed edge write path — Agents declare typed relationships at write time. ADR dependency chains, goal tracing, lesson→decision provenance, and research domain evidence graphs become first-class graph records — not conventions buried in content fields."

The feature delivers exactly this. Every deliverable in SCOPE.md (edges param, context_edge tool, 10 RelationType variants, bidirectional Contradicts, stale-dependency observability, DependencyOnDeprecated rule) maps directly to the roadmap specification.

The vision also states the product is "configured not rebuilt" — the 10 new RelationType variants include both SDLC and research domain semantics, and SCOPE.md explicitly notes "six serve both SDLC and research domain semantics — no domain-exclusive variants." This is consistent with the domain-agnostic direction.

Hash chain integrity, audit log completeness, and capability-gate enforcement are all preserved. The `context_edge` tool uses `Capability::Write` (no new capability). Edge attribution is attributed to `EDGE_SOURCE_AGENT` with `created_by` tracking (subject to the WARN above).

**Verdict: PASS.** The feature is unambiguously the correct next item per the roadmap dependency graph (requires Wave 1A complete — confirmed complete per vision doc) and advances the typed graph expansion without reaching into Phase 2 scope.

---

### Milestone Fit

Wave 1A is confirmed complete in the vision doc. The dependency graph shows:

```
Wave 1A complete → Wave 1B begins
W1B-1 (Typed edge write path) → W1B-2a (context_graph)
```

vnc-015 is W1B-1. It does not introduce schema migrations (compliant with the "no schema migration" constraint). It does not deliver context_graph traversal (Phase 2 scope correctly excluded in SCOPE.md Non-Goals). PPR expansion is limited to `RelatedTo` only — all other 9 new variants (including `Advances` and `Motivates`) are write-only in this feature, with directed-edge PPR semantics deferred to Phase 2.

The feature explicitly calls out that it is "Phase 1 of the ASS-057 roadmap. Phase 2 (context_graph traversal tool) depends on this feature having populated the graph." This sequencing matches the vision's W1B-2a dependency on W1B-1.

**Verdict: PASS.** Correct milestone, correct scope boundaries, no future-milestone capability pulled forward.

---

### Architecture Review

The architecture is well-structured across 9 components. Key assessments:

**Strengths:**
- Component extraction to `edge_write.rs` (ADR-005) correctly avoids tools.rs bloat and creates a reusable pub(crate) surface.
- The validation pipeline (Phase A pre-insert, Phase B write) is clearly ordered and consistent with failure posture. ADR-002 documents the all-or-nothing approach.
- Constructor injection for `DependencyOnDeprecated` (ADR-004) follows the `PhaseDurationOutlierRule` precedent without requiring trait interface changes.
- The `redirect_graph_edge` transactional exception (ADR-009) is the correct call: a partial redirect is worse than a partial store because it deletes before confirming insertion success.
- Integration surface table is complete and accurate.

**Concern — Component numbering gap**: Architecture lists Components 1–7 and Component 9. Component 8 is absent. This is either a drafting artifact (the context_edge handler was originally Component 8 and was renumbered 9) or a missing section. It creates no functional gap but is a documentation inconsistency.

**Concern — OQ-2 not closed by ADR-008 (WARN 1 above)**: ADR-008 adopts `"agent"` convention for the `source` column but does not resolve what happens to `created_by` in `write_graph_edge`. The architecture body and the specification are in conflict on this point.

**Concern — OQ-6 not closed by ADR-009 (WARN 2 above)**: The architecture body says "redirect_graph_edge executes an atomic SQLite transaction" but then Open Question 6 asks whether the implementation needs a new transaction API. These should not coexist. The architecture should close the OQ.

**Verdict: PASS** overall for architectural coherence, with the two WARNs above requiring resolution before delivery.

---

### Specification Review

The specification is thorough and precise. Functional requirements FR-01 through FR-17 map cleanly to SCOPE.md goals. Acceptance criteria AC-01 through AC-26 are testable as written. The domain model section (EdgeInput, EdgeParams, RelationType enum, validation failure modes) is complete.

**Specific observations:**

- FR-10 (10×4 variant compliance matrix) and FR-04 (mandatory change sites) directly mitigate SR-01, the highest-probability implementation defect. This is good design.
- NFR-07 (tools.rs line count) correctly calls for verification before implementation — spec defers to architect as required.
- The "NOT in Scope" section is comprehensive and explicitly excludes context_graph, batch write, schema migration, and NLI scoping — all items that would be premature additions.
- FR-16 `context_edge` is well-specified with a full validation pipeline, mode semantics, error codes, and pure-graph-operation constraint.
- Constraint 6 ("Bidirectional Contradicts writes are fire-and-forget sequential, not a DB transaction") directly contradicts the architecture body's statement that `redirect_graph_edge` uses an atomic transaction. This divergence between spec and architecture is the same OQ-6 issue. If constraint 6 is read by a delivery agent as applying to Contradicts inserts in `validate_and_write_edges` (correct — sequential is acceptable there) but also as applying to redirect (incorrect — redirect must be transactional), data loss results.

**Verdict: PASS** with the caveat that Constraint 6 should be annotated to clarify it applies to `validate_and_write_edges` Contradicts writes, not to `redirect_graph_edge`.

---

### Risk Strategy Review

The risk strategy is the strongest of the three documents. Key observations:

- R-02 (redirect transaction pool risk) and R-01 (from_str silent drop) are both rated Critical with concrete test scenarios that can be used as delivery gates. Elevating R-02 from the SCOPE-RISK-ASSESSMENT's Medium to Critical based on lesson #2269 is the correct call.
- R-09 (self-referential check sequencing) is a subtle risk not flagged in SCOPE-RISK-ASSESSMENT. The insight that `source_id` is unknown before insert (auto-increment) and that the check must run post-insert is architecturally significant. This risk is well-documented.
- R-10 (default_rules signature change) is correctly identified as a compile-time blast — the "callers that silently pass `vec![]`" scenario is exactly the class of defect that passes compilation but produces no findings.
- Security risks section covers ownership bypass (Admin agents not exempt — correct), source_id injection, edge type string injection, and SQL injection in stale_dependency_edges query. These are complete for a daemon-local deployment tier.

**One gap**: R-09 identifies that the self-referential check must run post-insert, but neither the Architecture nor Specification closes how this is actually implemented. The Architecture says "validate before entry insert" (Phase A), and the Specification FR-05 says all three validation checks run "Before any entry is inserted." If source_id is an auto-increment, Phase A cannot check self-reference against the actual ID. R-09 flags this but the solution path is not specified anywhere in the source documents. Delivery will need to resolve: either use a pre-allocated ID approach, or the self-referential check for `context_store` is deferred to Phase B (post-insert), which contradicts both the spec and architecture text.

**Verdict: PASS** with the self-referential check sequencing gap (R-09) flagged for resolution. The risk strategy correctly identifies it but the architecture and specification leave the implementation resolution undefined.

---

## R-09 Self-Referential Check — Architectural Gap (WARN)

**What**: FR-05 / NFR-04 / Architecture Phase A all state that self-referential edge validation occurs before any entry is inserted. But for `context_store`, `source_id` is the auto-increment DB-assigned ID, which is not known until after insert. R-09 in the risk strategy explicitly identifies this: "if the implementation runs it pre-insert against a placeholder (e.g., 0 or u64::MAX), the check is vacuous." The specification and architecture do not provide a resolution — they state the requirement (pre-insert) but not how to satisfy it given the ID sequencing constraint.

**Why it matters**: A vacuous self-referential check would allow callers to store entries with self-referential edges, producing graph cycles that corrupt PPR. This is classified as Medium/Medium in the risk register but is an architectural gap, not just a test gap.

**Recommendation**: The architecture should document one of two resolutions before delivery: (A) pre-allocate the entry ID (if the store supports it) and run Phase A with the real ID, (B) move the self-referential check to Phase B (post-insert, before edge writes), with both the spec and architecture updated to reflect this exception to the "validate before insert" rule. Option B is simpler but requires spec and architecture updates to avoid contradicting NFR-04.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found entries #2298 (config key semantic divergence), #3337 (architecture diagram header divergence), #3426 (formatter overhaul section-order regression). Most relevant: #3337 (architecture informal headers diverging from spec) — confirms the OQ-6/Constraint 6 divergence between architecture body and spec is a known class of inter-document misalignment that has caused tester confusion in prior features.
- Stored: nothing novel to store. The OQ-6 / Constraint 6 divergence is a feature-specific drafting artifact. The `created_by` attribution conflict (WARN 1) is a feature-specific design resolution gap. Neither generalizes to a cross-feature pattern beyond what #3337 already captures.
