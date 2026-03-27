# Alignment Report: col-029

> Reviewed: 2026-03-26
> Artifacts reviewed:
>   - product/features/col-029/architecture/ARCHITECTURE.md
>   - product/features/col-029/specification/SPECIFICATION.md
>   - product/features/col-029/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/col-029/SCOPE.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly enables W3-1 entry feature vector (`graph_degree` field) and W1-4 observability |
| Milestone Fit | PASS | Wave 1 / Wave 1A support work; no future-milestone capabilities built |
| Scope Gaps | WARN | Spec FR-11 error propagation diverges from architecture's non-fatal pattern; operator impact is material |
| Scope Additions | WARN | Architecture adds `EDGE_SOURCE_NLI` constant and `lib.rs` re-export — not in SCOPE.md, but low-risk |
| Architecture Consistency | PASS | Architecture, specification, and SCOPE.md are internally consistent across all four layers |
| Risk Completeness | PASS | Risk register covers all critical paths; Unimatrix knowledge references are accurate |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `EDGE_SOURCE_NLI` constant + `lib.rs` re-export | SCOPE.md specifies SR-01 as a risk but does not explicitly require a named constant as the resolution. Architecture (ADR-001) and Specification (Constraints section) both mandate `EDGE_SOURCE_NLI: &str = "nli"` in `unimatrix-store/src/read.rs`, re-exported from `lib.rs`. This is a non-breaking cross-crate coordination addition. Rationale is well-documented. |
| Variance | FR-11 error propagation model | SCOPE.md AC-11 says compute is called in Phase 5 with no error-handling specification. Architecture says error is non-fatal (`tracing::warn!` + skip). Specification FR-11 says error must propagate via `ServiceError::Core(CoreError::Store(e))`. These two error models contradict each other. See Variances section. |
| Simplification | Integration tests omitted | SCOPE.md states "Unit-test the SQL computation function." Specification (Workflow 3) and RISK-TEST-STRATEGY both confirm no integration tests are required beyond unit tests. Rationale: SQL is fully exercised in unit tests using `open_test_store()`. Accepted simplification — consistent with `compute_status_aggregates` precedent. |

---

## Variances Requiring Approval

### VARIANCE 1 — FR-11 vs. Architecture: Contradicting error-handling models

1. **What**: The Specification (FR-11) and the Architecture (Layer 3) specify opposite error-handling behaviors for `compute_graph_cohesion_metrics()` failure in `compute_report()` Phase 5.

   - **SPECIFICATION FR-11** states: "On error the call must propagate via `ServiceError::Core(CoreError::Store(e))`." This means a failure aborts the entire `context_status` response with an error.
   - **ARCHITECTURE (Layer 3)** states: "Failure is non-fatal (warn + skip), consistent with how Phase 4 co-access stats errors are handled. The report is still returned; cohesion fields default to zero."
   - The architecture even includes example code showing `Err(e) => tracing::warn!("graph cohesion metrics failed: {e}")`.

2. **Why it matters**: The implementer will receive two directly conflicting instructions from two source-of-truth documents. Choosing the spec (fatal propagation) breaks the `context_status` tool whenever the write pool is under contention (R-07) — an operator monitoring NLI inference health would get an error response instead of a partial report. Choosing the architecture (non-fatal) violates the written specification, making AC-11 ambiguous. The RISK-TEST-STRATEGY (R-07 failure modes table) is consistent with the architecture's non-fatal model, meaning three of the four documents align on non-fatal and only the Specification diverges.

3. **Recommendation**: Correct SPECIFICATION FR-11 to match the architecture's non-fatal pattern. The architecture's choice is strongly justified — it is consistent with the Phase 4 precedent, R-07 explicitly identifies pool contention as a risk with non-fatal handling, and the RISK-TEST-STRATEGY's failure modes table describes the non-fatal behavior as the expected outcome. The specification appears to have been written before the architecture resolved this design question via ADR-003.

---

## Detailed Findings

### Vision Alignment

col-029 is directly traceable to two vision-layer needs:

**W3-1 entry feature vector**: The PRODUCT-VISION.md W3-1 section lists `graph_degree` as a required entry feature for the GNN session-conditioned relevance function. `mean_entry_degree` and the underlying `connected_entry_count` computation are precisely the graph topology data W3-1 needs as an input feature. Without this feature, the `graph_degree` entry feature is uncomputable from the live store.

**W1-4 NLI observability**: W1-4 (NLI re-ranking, COMPLETE) writes `Contradicts` and `Supports` edges to `GRAPH_EDGES` with `source='nli'`. The vision states: "Contradiction detection is semantic." col-029's `inferred_edge_count` and `cross_category_edge_count` metrics are the first operator-visible confirmation that semantic edge inference is producing a useful graph structure. The vision's "trustworthy, correctable, and ever-improving" knowledge integrity claim depends on operators being able to verify that inference is working as intended.

**Domain agnosticism**: The feature adds no domain-specific language. The metrics are structural (graph topology) and source-tagged (`'nli'`), not vocabulary-coupled to any specific domain. This is consistent with the W0-3 direction toward domain-agnostic operator visibility.

**Assessment**: PASS. The feature is a targeted observability enabler for Wave 1-complete infrastructure and a prerequisite data source for Wave 3. It does not conflict with any vision principle.

---

### Milestone Fit

col-029 sits at the intersection of Wave 1 (NLI graph infrastructure, COMPLETE) and Wave 1A (adaptive intelligence pipeline, IN PROGRESS). Its position is appropriate:

- The feature depends on `GRAPH_EDGES` (W1-1, COMPLETE) and NLI inference (W1-4, COMPLETE). Both prerequisites are done.
- It does not build any Wave 1A, Wave 2, or Wave 3 capability. The SCOPE.md Non-Goals explicitly exclude lambda changes, alerting thresholds, and proactive maintenance actions — all of which would be Wave 1A or later scope.
- The `EDGE_SOURCE_NLI` constant addition is a Wave 1 housekeeping item that resolves a string-coupling risk (SR-01) between col-029 and GH #412. This is appropriately scoped as part of the same work, not deferred.
- No future-milestone capabilities are built ahead of their wave. The six metrics are diagnostic/observational — they feed human understanding and eventually W3-1 feature extraction, but they do not implement any learning, GNN training, or scoring changes.

**Assessment**: PASS.

---

### Architecture Review

The architecture is well-structured across four layers (store, status report struct, service, formatting) and traces cleanly to the scope requirements.

**Strengths**:
- Component breakdown matches the `compute_status_aggregates` precedent pattern cited in SCOPE.md Background Research.
- SQL design is concrete and addresses SR-04 (cartesian product risk) with explicit JOIN alias strategy and a UNION sub-query alternative for connected entry dedup.
- The `bootstrap_only=0` filter rationale (matching `TypedRelationGraph.inner` semantics) is correctly articulated.
- ADR-001 through ADR-004 are referenced and their resolutions are traceable to specific implementation decisions.
- The `connected_raw` overcounting problem (noted in the SQL Design section) is acknowledged and two resolution strategies are offered with preference stated.
- File size impact is calculated (`read.rs` ~1570 → ~1625 lines) and the existing exceedance of the 500-line rule is noted as a housekeeping concern.

**Concern — informal section headers in diagrams (pattern #3337)**: The architecture's component interaction diagram uses Phase labels (`Phase 5`, `Phase 5b`) that may not exactly match the string labels used in `compute_report()` tests or assertions. The risk-test strategy does not reference these labels directly. This is informational only — the pattern (#3337) notes that testers asserting against architecture diagram headers can cause failures.

**Assessment**: PASS.

---

### Specification Review

The specification is complete and precise. All 13 AC criteria from SCOPE.md are present and three additional criteria (AC-14, AC-15, AC-16) are added to cover gaps identified during specification (SR-04 double-join, tick exclusion, bootstrap NLI exclusion). This is appropriate gap-filling, not scope addition.

**Domain models**: `GraphCohesionMetrics` field semantics are defined with precision including edge cases (zero active entries → 0.0, in+out degree definition, active-only join semantics).

**Ubiquitous Language**: The specification defines seven key terms with precise SQL-level semantics (Active entry = status=0, Non-bootstrap edge = bootstrap_only=0, etc.). This is appropriate for a feature that requires exact SQL interpretation.

**Constraint coverage**: All seven SCOPE.md constraints are reflected in the NFR section with correct specification language.

**Error-handling divergence (FR-11)**: As documented in Variances. FR-11 specifies fatal propagation; the architecture, risk-test-strategy, and SCOPE.md background all use the non-fatal pattern. This is the only material inconsistency.

**Assessment**: PASS on completeness; the FR-11 variance requires resolution before implementation.

---

### Risk Strategy Review

The risk register is thorough and well-grounded in prior codebase knowledge. Key observations:

**R-01 (COUNT DISTINCT double-count)** is correctly classified as Critical/High. The references to Unimatrix entries #1043 and #1044 confirm this is a known failure mode in this codebase, not a hypothetical. The test coverage requirement (bidirectional chain, not star topology) is a concrete, correct prescription.

**R-02 (NULL guard)** correctly identifies the LEFT JOIN NULL propagation problem. The CASE guard specified in ADR-004 (`ge.id IS NOT NULL AND src_e.category IS NOT NULL AND tgt_e.category IS NOT NULL`) is verified in the architecture's SQL Design section.

**R-03 (bootstrap NLI leak)** aligns with AC-16, which was added to the specification as a gap-fill. The three-scenario test matrix (bootstrap-only NLI, non-bootstrap NLI, mixed) is correctly specified.

**R-07 (write pool contention)**: The risk is documented as accepted. The non-fatal error path test covers the consequence. The note that `context_status` is an infrequent diagnostic call (not a hot path) is a correct context for the accepted risk level.

**R-09 (EDGE_SOURCE_NLI re-export)**: This risk is well-specified and the compile-time verification approach is correct. The scope addition of the constant and re-export (documented above in Scope Alignment) directly addresses this risk.

**Coverage**: 27 required test scenarios across 10 risks. The Critical and High risks are covered with concrete, executable scenario descriptions. The Medium risks are covered by either code-review/static checks or existing test infrastructure.

**Security risks**: Correctly assessed as none specific to this feature. The function is parameterless, read-only, and operates on trusted internal data. No injection surface.

**Assessment**: PASS.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found #2298 (config key semantic divergence / alignment), #3426 (formatter section-order regression), #3337 (architecture diagram informal headers diverging from spec, causing tester assertion failures). Pattern #3337 is applicable: the architecture's Phase label diagram should not be used as assertion targets by testers.
- Stored: nothing novel to store — the FR-11 fatal vs. non-fatal error propagation divergence between spec and architecture is feature-specific (not a recurring cross-feature pattern). The root cause is a spec written before an architecture ADR resolved the design question. This does not generalize beyond col-029 without more instances.
