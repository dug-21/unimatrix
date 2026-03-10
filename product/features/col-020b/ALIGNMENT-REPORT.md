# Alignment Report: col-020b

> Reviewed: 2026-03-10
> Artifacts reviewed:
>   - product/features/col-020b/architecture/ARCHITECTURE.md
>   - product/features/col-020b/specification/SPECIFICATION.md
>   - product/features/col-020b/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Bug fix improves observability of the self-learning pipeline, directly supports "auditable knowledge lifecycle" |
| Milestone Fit | PASS | col-020b is a bug fix within Activity Intelligence milestone (col-020 follow-up) |
| Scope Gaps | PASS | All 16 acceptance criteria addressed across source documents |
| Scope Additions | PASS | No features added beyond what SCOPE.md requests |
| Architecture Consistency | PASS | Changes localized to 2 crates, respects service layer boundaries and existing patterns |
| Risk Completeness | PASS | 13 risks identified, all scope risks (SR-01 through SR-08) traced to architecture risks with resolutions |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| (none) | — | No scope gaps, additions, or unauthorized simplifications detected |

All 16 acceptance criteria from SCOPE.md (AC-01 through AC-16) are addressed:

- AC-01 through AC-05 (normalization, classification, counters): Covered by ARCHITECTURE C1-C3, SPEC FR-01 through FR-03, RISK R-01/R-08/R-10
- AC-06 (SessionSummary renames with serde compat): Covered by ARCHITECTURE C4, SPEC FR-03/Backward Compatibility, RISK R-02
- AC-07 through AC-10 (FeatureKnowledgeReuse semantics): Covered by ARCHITECTURE C5, SPEC FR-04/FR-06, RISK R-04/R-05
- AC-11 (RetrospectiveReport rename): Covered by ARCHITECTURE C4, SPEC FR-05, RISK R-02
- AC-12 (knowledge_curated backward compat): Covered by ARCHITECTURE C4, SPEC Backward Compatibility, RISK R-03
- AC-13 (existing tests updated): Covered by SPEC Test Requirements "Updated Existing Tests"
- AC-14 (MCP-prefixed test coverage): Covered by SPEC Test Requirements "New Tests Required", RISK R-08
- AC-15 (single-session delivery regression): Covered by SPEC AC-07/AC-15, RISK R-04
- AC-16 (debug tracing): Covered by ARCHITECTURE C6, SPEC FR-07, RISK R-06

## Variances Requiring Approval

None. All source documents align with SCOPE.md and product vision.

## Detailed Findings

### Vision Alignment

The product vision states the core value proposition is "auditable knowledge lifecycle" with "invisible delivery" and "confidence evolution from real usage signals." col-020b directly supports this by fixing the knowledge flow counters (`knowledge_served`, `knowledge_stored`) that measure how knowledge is delivered and created during agent sessions. Without this fix, the retrospective pipeline cannot accurately report on knowledge utilization -- a core feedback loop in the self-learning pipeline.

The vision's emphasis on "what has actually worked" and "not what someone wrote in a wiki six months ago" is supported by the revised `FeatureKnowledgeReuse` semantics, which now count all delivered entries rather than only cross-session reuse. This provides a more accurate picture of which knowledge the system is actually serving to agents.

The addition of `knowledge_curated` (counting `context_correct`, `context_deprecate`, `context_quarantine`) aligns with the vision's "correctable and auditable" principle -- the system now tracks curation activity as a first-class metric.

### Milestone Fit

col-020b is a bug fix for col-020 (Multi-Session Retrospective), which is Wave 2 of the Activity Intelligence milestone. The product vision roadmap lists col-020 as a Wave 2 feature. This is a direct follow-up fix, not an expansion of scope. The feature stays within the Activity Intelligence milestone boundary and does not pull in capabilities from future milestones (Graph Enablement, Platform Hardening).

### Architecture Review

The architecture document (7 components, C1-C7) maps cleanly to the SCOPE.md proposed approach (sections A-F). Key observations:

1. **Crate boundaries respected**: Changes are confined to `unimatrix-observe` (C1-C4, C7) and `unimatrix-server` (C5-C6), matching the 2-crate scope from SCOPE.md. The Store crate is read-only.

2. **ADR decisions address scope risks**: 5 ADRs are defined, each traceable to a scope risk (SR-01 through SR-08). ADR-002 (Rust-only tests) resolves SR-04/SR-08. ADR-003 (unidirectional serde compat) resolves SR-01/SR-02. ADR-005 (time-boxed #193 investigation) resolves SR-03.

3. **Component interaction diagram** accurately reflects the data flow from `tools.rs` through `session_metrics.rs` and `knowledge_reuse.rs` to `types.rs` and back to MCP JSON response.

4. **`normalize_tool_name` as private function** (ADR-001) is appropriate -- the normalization concern is local to session metric computation, not a shared utility. This matches the vision's service layer architecture where business logic is encapsulated within modules.

### Specification Review

The specification is thorough and well-structured:

1. **8 functional requirements** (FR-01 through FR-08) cover all SCOPE.md goals (1-8).
2. **16 acceptance criteria** mirror SCOPE.md's AC-01 through AC-16 exactly, with added verification methods.
3. **Backward Compatibility section** explicitly addresses SR-01 with a clear rationale for unidirectional serde compat. The analysis that `RetrospectiveReport` is ephemeral (MCP tool output, never persisted) is sound.
4. **7 constraints** (C-01 through C-07) match SCOPE.md constraints with the addition of C-07 (time-boxed #193 investigation from scope risk assessment).
5. **NOT In Scope section** aligns with SCOPE.md non-goals, including the explicit exclusion of infra-001 integration tests (deferred per ADR-002).
6. **2 open questions** (OQ-01, OQ-02) are carried forward appropriately from scope -- neither is resolved prematurely.

The specification adds domain model definitions and entity relationship diagrams not in scope, but these are documentation aids within the specification, not scope additions.

### Risk Strategy Review

The risk strategy is comprehensive:

1. **13 risks identified** spanning normalization edge cases (R-01), serde interactions (R-02, R-03, R-13), semantic revision correctness (R-04, R-05), data flow bugs (R-06, R-12), compilation (R-07), test coverage gaps (R-08), classification correctness (R-09), normalization consistency (R-10), and downstream consumer impact (R-11).

2. **All 8 scope risks traced**: The "Scope Risk Traceability" table maps every SR-* to corresponding R-* risks with resolution references to ADRs.

3. **Critical risks identified correctly**: R-06 (#193 data flow) and R-08 (MCP-prefixed test gap) are rated Critical. R-06 is acknowledged as an accepted gap with a diagnostic path (debug tracing + ADR-005 time-box). R-08 is addressed with mandatory new unit tests.

4. **Edge cases well-enumerated**: 7 edge cases including empty tool name, double prefix, zero entries, duplicate entry IDs across sources, and mixed bare/prefixed names in the same session.

5. **Security assessment is proportionate**: The feature processes only internal trusted data (hook events, Store records). The security section correctly identifies zero external attack surface and bounded blast radius.

6. **Coverage summary**: 45 total test scenarios across 13 risks with clear priority weighting (Critical: 9 scenarios, High: 15, Medium: 14, Low: 7).
