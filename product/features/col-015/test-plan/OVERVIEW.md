# Test Plan Overview: col-015

## Strategy

col-015 is itself a test infrastructure feature. The "tests" are the delivered test files. Validation means the tests compile, pass, and cover the identified risks.

## Risk-to-Test Mapping

| Risk | Test Coverage |
|------|--------------|
| R-01 (Kendall tau error) | T-KT-01..05: dedicated unit tests with reference values |
| R-02 (ONNX model absence) | T-E2E-skip: skip_if_no_model() pattern |
| R-03 (Golden test brittleness) | T-REG-01..03: 4-decimal precision, pairwise ordering |
| R-04 (Profile conversion loss) | T-PROF-01..02: round-trip validation |
| R-05 (Extraction store seeding) | T-EXT-01..06: documented seeding patterns |
| R-06 (TestServiceLayer divergence) | T-TSL-01: verify construction |
| R-07 (Ablation threshold too low) | T-ABL-01..06: tau threshold 0.9 |
| R-08 (Co-access non-determinism) | T-E2E-04: timestamps at extremes |

## Component Test Plan Summary

| Component | Test File(s) | Test Count | Risks |
|-----------|-------------|-----------|-------|
| shared-fixtures | test_scenarios.rs (unit tests) | 7 | R-01, R-04 |
| calibration-tests | pipeline_calibration.rs | 11 | R-03, R-07 |
| calibration-tests | pipeline_retrieval.rs | 5 | R-03 |
| regression-tests | pipeline_regression.rs | 3 | R-03 |
| extraction-tests | extraction_pipeline.rs | 6 | R-05 |
| server-e2e-tests | pipeline_e2e.rs | 7 | R-02, R-06, R-08 |
| **Total** | | **39** | **8 risks** |

## Integration Harness Plan

This feature does NOT require new Python integration tests in infra-001. Rationale:
- col-015 adds test infrastructure, not production behavior
- No MCP-visible behavior changes
- Existing smoke tests cover regression safety
- All validation is Rust-native (cargo test)

Stage 3c should run: `cargo test --workspace` to verify no regressions. Integration smoke tests via infra-001 are optional since no production code changes.
