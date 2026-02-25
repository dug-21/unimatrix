# Test Plan Overview: crt-004 Co-Access Boosting

## Test Strategy

Tests follow the risk-based test strategy (RISK-TEST-STRATEGY.md). Each component has a dedicated test plan covering the risks it owns. Tests build on existing crt-001/002/003 test infrastructure.

## Component Test Plans

| Component | Test Plan | Risk Coverage | New Tests |
|-----------|-----------|--------------|-----------|
| C1: co-access-storage | test-plan/co-access-storage.md | R-03, R-07, R-09 | ~18 |
| C2: session-dedup | test-plan/session-dedup.md | R-05 | ~6 |
| C3: co-access-recording | test-plan/co-access-recording.md | R-04, R-12 | ~7 |
| C4: co-access-boost | test-plan/co-access-boost.md | R-02, R-06 | ~14 |
| C5: confidence-extension | test-plan/confidence-extension.md | R-01, R-10 | ~14 |
| C6: tool-integration | test-plan/tool-integration.md | R-08, R-11, R-13 | ~12 |

## Test Infrastructure

### Existing Fixtures (reuse)
- `test_store()` / `open_test_store()` -- temporary redb Store
- `make_test_entry()` in confidence.rs tests -- EntryRecord builder
- Server integration test setup from crt-001/002/003

### New Fixtures Needed
- `store_co_access_pair(store, id_a, id_b, count, timestamp)` -- convenience for test setup
- `create_test_entries(store, n)` -- bulk entry creation returning entry IDs

## Risk Coverage Matrix

| Risk | Scenario Count | Component |
|------|---------------|-----------|
| R-01 | 6 | C5 |
| R-02 | 4 | C4 |
| R-03 | 4 | C1 |
| R-04 | 5 | C3 |
| R-05 | 3 | C2 |
| R-06 | 3 | C4 |
| R-07 | 5 | C1 |
| R-08 | 3 | C6 |
| R-09 | 3 | C1 |
| R-10 | 5 | C5 |
| R-11 | 4 | C6 |
| R-12 | 2 | C3 |
| R-13 | 3 | C6 |
| **Total** | **50** | |

## Test Execution Order

1. C1 tests (storage foundation -- must pass first)
2. C2 tests (dedup -- independent of C1)
3. C5 tests (confidence weights -- can run early)
4. C4 tests (boost computation -- depends on C1)
5. C3 tests (recording -- depends on C1, C2)
6. C6 tests (integration -- depends on all)

## Modified Existing Tests

The following existing tests need updated expected values:
- `confidence::tests::weight_sum_invariant` -- assert sum == 0.92 (was 1.0)
- `confidence::tests::compute_confidence_all_defaults` -- new expected value with reduced weights
- `confidence::tests::compute_confidence_all_max` -- new expected range with max 0.92
