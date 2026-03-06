# Test Plan Overview: crt-008 Continuous Self-Retraining

## Test Strategy

All tests are risk-driven, mapped from the Risk & Test Strategy document. Each risk has at least one test verifying its mitigation.

## Risk to Test Mapping

| Risk | Test(s) | Component |
|------|---------|-----------|
| R-01 (EWC gradient ordering) | T-R01-01, T-R01-02, T-R01-03 | trait-refactor |
| R-02 (Concurrent training) | T-R02-01 | training-service |
| R-03 (NaN/Inf propagation) | T-R03-01 | rollback-enhancements |
| R-04 (Trust source bypass) | T-R04-01, T-R04-02, T-R04-03 | feedback-hooks |
| R-05 (Threshold never reached) | T-R05-01 | training-service |
| R-06 (Quality regression) | T-R06-01 | rollback-enhancements |

## Test Count by Component

| Component | Unit Tests | Integration |
|-----------|-----------|-------------|
| trait-refactor | 5 (T-FR00-01, T-FR00-02, T-R01-01, T-R01-02, T-R01-03) | 0 |
| training-types | 10 (T-FR01-01, T-FR02-01 through T-FR02-09) | 0 |
| training-service | 9 (T-FR04-01 through T-FR04-03, T-FR05-01 through T-FR05-03, T-R02-01, T-FR-CONFIG-01, T-R05-01) | 0 |
| rollback-enhancements | 2 (T-R03-01, T-R06-01) | 0 |
| feedback-hooks | 3 (T-R04-01, T-R04-02, T-R04-03) | 0 |
| ground-truth-backfill | 2 (T-FR10-01, T-FR10-02) | 0 |
| integration-test | 0 | 1 (T-INT-01) |
| **Total** | **31** | **1** |

## Integration Harness Plan

### Existing Suites

No product/test/infra-001 suites apply directly to unimatrix-learn. The crate is pure Rust with no MCP server dependency for unit testing.

### New Integration Tests

- `crates/unimatrix-learn/tests/retraining_e2e.rs`: T-INT-01 end-to-end test
  - Requires: tokio runtime (for spawn_blocking), tempdir, full TrainingService
  - Exercises: signal -> label -> reservoir -> threshold -> train -> shadow save -> verify different predictions

### Test Execution Order

1. Unit tests: `cargo test -p unimatrix-learn`
2. Full workspace: `cargo test --workspace`
3. Integration: runs as part of `cargo test -p unimatrix-learn` (tests/ directory)

## Backward Compatibility Verification

All existing crt-007 tests must pass unchanged after Wave 0. The trait refactor adds new methods but `train_step` becomes a default impl that delegates to them, preserving exact behavior.

## Test Infrastructure

- All tests use `tempfile::TempDir` for model storage (no persistent state)
- Training service tests use `LearnConfig::default()` with overrides
- No new test dependencies required (tempfile, tokio already in workspace)
