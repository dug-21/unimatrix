# vnc-009 Test Plan Overview

## Test Strategy

Testing is risk-driven per RISK-TEST-STRATEGY.md. 12 risks, 50 scenarios, 4 high-priority
risks requiring integration-level verification.

### Risk Priority Matrix

| Priority | Risks | Focus |
|----------|-------|-------|
| High | R-01 (vote semantics), R-03 (JSON compat), R-04 (prefix stripping), R-11 (ServiceLayer ctor) | Regression + snapshot tests |
| Medium | R-06 (eviction), R-07 (briefing rate), R-09 (UDS exempt), R-10 (spawn safety) | Unit tests with controlled inputs |
| Low | R-02 (mutex contention), R-05 (backward compat), R-08 (audit blocking), R-12 (serde propagation) | Compilation + basic unit tests |

## Test Infrastructure

All tests extend existing TestHarness patterns from the server crate:
- `SecurityGateway::new_permissive()` -- extended with permissive rate limiter
- `tempfile::tempdir()` for store-backed tests
- `Arc<Store>` + `Arc<UsageDedup>` for UsageService tests
- Existing `make_server()` helper in server.rs tests

No new test infrastructure crates or isolated scaffolding.

## Integration Harness Plan

### Existing suites that apply to vnc-009

1. **server.rs unit tests** (lines 1000+): `record_usage_for_entries` tests become
   UsageService tests. Same scenarios, different API surface.
2. **gateway.rs unit tests** (lines 209+): SecurityGateway tests. Extended with rate
   limiting tests.
3. **tools.rs integration tests**: MCP tool handlers tested via trait dispatch. Extended
   to verify session_id threading and caller_id propagation.

### New integration tests needed

| Test | Risk Coverage | Location |
|------|--------------|----------|
| UsageService McpTool vote regression | R-01 | services/usage.rs tests |
| UsageService McpTool dedup regression | R-01 | services/usage.rs tests |
| UsageService fire-and-forget timing | R-10 | services/usage.rs tests |
| UsageService spawn safety (drop while running) | R-10 | services/usage.rs tests |
| RateLimiter boundary (300th/301st) | R-06 | services/gateway.rs tests |
| RateLimiter lazy eviction | R-06 | services/gateway.rs tests |
| RateLimiter UDS exemption | R-09 | services/gateway.rs tests |
| Rate limit + briefing semantic interaction | R-07 | services/briefing.rs tests or gateway tests |
| StatusReportJson snapshot | R-03 | mcp/response/status.rs tests |
| Session ID prefix/strip unit | R-04 | services/mod.rs tests |
| Session ID deserialization compat | R-05 | mcp/tools.rs tests |
| ServiceLayer::new() with UsageService | R-11 | services/mod.rs tests |

### Integration smoke tests

The workspace-level `cargo test` serves as the integration smoke test. All 739+ server
tests must pass. No separate pytest suite needed for this feature (changes are in Rust
only, no Python test infra affected).

## Per-Component Test Count Expectations

| Component | New Tests | Risk Coverage |
|-----------|----------|---------------|
| usage-service | ~15 | R-01, R-10, R-11 |
| rate-limiter | ~12 | R-02, R-06, R-07, R-09 |
| session-aware-mcp | ~10 | R-04, R-05 |
| status-serialize | ~5 | R-03, R-12 |
| uds-auth-audit | ~3 | R-08 |
| **Total new** | **~45** | All 12 risks |

Post-vnc-009 expected: 739 + ~45 = ~784 server tests minimum.
