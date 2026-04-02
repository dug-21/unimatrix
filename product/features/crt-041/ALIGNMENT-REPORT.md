# Alignment Report: crt-041

> Reviewed: 2026-04-02
> Artifacts reviewed:
>   - product/features/crt-041/architecture/ARCHITECTURE.md
>   - product/features/crt-041/specification/SPECIFICATION.md
>   - product/features/crt-041/RISK-TEST-STRATEGY.md
>   - product/features/crt-041/architecture/ADR-001-graph-enrichment-module-structure.md
>   - product/features/crt-041/architecture/ADR-002-s2-safe-sql-construction.md
>   - product/features/crt-041/architecture/ADR-003-s8-watermark-strategy.md
>   - product/features/crt-041/architecture/ADR-004-graphcohesionmetrics-extension.md
>   - product/features/crt-041/architecture/ADR-005-inferenceconfig-dual-maintenance-guard.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-041/SCOPE.md
> Risk source: product/features/crt-041/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly serves W3-1 graph training data; signal_origin tagging preserved |
| Milestone Fit | PASS | Correctly positioned in Wave 1 intelligence foundation; no future-milestone scope creep |
| Scope Gaps | WARN | SCOPE.md §AC-17 specifies 9-term default for s2_vocabulary; all three source docs correctly carry empty-default per §Design Decision 3 — SCOPE.md itself is internally inconsistent |
| Scope Additions | VARIANCE | SPECIFICATION.md FR-33 and §Dependencies add two new `GraphCohesionMetrics` fields; ARCHITECTURE.md ADR-004 correctly resolves these as already existing (Option B, no change needed) — the spec contains a now-superseded field addition that contradicts the ADR |
| Architecture Consistency | PASS | All five ADRs are internally consistent; tick ordering, module placement, and integration points align across ARCHITECTURE.md and SPECIFICATION.md |
| Risk Completeness | PASS | RISK-TEST-STRATEGY.md covers all 17 risks raised in SCOPE-RISK-ASSESSMENT.md with scenario-level traceability |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Inconsistency (SCOPE.md internal) | s2_vocabulary default | SCOPE.md §AC-17 says default is the "9-term software engineering list from ASS-038". SCOPE.md §Design Decision 3 (lower in the same file) overrides this to empty. All three source docs (ARCHITECTURE.md, SPECIFICATION.md, ADR-005) correctly implement empty default. The AC-17 text in SCOPE.md was not updated to reflect the Design Decision 3 resolution. |
| Variance (Spec vs Architecture) | GraphCohesionMetrics new fields | SPECIFICATION.md FR-33 declares two new fields (`cross_category_edge_count`, `isolated_entry_count`) to be added to `GraphCohesionMetrics`. SPECIFICATION.md §Dependencies also states "Gains 2 new fields." ADR-004 and ARCHITECTURE.md §Component 3 explicitly resolve that both fields already exist from col-029 — no new fields are needed. The spec FR-33 and Dependencies section were not updated to remove the now-closed add-new-fields requirement. |
| Simplification (accepted) | S8 single-function entry point | SCOPE.md §Proposed Approach defines S1, S2, S8 as three separate public(crate) functions. SPECIFICATION.md FR-29 wraps them under a single `run_graph_enrichment_tick` public entry point. Rationale: clean encapsulation, consistent with tick caller pattern. No semantic change. Acceptable. |
| Simplification (accepted) | GraphCohesionMetrics not changed | SCOPE.md §Goals 7 implies new eval-gate fields. ADR-004 correctly identifies both fields already exist. The simplification eliminates unneeded store-layer changes. |

---

## Variances Requiring Approval

### VARIANCE-01: SPECIFICATION.md FR-33 contradicts ADR-004 on GraphCohesionMetrics

**What**: SPECIFICATION.md FR-33 (lines 204–218) declares that `GraphCohesionMetrics` "shall gain two new fields" — `cross_category_edge_count` and `isolated_entry_count` — and documents their SQL definitions. The §Dependencies table (line 544) also states "Gains 2 new fields." ADR-004, however, explicitly documents that both fields already exist from col-029 and that no new fields are needed (Option B chosen, "No changes to GraphCohesionMetrics").

**Why it matters**: If a delivery agent follows SPECIFICATION.md FR-33 literally, they will attempt to add fields that already exist, causing a compile error or duplicate-field situation. If they follow ADR-004, they skip the store-layer change entirely. The contradiction creates delivery ambiguity — the delivery agent must pick which document to trust.

**Recommendation**: SPECIFICATION.md FR-33 and the §Dependencies row for `GraphCohesionMetrics` must be corrected before delivery begins. The correct text should reflect ADR-004: both fields already exist from col-029; no new fields are added by crt-041. The AC-29 test scenario in SPECIFICATION.md (lines 378–382) is also affected — it tests field values, which is correct, but the setup text says "gains" these fields. The AC-29 test itself remains valid (verifying existing fields return expected values); only the framing needs correction.

**Blocking for delivery**: Yes. A delivery agent starting from SPECIFICATION.md will attempt redundant work. Spec must be corrected before wave 1 begins.

---

## Detailed Findings

### Vision Alignment

**PASS.**

The product vision (W3-1 section) requires: "Entry features (node): [...] graph_degree, [...] nli_edge_confidence". The vision's GNN training spec explicitly requires distinguishable edge types via `signal_origin`. crt-041 directly serves this by:

1. Writing `source = 'S1'`, `'S2'`, `'S8'` into `graph_edges.source` — the canonical signal_origin field per SCOPE.md §Background Research.
2. Defining named constants `EDGE_SOURCE_S1/S2/S8` that GNN feature construction (W3-1) will use for edge-type assignment.
3. Growing cross-category edge density toward the ≥3,000 threshold required for PPR expander (Group 4) viability — PPR is the mechanism by which W3-1 can traverse the graph for proactive delivery candidates.

The vision's domain-agnostic requirement (W0-3: "configured not rebuilt") is satisfied by the empty-default `s2_vocabulary`. The feature does not hardcode any domain-specific behavior. The 9-term list is documentation only, not a default.

The additive-only edge policy (SCOPE.md §Design Decision 7, SPECIFICATION.md §Ubiquitous Language "Additive-only") aligns with the vision's self-learning flywheel: "every session makes it better." Edges are never removed on signal loss — the graph grows, which is the correct posture for a system whose graph density enables progressive PPR quality improvement.

No ML model is introduced. All three sources are pure SQL. This is consistent with the vision's milestoning — GNN comes in W3-1 after sufficient graph density; this feature creates that density using deterministic signals.

### Milestone Fit

**PASS.**

crt-041 belongs to the Cortical phase (crt-*), which corresponds to Wave 1 intelligence foundation work. The feature's stated positioning — building edge density before the PPR expander (Group 4) is viable — is correct. Group 4 PPR is a Wave 1A or Wave 3 deliverable; crt-041 is a prerequisite, not an implementation of it.

No Wave 2 concerns (containerization, HTTP, OAuth) are touched. No Wave 3 capabilities (GNN training loop, learned weights) are pre-implemented. The feature is correctly scoped to the current milestone's graph density gap.

The feature correctly defers S3, S4, S5 sources (fewer than 20 pairs at current corpus — SCOPE.md §Non-Goals) in line with the vision's principle of not building ahead of data.

### Architecture Review

**PASS.**

The five ADRs are internally consistent and each addresses a distinct design question raised in SCOPE-RISK-ASSESSMENT.md:

- **ADR-001** (module structure): Correctly chooses Option B (new file) over Option A (extending the already-2000-line `nli_detection_tick.rs`). The 500-line workspace rule is explicitly maintained. The crt-040 prerequisite gate is clearly specified with a grep pre-flight check.

- **ADR-002** (S2 SQL construction): Correctly chooses `sqlx::QueryBuilder::push_bind` over string interpolation. The S2 "shared terms" definition (union count ≥2 across pair) matches SCOPE.md §Background Research and avoids the ambiguity raised in SR-01.

- **ADR-003** (S8 watermark): Correctly chooses event_id-based watermark (Option B) over timestamp window (Option A), citing the established counters table pattern. The write-after-commit ordering invariant and malformed-JSON skip logic are both specified. Batch cap semantics (cap on pairs, not rows) are correctly resolved here, addressing SR-08 and R-10.

- **ADR-004** (GraphCohesionMetrics): Correctly resolves SR-05 by discovering both fields already exist from col-029. Chooses Option B (no new fields). This is correct architecture. The problem is that SPECIFICATION.md was not updated to reflect this decision (see VARIANCE-01).

- **ADR-005** (InferenceConfig dual-maintenance): Correctly enumerates all five update sites per field (struct field, default fn, validate(), impl Default, merge_configs). The `s2_vocabulary` empty-default rationale directly cites W0-3 domain-agnostic vision requirement.

ARCHITECTURE.md §Component 3 correctly reflects ADR-004 ("GraphCohesionMetrics already contains isolated_entry_count and cross_category_edge_count as of col-029. No new fields needed"). The integration surface table is consistent with the spec's functional requirements.

The tick ordering in ARCHITECTURE.md (S1 → S2 → S8 after structural_graph_tick) is consistent with SPECIFICATION.md FR-29 and FR-30.

One minor note: ARCHITECTURE.md §Component 1 lists individual `run_s1_tick`, `run_s2_tick`, `run_s8_tick` as the public functions, while SPECIFICATION.md FR-29 introduces a single `run_graph_enrichment_tick` wrapper. This is not a conflict — the wrapper pattern is a refinement, and the underlying three functions still exist as defined in ADR-001. Delivery agent should use the FR-29 wrapper as the primary entry point.

### Specification Review

**PASS with one VARIANCE (VARIANCE-01 above).**

Functional requirements FR-01 through FR-32 are complete and match SCOPE.md acceptance criteria AC-01 through AC-24 with high fidelity. Each SCOPE.md AC maps to at least one spec FR.

The critical vision-alignment points are all correctly reflected:

1. **S2 empty default**: FR-25 table shows `s2_vocabulary` default as `[]` (empty; operator opt-in) — matches §Design Decision 3 resolution. CORRECT.

2. **No ML model**: NFR-01 explicitly prohibits ONNX, rayon, spawn_blocking. CORRECT.

3. **signal_origin tagging**: FR-04 writes `source = EDGE_SOURCE_S1`, FR-12 writes `EDGE_SOURCE_S2`, FR-20 writes `EDGE_SOURCE_S8`. Named constants defined in FR-24. GNN training readiness preserved. CORRECT.

4. **Additive-only**: C-04 states edges persist until endpoint deletion/quarantine. Explicitly defers reconciliation. CORRECT.

5. **Dual-endpoint quarantine guard**: FR-03, FR-11, FR-19 all mandate dual-JOIN quarantine filtering. References production bug entry #3981. CORRECT.

**The variance**: FR-33 adds two `GraphCohesionMetrics` fields that ADR-004 establishes already exist. The §Dependencies table compounds this with "Gains 2 new fields." This is a documentation artifact from before ADR-004 was written — the spec was not reconciled with the ADR. See VARIANCE-01.

The spec's AC-29 test is valid (verify field values from a synthetic corpus) but its framing ("GraphCohesionMetrics gains...") must be corrected to say these fields already exist and are being verified.

### Risk Strategy Review

**PASS.**

RISK-TEST-STRATEGY.md covers all 9 risks from SCOPE-RISK-ASSESSMENT.md (SR-01 through SR-09), plus adds 8 additional implementation-level risks (R-10 through R-17) that are well-calibrated for the implementation complexity:

- R-10 (batch cap semantics) addresses an important ambiguity not in the original scope risk assessment — cap on pairs vs rows.
- R-11 (S2 false-positive matching) and R-17 (validate() zero-value) are implementation gotchas that justify inclusion.
- The scope-risk traceability table (§Scope Risk Traceability) maps each SR-* to its resolution path, confirming SR-05 is resolved via ADR-004.

The 17-risk, 30-scenario coverage is proportional for a pure SQL tick feature of this complexity. Critical risks (R-01 through R-03) require multiple test scenarios each with explicit fixture requirements.

One minor gap: the RISK-TEST-STRATEGY.md Knowledge Stewardship block states "nothing novel to store." This is appropriate given the existing patterns (#4026 for S8 watermark, #3817/#3980/#3981 for dual-join) were already in Unimatrix.

---

## Known Correct Resolutions (No Action Needed)

The following items from SCOPE.md appear potentially open but are correctly resolved in the source documents:

- **SCOPE.md §AC-17 vs §Design Decision 3 conflict** (s2_vocabulary default): Design Decision 3 wins. All source docs implement empty default. SCOPE.md AC-17 text is a documentation artifact that should be cleaned up but does not affect delivery.
- **SR-05 (GraphCohesionMetrics field definition unknown)**: Resolved by ADR-004. Both fields exist from col-029 with exact SQL definitions known. Risk closed.
- **OQ-01 (S1 GROUP BY materialization)**: Addressed in RISK-TEST-STRATEGY.md R-04 — requires query plan verification and NFR-03 timing test. Implementation brief must document the query plan result.
- **OQ-03 (S1/S2 stale edge compaction)**: Addressed in RISK-TEST-STRATEGY.md R-09 — must be answered in implementation brief. If crt-039 compaction does not cover S1/S2, deferred with documentation. Explicitly marked as deferred in SCOPE.md §Design Decision 7 and SPECIFICATION.md §Ubiquitous Language.
- **write_graph_edge prerequisite**: Hard gate specified in ADR-001, SPECIFICATION.md FR-32, RISK-TEST-STRATEGY.md R-08. Delivery agent has clear pre-flight check.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for topic `vision` — found entries #2298 (config semantic divergence), #3742 (optional-future-branch scope warning), #3337 (architecture-diagram header divergence causing tester assertion failures), #3746 (pipeline step ordering gotcha). Entry #3337 is relevant: architecture and spec divergence on a structural definition can cause testers to implement against wrong state. This is exactly the VARIANCE-01 pattern (spec FR-33 says fields will be added; architecture ADR-004 says they already exist). Entry #3337 directly validates the decision to classify VARIANCE-01 as blocking.
- Stored: attempted via `/uni-store-pattern` — pattern identified: "Spec-ADR reconciliation gap — when an ADR resolves a scope risk by discovering an existing capability, the spec must be updated to remove the now-superseded requirement before delivery begins; failing to do so creates delivery ambiguity." Store rejected: agent thread lacks Write capability (anonymous identity). Pattern is novel; should be stored by a credentialed agent post-session. Suggested title: "Spec-ADR reconciliation gap causes delivery ambiguity — block until spec updated". Tags: `[alignment, spec-adr-reconciliation, crt-041, variance-pattern, vision]`
