# Test Plan Overview: crt-006 Adaptive Embedding

## Test Strategy

Testing follows a bottom-up approach: unit tests validate each component in isolation, then integration tests validate the assembled pipeline through the MCP server. Risk coverage is traced from the Risk-Based Test Strategy (RISK-TEST-STRATEGY.md) to specific test cases.

## Component Test Plan Map

| Component | Test Plan | Unit Test File | Key Risks Covered |
|-----------|-----------|----------------|-------------------|
| lora | test-plan/lora.md | `crates/unimatrix-adapt/src/lora.rs` (#[cfg(test)]) | R-01, R-05, R-09, R-12 |
| training | test-plan/training.md | `crates/unimatrix-adapt/src/training.rs` (#[cfg(test)]) | R-02, R-06, R-11 |
| regularization | test-plan/regularization.md | `crates/unimatrix-adapt/src/regularization.rs` (#[cfg(test)]) | R-07 |
| prototypes | test-plan/prototypes.md | `crates/unimatrix-adapt/src/prototypes.rs` (#[cfg(test)]) | R-08 |
| episodic | test-plan/episodic.md | `crates/unimatrix-adapt/src/episodic.rs` (#[cfg(test)]) | -- (lowest priority, SR-04) |
| persistence | test-plan/persistence.md | `crates/unimatrix-adapt/src/persistence.rs` (#[cfg(test)]) | R-04 |
| service | test-plan/service.md | `crates/unimatrix-adapt/src/service.rs` (#[cfg(test)]) | R-03, R-05, R-12 |
| server-integration | test-plan/server-integration.md | `product/test/infra-001/suites/test_adaptation.py` | IR-01..IR-05, R-10 |

## Risk-to-Test Traceability

| Risk | Severity | Test Cases | Component |
|------|----------|------------|-----------|
| R-01 Gradient error | Critical | T-LOR-04 (finite diff), T-LOR-05 (convergence), T-TRN-07 (round-trip) | lora, training |
| R-02 InfoNCE NaN/Inf | High | T-TRN-04 (extreme sim), T-TRN-05 (extreme dissim), T-TRN-06 (mixed) | training |
| R-03 Training regression | High | T-SVC-04 (cross-topic), T-SVC-05 (EWC forgetting), A-03 (integration) | service |
| R-04 State deser failure | High | T-PER-02 (version upgrade), T-PER-03 (corrupt), T-PER-04 (zero-byte), T-PER-05 (dim mismatch) | persistence |
| R-05 Concurrent race | High | T-SVC-06 (concurrent read/write) | service |
| R-06 Reservoir bias | Medium | T-TRN-08 (uniform), T-TRN-09 (skewed), T-TRN-10 (overflow) | training |
| R-07 EWC drift | Medium | T-REG-04 (long-seq stability), T-REG-05 (effectiveness) | regularization |
| R-08 Prototype instability | Medium | T-PRO-06 (rapid correction), T-PRO-07 (convergence) | prototypes |
| R-09 Forward pass latency | Medium | T-LOR-07 (benchmark) | lora |
| R-10 Consistency false pos | High | T-SRV-04 (integration: A-04) | server-integration |
| R-11 Reservoir overflow | Medium | T-TRN-10 (capacity overflow) | training |
| R-12 Cold-start | Low | T-LOR-03 (near-identity), T-SVC-01 (identity output), A-01 (integration) | lora, service |
| R-13 ndarray compat | High | Compile gate: `cargo check`, `cargo test` | -- (CI) |

## Integration Test Traceability

| Integration Risk | Integration Test | Description |
|-----------------|-----------------|-------------|
| IR-01 Write path insert | A-01, A-03 | Store entry via context_store, verify search finds it |
| IR-02 Query/entry space match | A-01, A-03 | Search returns semantically correct entries |
| IR-03 Co-access feeds reservoir | A-03, A-05 | Searches generate training signal |
| IR-04 Shutdown persistence | A-02 | Restart preserves adaptation state |
| IR-05 Maintenance re-indexing | A-04 | Status check with adaptation active |

## Test Naming Convention

Unit tests: `T-{COMPONENT_PREFIX}-{NN}` where prefix is:
- `LOR` = lora
- `TRN` = training
- `REG` = regularization
- `PRO` = prototypes
- `EPI` = episodic
- `PER` = persistence
- `SVC` = service
- `SRV` = server-integration

Integration tests: `A-{NN}` (as defined in SCOPE.md)

## Test Execution Order

1. `cargo test -p unimatrix-adapt` -- all unit tests for the new crate
2. `cargo test` -- full workspace to verify no regressions
3. `pytest product/test/infra-001/suites/test_adaptation.py` -- integration tests
4. `pytest product/test/infra-001/suites/ -m smoke` -- smoke gate

## Coverage Targets

- Every public function in `unimatrix-adapt` has at least one test
- Every risk R-01 through R-13 has at least one test case
- Every integration risk IR-01 through IR-05 has at least one integration test
- Every edge case EC-01 through EC-10 is covered by either unit or integration test
- Gradient correctness validated at ranks 2, 4, 8, 16 (R-01 requirement)
- InfoNCE tested with extreme similarity inputs (R-02 requirement)
- Reservoir sampling validated statistically (R-06 requirement)
