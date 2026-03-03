# Test Plan Overview: vnc-006 — Service Layer + Security Gateway

## Test Strategy

All testing is rooted in the Risk-Based Test Strategy (RISK-TEST-STRATEGY.md). Each component test plan maps directly to risk scenarios and acceptance criteria.

## Risk Mapping

| Risk | Priority | Component | Test Plan Section |
|------|----------|-----------|-------------------|
| R-01: Search result ordering divergence | High | search | TS-01 through TS-03 |
| R-02: Atomic transaction failure | High | store-ops, insert-in-txn | TS-04, TS-05 |
| R-03: S1 false positives on search queries | Med | gateway | TS-06 through TS-08 |
| R-04: Internal bypass abuse | Med | gateway, service-layer | TS-09, TS-10 |
| R-05: Confidence batching timing | Low | confidence | TS-11 through TS-13 |
| R-06: Existing test breakage | High | transport-rewiring | TS-14 |
| R-07: Audit blocking fire-and-forget | High | gateway | TS-15 |
| R-08: insert_in_txn divergence | Med | insert-in-txn | TS-16, TS-17 |
| R-11: ServiceError context loss | Med | service-layer | TS-18 |

## AC Mapping

| AC | Test ID | Component |
|----|---------|-----------|
| AC-01 | TS-01 | search |
| AC-02 | TS-19 | transport-rewiring |
| AC-03 | TS-20 | transport-rewiring |
| AC-04 | TS-21 | transport-rewiring |
| AC-05 | TS-04 | store-ops |
| AC-06 | TS-06 | gateway |
| AC-07 | TS-07 | gateway |
| AC-08 | TS-08 | gateway |
| AC-09 | TS-22 | search |
| AC-10 | TS-23 | gateway |
| AC-11 | TS-24 | service-layer |
| AC-12 | TS-09 | service-layer |
| AC-13 | TS-25 | transport-rewiring |
| AC-14 | TS-26 | transport-rewiring |
| AC-15 | TS-14 | transport-rewiring |
| AC-16 | TS-27 | transport-rewiring |
| AC-17 | TS-14 | transport-rewiring |

## Integration Harness Plan

### Existing Test Infrastructure

The server crate has ~680 existing tests using:
1. **TestHarness** (in server.rs tests) -- creates a full UnimatrixServer with tempdir Store
2. **Direct Store construction** -- tempdir-based for unit tests
3. **tokio::test** -- async test runtime for all async tests

### Integration Test Suites from product/test/infra-001/

The following existing suites apply to vnc-006:
- Smoke tests: basic server startup and tool invocation
- Search integration tests: query-response verification
- Store integration tests: write-read consistency

### New Integration Tests Needed

| Test Suite | Purpose | Location |
|------------|---------|----------|
| Service comparison | Verify SearchService matches existing inline path | server tests |
| Atomic write | Verify entry+audit atomicity via StoreService | server tests |
| Gateway integration | End-to-end security gate verification | server tests |
| Transport delegation | Verify tools.rs/uds_listener.rs delegate correctly | server tests |

### Test Execution Order

1. Unit tests: `cargo test --workspace` (all existing + new service tests)
2. Integration smoke tests: `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`
3. Full integration: `cargo test --package unimatrix-server`
4. Test count verification: `cargo test --package unimatrix-server -- --list 2>/dev/null | wc -l` >= 680

## Coverage Summary

| Component | Unit Tests | Integration Tests | Risk Coverage |
|-----------|-----------|-------------------|---------------|
| gateway | 12 | 2 | R-03, R-04, R-07, R-09 |
| search | 4 | 3 | R-01 |
| store-ops | 3 | 3 | R-02 |
| confidence | 4 | 1 | R-05 |
| service-layer | 5 | 0 | R-04, R-11 |
| insert-in-txn | 3 | 2 | R-08 |
| transport-rewiring | 0 | 6 | R-06 |
| **Total** | **31** | **17** | **All 12 risks** |
