# Agent Report: vnc-022-agent-2-testplan

## Task
Design per-component test plans for vnc-022 (Remote Observation Transport), rooted in the Risk-Test Strategy.

## Output Files

| File | Lines | Coverage |
|------|-------|----------|
| test-plan/OVERVIEW.md | ~95 | Test strategy, risk mapping, integration harness plan |
| test-plan/compact-payload-wire.md | ~60 | AC-09, R-07: wire type serde round-trip |
| test-plan/capability-extension.md | ~55 | R-06: SessionWrite in HTTP capabilities |
| test-plan/dispatch-request-refactor.md | ~70 | AC-18, AC-19, R-02: UDS regression, capability parameterization |
| test-plan/observe-context.md | ~55 | R-01, R-11: ObserveContext struct correctness, Clone derivation |
| test-plan/observe-handler.md | ~175 | AC-01-08, AC-10, AC-14, AC-15, AC-17, R-03-05, R-08-10, R-12-14 |

## Risk Coverage Summary

All 14 risks from RISK-TEST-STRATEGY.md have at least one mapped test:
- 5 High priority (R-01, R-02, R-03, R-06, R-10): 15+ scenarios across components
- 5 Medium priority (R-04, R-05, R-08, R-11, R-13): 14 scenarios
- 4 Low priority (R-07, R-09, R-12, R-14): 9 scenarios

## Integration Harness Plan

- Smoke gate: mandatory (`pytest -m smoke`)
- Suites to run: protocol, tools, lifecycle, security (UDS regression detection)
- No new infra-001 tests needed: /observe is HTTP, infra-001 is MCP/stdio; Rust integration tests cover HTTP path
- Gap documented: no HTTP-level E2E in infra-001 (future work)

## Existing Test Updates Required

1. `test_post_observe_returns_501_stub` (router/tests.rs): update from 501 assertion to real handler behavior
2. `test_valid_token_inserts_resolved_identity_into_extensions` (auth/tests.rs): `"caps":3` -> `"caps":4`
3. `test_bearer_validator_trait_valid_token` (auth/tests.rs): capabilities vec must include SessionWrite

## Open Questions

None. All design questions resolved in source documents and ADRs.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 10 entries returned. Key: #4695 (ADR-004 response mapping), #4693 (ADR-002 capability param), #4696 (ADR-005 PreCompact forward compat), #4692 (ADR-001 ObserveContext), #4694 (ADR-003 session ID scoping). All directly relevant.
- Queried: context_search for "vnc-022 architectural decisions" -- 3 ADR entries. All consumed.
- Queried: context_search for "HTTP integration testing patterns" -- 5 entries. Pattern #3479 (cross-module coupled tests) informed integration test dependency planning. Others less relevant.
- Stored: nothing novel to store -- test plan follows established patterns from existing router/tests.rs and auth/tests.rs; no new testing infrastructure technique discovered.
