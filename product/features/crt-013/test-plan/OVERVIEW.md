# crt-013 Test Plan Overview

## Test Strategy

Three test levels:
1. **Unit tests** — Component-level assertions (confidence invariants, env var parsing, SQL equivalence)
2. **Integration tests** — Full search pipeline tests for penalty validation (Rust-level)
3. **Integration harness** — infra-001 smoke + relevant suites (MCP-level)

## Risk-to-Test Mapping

| Risk | Priority | Test Coverage |
|------|----------|--------------|
| R-01: W_COAC removal breaks invariants | Medium | weight_sum_stored_invariant passes, cargo build clean, grep verification |
| R-02: Episodic removal breaks compilation | Low | cargo build --workspace, grep verification |
| R-03: Status penalty insufficient at extremes | High | T-SP-01, T-SP-02 (ranking assertions) |
| R-04: Penalty test flakiness | High | Pre-computed embeddings, relative ranking assertions only |
| R-05: crt-011 dependency | High | Deterministic confidence values injected in test fixtures |
| R-06: SQL aggregation diverges | Medium | AC-10 comparison test (both paths, field-by-field) |
| R-07: Briefing k env var edge cases | Low | Unit tests for parse_semantic_k() |
| R-08: Active entries missing tags | Medium | load_active_entries_with_tags() tag verification |
| R-09: Co-access + penalty interaction | Medium | T-SP-04 (deprecated excluded from boost) |
| R-10: AdaptationService API change | Low | cargo build (compiler catches) |
| R-11: SQL diverges at scale | Low | Extreme correction_count in comparison test |
| R-12: Briefing k construction-time only | Low | Code comment (no test) |

## Cross-Component Test Dependencies

- C1 (removal) has no test dependencies on other components
- C2 (penalty tests) depends on search pipeline being functional (C1 removal must not break it)
- C3 (briefing config) depends on briefing test infrastructure
- C4 (status scan) depends on Store methods being available

## Integration Harness Plan

### Existing Suites to Run

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory gate for any change |
| `tools` | Search tool behavior (penalty effects visible at MCP level) |
| `lifecycle` | Status transitions, correction chains |
| `confidence` | Co-access boost, re-ranking |

### New Integration Tests Needed

None in infra-001. All penalty validation is done via Rust-level integration tests (T-SP-01 through T-SP-06) which exercise the full search pipeline without going through MCP JSON-RPC. This avoids test infrastructure coupling while still validating the complete pipeline.

### Rationale

The penalty behavior is internal to the search scoring pipeline. MCP-level tests cannot reliably assert on ranking order since they would need to control embedding similarity, which requires direct vector injection. Rust-level integration tests provide better control and determinism.
