# Alignment Report: crt-002

> Reviewed: 2026-02-24
> Artifacts reviewed:
>   - product/features/crt-002/architecture/ARCHITECTURE.md
>   - product/features/crt-002/specification/SPECIFICATION.md
>   - product/features/crt-002/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Implements M4 crt-002 confidence evolution as specified |
| Milestone Fit | PASS | Squarely within Milestone 4 (Learning & Drift) |
| Scope Gaps | PASS | All 22 acceptance criteria addressed in specification and architecture |
| Scope Additions | PASS | No scope additions beyond SCOPE.md |
| Architecture Consistency | PASS | Consistent with crt-001 patterns (fire-and-forget, spawn_blocking, combined transactions) |
| Risk Completeness | PASS | 12 risks, 45 test scenarios; all 9 scope risks traced |
| Formula Deviation | WARN | Additive formula replaces product vision's multiplicative formula |
| Confidence Floor | WARN | No explicit floor vs vision's "floor at 0.1" |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Multiplicative -> Additive formula | Vision specifies `confidence = base * usage * freshness * correction * helpfulness`. Architecture and specification use additive weighted composite. This is a well-documented, research-backed deviation (crt-001 USAGE-TRACKING-RESEARCH.md Section 4). ADR-001 and ADR-003 document the rationale. |
| Simplification | No confidence floor | Vision specifies "floor at 0.1". SCOPE.md Decision 8 removes the floor. ADR-003 documents the rationale: the emergent minimum from the formula (~0.04 for worst-case deprecated, ~0.19 for worst-case active) provides a natural floor without artificial clamping. |
| Simplification | `version` parameter removed from `correction_score` | SCOPE.md listed `correction_score(correction_count, version)`. Architecture and specification simplified to `correction_score(correction_count)` only. The `version` field does not add meaningful signal beyond correction_count for the bracket-based scoring. |

## Variances Requiring Approval

None requiring approval. Both WARN items are documented simplifications with clear rationale:

1. **W1: Additive vs Multiplicative Formula**
   - **What**: Product vision defines `confidence = base * usage * freshness * correction * helpfulness`. The design uses an additive weighted composite.
   - **Why it matters**: This is a formula-level architectural change to the product vision's confidence computation approach.
   - **Recommendation**: Accept. The research spike (product/features/crt-001/USAGE-TRACKING-RESEARCH.md) provides rigorous analysis showing multiplicative has zero-factor collapse and superlinear gaming amplification. The additive approach is strictly superior for gaming resistance. The vision was written before the research spike completed.

2. **W2: No Confidence Floor**
   - **What**: Product vision specifies "floor at 0.1". The design has no explicit floor.
   - **Why it matters**: Deviates from stated product vision parameter.
   - **Recommendation**: Accept. ADR-003 demonstrates the emergent minimum (~0.19 for active entries) exceeds the vision's 0.1 floor. The floor is structurally guaranteed by the formula weights without an artificial clamp.

## Detailed Findings

### Vision Alignment

The product vision (PRODUCT-VISION.md, M4 crt-002 row) defines confidence evolution as:

> "Helpfulness factor added to confidence formula: `confidence = base * usage * freshness * correction * helpfulness`. Before usage data, factor = 1.0 (neutral). Confidence boost (+0.03/access), time decay (-0.005/hr), floor at 0.1."

The design implements all the *intent* of this specification:
- Helpfulness factor: implemented via Wilson score lower bound (superior to naive ratio)
- Usage factor: implemented via log-transformed access count (gaming resistant)
- Freshness/time decay: implemented via exponential decay with 1-week half-life
- Correction factor: implemented via bracket-based scoring
- Confidence boost on access: achieved through usage_score increasing with access_count
- Base factor: implemented as status-dependent score

The specific parameters differ from the vision ("+0.03/access" replaced by log transform, "-0.005/hr" replaced by exponential decay with 168h half-life), but these are implementation refinements backed by the gaming resistance research, not scope deviations.

The vision also states: "Gaming resistance note: The multiplicative formula should be replaced with an additive weighted composite." This note was added to the vision based on the crt-001 research spike, confirming the additive approach is the intended direction.

### Milestone Fit

crt-002 sits squarely in Milestone 4 (Learning & Drift, Cortical phase). It is the second feature in the cortical sequence:
- crt-001 (Usage Tracking) -- merged, provides raw signals
- **crt-002 (Confidence Evolution)** -- this feature, consumes signals
- crt-003 (Contradiction Detection) -- next, reads confidence for severity assessment
- crt-004 (Co-Access Boosting) -- reads confidence as baseline

The feature does not implement any capabilities from other milestones. It does not reach into M5 (orchestration), M6 (UI), or M7 (multi-project). The search re-ranking is a natural integration within the existing M2 server, not a new M2 feature.

### Architecture Review

The architecture is consistent with established patterns:
- **Fire-and-forget**: Confidence updates follow the same async pattern as crt-001 usage recording (ADR-004 in crt-001)
- **spawn_blocking**: Store writes wrapped in spawn_blocking per established pattern (nxs-004, crt-001)
- **Combined transactions**: Merging confidence into the usage write transaction follows the combined-transaction precedent from vnc-002/vnc-003
- **Function pointer for dependency inversion**: The store accepts a `dyn Fn` rather than depending on the server crate, maintaining the clean dependency direction (store <- core <- server)

Five ADRs are well-scoped (one decision per ADR) and address all scope risks that required architectural resolution.

The component breakdown (C1-C5) covers all integration points. The integration surface table is comprehensive with exact function signatures.

### Specification Review

All 22 acceptance criteria from SCOPE.md are addressed:
- AC-01 through AC-08: Covered by FR-01 through FR-02 (formula and components)
- AC-09 through AC-12: Covered by FR-04 through FR-07 (confidence on retrieval, insert, correct, deprecate)
- AC-13 through AC-14: Covered by FR-08 (search re-ranking)
- AC-15: Covered by FR-03 (Wilson score z=1.96)
- AC-16: Covered by specification constants section
- AC-17: Covered by FR-09 (targeted update)
- AC-18: Covered by FR-02a (base_score for deprecated)
- AC-19: Covered by FR-04c (fire-and-forget)
- AC-20: Covered by NFR-03 (testability)
- AC-21: Covered by FR-03 (Wilson edge cases)
- AC-22: Covered by NFR-04 (backward compatibility)

Minor note: SCOPE.md AC-06 lists `correction_score(correction_count, version)` but the specification simplifies to `correction_score(correction_count)`. This is a scope simplification (version does not add signal), not a gap.

### Risk Strategy Review

12 risks identified, 45 test scenarios. All 9 scope risks (SR-01 through SR-09) are traced in the Scope Risk Traceability table. Coverage is proportional: Critical risks (R-02 weight sum, R-05 mutation paths) get comprehensive coverage; Low risks (R-12 f64 cast) get minimal coverage.

The security risk assessment is appropriate for a feature that computes rather than accepts values. The key insight (SR-SEC-01) that gaming resistance does not depend on formula secrecy is correct and well-documented.

Integration risks (IR-01 through IR-03) cover the most likely failure modes at component boundaries. Edge cases (EC-01 through EC-05) exercise the formula at extreme values.

R-11 (existing test breakage) correctly identifies the highest-likelihood risk and provides a clear mitigation strategy.
