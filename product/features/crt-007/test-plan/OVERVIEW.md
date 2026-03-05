# Test Plan Overview: crt-007 Neural Extraction Pipeline

## Test Strategy

Tests are organized by risk category (R-01 through R-09) per the
RISK-TEST-STRATEGY.md. Each risk has dedicated test cases that validate
both the architectural mitigation and the functional behavior.

## Risk Coverage Mapping

| Risk | Priority | Component | Test Plan | Test Count |
|------|----------|-----------|-----------|------------|
| R-01 (ruv-fann) | P0 | model-trait | test-plan/model-trait.md | 5 |
| R-02 (adapt refactoring) | P0 | learn-crate | test-plan/learn-crate.md | 5 |
| R-03 (SignalDigest) | P1 | model-trait | test-plan/model-trait.md | 4 |
| R-04 (shadow accuracy) | P1 | shadow | test-plan/shadow.md | 4 |
| R-05 (integration) | P1 | integration | test-plan/integration.md | 3 |
| R-06 (corruption) | P2 | registry | test-plan/registry.md | 5 |
| R-07 (bias calibration) | P1 | classifier-scorer | test-plan/classifier-scorer.md | 4 |
| R-08 (schema migration) | P1 | shadow | test-plan/shadow.md | 4 |
| R-09 (performance) | P2 | integration | test-plan/integration.md | 4 |
| **Total** | | | | **38** |

## Test Location Strategy

- **Unit tests**: inline `#[cfg(test)] mod tests` in each source file
- **Integration tests**: `crates/unimatrix-learn/tests/` for cross-component tests
- **Server integration**: existing infra-001 smoke tests verify no regression

## Integration Harness Plan

### Existing Suites (product/test/infra-001/)

- **Smoke tests**: Server startup, basic MCP operations -- verify crt-007 changes
  don't break server initialization or existing tool calls.
- Run: `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`

### New Integration Tests

1. **integration_shadow.rs** (`crates/unimatrix-learn/tests/integration_shadow.rs`)
   - End-to-end: create models -> build digest -> predict -> log evaluation -> query accuracy
   - Requires tempdir with SQLite database
   - Covers: T-R04-1, T-R04-2, T-R04-3, T-R05-3, T-R09-3

2. **integration_registry.rs** (`crates/unimatrix-learn/tests/integration_registry.rs`)
   - Model lifecycle: create -> register -> promote -> rollback -> persist -> reload
   - Covers: T-R06-1 through T-R06-5

3. **Server-level regression**: No new server integration tests needed (covered by
   infra-001 smoke). The extraction_tick neural enhancement is best tested via unit
   tests on the `neural_enhance` function with mock inputs.

## Execution Order

1. `cargo test -p unimatrix-learn` -- all learn crate unit + integration tests
2. `cargo test -p unimatrix-adapt` -- verify no regressions (P0 gate)
3. `cargo test -p unimatrix-engine` -- trust_score test
4. `cargo test -p unimatrix-store` -- migration tests
5. `cargo test --workspace` -- full workspace green
6. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60`

## Test Data Strategy

- **SignalDigest**: constructed with known normalized values, verified against expected output
- **Models**: created with baseline weights, predictions verified for known inputs
- **Shadow evaluations**: synthetic records inserted into in-memory SQLite
- **Registry**: tempdir-based, JSON persistence verified via round-trip
- **Migration**: in-memory SQLite with manually set schema_version=7
