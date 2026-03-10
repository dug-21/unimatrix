# Alignment Report: vnc-011

> Reviewed: 2026-03-10
> Artifacts reviewed:
>   - product/features/vnc-011/architecture/ARCHITECTURE.md
>   - product/features/vnc-011/specification/SPECIFICATION.md
>   - product/features/vnc-011/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | WARN | Actionability tagging deferred from vision-listed vnc-011 deliverable; documented and acknowledged |
| Milestone Fit | PASS | Correctly targets Activity Intelligence Wave 3; no future-milestone overreach |
| Scope Gaps | PASS | All SCOPE items addressed in source documents |
| Scope Additions | VARIANCE | FR-13 adds rendering of rework_session_count and context_reload_pct, which SCOPE explicitly excludes |
| Architecture Consistency | VARIANCE | Architecture ADR-001 contradicts Specification on evidence_limit default behavior; ADR-002 contradicts Specification on deterministic vs random evidence selection |
| Risk Completeness | PASS | 14 risks, 4 integration risks, 36 test scenarios; all SCOPE risks traced to architecture mitigations |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | FR-13: Rework and context reload rendering | SCOPE None-handling table explicitly states rework_session_count and context_reload_pct are "Not rendered (no dedicated section)". Specification FR-13 adds rendering for both. |
| Simplification | Actionability tagging deferred | PRODUCT-VISION lists `[actionable]/[expected]/[informational]` as vnc-011 deliverable. SCOPE explicitly excludes. Architecture and Specification acknowledge deferral. Rationale: reduces scope for MVP; CollapsedFinding struct can accommodate tags later. |

## Variances Requiring Approval

### 1. evidence_limit default: Architecture vs Specification disagree

**What**: Architecture C1 and ADR-001 specify format-dependent evidence_limit defaults: 0 for markdown, 3 for JSON (SR-03 mitigation). Specification FR-02 and C-03 specify a global default change to 0 for both formats. SCOPE line reads: "`evidence_limit` default changed from 3 to 0" (no format qualification).

**Why it matters**: This is a direct contradiction between two source documents. If the implementer follows Architecture, JSON consumers keep their existing 3-record default. If the implementer follows Specification, JSON consumers silently receive all evidence by default -- a behavioral breaking change. The Scope Risk Assessment (SR-03) flags this as the highest-priority risk and recommends format-dependent defaults.

**Recommendation**: Resolve before implementation. The Architecture's format-dependent approach (ADR-001) is the safer choice and aligns with SR-03 mitigation. If global change is intended, update Architecture to match Specification and document the breaking change explicitly.

### 2. Evidence selection: Architecture says deterministic, Specification says random

**What**: Architecture ADR-002 states "Deterministic example selection via timestamp ordering (SR-02 mitigation)". RISK-TEST-STRATEGY R-05 scenario 4 tests "earliest 3" by timestamp. However, Specification FR-08 says "Selection uses the standard library random facilities... determinism is not required for MVP." Specification "NOT in Scope" section explicitly lists "Deterministic evidence selection (SR-02 accepted for MVP)".

**Why it matters**: Architecture made a design decision (ADR-002) to resolve SR-02 with deterministic timestamp-based selection. Specification contradicts this by accepting random selection. The Risk-Test-Strategy aligns with Architecture (testing for "earliest 3"). An implementer will receive conflicting instructions.

**Recommendation**: Resolve before implementation. Architecture's deterministic approach is testable and debuggable. If random selection is preferred, update Architecture to remove ADR-002 and update RISK-TEST-STRATEGY R-05 scenarios. If deterministic is preferred, update Specification FR-08 to match Architecture ADR-002.

### 3. FR-13 renders data that SCOPE excludes from rendering

**What**: Specification FR-13 adds rendering of `rework_session_count` and `context_reload_pct`. SCOPE's None Field Handling table (lines 129-130) explicitly states both are "Not rendered (no dedicated section)". Architecture's None Field Handling table (lines 129-130) also states "Not rendered".

**Why it matters**: The Specification adds a requirement that neither SCOPE nor Architecture anticipated. This is a scope addition -- the human did not ask for this rendering. While the data exists in the report struct, SCOPE deliberately chose not to render it.

**Recommendation**: Remove FR-13 from Specification to align with SCOPE, or get explicit human approval to add this rendering. The data is available in JSON format for consumers who need it.

## Detailed Findings

### Vision Alignment

vnc-011 is explicitly listed in PRODUCT-VISION.md under "Activity Intelligence / Wave 3" (line 77):

> vnc-011: Retrospective ReportFormatter -- Markdown-format retrospective output for LLM consumers. Session table, finding collapse (related hotspots -> single finding), actionability tagging (`[actionable]`/`[expected]`/`[informational]`), narrative collapse, baseline filtering to outliers only. ~80% token reduction from current JSON default. JSON preserved via `format: "json"`. No dependencies on Wave 1/2. (#91)

The vision description includes actionability tagging as a deliverable. SCOPE explicitly defers it (SCOPE line 99: "Actionability tagging (`[actionable]`/`[expected]`/`[informational]`)"). All three source documents acknowledge this deferral. Architecture line 135-137 notes the deferral and confirms the CollapsedFinding struct can accommodate tags later. Specification C-05 (line 188-189) acknowledges the deferral.

This is a WARN, not a VARIANCE, because: (a) SCOPE is authoritative for what the human asked for, (b) the deferral is documented with rationale, and (c) no architectural decision precludes adding the feature later.

All other vision-listed vnc-011 deliverables are addressed: session table, finding collapse, narrative collapse, baseline filtering, ~80% token reduction target, JSON preservation.

The feature aligns with the product's strategic direction of optimizing knowledge delivery for LLM consumers. The formatter-only approach respects the existing pipeline architecture. No shortcuts contradict the product vision.

### Milestone Fit

vnc-011 targets Activity Intelligence Wave 3. The vision states Wave 3 depends on Wave 2 but also notes vnc-011 has "No dependencies on Wave 1/2" (line 77). SCOPE confirms dependencies only on col-020 (complete) and col-020b (in progress). No future-milestone capabilities are being built. Milestone discipline: PASS.

### Architecture Review

The architecture is well-structured: pure formatter function, clear component boundaries (C1-C3), explicit integration surface with type signatures, and comprehensive None-field handling table.

Strengths:
- Formatter-only constraint is maintained throughout
- Read-only consumption of observe types is clean
- ADR-001 (format-dependent evidence_limit) is a sound mitigation for SR-03
- ADR-002 (deterministic selection) improves testability
- ADR-003 (separate module) is good separation of concerns

Issues:
- ADR-001 contradicts Specification FR-02/C-03 (see Variance 1)
- ADR-002 contradicts Specification FR-08 (see Variance 2)
- Architecture None-handling table excludes rework/context_reload from rendering, contradicting Specification FR-13 (see Variance 3)

The Architecture's Collapsed Finding internal type differs slightly from the Specification's version (Architecture has `total_events: f64` and explicit `tool_breakdown`, `examples`, `cluster_count`, `sequence_pattern` fields; Specification has `findings: Vec<&HotspotFinding>`, `total_measured: f64`, `narrative: Option<&HotspotNarrative>`, `evidence_pool: Vec<&EvidenceRecord>`). These are implementation-level differences in the same internal struct and do not represent a functional mismatch -- the Specification's version is more flexible (keeping references to source data), while Architecture's is more pre-computed. Either approach satisfies the requirements. No action needed.

### Specification Review

The specification is thorough with 14 functional requirements, 4 non-functional requirements, and 16 acceptance criteria. Constraints are clearly enumerated. Domain models are documented.

FR-01 through FR-12 align with SCOPE. FR-14 aligns with SCOPE's key constraint. FR-13 is a scope addition (see Variance 3).

NFR-01 (80% token reduction) aligns with SCOPE success criterion 10. NFR-02 (no new dependencies) is prudent. NFR-03 (5ms performance) is reasonable for pure computation. NFR-04 (backward compatibility) is correctly scoped.

The "NOT in Scope" section (lines 206-215) is comprehensive and aligns with SCOPE's exclusions, except it lists "Deterministic evidence selection (SR-02 accepted for MVP)" and "Format-dependent evidence_limit defaults (SR-03: global change is intentional per SCOPE)" -- both of which contradict Architecture ADR-001 and ADR-002 respectively.

### Risk Strategy Review

The Risk-Test-Strategy is thorough: 14 risks with severity/likelihood ratings, 4 integration risks, 36 test scenarios, edge cases, security analysis, failure modes, and scope risk traceability.

All 8 SCOPE risks (SR-01 through SR-08) are traced to architecture resolutions in the traceability table (lines 176-184). The risk register covers the critical paths: finding collapse severity (R-01), evidence_limit default change (R-02), None-field handling (R-03), and narrative matching (R-04).

R-05 test scenarios align with Architecture ADR-002 (deterministic selection), contradicting Specification FR-08 (random selection). This is part of Variance 2.

Security analysis is appropriate: formatter is a pure function, no new attack surface. The markdown injection note (evidence descriptions from trusted internal data) is a correct assessment.

Integration risks IR-01 through IR-04 are well-identified. IR-04 (feature gate requirement) is a practical build concern worth testing.

No significant risk gaps identified. The strategy is proportional to the feature's complexity.
