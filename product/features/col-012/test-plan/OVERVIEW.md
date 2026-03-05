# Test Plan Overview: col-012 Data Path Unification

## Test Strategy

The col-012 test strategy is rooted in 10 identified risks (R-01 through R-10) covering 31 scenarios. Tests are organized by component, with integration tests validating cross-component behavior.

## Risk Mapping

| Risk | Priority | Component Test Plan | Test Type |
|------|----------|-------------------|-----------|
| R-01 (field extraction) | High | event-persistence | Unit + Integration |
| R-02 (migration failure) | Med | schema-migration | Integration |
| R-03 (mapping fidelity) | High | sql-implementation | Integration |
| R-04 (spawn_blocking failure) | Med | event-persistence | Integration |
| R-05 (NULL feature_cycle) | High | sql-implementation | Unit + Integration |
| R-06 (timestamp overflow) | Low | event-persistence | Unit |
| R-07 (batch partial failure) | Med | event-persistence | Integration |
| R-08 (hook script breakage) | Low | jsonl-removal | Manual review |
| R-09 (status response fields) | Med | retrospective-migration | Integration |
| R-10 (input type mismatch) | High | sql-implementation | Integration |

## Integration Harness Plan

### Existing test infrastructure

The project uses `cargo test --workspace` for all Rust tests. No Python integration test infrastructure applies to this feature (the `product/test/infra-001/` suites are for MCP tool testing, not schema/UDS testing).

### New integration tests needed

1. **Migration integration test** (unimatrix-store)
   - Open a v6 database, verify migration to v7 creates observations table
   - Open a fresh database, verify observations table exists
   - Verify idempotent re-migration

2. **RecordEvent persistence test** (unimatrix-server)
   - Send RecordEvent via UDS handler, verify row in observations table
   - Covers all 4 hook types (PreToolUse, PostToolUse, SubagentStart, SubagentStop)

3. **Round-trip test** (unimatrix-server + unimatrix-observe)
   - Write event via RecordEvent -> read via SqlObservationSource -> compare fields
   - Key R-03 and R-10 coverage

4. **Full retrospective test** (unimatrix-server)
   - Populate observations + sessions tables
   - Call context_retrospective pipeline
   - Verify report structure

### Integration test placement

- Schema tests: `crates/unimatrix-store/tests/` (integration test)
- Event persistence tests: `crates/unimatrix-server/src/uds/listener.rs` (mod tests)
- SQL source tests: `crates/unimatrix-server/src/services/observation.rs` (mod tests)
- Round-trip tests: `crates/unimatrix-server/src/services/observation.rs` (mod tests)

## Test Counts Estimate

| Component | Unit Tests | Integration Tests | Total |
|-----------|-----------|------------------|-------|
| schema-migration | 0 | 3 | 3 |
| event-persistence | 3 | 5 | 8 |
| observation-source | 1 | 0 | 1 |
| sql-implementation | 4 | 3 | 7 |
| retrospective-migration | 0 | 3 | 3 |
| jsonl-removal | 1 | 0 | 1 |
| **Total** | **9** | **14** | **23** |
