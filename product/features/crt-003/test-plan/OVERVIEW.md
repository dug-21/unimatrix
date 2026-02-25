# Test Plan Overview: crt-003

## Test Strategy

Tests follow the cumulative test infrastructure established by prior features (nxs-001 through crt-002). The server crate uses `make_server()` and `insert_test_entry()` helpers from existing test infrastructure.

## Component Test Distribution

| Component | Unit Tests | Integration Tests | Risk Coverage |
|-----------|-----------|-------------------|---------------|
| C1: status-extension | 7 | 0 | R-01 |
| C2: retrieval-filtering | 0 | 6 | R-02, R-12 |
| C3: quarantine-tool | 0 | 10 | R-03, R-08, R-09, R-10 |
| C4: contradiction-detection | 6 | 3 | R-04, R-05, R-06, R-07, R-11 |
| C5: status-report-extension | 2 | 4 | R-03 |
| **Total** | **15** | **23** | **12 risks** |

## Test Files

| Component | Test Location | Type |
|-----------|--------------|------|
| C1: status-extension | `crates/unimatrix-store/src/schema.rs` (inline `#[cfg(test)]`) | Unit |
| C2: retrieval-filtering | `crates/unimatrix-server/src/tools.rs` (inline `#[cfg(test)]`) | Integration |
| C3: quarantine-tool | `crates/unimatrix-server/src/tools.rs` (inline `#[cfg(test)]`) | Integration |
| C4: contradiction-detection | `crates/unimatrix-server/src/contradiction.rs` (inline `#[cfg(test)]`) | Unit + Integration |
| C5: status-report-extension | `crates/unimatrix-server/src/tools.rs` (inline `#[cfg(test)]`) | Integration |

## Test Dependencies

- C1 tests must pass before C2/C3/C5 tests can compile (Status::Quarantined variant required)
- C4 tests are independent (new module, does not depend on C1 for compilation)
- C2 tests require C1 (quarantine entries to test filtering)
- C3 tests require C1 + C2 (quarantine/restore + verify filtering effects)
- C5 tests require C1 + C4 (status report includes quarantine counts + contradiction data)

## Risk-to-Test Mapping

| Risk | Test(s) | Component |
|------|---------|-----------|
| R-01 | C1 unit tests (7 match site verifications) | C1 |
| R-02 | C2 integration tests (search/lookup/briefing/get exclusion) | C2 |
| R-03 | C3 counter verification tests + C5 counter display tests | C3, C5 |
| R-04 | C4 false positive unit tests (complementary/agreement entries) | C4 |
| R-05 | C4 true positive unit tests (negation/directive/sentiment) | C4 |
| R-06 | C4 performance integration test (empty/single/100 entries) | C4 |
| R-07 | C4 embedding consistency integration test | C4 |
| R-08 | C3 confidence drift integration test | C3 |
| R-09 | C3 idempotency integration test | C3 |
| R-10 | C3 STATUS_INDEX verification tests | C3 |
| R-11 | C4 dedup unit test | C4 |
| R-12 | C2 context_correct rejection test | C2 |

## Existing Test Regression

All 371 existing tests must continue to pass. The only test that requires modification is `test_status_try_from_invalid` in `schema.rs`, which currently asserts `Status::try_from(3u8)` is `Err`. This must change to assert `Ok(Status::Quarantined)`, and the invalid test should use `4u8`.
