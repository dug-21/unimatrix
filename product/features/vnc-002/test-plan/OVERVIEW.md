# vnc-002 Test Plan Overview

## Test Strategy

Tests are organized per component, with risk-first ordering. All tests build on vnc-001's existing 72 tests (cumulative infrastructure).

## Risk-to-Test Mapping

| Risk | Priority | Component | Test File |
|------|----------|-----------|-----------|
| R-03: Combined transaction atomicity | Critical | audit-optimization | test-plan/audit-optimization.md |
| R-05: Capability check bypass | Critical | tools | test-plan/tools.md |
| R-04: Input validation bypass | Critical | validation | test-plan/validation.md |
| R-01: Scanning false positives | Critical | scanning | test-plan/scanning.md |
| R-12: Audit ID monotonicity | Critical | audit-optimization | test-plan/audit-optimization.md |
| R-16: vnc-001 regression | Critical | tools | test-plan/tools.md |
| R-02: Near-duplicate threshold | High | tools | test-plan/tools.md |
| R-10: Search filter mismatch | High | tools | test-plan/tools.md |
| R-06: EmbedNotReady handling | High | tools | test-plan/tools.md |
| R-11: i64/u64 conversion | High | validation | test-plan/validation.md |
| R-13: Default status filter | High | tools | test-plan/tools.md |
| R-14: write_in_txn isolation | High | audit-optimization | test-plan/audit-optimization.md |
| R-08: Category correctness | High | categories | test-plan/categories.md |
| R-09: Format response validity | High | response | test-plan/response.md |
| R-07: Output framing boundary | Medium | response | test-plan/response.md |
| R-15: OnceLock concurrency | Low | scanning | test-plan/scanning.md |

## Test Categories

### Unit Tests (no I/O, no database)
- validation.rs: all validation functions
- scanning.rs: pattern matching
- response.rs: response formatting
- categories.rs: allowlist operations
- error.rs: error mapping

### Integration Tests (database required)
- audit.rs: write_in_txn + combined transaction
- server.rs: insert_with_audit
- tools.rs: full tool flows (requires store + vector + embed mock)

## Test Infrastructure Approach

- Tests for validation, scanning, categories, response are pure unit tests (no fixtures needed)
- Tests for audit-optimization and tools use the existing `make_server()` pattern from server.rs tests
- `make_server()` test helper must be updated to include the new `categories` and `store` fields
- EmbedServiceHandle needs a `set_ready_for_test` method for integration tests (or mock the embed adapter)

## Expected Test Counts

| Component | New Tests | Total (with vnc-001) |
|-----------|----------|---------------------|
| error-extensions | ~6 | ~18 |
| validation | ~25 | ~25 |
| scanning | ~20 | ~20 |
| categories | ~10 | ~10 |
| response | ~15 | ~15 |
| audit-optimization | ~8 | ~18 |
| tools | ~40 | ~48 |
| **Total** | ~124 | ~154 |
