# Alignment Report: col-020

> Reviewed: 2026-03-10
> Artifacts reviewed:
>   - product/features/col-020/architecture/ARCHITECTURE.md
>   - product/features/col-020/specification/SPECIFICATION.md
>   - product/features/col-020/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly listed in Activity Intelligence milestone, Wave 2 |
| Milestone Fit | PASS | Wave 2 feature with correct dependencies on col-017 and nxs-010 |
| Scope Gaps | PASS | All SCOPE.md goals and ACs addressed in source documents |
| Scope Additions | WARN | AttributionCoverage added (justified by SR-07 risk mitigation) |
| Architecture Consistency | VARIANCE | Rework case sensitivity conflict between architecture and specification |
| Risk Completeness | PASS | All 9 scope risks traced, 15 risks identified, 42 test scenarios |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `AttributionCoverage` struct and `attribution_coverage` field on report | Not in SCOPE.md. Added by architecture (ADR-003) and specification (FR-05.6) to address SCOPE-RISK-ASSESSMENT SR-07 recommendation. Justified addition -- SR-07 was rated High severity. |
| Addition | Grep in file path extraction mapping | Architecture ADR-004 mapping table includes Grep (`path` field). SCOPE.md section "File path extraction from tool inputs" constraint lists Read/Edit/Write/Glob only. Risk strategy R-06 flags this gap explicitly. Minor addition that improves data completeness. |
| Simplification | build_report() signature unchanged | SCOPE.md proposed either extending build_report() parameters or post-build mutation. Architecture and specification chose post-build mutation, following existing pattern for narratives/recommendations. Rationale documented. |
| Simplification | Helpful signal excluded from Tier 1 | SCOPE.md AC-06 mentions "explicit helpful signals on cross-session entries" as part of Tier 1. Resolved question #2 in SCOPE.md clarifies knowledge_in excludes injection_log. Architecture open question #2 further clarifies helpful signals lack per-session attribution. Spec excludes it. Consistent with SCOPE resolved questions, though AC-06 text is ambiguous. |

## Variances Requiring Approval

1. **What**: Rework outcome detection case sensitivity is contradictory across documents.
   - Architecture (line 189): "case-insensitive substring"
   - Specification FR-03.1 (line 45): "case-sensitive match"
   - SCOPE.md resolved question #1: No case specification stated
   - Risk strategy R-08 test scenarios: Do not specify case behavior

   **Why it matters**: This is a direct contradiction between two source-of-truth documents. The implementation will follow one or the other, and the test strategy cannot validate correctness when the requirement is ambiguous. The structured outcome tags from col-001 (`result:rework`, `result:failed`) are lowercase, so case sensitivity likely does not matter in practice, but the documents must agree.

   **Recommendation**: Resolve to case-sensitive (matching the spec FR-03.1 and the col-001 structured tag format). Update architecture line 189 to say "case-sensitive". This is the safer choice -- the structured tags from col-001 are the expected input, and case-insensitive matching risks false positives on free-form text.

## Detailed Findings

### Vision Alignment

col-020 is explicitly listed in the product vision roadmap under "Activity Intelligence" milestone, Wave 2:

> **col-020: Multi-Session Retrospective** -- Retrospective spans all sessions for a topic. New cross-session metrics: context reload rate, knowledge reuse, session efficiency trends, rework session count. Updates topic_deliveries aggregates.

The feature directly serves the vision's core value proposition: "auditable knowledge lifecycle." By measuring whether knowledge stored in one session is retrieved in later sessions (`knowledge_reuse`), col-020 provides the first quantitative proof of Unimatrix's cross-session value. This is well-aligned with the vision statement: "When an agent asks 'how do I write integration tests?', the answer reflects what has actually worked."

The feature also aligns with the vision's emphasis on invisible delivery by measuring injection_log-based reuse (knowledge delivered via hooks without explicit agent action).

No vision concerns.

### Milestone Fit

col-020 is correctly positioned in Wave 2 of Activity Intelligence, with dependencies on:
- col-017 (Wave 1, topic attribution) -- hard dependency, acknowledged in all three documents
- nxs-010 (Wave 2, schema evolution) -- hard dependency, confirmed as landed

The vision roadmap lists `session_efficiency_trend` as a col-020 deliverable. SCOPE.md explicitly drops this as a non-goal with documented rationale (efficiency comparisons require session-type awareness). This is a legitimate scope reduction from the roadmap's initial description and is well-justified. Not flagged as a variance because the scope document is the authority for feature boundaries, and the rationale is sound.

No milestone concerns.

### Architecture Review

The architecture is well-structured with 6 components (C1-C6), clear data flow, and explicit scope risk mitigations for all 9 risks from SCOPE-RISK-ASSESSMENT.

**Strengths**:
- ADR-001 (server-side knowledge reuse) documents the observe/server boundary exception with clear scoping rules
- ADR-002 (idempotent counters via absolute-set) directly resolves SR-09
- ADR-003 (attribution metadata) directly resolves SR-07
- ADR-004 (file path mapping) directly resolves SR-04
- Error propagation is explicitly best-effort (compute if possible, None if not), protecting existing pipeline output
- Open question #1 (batch query chunking) acknowledges scale concern with a practical threshold

**Issue**: Case-insensitive rework detection (architecture line 189) contradicts spec FR-03.1 (case-sensitive). See Variances section.

**Note**: Architecture open question #2 narrows AC-06 from SCOPE.md by excluding helpful-signal-based reuse from v1. This is consistent with SCOPE.md resolved question #2 but creates a minor discrepancy with AC-06's literal text ("explicit helpful signals on cross-session entries"). The spec and architecture are internally consistent on this point; the AC text in SCOPE.md is slightly overinclusive.

### Specification Review

The specification is thorough with 8 functional requirement groups (FR-01 through FR-08), 5 non-functional requirements, 16 acceptance criteria, 3 domain models, 2 user workflows, and a comprehensive constraints section.

**Strengths**:
- FR-01.4 specifies the exact tool-to-field mapping for file path extraction
- FR-02.6 specifies robust JSON parsing with graceful degradation
- FR-03.1 enumerates exact outcome patterns (addressing SR-03)
- FR-04.2 resolves concurrent session ordering with lexicographic tiebreaker (addressing SR-06)
- FR-06.2 mandates idempotent counter updates (addressing SR-09)
- NFR-05 codifies attribution transparency (addressing SR-07)
- "NOT in Scope" section mirrors SCOPE.md non-goals exactly

**Minor note**: FR-02.6 says malformed JSON is "logged at warn level" while SCOPE.md (proposed approach) and architecture say "debug level." This is a trivial discrepancy with no functional impact. The spec's choice of warn is arguably more appropriate for a data quality signal.

**Minor note**: Spec FR-01.4 does not include Grep in the file path mapping, while architecture does. The spec is consistent with SCOPE.md; the architecture adds Grep. The risk strategy R-06 explicitly calls out this discrepancy as a test scenario. This should be resolved -- either add Grep to the spec or remove it from the architecture mapping table.

### Risk Strategy Review

The risk strategy is comprehensive:
- 15 risks identified across correctness, performance, integration, and security domains
- All 9 scope risks from SCOPE-RISK-ASSESSMENT traced to architecture risks with resolutions
- 42 test scenarios across high/medium/low priority
- Integration risks between components identified (C1<->C6 PreToolUse filtering, C3<->C4 field semantics, C4 method conflicts, C2<->vnc-011 completeness gap)
- Edge cases enumerated (single observation, same-session exclusion, duplicate entry IDs in query_log)
- Security risks assessed and dismissed with rationale
- Failure modes table covers 7 failure scenarios with expected behavior

**Strengths**:
- R-12 (double-counting across query_log and injection_log) catches a subtle correctness risk not mentioned in SCOPE.md
- R-14 (new steps aborting existing pipeline) validates the architecture's best-effort pattern
- Integration risk about PreToolUse vs PostToolUse double-counting is an important implementation detail
- Coverage summary correctly counts 7 high-priority risks (the table header says 6 but lists 7 IDs)

**Minor issue**: Coverage summary line says "6 (R-01, R-02, R-03, R-04, R-05, R-10, R-14)" but that is 7 risk IDs. The count should be 7, not 6. This is a typo in the risk strategy document.
