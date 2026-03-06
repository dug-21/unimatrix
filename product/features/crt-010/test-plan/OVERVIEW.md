# crt-010 Test Plan Overview

## Test Strategy

Tests are organized by risk severity, with critical risks (R-01) validated first. The test plan maps directly to the 12 risks in RISK-TEST-STRATEGY.md and the 17 acceptance criteria in ACCEPTANCE-MAP.md.

## Risk-to-Test Mapping

| Risk | Severity | Component Tests | AC Coverage |
|------|----------|----------------|-------------|
| R-01 (get_embedding API) | Critical | c7-penalty-constants (cosine_similarity), integration via VectorIndex | AC-04, AC-05 |
| R-02 (Penalty ranking) | High | c1-retrieval-mode, c7-penalty-constants | AC-02, AC-03 |
| R-03 (Strict empty results) | High | c1-retrieval-mode, c4-uds-hardening | AC-10 |
| R-04 (Latency) | High | Manual/benchmark | AC-16 |
| R-05 (Dangling supersession) | Med | c2-supersession-injection | AC-07 |
| R-06 (Co-access signature) | High | c3-coaccess-exclusion | AC-08, AC-09 |
| R-07 (Explicit status + injection) | Med | c5-mcp-asymmetry | AC-14, AC-14b |
| R-08 (Post-compaction) | Resolved | c6-compaction-pruning | AC-12 |
| R-09 (Race condition) | Med | Error handling in c2 | AC-07 |
| R-10 (Briefing over-filtering) | Med | c4-uds-hardening | AC-11 |
| R-11 (Denormalized vectors) | Med | c7-penalty-constants | AC-05 |
| R-12 (Default Flexible change) | Med | c5-mcp-asymmetry | AC-13 |

## Integration Harness Plan

### Existing Suites from product/test/infra-001/

The infra-001 integration test suite tests end-to-end MCP tool behavior. For crt-010:

1. **Smoke tests**: Run `product/test/infra-001/suites/ -m smoke` to verify server startup, basic search, and tool availability are not broken
2. **Search suite**: Existing search tests verify basic search functionality — should pass unchanged since Flexible mode is backward-compatible

### New Integration Tests Needed

Integration tests should be added to the Rust test suite (not Python infra-001) since they test internal SearchService behavior:

1. **Strict mode full pipeline** (AC-01, AC-10): Insert mixed-status entries, search via Strict mode, verify only Active non-superseded entries returned
2. **Flexible mode penalty ranking** (AC-02, AC-03, AC-13): Insert Active + Deprecated entries at known similarities, verify ranking after penalties
3. **Supersession injection end-to-end** (AC-04, AC-06): Insert deprecated entry with superseded_by, verify successor appears in results
4. **Co-access with deprecated exclusion** (AC-08): Set up co-access pairs involving deprecated entries, verify zero boost
5. **Compaction verification** (AC-12): Insert, deprecate, compact, verify VECTOR_MAP state

### Test Execution Order

1. Unit tests per component (cargo test --workspace)
2. Integration smoke tests (infra-001)
3. New integration tests in Rust test modules

## Coverage Requirements

- All 17 ACs must have at least one test
- All Critical and High risks must have dedicated test scenarios
- Existing tests must continue to pass (backward compatibility)
- No test infrastructure changes — extend existing test helpers
