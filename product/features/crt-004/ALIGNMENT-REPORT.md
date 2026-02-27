# Alignment Report: crt-004

> Reviewed: 2026-02-25
> Artifacts reviewed:
>   - product/features/crt-004/architecture/ARCHITECTURE.md
>   - product/features/crt-004/specification/SPECIFICATION.md
>   - product/features/crt-004/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements M4 crt-004 as specified in vision |
| Milestone Fit | PASS | Stays within M4 scope; no M5+ capabilities |
| Scope Gaps | PASS | All 22 ACs from SCOPE covered in specification and architecture |
| Scope Additions | WARN | ADR-003 split integration pattern is an architectural addition beyond SCOPE's "seventh confidence factor" framing |
| Architecture Consistency | PASS | Follows established patterns from crt-001/002/003 |
| Risk Completeness | PASS | 13 risks, 42 scenarios, all scope risks traced |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | Confidence factor integration | SCOPE.md states "Add a seventh factor to the confidence composite formula." Architecture splits this into a stored 6-factor formula (0.92) + query-time co-access affinity (0.08). The effective result is the same seven-factor composite, but the integration path differs from a literal seventh factor in compute_confidence. Rationale: preserves function pointer signature (SR-04). Acceptable simplification. |
| Addition | Store::record_co_access_pairs | Architecture introduces a second write method (pre-computed pairs) in addition to record_co_access (generates pairs internally). Not in SCOPE. Justified by the dedup-then-write pattern in server.rs. |
| Addition | Graceful degradation | Specification defines failure modes (CO_ACCESS read fails -> no boost, returns pre-crt-004 behavior). Not explicit in SCOPE but consistent with existing patterns (embed service degradation in crt-003). |

## Variances Requiring Approval

None. All additions are justified by architectural necessity or consistency with existing patterns.

## Detailed Findings

### Vision Alignment

The product vision states for crt-004: "Track entries frequently retrieved together. Boost co-accessed entries in search results. Lightweight version of PageRank on access graph -- 80% of value, 20% of complexity."

The architecture delivers exactly this:
- **Tracks entries frequently retrieved together**: CO_ACCESS table with pairwise counts (C1)
- **Boosts co-accessed entries in search results**: Post-rerank boost step (C4, C6)
- **Lightweight version of PageRank**: Pairwise frequency tracking, no graph materialization, no iterative convergence. The "80/20" claim is honored -- the architecture explicitly lists "no graph algorithms" as a non-goal.

The broader M4 goal ("Knowledge quality improves automatically. Unused entries fade. Helpful entries strengthen. Contradictions surface.") is advanced by crt-004's co-access signal: entries that are frequently useful together get stronger search presence.

The auditable knowledge lifecycle value proposition is maintained: co-access data is tracked in a redb table (auditable), pairwise (transparent), with configurable staleness (correctable). No opaque ML models.

### Milestone Fit

crt-004 is the fourth and final feature in M4 (Learning & Drift). It depends on:
- crt-001 (Usage Tracking): Provides the usage pipeline (`record_usage_for_entries`) that co-access recording extends
- crt-002 (Confidence Evolution): Provides the confidence formula that co-access factor extends
- crt-003 (Contradiction Detection): Provides the quarantine status that co-access filtering respects

No M5+ capabilities are introduced. Process proposals, retrospective analysis, and feature lifecycle management are correctly excluded. The co-access data could feed future M5 features (e.g., "entries frequently used together suggest a missing composite entry") but the architecture does not implement this.

### Architecture Review

The architecture follows established patterns:
- **New redb table**: CO_ACCESS follows the same (key, bincode bytes) pattern as ENTRIES, AUDIT_LOG, AGENT_REGISTRY
- **New server module**: coaccess.rs follows the pattern of contradiction.rs (crt-003) and confidence.rs (crt-002)
- **Fire-and-forget recording**: Same spawn_blocking pattern as usage recording (crt-001)
- **Session dedup extension**: Same UsageDedup struct with added field (crt-001 pattern)
- **StatusReport extension**: Same pattern as crt-003's quarantine and contradiction fields

The ADR-001 decision (full table scan for partner lookup) is pragmatic. At expected scale (1K-10K pairs), the scan is fast. The interface hides the implementation, allowing future optimization.

The ADR-002 decision (log-transform + cap for boost) is consistent with crt-002's usage_score pattern. The math is identical; only the constants differ.

The ADR-003 decision (split integration for confidence) is the cleanest resolution of SR-04. It avoids breaking the function pointer signature while delivering the effective seven-factor behavior.

### Specification Review

All 22 acceptance criteria from SCOPE.md are present in the specification's AC table with verification methods. The specification adds:
- Explicit functional requirements (FR-01 through FR-08) that break ACs into testable units
- Non-functional requirements with measurable targets (NFR-01 through NFR-06)
- Domain model definitions for key terms
- User workflow descriptions

The domain models section correctly defines CoAccessRecord, Co-Access Partner, Anchor Entry, Co-Access Boost, Effective Confidence, and Staleness -- all terms used across the source documents.

### Risk Strategy Review

13 risks identified with 42 test scenarios. All 9 scope risks (SR-01 through SR-09) are traced in the Scope Risk Traceability table with clear resolutions. Critical risks (R-01 weight regression, R-02 feedback loop) have comprehensive test scenarios.

The failure modes table covers all graceful degradation paths. Security assessment identifies co-access count inflation, data exfiltration via status, and DoS via large result sets -- all with mitigations.

One observation: R-03 (full table scan latency) has an NFR-02 target of < 20ms at 10K pairs. This is based on ADR-001's analysis but may be challenging to test reliably in CI (timing-dependent tests are flaky). The risk strategy correctly notes "performance assertions at medium scale if test infrastructure supports timing."
