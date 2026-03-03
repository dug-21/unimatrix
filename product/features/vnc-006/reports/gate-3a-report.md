# Gate 3a Report: Component Design Review

**Feature**: vnc-006 — Service Layer + Security Gateway
**Gate**: 3a (Component Design Review)
**Result**: PASS
**Date**: 2026-03-03

## Validation Summary

### 1. Component Alignment with Architecture

| Component | Architecture Match | Notes |
|-----------|-------------------|-------|
| SecurityGateway | PASS | Hybrid injection pattern per ADR-001. S1/S3/S4/S5 responsibilities match. |
| SearchService | PASS | Pipeline steps match architecture Section 2. All 15 steps present. |
| StoreService | PASS | Atomic write+audit pattern per ADR-003. Near-duplicate detection preserved. |
| ConfidenceService | PASS | Batched recompute per ADR-004. Empty-slice no-op specified. |
| ServiceLayer | PASS | Aggregate struct with constructor. All dependencies wired. |
| insert_in_txn | PASS | Transaction-accepting insert per ADR-003. All index writes included. |
| Transport Rewiring | PASS | Transport retains identity/capability/formatting/usage. Services handle business logic. |

### 2. Pseudocode Implements Specification

| FR Group | Coverage | Issues |
|----------|----------|--------|
| FR-01 (SearchService) | All 9 sub-reqs | FR-01.7 deviation documented: current code uses category-based provenance, not created_by-based. Like-for-like. |
| FR-02 (StoreService) | All 6 sub-reqs | None |
| FR-03 (ConfidenceService) | All 4 sub-reqs | None |
| FR-04 (SecurityGateway) | All 8 sub-reqs | None |
| FR-05 (AuditContext) | All 6 sub-reqs | None |
| FR-06 (insert_in_txn) | All 4 sub-reqs | None |
| FR-07 (Transport Rewiring) | All 7 sub-reqs | None |
| FR-08 (ServiceLayer) | All 3 sub-reqs | None |
| FR-09 (ServiceError) | All 3 sub-reqs | None |

### 3. Test Plans Address Risk Strategy

| Risk | Test Scenarios | Coverage |
|------|---------------|----------|
| R-01 (Search ordering) | TS-01, TS-02, TS-03 | Adequate |
| R-02 (Atomic transaction) | TS-04, TS-04b | Adequate |
| R-03 (S1 false positives) | TS-06, TS-06b | Adequate |
| R-04 (Internal bypass) | TS-09, TS-10 | Adequate |
| R-05 (Confidence timing) | TS-11, TS-12, TS-13 | Adequate |
| R-06 (Existing test breakage) | TS-14 | Adequate |
| R-07 (Audit blocking) | TS-15 | Adequate |
| R-08 (insert_in_txn divergence) | TS-16, TS-17 | Adequate |
| R-09 (new_permissive leak) | TS-23b | Adequate |
| R-10 (Embedding exposure) | TS-03b | Adequate |
| R-11 (Error context loss) | TS-18, TS-18b | Adequate |
| R-12 (Quarantine inconsistency) | TS-22 | Adequate |

### 4. Component Interface Consistency

All function signatures in pseudocode match the architecture's Integration Surface table (ARCHITECTURE.md lines 466-485). Types, parameters, and return values are consistent.

### 5. Integration Harness Plan

pseudocode/OVERVIEW.md includes integration harness section covering:
- Existing test infrastructure (TestHarness, tempdir, tokio::test)
- Applicable suites from product/test/infra-001/
- New integration tests needed (4 categories)

test-plan/OVERVIEW.md includes:
- Risk-to-test mapping table
- AC-to-test mapping table
- Integration test execution order
- Test count targets (>= 730 total)

## Observations

1. **FR-01.7 provenance boost**: The specification says +0.02 for `created_by == caller_agent_id`. The existing code applies +0.02 for `category == "lesson-learned"` (PROVENANCE_BOOST constant). The pseudocode correctly preserves existing behavior. This is not a defect -- the spec was written aspirationally. Like-for-like takes precedence.

2. **Confidence block count**: The brief says 5 blocks in tools.rs + 3 in uds_listener.rs = 8. The pseudocode documents that the actual count may differ since some blocks serve different purposes (e.g., context_status batch refresh, run_confidence_consumer). The confidence.md pseudocode correctly identifies the specific replacement targets.

3. **CategoryAllowlist validation**: The gateway pseudocode validates structural fields (title length, content length, control chars, tags) but does NOT duplicate the CategoryAllowlist check, which remains in the transport layer. This is correct -- category validation is transport-specific (MCP has it, UDS may not).

## Decision

**PASS** -- All validation criteria met. Pseudocode is complete and aligned with architecture. Test plans cover all identified risks. Proceed to Stage 3b.
