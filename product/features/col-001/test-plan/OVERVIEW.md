# col-001 Test Plan Overview

## Test Strategy

Tests are organized by component, with each component's test plan mapping directly to risks from the Risk-Based Test Strategy. Tests build on existing infrastructure (test_helpers, tempfile, server integration patterns).

## Risk-to-Test Mapping

| Risk | Component | Test Type | Scenario Count |
|------|-----------|-----------|---------------|
| R-01 (Transaction rollback) | store-pipeline | Integration | 2 |
| R-02 (Overly strict tags) | outcome-tags | Unit | 6 |
| R-03 (Non-outcome leakage) | store-pipeline | Integration | 4 |
| R-04 (StoreParams compat) | store-pipeline | Unit + Integration | 4 |
| R-05 (Index population gap) | store-pipeline | Integration | 4 |
| R-06 (Incorrect stats) | status-extension | Integration | 5 |
| R-07 (Store::open 13th table) | outcome-index | Unit | 3 |
| R-08 (Colon tags on non-outcome) | store-pipeline | Integration | 3 |
| R-09 (Error message clarity) | outcome-tags | Unit | 3 |
| R-10 (Orphan outcome awareness) | store-pipeline | Integration | 3 |
| R-11 (Status scan performance) | status-extension | Monitor | 1 |
| R-12 (Concurrent stores) | -- | Covered by redb | 0 |

## Test Infrastructure

### Existing
- `unimatrix_store::test_helpers` -- Store creation helpers
- `tempfile::TempDir` -- Ephemeral databases
- Server integration test patterns from vnc-002, vnc-003, crt-001 through crt-004

### New
- Outcome entry helper: function to build a standard outcome StoreParams for tests
- Feature cycle assertion helper: verify OUTCOME_INDEX contains expected (cycle, id) pairs

## Coverage Summary

| Component | Unit Tests | Integration Tests | Total |
|-----------|-----------|------------------|-------|
| outcome-index | 3 | 0 | 3 |
| outcome-tags | 20 | 0 | 20 |
| store-pipeline | 4 | 12 | 16 |
| status-extension | 0 | 6 | 6 |
| **Total** | **27** | **18** | **45** |
