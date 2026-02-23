# Test Plan Overview: vnc-003 v0.2 Tool Implementations

## Overall Test Strategy

All tests are written using the existing test infrastructure (cumulative pattern).
Tests use `make_server()` from `server::tests` for integration tests and
direct struct construction for unit tests.

## Risk-to-Test Mapping

| Risk ID | Priority | Test Location | Coverage |
|---------|----------|---------------|----------|
| R-01 | Critical | test-plan/tool-handlers.md (correction chain tests) | 6 scenarios |
| R-02 | Critical | test-plan/vector-index-api.md + test-plan/server-transactions.md | 6 scenarios |
| R-03 | Critical | test-plan/server-transactions.md (counter tests) | 4 scenarios |
| R-04 | High | test-plan/tool-handlers.md (deprecated entry rejection) | 3 scenarios |
| R-05 | High | test-plan/tool-handlers.md (content scanning) | 4 scenarios |
| R-06 | High | test-plan/tool-handlers.md (capability checks) | 3 scenarios |
| R-07 | High | test-plan/tool-handlers.md (token budget) | 4 scenarios |
| R-08 | High | test-plan/tool-handlers.md (embed not ready) | 3 scenarios |
| R-09 | Medium | test-plan/tool-handlers.md (category inheritance) | 3 scenarios |
| R-10 | Medium | test-plan/tool-handlers.md (status report consistency) | 3 scenarios |
| R-11 | Medium | test-plan/tool-handlers.md (deprecation idempotency) | 3 scenarios |
| R-12 | Medium | test-plan/tool-handlers.md (feature boost) | 3 scenarios |
| R-14 | Medium | test-plan/vector-index-api.md | 3 scenarios |

## Test Organization

- **Unit tests**: In the same file as the code (`#[cfg(test)] mod tests`)
- **Integration tests**: Tests that cross component boundaries use `make_server()`
- **Model-dependent tests**: Tests requiring the embedding model are marked with comments

## Existing Test Baseline

506 tests across 5 crates must continue to pass (AC-40).
Tests that assert category count == 6 must be updated to 8.
Tests that use `make_server()` must be updated for new constructor signature.
