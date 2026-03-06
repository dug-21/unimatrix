# Test Plan Overview: crt-007 Neural Extraction Pipeline

## Test Strategy

Tests are organized per-component and mapped to risks from RISK-TEST-STRATEGY.md.
Unit tests live alongside implementation in each crate. Integration tests use
the infra-001 harness for system-level validation.

## Risk Coverage Mapping

| Risk | Priority | Component | Test Count | Coverage |
|------|----------|-----------|------------|----------|
| R-01 | High | learn-crate | 5 | Full adapt test suite + regression comparison |
| R-02 | High | learn-crate | 3 | Known-value EwcState tests with flat interface |
| R-03 | Med | classifier-scorer | 4 | Numerical gradient checks per layer type |
| R-04 | Med | classifier-scorer | 4 | Baseline output smoke tests |
| R-05 | High | shadow, registry | 4 | Promotion criteria + rollback flow |
| R-06 | Med | registry, model-trait | 3 | Save/load roundtrip + corrupt handling |
| R-07 | Low | classifier-scorer | 2 | Timing assertions (release mode) |
| R-08 | Low | shadow | 1 | Batch write performance |
| R-09 | Low | classifier-scorer | 1 | Non-degenerate output with zero-padded input |
| R-10 | Med | shadow | 3 | Rolling accuracy edge cases |

**Total: 30 risk-driven test scenarios across 6 components.**

## Integration Harness Plan

### Existing suites to run (per USAGE-PROTOCOL.md)

| Suite | Reason |
|-------|--------|
| `smoke` | Minimum gate -- regression baseline |
| `confidence` | trust_source "neural" => 0.40 weight change |
| `lifecycle` | Schema version bump for shadow_evaluations |

### New integration tests

No new infra-001 tests needed. Shadow mode is internal pipeline behavior
validated by unimatrix-learn and unimatrix-observe unit tests. The confidence
suite covers trust_score changes (existing test_confidence_valid_range already
validates the composite formula).

### Test execution order (Stage 3c)

1. `cargo test --workspace` -- all unit tests
2. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`
3. `python -m pytest suites/test_confidence.py -v --timeout=60`
4. `python -m pytest suites/test_lifecycle.py -v --timeout=60`

## Component Test Summary

| Component | Test Plan | Expected Tests |
|-----------|-----------|---------------|
| learn-crate | test-plan/learn-crate.md | 8 |
| model-trait | test-plan/model-trait.md | 5 |
| classifier-scorer | test-plan/classifier-scorer.md | 11 |
| registry | test-plan/registry.md | 7 |
| shadow | test-plan/shadow.md | 8 |
| integration | test-plan/integration.md | 4 |
| **Total** | | **43** |
