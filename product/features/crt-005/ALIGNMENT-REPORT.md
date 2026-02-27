# Alignment Report: crt-005 (Coherence Gate)

> Reviewed: 2026-02-27
> Artifacts reviewed:
>   - product/features/crt-005/architecture/ARCHITECTURE.md
>   - product/features/crt-005/architecture/ADR-001-f64-scoring-boundary.md
>   - product/features/crt-005/architecture/ADR-002-maintenance-opt-out.md
>   - product/features/crt-005/architecture/ADR-003-lambda-dimension-weights.md
>   - product/features/crt-005/architecture/ADR-004-graph-compaction-atomicity.md
>   - product/features/crt-005/specification/SPECIFICATION.md
>   - product/features/crt-005/SCOPE.md
>   - product/features/crt-005/SCOPE-RISK-ASSESSMENT.md
>   - (RISK-TEST-STRATEGY.md does not yet exist)
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Lambda metric, inline maintenance, and f64 upgrade all map directly to the vision's crt-005 description |
| Milestone Fit | PASS | Correctly sequenced after col-001, before col-002; stays within M4/M5 boundary |
| Scope Gaps | WARN | RISK-TEST-STRATEGY.md is missing; one SCOPE open question resolved without explicit SCOPE update |
| Scope Additions | PASS | No capabilities beyond what SCOPE.md requests |
| Architecture Consistency | PASS | Follows established patterns: no background threads, object-safe traits, build-new-then-swap, inline maintenance |
| Risk Completeness | WARN | RISK-TEST-STRATEGY.md absent; SCOPE-RISK-ASSESSMENT.md covers design risks but not test strategy |
| f64 Scoring Boundary | PASS | Clear rationale for scoring-pipeline-only upgrade; embeddings correctly remain f32 |
| Lambda Serves col-002 | PASS | Lambda provides the reliable quality signal col-002 needs; ordering is explicit |
| Self-Learning Engine Alignment | PASS | Inline maintenance, confidence refresh, and graph compaction directly serve the "self-learning context engine" vision |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | RISK-TEST-STRATEGY.md | SCOPE.md lists this as a required Phase 2 artifact per feature directory structure conventions, but it does not yet exist |
| Gap | SCOPE open question #3 resolution | SCOPE asks "What are appropriate dimension weights?" -- ADR-003 resolves this with unequal weights and re-normalization, but SCOPE.md is not updated to mark the question resolved |
| Gap | SCOPE open question #2 resolution | SCOPE asks "Should graph compaction re-embed from content or read raw embeddings?" -- ADR-004 resolves this (re-embed from content), but SCOPE.md is not updated |
| Gap | SCOPE open question #4 resolution | SCOPE asks about f64 test update strategy -- Architecture C2 resolves it (Tier 1 first), but SCOPE.md is not updated |
| Simplification | Embedding consistency defaulting to 1.0 in StatusReport | SCOPE AC-05 says "defaults to 1.0 when checks are not performed." Architecture ADR-003 resolves the SR-08 concern by excluding unavailable dimensions from lambda computation while still displaying 1.0 in StatusReport for the field value. This is an acceptable simplification with documented rationale. |
| Simplification | confidence_freshness_score returns tuple | SCOPE defines the dimension score as a ratio. Architecture C4 returns `(f64, u64)` (score + stale count) from the function. Minor interface enrichment to avoid a second scan. Acceptable. |

## Variances Requiring Approval

None. All design decisions align with the product vision and approved scope. The gaps identified above are process artifacts (missing RISK-TEST-STRATEGY.md, unresolved open questions in SCOPE.md) rather than design variances.

## Detailed Findings

### Vision Alignment

The product vision describes crt-005 in full detail (PRODUCT-VISION.md, Milestone 4 table):

> **Coherence Gate | crt-005 |** Unified structural health metric (lambda) monitoring knowledge base coherence across four dimensions and gating autonomous self-maintenance. [followed by detailed dimension descriptions]

The source documents deliver precisely what the vision describes:

1. **Lambda metric**: SPECIFICATION FR-100 defines the composite lambda as a weighted average of four dimensions. ARCHITECTURE C4 implements it with pure dimension score functions and a `compute_lambda` function. This matches the vision's "unified structural health metric (lambda) monitoring knowledge base coherence across four dimensions."

2. **Four dimensions**: The vision lists confidence staleness, HNSW graph degradation, embedding consistency, and contradiction density. All four are implemented as dimension score functions in C4 (confidence_freshness_score, graph_quality_score, embedding_consistency_score, contradiction_density_score). Each produces a score in [0.0, 1.0].

3. **Lazy confidence refresh**: The vision states "entries whose stored confidence age exceeds a staleness threshold (configurable, default 24h) are recomputed." SPECIFICATION FR-200/FR-201 and ARCHITECTURE C5 implement exactly this, using `max(updated_at, last_accessed_at)` as the staleness proxy with a default 24h threshold.

4. **Graph compaction**: The vision states "when stale ratio exceeds a threshold (default 10%), rebuild affected graph regions." SPECIFICATION FR-300/FR-301 and ARCHITECTURE C3/C8 implement this with a build-new-then-swap design. The vision says "rebuild affected graph regions" but the implementation does a full rebuild because hnsw_rs does not support partial deletion. This is an engineering constraint, not a variance.

5. **Inline maintenance**: The vision states "Maintenance operations execute inline during `context_status` calls -- no background threads, no timers, no new async patterns." SPECIFICATION NFR-07 and ARCHITECTURE explicitly enforce this constraint. PASS.

6. **Coherence field in StatusReport**: The vision states "Composite lambda score (0.0-1.0) combining all four dimensions, exposed as `coherence` field in `StatusReport`." SPECIFICATION FR-510 and ARCHITECTURE C6 deliver this.

7. **Maintenance gating**: The vision states "When lambda drops below configurable threshold (default 0.8), `context_status` response includes maintenance recommendations." SPECIFICATION FR-400/FR-401 implement this. PASS.

The vision's "mathematical foundation" note references "structural de-alignment via irrational constants (Weyl equidistribution theorem)" from ass-012 research. The SCOPE explicitly defers pi-based calibration as a non-goal ("crt-005 upgrades the precision; future work may adopt pi-derived constants"). This is correct: the f64 upgrade enables the possibility without committing to it now.

**Assessment: PASS.** The source documents deliver exactly what the vision describes for crt-005.

### Milestone Fit

The product vision's dependency graph shows:

```
crt-001/002/003/004 (COMPLETE)
  |
  +-- col-001: Outcome Tracking (COMPLETE)
  +-- crt-005: Coherence Gate (ships after col-001)
       +-- col-002: Retrospective Pipeline (ships after crt-005)
```

SCOPE.md states: "Depends on: col-001 (Outcome Tracking) -- complete; crt-001 through crt-004 -- all complete" and "Blocks: col-002 (Retrospective Pipeline) -- needs reliable knowledge quality signals."

The ARCHITECTURE states: "crt-005 is the capstone of the Cortical (M4) phase and the direct prerequisite for col-002."

This exactly matches the vision's ordering. The feature does not pull in col-002/003/004 capabilities. It does not reach into M6 (UI), M7 (multi-project), or M8 (thin-shell).

The vision states M4's goal: "Turn passive knowledge accumulation into active learning -- the bridge from Proposal A to C." crt-005 is the final M4 feature. The lambda metric with inline maintenance completes the "active learning" capability: the system now monitors its own health and self-corrects during routine operations.

**Assessment: PASS.**

### f64 Scoring Upgrade Alignment

The vision describes the f64 upgrade as part of crt-005's SCOPE:

> "crt-005 introduces lazy confidence refresh" and "Dimension 5 -- Scoring precision ceiling... The f32 ceiling also constrains future scale"

The vision does not prescribe f64 specifically, but it describes the problem (f32 precision ceiling, JSON artifacts, constraining future scale) and places it squarely in crt-005's scope. The source documents deliver the upgrade as ADR-001 (f64 scoring boundary) with a clear rationale.

ADR-001's decision to keep contradiction detection constants at f32 is well-reasoned. The contradiction module compares against HNSW similarity scores (inherently f32 from hnsw_rs), so promoting those to f64 adds casts without precision benefit. The scoring pipeline (confidence, re-ranking, co-access boost) is the domain where f64 precision matters.

The architecture's exhaustive f32-to-f64 inventory (C2) addresses SR-02 directly, listing every constant, function signature, and type change across all five crates. This is thorough.

The schema migration v2->v3 is designed as atomic (single redb write transaction) per SR-01 and ADR-004. The migration uses an intermediate V2EntryRecord struct to avoid reading f32 bytes as f64 -- correct per the SR-01 assumption about bincode serialization.

**Assessment: PASS.** The f64 upgrade aligns with the vision's quality and scale goals.

### Lambda Metric Serving col-002

The vision states col-002 (Retrospective Pipeline): "aggregates outcomes across features, detects patterns, generates process-proposal entries with evidence." The SCOPE ordering note states: "Should complete before col-002 -- col-002 draws conclusions from knowledge quality signals; stale confidence and degraded HNSW graphs produce misleading retrospective insights."

The architecture delivers this: lambda and its four dimensions provide col-002 with a reliable quality signal. When lambda is high, col-002 can trust that the data it analyzes (confidence scores, search results) is structurally sound. When lambda is low, col-002 knows the data may be degraded.

The maintenance opt-out (ADR-002) also serves col-002: a retrospective agent can call `context_status(maintenance: false)` to get the coherence snapshot without triggering writes, then separately call with `maintenance: true` to fix issues before running the retrospective.

**Assessment: PASS.** Lambda directly serves col-002's needs as a data quality gatekeeper.

### Self-Learning Engine Alignment

The product vision's core identity is "self-learning context engine." The vision's M4 milestone goal is "Knowledge quality improves automatically."

crt-005 delivers three forms of automatic quality improvement:

1. **Confidence refresh**: Stale entries are recomputed during routine `context_status` calls. No human intervention required.
2. **Graph compaction**: Stale HNSW nodes are eliminated when their ratio exceeds a threshold. Self-healing.
3. **Maintenance recommendations**: When lambda drops below threshold, actionable recommendations guide the human or agent. This is not automatic action (non-goal: no automatic quarantine) but it is automatic detection with guided remediation.

The maintenance opt-out parameter (ADR-002) correctly preserves the "self-healing by default" behavior (`maintenance` defaults to true) while giving diagnostic-only callers an escape hatch. This aligns with the vision's "human visibility and control" principle -- humans can observe without triggering changes.

**Assessment: PASS.**

### Architecture Review

The architecture follows all established design principles:

1. **No background threads**: NFR-07, ARCHITECTURE explicitly states all operations inline during `context_status`. No new async spawn patterns. Consistent with the server's single-threaded stdio architecture.

2. **Object-safe traits**: C3 adds `compact(&self, Vec<(u64, Vec<f32>)>) -> Result<(), CoreError>` to VectorStore. This is object-safe: `&self`, no generics, concrete return type. Consistent with existing VectorStore methods.

3. **Inline maintenance**: Confidence refresh and graph compaction are piggybacked on `context_status` calls, following the pattern established by crt-004's co-access staleness cleanup.

4. **Build-new-then-swap**: ADR-004 mandates that graph compaction builds a new HNSW index before replacing the old one. The old index remains functional during the build phase. The revised sequence (VECTOR_MAP first, then in-memory swap) eliminates the inconsistency window identified in the ADR's analysis.

5. **Schema migration atomicity**: Single redb write transaction, all-or-nothing. Follows the established migration pattern from nxs-004 and crt-001.

6. **Pure dimension score functions**: C4 specifies that all dimension score functions are pure (deterministic, no side effects, no I/O). This is testable and consistent with the project's preference for pure computation separated from I/O.

7. **Separation of concerns**: VectorIndex receives pre-computed embeddings (does not depend on embed service). The server crate orchestrates embed service access. Consistent with the existing separation between vector and embed crates.

8. **Two-tier delivery**: The architecture defines Tier 1 (read-only: f64 upgrade + lambda computation) and Tier 2 (read-write: confidence refresh + graph compaction). This addresses SR-06 (scope boundary risk) and allows partial delivery in a coherent state.

**Assessment: PASS.**

### Specification Review

The specification covers all 32 acceptance criteria from SCOPE.md. The AC traceability table (section "Acceptance Criteria Traceability") maps each AC to functional requirements and verification methods.

Functional requirements are organized into six groups:
- FR-1xx: Coherence metric computation (6 requirements)
- FR-2xx: Confidence refresh (6 requirements, including maintenance opt-out)
- FR-3xx: Graph compaction (8 requirements, including embed service gating)
- FR-4xx: Maintenance recommendations (4 requirements)
- FR-5xx: f64 scoring upgrade (11 requirements, including embedding exclusion)
- FR-6xx: Response formatting (4 requirements)

Non-functional requirements cover latency bounds, migration safety, backward compatibility, safety constraints, object safety, no-background-threads, and test continuity.

Error handling is thorough: five error cases with specific handling behavior documented (embed service unavailable, mid-compaction failure, schema migration failure, confidence refresh write failure, division by zero).

Open questions for the architect (dimension weights, compaction embedding source, batch refresh cap, f64 test strategy) are all resolved in the architecture via ADR-003, ADR-004, C4, and C2 respectively. SCOPE.md open questions should be marked resolved.

**Assessment: PASS.**

### Scope Additions Check

No capabilities are introduced in the source documents beyond what SCOPE.md requests:

- The `maintenance` parameter on `context_status` was recommended by SR-07 in the SCOPE-RISK-ASSESSMENT.md. It is a risk mitigation, not a scope addition.
- ADR-003's unequal dimension weights and re-normalization answer SCOPE open question #3. Not a scope addition.
- The two-tier delivery split is a delivery strategy, not new functionality.
- The `CoherenceWeights` struct is an implementation detail of the configurable weights that SCOPE AC-07 requests.

**Assessment: PASS.** No scope additions.

### Scope Gaps Check

1. **RISK-TEST-STRATEGY.md is missing.** Per the feature directory structure conventions (CLAUDE.md), this is a required Phase 2 artifact. The SCOPE-RISK-ASSESSMENT.md (Phase 1b) exists and identifies 11 risks, but the full test strategy with test scenarios mapped to risks has not been produced. This is a process gap, not a design gap. The feature design is complete and can proceed, but the RISK-TEST-STRATEGY.md should be produced before implementation begins.

2. **SCOPE open questions not updated.** The architecture resolves all four open questions (dimension weights, compaction source, batch cap, f64 test strategy) but SCOPE.md still lists them as open. This is a documentation hygiene issue, not a design gap.

3. **No confidence refresh during context_search.** SCOPE explicitly lists this as a non-goal ("The initial implementation limits lazy refresh to `context_status` only"). The architecture respects this non-goal. Not a gap.

**Assessment: WARN.** RISK-TEST-STRATEGY.md is absent. Open questions in SCOPE.md should be marked resolved.

### Risk Strategy Review

The SCOPE-RISK-ASSESSMENT.md identifies 11 risks across technology (SR-01 through SR-05), scope boundary (SR-06 through SR-08), and integration (SR-09 through SR-11). All risks have severity, likelihood, and recommendations.

The architecture addresses all risk recommendations:

| Risk | Recommendation | Architecture Response |
|------|---------------|----------------------|
| SR-01 (migration atomicity) | Atomic all-or-nothing | C1: single redb write transaction |
| SR-02 (f32 sweep completeness) | Exhaustive inventory | C2: full table of every f32 site |
| SR-03 (compaction failure) | Build-new-then-swap | ADR-004: detailed failure handling |
| SR-04 (embed service gating) | Gate on readiness | C8: embed service availability check |
| SR-05 (threshold tuning) | Named constants | C4: all thresholds are named constants |
| SR-06 (partial delivery) | Define minimal viable subset | Tier 1 / Tier 2 delivery split |
| SR-07 (behavioral contract change) | Opt-out parameter | ADR-002: maintenance parameter |
| SR-08 (unavailable dimension inflation) | Exclude or document | ADR-003: exclude and re-normalize |
| SR-09 (test blast radius) | Estimate and plan | C2: 60-80 tests, mechanical updates |
| SR-10 (trait object safety) | Verify | C3: object-safe compact method |
| SR-11 (write contention) | Accept at current scale | Documented as known limitation |

However, the RISK-TEST-STRATEGY.md (with test scenarios, risk coverage matrix, and test plan) is absent. The SCOPE-RISK-ASSESSMENT covers design risks, but not the test strategy itself.

**Assessment: WARN.** Risk identification is thorough; test strategy document is absent.
