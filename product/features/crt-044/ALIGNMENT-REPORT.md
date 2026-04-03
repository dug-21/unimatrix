# Alignment Report: crt-044

> Reviewed: 2026-04-03
> Artifacts reviewed:
>   - product/features/crt-044/architecture/ARCHITECTURE.md
>   - product/features/crt-044/specification/SPECIFICATION.md
>   - product/features/crt-044/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-044/SCOPE.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Graph integrity and correctness support the intelligence pipeline vision |
| Milestone Fit | PASS | Wave 1A prerequisite work; squarely within Cortical phase |
| Scope Gaps | PASS | All six SCOPE.md goals fully addressed across source documents |
| Scope Additions | WARN | SPECIFICATION.md adds three ACs (AC-12–AC-14) beyond SCOPE.md; additions are risk-driven and beneficial but not explicitly requested in SCOPE.md |
| Architecture Consistency | PASS | Architecture is internally consistent and cross-references vision patterns correctly |
| Risk Completeness | PASS | All six SCOPE-RISK-ASSESSMENT risks traced; 10 novel risks added with appropriate severity/coverage |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | AC-12 (pairs_written PR documentation requirement) | In SPECIFICATION.md but not in SCOPE.md — addresses SR-01 from SCOPE-RISK-ASSESSMENT |
| Addition | AC-13 (false-return no-warn test) | In SPECIFICATION.md but not in SCOPE.md — addresses SR-02 from SCOPE-RISK-ASSESSMENT |
| Addition | AC-14 (two-run idempotency test with pre-existing reverse edge) | In SPECIFICATION.md but not in SCOPE.md — addresses SR-05 from SCOPE-RISK-ASSESSMENT |
| Simplification | OQ-1/OQ-2/OQ-3 resolved in SCOPE.md itself | All open questions resolved before architecture and specification were written; not simplifications but completions. Rationale: the researcher resolved these as part of the scope phase, appropriate. |

The three added ACs (AC-12 through AC-14) are derived directly from the SCOPE-RISK-ASSESSMENT (SR-01, SR-02, SR-05). The risk-to-AC traceability chain is explicit in SPECIFICATION.md §Acceptance Criteria and RISK-TEST-STRATEGY.md §Scope Risk Traceability. The additions strengthen the feature, not expand its functional scope.

---

## Variances Requiring Approval

### WARN: Scope Additions — AC-12, AC-13, AC-14

**What**: SPECIFICATION.md adds three acceptance criteria (AC-12, AC-13, AC-14) that are not present in SCOPE.md. These require:
- AC-12: PR description must document the `pairs_written` semantic change.
- AC-13: A test asserting no warn-level logging when `write_graph_edge` second call returns false.
- AC-14: A distinct idempotency test against a partially-bidirectional input state.

**Why it matters**: SCOPE.md defines what was asked for. AC-09 and AC-07 already cover some idempotency territory. AC-12 through AC-14 extend the test surface beyond the original ask.

**Recommendation**: Accept. These additions are traceable to SCOPE-RISK-ASSESSMENT risks (SR-01, SR-02, SR-05), are additive-only (no scope reduction, no logic change), and reduce delivery risk for a feature where silent failures are the predominant failure mode. No human approval is required before delivery proceeds, but the delivery agent should be aware these ACs exist and are binding.

---

## Detailed Findings

### Vision Alignment

The product vision (Wave 1A) describes the intelligence pipeline as a "session-conditioned, self-improving relevance function." The PPR expander (`graph_expand`) is the graph traversal component that expands the candidate pool for this pipeline. crt-044 corrects a structural defect — half the graph relationship signal was invisible to the expander — that would prevent crt-042's eval gate from producing meaningful P@5 improvements.

The vision explicitly states: "A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency." Bidirectionality of symmetric relationships (tag co-occurrence, structural vocabulary, co-retrieval affinity) is a prerequisite for the graph to faithfully represent these relationships. A forward-only graph misrepresents the symmetry of S1/S2/S8 signals.

The `// SECURITY:` comment addition aligns with the vision's integrity chain emphasis: "tamper-evident from first write to last." Making quarantine obligations visible at every call site is a form of integrity enforcement at the API contract level.

No vision principles are contradicted. No future-milestone capabilities (W2, W3) are being built ahead of schedule.

### Milestone Fit

crt-044 is a Cortical phase feature. Its explicit dependency chain is:
- crt-041 (S1/S2/S8 single-direction write, shipped)
- crt-042 (graph_expand Outgoing-only traversal, shipped)
- crt-044 (bidirectional back-fill + forward writes, this feature) → enables crt-042 eval gate

The wave placement is Wave 1A infrastructure: graph correctness is a prerequisite for the PPR expander to produce meaningful intelligence pipeline improvements (WA-0 through WA-4 completed, eval gate pending). This is not building ahead into Wave 2 or Wave 3 territory. The schema migration (v19→v20) follows the established migration cadence.

One notable timing constraint: crt-043 is in delivery and treats v20 as its migration baseline (v20→v21). SPECIFICATION.md and RISK-TEST-STRATEGY.md both identify this (R-02) as the highest-probability integration failure. This is a delivery sequencing concern, not a design defect. The documents correctly flag it as a pre-merge gate.

### Architecture Review

The architecture is well-scoped and internally consistent. Specific findings:

**Strength — Pattern adherence**: The architecture explicitly references the crt-035 back-fill template (entry #3889) and the `co_access_promotion_tick.rs` two-call pattern. Both precedents are correctly applied. The SQL for Statement A and Statement B matches the crt-035 template exactly, with appropriate `relation_type` and `source` substitutions.

**Strength — SR-02 resolution**: The `write_graph_edge` return value contract (entry #4041 three-case model) is explicitly documented in the architecture. The distinction between Ok(false) (UNIQUE conflict, expected) and Err (SQL error, log internally) is spelled out. This prevents the most common implementation mistake for this pattern.

**Strength — Component isolation**: The architecture correctly identifies that `graph_expand` traversal logic does NOT need to change — the fix is at the write site. This is the minimum-change solution.

**Finding — ARCHITECTURE.md crate attribution (resolved)**: The architecture correctly stated `crates/unimatrix-engine/src/graph_expand.rs`. The specification incorrectly listed `unimatrix-server` as the owning crate. Specification corrected before Session 2. Authoritative path: `crates/unimatrix-engine/src/graph_expand.rs`.

**Finding — `pairs_written` counter naming**: ARCHITECTURE.md refers to the S1/S2 counter as `edges_written` and the S8 counter as `pairs_written`. SPECIFICATION.md FR-T-04 and C-06 address only `pairs_written` (S8). The naming difference is correct — it reflects the existing code, not a spec ambiguity. No action required.

### Specification Review

The specification is complete, precise, and fully traces to SCOPE.md. All eleven original ACs are carried forward verbatim with verification methods added. Three additional ACs (AC-12 through AC-14) are added from the risk assessment, each with explicit SR-ID traceability.

**Strength — Open questions closed**: All three SCOPE.md open questions (OQ-1, OQ-2, OQ-3) are resolved before the specification was written. The spec carries the resolutions as constraints (C-06, C-03) and ACs (AC-08, AC-12), with no ambiguity remaining for delivery.

**Strength — Non-goals enforcement**: The specification's NOT In Scope section is a verbatim mirror of SCOPE.md's Non-Goals. No non-goal has crept into the functional requirements or ACs. NLI intentional unidirectionality (col-030 ADR) is preserved as an explicit exclusion in both FR-M-06 and C-04.

**Finding — crt-043 delivery ordering note**: SPECIFICATION.md §Open Questions includes a delivery-sequencing note for the implementation agent regarding crt-043's v20 baseline. This note is correctly placed — it is not an ambiguity in the specification but a deployment coordination concern. The implementation agent must act on it. The RISK-TEST-STRATEGY covers this as R-02 with a pre-merge gate requirement.

**Finding — AC-01 verification completeness**: AC-01's verification method states the count must be `COUNT(*) > 0 AND COUNT(*) = (SELECT COUNT(*) FROM GRAPH_EDGES WHERE relation_type = 'Informs')`. This verification query does not scope to `source IN ('S1','S2')` — it counts ALL Informs edges, including `nli` and `cosine_supports`. If those sources have forward-only edges (which they should, by design), the count equality assertion would fail. The test in practice should scope both sides to `source IN ('S1','S2')`. This is a test-writing concern, not a functional requirement defect. The delivery agent should be aware.

### Risk Strategy Review

The RISK-TEST-STRATEGY is thorough and correctly risk-ranked. Ten risks are identified; the three Critical risks (R-01, R-02, R-03) receive the most detailed test scenario coverage.

**Strength — Per-source regression gate design**: R-03 (one tick function omits second call) is explicitly designed to fail independently per source — three separate tests, not one combined count check. This directly addresses the lesson from crt-042 (entry #4076: zero mandatory tests shipped) and ensures source-specific regressions are detectable without ambiguity.

**Strength — R-02 pre-merge gate**: The delivery sequencing risk (crt-043 consuming v20 first) is correctly identified as un-testable in CI alone. The strategy calls for a code-review gate: reviewer confirms base branch `CURRENT_SCHEMA_VERSION = 19` before merge. This is the appropriate mitigation for a deployment-order risk.

**Strength — Scope risk traceability**: The §Scope Risk Traceability table maps every SCOPE-RISK-ASSESSMENT risk (SR-01 through SR-06) to an architecture risk (R-ID), an ADR reference, and the specific ACs that cover it. The traceability chain is complete.

**Finding — R-08 acceptance without a future refactor checklist**: R-08 (security comment staleness) is accepted per ADR-003 with a note that "future refactors of SecurityGateway should include a grep of `// SECURITY:` comments as a pre-merge checklist item." This recommendation has no enforcement mechanism — no AC, no procedure stored in Unimatrix, no checklist entry. This is an accepted low-severity risk, but the recommendation is advisory only. Human awareness: if SecurityGateway is ever refactored, the `// SECURITY:` comment in `graph_expand.rs` must be manually reviewed.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `vision alignment scope addition milestone discipline` — found entries #2298 (config key semantic divergence), #3742 (deferred branch scope-addition WARN pattern). Entry #3742 is directly applicable: it establishes the pattern that optional future branches in architecture that diverge from scope deferral should be flagged as WARN. crt-044 does not add deferred future branches — all work is immediate and scoped. Pattern confirmed: no WARN triggered by this pattern.
- Stored: nothing novel to store — the scope addition pattern here (risk-assessment-driven AC additions) is feature-specific and traceable. If the same pattern (SCOPE-RISK-ASSESSMENT driving spec ACs beyond SCOPE.md) appears in 2+ future features, it warrants a stored convention clarifying that risk-driven ACs are acceptable scope additions when SR-ID-traced.
