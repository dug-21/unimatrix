# Alignment Report: vnc-003

> Reviewed: 2026-02-23
> Artifacts reviewed:
>   - product/features/vnc-003/architecture/ARCHITECTURE.md
>   - product/features/vnc-003/specification/SPECIFICATION.md
>   - product/features/vnc-003/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Implements vnc-003 as specified in the M2 roadmap |
| Milestone Fit | PASS | All 4 tools match the M2 vnc-003 description |
| Scope Gaps | PASS | All SCOPE.md items addressed in source documents |
| Scope Additions | PASS | No items in source docs beyond SCOPE.md |
| Architecture Consistency | PASS | Extends established vnc-002 patterns; 7 ADRs for key decisions |
| Risk Completeness | PASS | 14 risks identified with 47+ test scenarios; security risks assessed |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| (none) | -- | No gaps, additions, or simplifications detected |

All 45 acceptance criteria from SCOPE.md are present in the specification. The architecture covers all 4 tools, the GH #14 fix, category extension, and response formatting. The risk strategy addresses all mutation paths, capability enforcement, and integration risks.

## Variances Requiring Approval

None. All design decisions align with the product vision and approved scope.

## Detailed Findings

### Vision Alignment

The product vision (PRODUCT-VISION.md) specifies vnc-003 as:

> `context_correct` (supersede with correction chain), `context_deprecate` (mark irrelevant), `context_status` (health metrics), `context_briefing` (compiled orientation). Security: Content scanning on `context_correct` writes. Capability checks (Write for correct/deprecate, Admin for status, Read for briefing). Security metrics in `context_status`.

All of these are fully addressed:
- **context_correct**: Implemented with correction chain (supersedes/superseded_by), content scanning, Write capability check.
- **context_deprecate**: Implemented with idempotency, Write capability check.
- **context_status**: Implemented with status counts, distributions, correction chain metrics, and security metrics (trust_source distribution, attribution gaps). Admin capability check.
- **context_briefing**: Implemented with conventions/duties lookup + semantic search, token budget, graceful embed degradation. Read capability check.
- **Security metrics**: trust_source distribution and entries without attribution are included in context_status.

The vision mentions "age distribution" and "stale entries" for context_status. The SCOPE.md explicitly resolves these as out of scope (Resolved Open Questions #2: "Drop time-based staleness entirely"). This is consistent -- utilization-based staleness depends on crt-001 (usage tracking) which is M4. No variance.

The vision mentions "write frequency by agent" and "content_hash mismatches" as security metrics. The SCOPE.md explicitly excludes content_hash re-validation (Non-Goals) and write frequency tracking depends on audit log analysis which is not part of this feature. These are reasonable scoping decisions that do not require approval.

### Milestone Fit

vnc-003 is the final feature in Milestone 2 (MCP Server, Vinculum Phase). After vnc-003:
- All 8 planned MCP tools (4 v0.1 + 4 v0.2) are implemented
- Knowledge lifecycle management is complete (store, search, lookup, get, correct, deprecate, status, briefing)
- The security enforcement layer (capability checks, content scanning, audit) covers all tools

No M3 or M4 capabilities are being pulled forward. The explicit non-goals (usage tracking, confidence computation, contradiction detection) are all deferred to appropriate future milestones.

### Architecture Review

The architecture extends vnc-002's patterns consistently:
- Same execution order (identity -> capability -> validation -> category -> scanning -> logic -> format -> audit)
- Same combined transaction pattern for mutations (extended to two-entry for corrections)
- Same response formatting pipeline (extended with 4 new format functions)
- GH #14 fix uses a clean approach (ADR-001): decouple VectorIndex into allocate + insert_hnsw_only

The 7 ADRs are well-scoped and follow the established format. Each captures a specific decision with clear consequences.

The addition of `vector_index: Arc<VectorIndex>` to `UnimatrixServer` is a necessary change for the GH #14 fix. It does not expand the server's responsibility -- it narrows the interface from the generic `AsyncVectorStore` to the specific methods needed for transaction coordination.

### Specification Review

The specification covers all 45 acceptance criteria with verification methods. Functional requirements are testable. Domain models (correction chain, status report, briefing) are clearly defined. Constraints match SCOPE.md.

The non-functional requirements are reasonable: no new dependencies (NFR-03), existing tests continue to pass (NFR-01), workspace conventions enforced (NFR-02).

### Risk Strategy Review

14 risks identified across critical/high/medium/low priorities. The risk strategy correctly focuses on:
- Transaction atomicity for the two-entry correction (R-01, R-03)
- The GH #14 fix regression risk (R-02)
- Security enforcement (R-05 capability bypass, R-06 content scanning)
- Edge cases (R-04 correcting deprecated entries, R-07 budget overflow)
- Integration risks (server constructor change, category count change, audit ID continuity)

The security risks section assesses each tool's untrusted input surface and blast radius, consistent with the risk strategist's mandate.

47+ test scenarios across 14 risks provide adequate coverage. The 3 critical risks alone have 16 scenarios.
