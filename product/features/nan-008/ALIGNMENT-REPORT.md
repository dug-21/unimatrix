# Alignment Report: nan-008

> Reviewed: 2026-03-26
> Artifacts reviewed:
>   - product/features/nan-008/architecture/ARCHITECTURE.md
>   - product/features/nan-008/specification/SPECIFICATION.md
>   - product/features/nan-008/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Additive metric extension to W1-3 eval harness; directly serves the intelligence measurement mandate |
| Milestone Fit | PASS | Belongs to W1-3 / Nanoprobes phase; no future-wave capability built |
| Scope Gaps | WARN | AC-09 / FR-12 baseline recording is a mandatory AC but depends on a runtime procedure that cannot be pre-verified in design |
| Scope Additions | WARN | RISK-TEST-STRATEGY.md adds one edge-case test (R-05 `test_icd_two_entries_one_category_each`) not enumerated in SCOPE.md AC-10; acceptable but noted |
| Architecture Consistency | PASS | All design decisions traceable to SCOPE.md constraints; open questions resolved in ARCHITECTURE.md ADRs |
| Risk Completeness | PASS | 13-risk register; all SCOPE-RISK-ASSESSMENT.md risks traced to architecture and test coverage |

**Overall: PASS with two WARNs. No VARIANCEs or FAILs. Cleared for delivery.**

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | Baseline recording execution (AC-09 / FR-12) | SCOPE.md requires `log.jsonl` to contain a nan-008 entry with real `cc_at_k` and `icd` values. ARCHITECTURE.md documents the procedure (ADR-005, Baseline Recording Procedure section). SPECIFICATION.md makes it Constraint 11. RISK-TEST-STRATEGY.md classifies it as R-06 (post-delivery verification). The gap: this AC is satisfiable only at delivery runtime — it cannot be pre-verified or unit-tested. Design documents acknowledge this but it remains an execution dependency, not a design defect. Classified WARN, not FAIL, because the procedure is fully specified. |
| Addition | R-05 test case `test_icd_two_entries_one_category_each` | RISK-TEST-STRATEGY.md adds this test (R-05 section) beyond the four boundary cases enumerated in SCOPE.md AC-10. It covers the `p = 0.5` path through `compute_icd`. This is a beneficial elaboration within the spirit of the scope, not a scope expansion. |
| Addition | R-10 `test_compute_comparison_delta_signs` (positive and negative variants) | RISK-TEST-STRATEGY.md adds two delta-sign unit tests not in SCOPE.md AC list. These close a real risk (sign-flip in `compute_comparison`) not surfaced in the original scope. Beneficial addition. |
| Addition | R-11 `test_aggregate_stats_cc_at_k_mean` for both CC@k and ICD | RISK-TEST-STRATEGY.md adds aggregate-mean unit tests not enumerated in SCOPE.md. These guard against a divide-by-wrong-count error in `compute_aggregate_stats`. Beneficial addition. |
| Addition | R-12 `test_cc_at_k_scenario_rows_sort_order` | Not in SCOPE.md. Guards sort direction in Distribution Analysis. Beneficial addition. |
| Addition | R-13 pre-delivery `eval --help` snapshot subcommand check | Operational check added by risk strategy; not in SCOPE.md. Low severity; advisory only. Beneficial. |
| Simplification | ICD per-phase breakdown (non-goal in SCOPE.md) | All three source documents consistently honour this deferral. No attempt to add it. Rationale: requires #397 (phase-in-scenarios). |
| Simplification | NEER metric (non-goal in SCOPE.md) | Consistently deferred across all documents. Rationale: requires session context across queries. |

---

## Variances Requiring Approval

None. All checks passed or produced WARNs with no blocking issues.

---

## Detailed Findings

### Vision Alignment

The product vision states (W1-3): "Every intelligence change is measured against real query scenarios before reaching agents. Regressions caught before production." It further specifies the gate condition for W1-4 and W3-1 on W1-3 eval harness results.

nan-008 extends W1-3 with two metrics (CC@k and ICD) that directly address a limitation the vision does not enumerate but is a logical consequence of the harness design: distribution-shifting features produce false regression signals under P@K and MRR. The feature is additive only — no eval harness components are removed or modified in ways that would break the existing P@K/MRR mandate. The gate conditions for W1-4 and W3-1 that reference eval harness results remain valid after nan-008; the harness now produces richer output.

The vision's domain-agnostic principle is explicitly honoured: SCOPE.md Goal 5, SPECIFICATION.md NFR-04, and ARCHITECTURE.md ADR-001 all source the category denominator from `KnowledgeConfig.categories` (the profile's config override), never from a hardcoded list. The feature cannot introduce domain coupling by design.

No conflict with the intelligence pipeline, confidence system, or graph persistence architecture. nan-008 operates entirely within `crates/unimatrix-server/src/eval/` and touches no production MCP path.

### Milestone Fit

W1-3 (Evaluation Harness) is listed in the product vision as an active Wave 1 item (status: `~1.5-2 weeks`). nan-008 is a direct extension of nan-007 (the W1-3 delivery). The Nanoprobes prefix (`nan-`) is the project's build/deploy/CI phase — the eval harness fits within this phase as tooling infrastructure. There is no future-milestone capability built: no GNN integration, no W3-1 signal work, no Wave 2 deployment infrastructure.

The vision references the eval harness as a gate for W2-4 (GGUF) and W3-1 (GNN). Adding CC@k and ICD strengthens those gate conditions by providing distribution-aware evidence. This is milestone-aligned, not milestone-jumping.

SCOPE.md Non-Goal: "No automated shipping gate" — the `>= 0.7` PPR target is human-reviewed. All three source documents preserve this. No hardcoded CI gate was introduced.

### Architecture Review

ARCHITECTURE.md is internally consistent and traces directly to SCOPE.md.

**Covered well:**
- Component breakdown maps precisely to the six files listed in SCOPE.md §"Change surface 1/2/3".
- The dual-type-copy problem (SR-01) is the highest-risk item in the feature; ARCHITECTURE.md devotes a dedicated section to it with an explicit synchronization checklist (7 steps) and mandates a round-trip test (ADR-003). This matches the SCOPE-RISK-ASSESSMENT.md SR-01 recommendation.
- All three open questions from SCOPE.md (OQ-1 through OQ-3) are resolved in ARCHITECTURE.md ADRs (ADR-001, ADR-002, ADR-005 respectively). No open questions remain.
- The ownership trace in ARCHITECTURE.md (SR-07 resolution) explicitly traces the borrow lifetime for `configured_categories` in `replay.rs` and confirms no lifetime conflict — this directly responds to SCOPE-RISK-ASSESSMENT.md SR-07.
- Baseline recording procedure (ADR-005) specifies 6 named steps, responds to SR-04.

**No concerns.**

### Specification Review

SPECIFICATION.md covers all 11 acceptance criteria from SCOPE.md and extends them to 14 (AC-12, AC-13, AC-14 added).

- AC-12 (round-trip test) maps to SCOPE-RISK-ASSESSMENT.md SR-01+SR-06 combined test recommendation — directly responsive.
- AC-13 (full rendered-markdown test for section ordering) addresses pattern #3426 evidence cited in SCOPE-RISK-ASSESSMENT.md SR-06.
- AC-14 (ICD annotation) closes SR-03 (ICD unbounded range miscomparison risk).

All 12 Functional Requirements (FR-01 through FR-12) map cleanly to SCOPE.md goals 1-9. The specification does not introduce any capability not in SCOPE.md. Non-goals in SCOPE.md (NEER, per-phase ICD, automated gate, scenario format changes) are all listed in SPECIFICATION.md §"NOT in Scope".

Constraints section (11 items) fully matches or elaborates SCOPE.md constraints (9 items). Elaborations respond to SCOPE-RISK-ASSESSMENT.md recommendations:
- Constraint 7 (division-by-zero + warn) responds to SR-02.
- Constraint 11 (baseline recording as named step) responds to SR-04.

One note: SPECIFICATION.md §OQ-01 carries forward open question about snapshot availability at delivery time. This is consistent with ARCHITECTURE.md ADR-005 but remains an execution dependency. See Scope Alignment "Gap" row above.

### Risk Strategy Review

The risk register has 13 risks covering the full threat surface identified in SCOPE-RISK-ASSESSMENT.md (SR-01 through SR-07) plus 6 additional implementation-level risks (R-05 through R-13) not in the scope assessment.

**Traceability**: RISK-TEST-STRATEGY.md §"Scope Risk Traceability" provides a complete SR→R mapping table covering all 7 scope risks:
- SR-01 → R-01 (mitigated by round-trip test, ADR-003)
- SR-02 → R-03, R-09 (mitigated by tracing::warn!, ADR-004)
- SR-03 → R-04 (mitigated by ln(n) annotation, ADR-002, FR-10, AC-14)
- SR-04 → R-06, R-13 (mitigated by named delivery step, ADR-005)
- SR-05 → accepted (NFR-07 delivery agent note)
- SR-06 → R-02 (mitigated by position assertion, ADR-003, AC-13; Pattern #3426 cited)
- SR-07 → resolved by architecture (ownership trace in ARCHITECTURE.md)

**Edge case coverage**: The edge cases section addresses five non-obvious failure modes including `ln(0)` propagation (R-05), category-not-in-configured-list leading to CC@k > 1.0 (edge case flagged as needing a test), k=0 result sets, and float aggregation precision.

**One flagged item in risk strategy**: The edge cases section notes: "The formula counts all distinct result categories without filtering against `configured_categories`. This means CC@k can exceed 1.0 if results contain categories absent from the configured list. A test case for this edge case is required." This is flagged in RISK-TEST-STRATEGY.md but not assigned a Risk ID or test scenario. It is not called out in SPECIFICATION.md FR-04. SCOPE.md formula states `CC@k = |{cat : exists entry in top-k with entry.category = cat}| / |configured_categories|` which counts all distinct result categories — potentially exceeding 1.0. This is a WARN-level ambiguity: the implementation choice (intersection vs. union) is not resolved in the specification. The risk strategy correctly identifies it but does not mandate resolution.

**Classified WARN**: Implementation could legitimately interpret the formula as counting only categories that are in the intersection of result categories and configured_categories (capping at 1.0) or as counting all distinct result categories regardless (allowing > 1.0). The spec does not disambiguate. This should be resolved in the delivery implementation and noted.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for "vision alignment review patterns scope variance" (pattern category) -- found 3 results: #2298 (config key semantic divergence pattern, dsn-001), #3426 (formatter overhaul section-order regression risk, col-026 -- directly applicable to nan-008 SR-06/R-02), #3337 (architecture diagram informal headers diverge from spec, crt-028). Pattern #3426 is relevant and was already cited in RISK-TEST-STRATEGY.md, confirming correct awareness.
- Stored: nothing novel to store -- the variances found are feature-specific (edge case formula ambiguity in CC@k intersection vs. union, baseline recording execution dependency). The dual-type-copy recurring failure is already pattern #3512 per RISK-TEST-STRATEGY.md §R-01 evidence. No new generalizable pattern emerged from this review.
