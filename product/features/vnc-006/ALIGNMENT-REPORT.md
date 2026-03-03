# Alignment Report: vnc-006

> Reviewed: 2026-03-03
> Artifacts reviewed:
>   - product/features/vnc-006/architecture/ARCHITECTURE.md
>   - product/features/vnc-006/specification/SPECIFICATION.md
>   - product/features/vnc-006/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements vnc-006 as defined in PRODUCT-VISION.md |
| Milestone Fit | PASS | Milestone 2 (Vinculum Phase), correctly positioned after col-008 |
| Scope Gaps | PASS | All 17 acceptance criteria addressed in architecture and specification |
| Scope Additions | PASS | No scope additions beyond SCOPE.md |
| Architecture Consistency | PASS | Consistent with existing server architecture, extends without replacing |
| Risk Completeness | PASS | 12 risks, 26 scenarios; scope risks fully traced |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| — | No gaps found | All SCOPE.md acceptance criteria (AC-01 through AC-17) have corresponding FRs in specification and components in architecture |
| — | No additions found | Architecture and specification stay within SCOPE.md boundaries; all non-goals (BriefingService, module reorg, rate limiting, etc.) remain explicitly excluded |

## Variances Requiring Approval

None. All artifacts align with both the product vision and the approved scope.

## Detailed Findings

### Vision Alignment

PRODUCT-VISION.md defines vnc-006 as: "Wave 1 of server refactoring. Extract transport-agnostic service layer from duplicated MCP/UDS business logic. SearchService unifies ~400 lines of duplicated search/rank/boost logic. ConfidenceService consolidates 8 fire-and-forget blocks (~160 lines). Security Gateway (S1-S5) enforces content scanning, input validation, quarantine exclusion, and structured audit at the service level."

The architecture delivers exactly this:
- SearchService (services/search.rs) unifies the search pipeline
- ConfidenceService (services/confidence.rs) consolidates fire-and-forget blocks
- SecurityGateway (services/gateway.rs) implements S1, S3, S4, S5 (S2 deferred per vision: "S1-S5")
- AuditContext with session_id and feature_cycle for retrospective compatibility

The vision also states "Like-for-like behavior — zero functional changes to either transport path." This is explicitly AC-13, AC-14, AC-17 in the specification, and R-01 in the risk strategy.

The vision mentions "Internal caller concept for service-initiated writes (auto-outcome)." This is implemented as AuditSource::Internal (FR-05.4, ADR-002).

**Status: PASS** — Full alignment with vision definition.

### Milestone Fit

vnc-006 is Milestone 2 (Vinculum Phase). The vision roadmap shows:
```
vnc-006: Service Layer + Security Gateway  <-- NEXT (after col-008)
  vnc-007: Briefing Unification
    vnc-008: Module Reorganization
      vnc-009: Cross-Path Convergence
```

The architecture respects wave independence (Constraint 9 in SCOPE.md). No forward dependencies on vnc-007/008/009. The service layer is designed to be extended by subsequent waves without rework.

The Phase-to-Proposal mapping classifies vnc-006-009 as "Infrastructure — unified service layer, security, modularity" enabling "transport-agnostic evolution." The architecture supports this by defining transport-agnostic service interfaces.

**Status: PASS** — Correct milestone, correct dependency ordering.

### Architecture Review

**Component structure**: Four services (Search, Store, Confidence, Gateway) in a services/ module. ServiceLayer aggregate struct. All pub(crate) visibility. Consistent with the "in-crate services/ module" decision from the research.

**Integration surface**: Comprehensive table with 16 integration points, all with types and signatures. Downstream agents will not need to invent names.

**ADRs**: Four decisions (hybrid gateway, AuditSource scan bypass, insert_in_txn, batched confidence). Each follows the Context/Decision/Consequences format. All reference specific prior decisions (Unimatrix ADR #53, crt-005 pattern).

**Existing architecture respected**: UnimatrixServer gains a ServiceLayer field but is not replaced. Store, VectorIndex, EmbedServiceHandle, AdaptationService all used as-is. AuditLog reused by SecurityGateway.

**Concern check — no new direct-storage coupling**: The vision notes "no new direct-storage coupling — StoreService and Store::insert_in_txn should be the only paths to database from service layer." The architecture has SearchService using AsyncVectorStore and AsyncEntryStore (existing async wrappers, not direct redb access) and StoreService using Store::insert_in_txn (the designated path). ConfidenceService uses Store::get and Store::update_confidence (existing methods). No new direct redb table access is introduced. PASS.

**Status: PASS**

### Specification Review

**FR coverage**: 9 functional requirement groups (FR-01 through FR-09) with 38 sub-requirements. All 17 SCOPE.md acceptance criteria are represented.

**NFR coverage**: 6 non-functional requirements covering behavior preservation, latency, test count, module size, fire-and-forget, and memory.

**Domain model**: 14 entities defined with relationships diagram. Ubiquitous language established (ServiceLayer, SecurityGateway, AuditContext, etc.).

**User workflows**: 4 workflows covering agent search (MCP), hook injection (UDS), agent store (MCP), and internal write. All trace through the service layer.

**Testability**: Every FR is verifiable. AC table includes verification methods for all 17 criteria.

**Status: PASS**

### Risk Strategy Review

**Coverage**: 12 risks identified, 26 test scenarios. All 3 high-priority risks (R-01 result ordering, R-06 test breakage, R-07 UDS audit blocking) have specific mitigation strategies.

**Security risks**: Explicitly assessed for search queries, write operations, and AuditSource forgery. Aligned with the security-surface-analysis.md findings (F-25, F-27, F-28).

**Scope risk traceability**: All 9 scope risks (SR-01 through SR-09) traced to architecture risks or marked as deferred/resolved. No orphan scope risks.

**Edge cases**: 13 edge cases documented including boundary values (k=0, k=100, k=101, query at 10,000/10,001 chars), empty store, embedding unavailability, all-quarantined results.

**Failure modes**: 6 failure modes with expected behavior and recovery strategy. Consistent with the fire-and-forget contract and existing error handling patterns.

**Status: PASS**
