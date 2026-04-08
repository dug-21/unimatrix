# Alignment Report: crt-051

> Reviewed: 2026-04-08
> Artifacts reviewed:
>   - product/features/crt-051/architecture/ARCHITECTURE.md
>   - product/features/crt-051/specification/SPECIFICATION.md
>   - product/features/crt-051/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Agent ID: crt-051-vision-guardian

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Fixes a semantic integrity defect in Lambda — directly supports the vision's "trustworthy, correctable, ever-improving" knowledge engine principle |
| Milestone Fit | PASS | Cortical-phase corrective bugfix; no future-wave capabilities pulled in |
| Scope Gaps | PASS | All 13 SCOPE.md acceptance criteria (AC-01 through AC-13) are addressed in the specification |
| Scope Additions | WARN | RISK-TEST-STRATEGY.md advocates for the architect's fixture approach over the spec writer's approach (R-02), introducing a minor cross-document discrepancy that delivery must resolve explicitly |
| Architecture Consistency | PASS | Architecture is internally consistent; all open questions from SCOPE.md are resolved; component diagram matches spec |
| Risk Completeness | PASS | All 7 SCOPE-RISK-ASSESSMENT.md risks (SR-01 through SR-07) are explicitly traced to RISK-TEST-STRATEGY.md risks (R-01 through R-08) with resolution dispositions |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Open Question 1 (normalization) resolved before delivery | SCOPE.md flagged pair-count vs. unique-entry-count as unresolved; ARCHITECTURE.md and SPECIFICATION.md both confirm pair-count per human decision. Acceptable — scope explicitly asks delivery to "confirm which interpretation the human intends." |
| Simplification | Open Question 2 (cold-start) resolved before delivery | SCOPE.md flagged cold-start distinguishability as a question. SPECIFICATION.md confirms score=1.0 is correct in both states; `contradiction_scan_performed` boolean is the operator-visible discriminator. No code change needed. Acceptable. |
| Simplification | Open Question 3 (JSON schema) resolved before delivery | SCOPE.md flagged whether API documentation needs updating. SPECIFICATION.md confirms one changelog line is confirmed; no schema change. Acceptable. |
| Addition (minor) | AC-14 through AC-17 added by SPECIFICATION.md | SCOPE.md has AC-01 through AC-13. Spec adds AC-14 (coherence.rs test rewrite), AC-15 (fixture update), AC-16 (phase ordering comment), AC-17 (cold-start named test). These derive directly from SCOPE-RISK-ASSESSMENT.md recommendations SR-01 through SR-05 — not novel scope additions. Acceptable and expected from a risk-informed spec. |
| Addition (minor) | RISK-TEST-STRATEGY.md fixture resolution recommendation | R-02 advocates `contradiction_count: 15` (architect) over `contradiction_density_score: 1.0` (spec, AC-15). The spec and risk strategy are in explicit disagreement on this single point. See Variances. |

---

## Variances Requiring Approval

### VARIANCE-01 (WARN): Fixture resolution disagreement between SPECIFICATION.md and RISK-TEST-STRATEGY.md

**What**: SPECIFICATION.md (AC-15) specifies the `make_coherence_status_report()` fixture in `response/mod.rs` should be updated to `contradiction_density_score: 1.0` with `contradiction_count: 0`. RISK-TEST-STRATEGY.md (R-02) explicitly recommends the opposite: set `contradiction_count: 15` and keep `contradiction_density_score: 0.7000`, giving the fixture a semantically non-trivial value that exercises the formula path.

The risk strategy document explicitly states: "Recommendation: Use the architect's approach — set `contradiction_count: 15`, keep `contradiction_density_score: 0.7000`" and provides a rationale that the spec's approach "produces `contradiction_count: 0` with `contradiction_density_score: 1.0` ... but tests nothing about the formula."

**Why it matters**: Delivery will receive two contradictory instructions. If the spec takes precedence (AC-15), the fixture is semantically valid but trivial — the formula path is never exercised in a fixture context. If the risk strategy's recommendation is followed, the fixture is more meaningful but AC-15 is technically violated. Neither outcome is catastrophic; the test suite passes either way. But the discrepancy creates ambiguity that could cause a delivery agent to choose the wrong interpretation or produce a report that looks like a spec violation.

**Recommendation**: Human decision required before delivery starts. Options:
1. Accept risk strategy recommendation: update SPECIFICATION.md AC-15 to specify `contradiction_count: 15` and `contradiction_density_score: 0.7000`. This is the architecturally stronger choice.
2. Retain spec AC-15 as written: update fixture to `contradiction_density_score: 1.0`, accept that the fixture is trivial. Add a separate unit test to exercise the non-trivial path if meaningful coverage on the formula is desired.
3. Accept either: explicitly annotate in the IMPLEMENTATION-BRIEF that delivery may use either approach and both are acceptable.

---

## Detailed Findings

### Vision Alignment

The product vision states: "It captures knowledge that emerges from doing work — in any domain — and makes it trustworthy, correctable, and ever-improving."

The Lambda coherence metric is the primary tool Unimatrix uses to assess knowledge-base health and guide maintenance decisions (`context_status` with `maintain: true`). `contradiction_density` carries weight 0.31 — the second-highest dimension. crt-051 fixes a structural defect where this dimension measures quarantine count (a status attribute with no causal relationship to contradiction health) rather than actual detected contradiction pairs.

The vision's "trustworthy" principle is directly implicated: an operator reading Lambda as a health signal receives a semantically meaningless `contradiction_density` value. A knowledge base with 0 contradictions but 10 quarantined entries is penalized; one with many real contradictions and 0 quarantined entries scores 1.0. This is the inverse of trustworthy.

The product vision's Critical Gaps table explicitly tracks: "Time-based freshness in Lambda — domain-specific assumption" as **Resolved** (crt-020). crt-051 follows the same corrective pattern: removing a structurally broken Lambda input and replacing it with a semantically valid one. This is consistent with the vision's emphasis on fixing Lambda inputs before building higher-level intelligence on top of them (see WA-0's stated rationale: "Before adding session-conditioned signals to the ranking pipeline, the pipeline's existing signals must be fused correctly").

**Finding: PASS.** The feature directly serves vision integrity.

### Milestone Fit

crt-051 is a Cortical-phase corrective fix targeting a confirmed bug in the Lambda scoring path (GH #540). It:
- Makes no changes to Lambda weights (preserved per ADR-001 crt-048)
- Adds no new Wave 1A, Wave 2, or Wave 3 capabilities
- Introduces no new dependencies, tables, or schema migrations
- Does not re-enable or introduce NLI infrastructure (explicitly excluded from scope)
- Makes no changes to `scan_contradictions` or `ContradictionScanCacheHandle`

The SCOPE.md Non-Goals section is precise and the source documents honor it fully. There is no evidence of future-milestone capability being pulled into this feature.

The Cortical phase (crt-*) is the appropriate home for Lambda scoring corrections. The vision's Critical Gaps table and roadmap do not assign a specific wave to this fix — it is a correctness fix for existing infrastructure, not a new capability, and fits naturally as a pre-Wave-1A integrity correction (consistent with WA-0's "fix before adding" principle).

**Finding: PASS.** Milestone fit is appropriate.

### Architecture Review

The architecture document is well-scoped and internally consistent. Specific findings:

- **Component breakdown**: Three components identified — `coherence.rs` (scoring function), `status.rs` (call site), `response/mod.rs` (test fixture). All three match the scope's Proposed Approach steps 1–4.
- **Before/after code**: Exact before and after signatures and implementations are shown. The after-implementation exactly matches SCOPE.md's Proposed Approach and all SPECIFICATION.md functional requirements.
- **Phase ordering invariant**: Architecture documents the load-bearing Phase 2 → Phase 5 ordering dependency explicitly, including the recommended comment text. This exceeds what SCOPE.md required (SCOPE.md flagged it as SR-03 "no code change needed, just documentation guard") and is appropriate.
- **SR-02 resolution**: Architecture recommends `contradiction_count: 15` fixture approach (architect's approach). This is the source of VARIANCE-01 — see above.
- **Open questions**: Architecture correctly notes all SCOPE open questions are resolved by the spawn prompt.
- **Component interaction diagram**: The data flow diagram from `ContradictionScanCacheHandle` through Phase 2 → Phase 5 → Lambda is accurate and matches the specification's domain model section.
- **Integration surface table**: Complete and consistent with specification's dependencies table.

One observation: ARCHITECTURE.md states "See ADR-001 for the decision rationale" (for the technology decisions section) but no ADR-001 entry is written in this document. The architecture refers to an ADR for the pair-count normalization decision that appears to have been stored in Unimatrix rather than in the document. This is consistent with the project's pattern (ADRs live in Unimatrix only, not in files per CLAUDE.md) — not a variance.

**Finding: PASS.**

### Specification Review

The specification is thorough and risk-informed. Findings:

- **FR-01 through FR-12**: All functional requirements are present and correctly derived from SCOPE.md goals and SCOPE-RISK-ASSESSMENT.md recommendations.
- **NFR-01 through NFR-07**: Non-functional requirements are complete: function purity, no schema changes, no new dependencies, f64 precision discipline, compilation, clippy, test suite.
- **AC-01 through AC-13**: All 13 SCOPE.md acceptance criteria are preserved verbatim with verification methods added. No SCOPE AC is dropped or weakened.
- **AC-14 through AC-17**: Risk-derived additions. AC-14 (SR-01 test rewrite), AC-15 (SR-02 fixture — source of VARIANCE-01), AC-16 (SR-03 phase comment), AC-17 (SR-05 cold-start named test). These are appropriate additions that follow the SCOPE-RISK-ASSESSMENT.md recommendations.
- **Test sites enumerated**: The spec enumerates exact file paths, line numbers, test names, and required changes for all three impacted files. This is precise and delivery-actionable.
- **Domain models**: The domain model section correctly distinguishes `contradiction_pair_count`, `ContradictionScanCacheHandle`, `StatusReport.contradiction_count`, `StatusReport.total_quarantined`, and Lambda. No confusion between the corrected and preserved uses of `total_quarantined`.
- **Open questions resolution**: All three SCOPE open questions are explicitly resolved with human-confirmed dispositions.
- **Changelog entry**: Exactly one line confirmed. Correct.

The specification's AC-15 (set fixture to `contradiction_density_score: 1.0`) contradicts RISK-TEST-STRATEGY.md R-02 recommendation. This is the only material discrepancy in the document set.

**Finding: PASS** (with VARIANCE-01 noted).

### Risk Strategy Review

The risk strategy is complete and directly traceable. Findings:

- **Scope risk traceability table**: All 7 SCOPE-RISK-ASSESSMENT.md risks (SR-01 through SR-07) are explicitly mapped to RISK-TEST-STRATEGY.md risks (R-01 through R-08) with resolution dispositions. SR-04 and SR-07 are correctly marked as resolved/accepted, not assigned to open risks.
- **R-01 (missed call site)**: Covered by three grep verifications — positive, reverse, and call-site enumeration. Correct.
- **R-02 (fixture discrepancy)**: The architect's approach recommendation is well-reasoned. The mathematical argument is sound: `1.0 - 15/50 = 0.7000` exactly. The rationale for why the spec's approach is weaker is clearly articulated. However, this creates VARIANCE-01 against SPECIFICATION.md AC-15.
- **R-03 through R-08**: All risks have clear test scenarios, coverage requirements, and severity/likelihood assessments. R-08 (grep gate false-positive) is appropriately classified Low and has a plausible mitigation.
- **Integration risks**: The call site isolation risk and the argument-transposition risk (both numeric args compile; transposition would produce wrong scores) are specifically called out. The mid-range test case (AC-05 / R-07) is explicitly designed to catch transposition.
- **Security risks**: Correctly assessed as minimal — no untrusted data enters the scoring function. The blast-radius analysis ("limited to Lambda computation quality; no data written") is accurate.
- **Edge cases EC-01 through EC-04**: Complete. EC-04 (poisoned RwLock → `contradiction_count: 0` → score 1.0 optimistic) is noted and accepted, consistent with the existing codebase pattern.

**Finding: PASS** (with VARIANCE-01 noted in context of R-02 vs AC-15 disagreement).

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found entries #3742 (optional future branch architecture must match scope intent) and #3337 (architecture diagram headers diverge from spec — testers assert wrong strings). Entry #3742 is the most relevant recurring pattern: "WARN if architecture and risk diverge from scope deferral." Applied: VARIANCE-01 is this exact pattern — the risk strategy recommends an approach that diverges from the specification's stated AC-15.
- Stored: nothing novel to store — the VARIANCE-01 pattern (risk strategy recommends a stronger fixture approach than the spec's conservative AC) is a feature-specific discrepancy arising from the architect writing RISK-TEST-STRATEGY.md after the spec was finalized. It does not generalize without more instances across multiple features.
