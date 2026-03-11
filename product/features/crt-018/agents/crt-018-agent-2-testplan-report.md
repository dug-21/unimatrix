# Agent Report: crt-018-agent-2-testplan

## Task
Test Plan Design (Stage 3a) for crt-018 Knowledge Effectiveness Analysis.

## Deliverables

| File | Path |
|------|------|
| Test Overview | `product/features/crt-018/test-plan/OVERVIEW.md` |
| Engine Tests | `product/features/crt-018/test-plan/effectiveness-engine.md` |
| Store Tests | `product/features/crt-018/test-plan/effectiveness-store.md` |
| Integration Tests | `product/features/crt-018/test-plan/status-integration.md` |

## Risk Coverage Summary

- 13 risks mapped to 28+ named test scenarios across 3 components
- 2 Critical risks (R-01, R-02): 11 scenarios covering classification priority ordering and NULL handling
- 3 High risks (R-03, R-04, R-05): 13 scenarios covering COUNT DISTINCT, calibration boundaries, division by zero
- 5 Medium risks (R-06, R-07, R-08, R-09, R-13): 10 scenarios covering performance, GC race, JSON compat, Settled logic, NaN
- 3 Low risks (R-10, R-11, R-12): 5 scenarios covering case sensitivity, spawn_blocking failure, markdown injection

## Integration Suite Plan

- **Mandatory**: smoke (minimum gate)
- **Required**: tools (context_status changes), lifecycle (status interaction)
- **New harness tests**: 3 planned (effectiveness with data, absent without data, summary format), with caveat that injection_log population may not be feasible through MCP alone — fallback to Rust-level integration tests

## Open Questions

1. **Injection_log population in infra-001**: The MCP protocol does not expose direct injection_log or session creation. Integration tests that verify effectiveness output through MCP may require either: (a) a populated_server fixture that includes session/injection data, or (b) reliance on natural injection through context_search + session recording. Stage 3c tester should determine feasibility.
2. **Orphaned entry_id handling**: The RISK-TEST-STRATEGY identifies an integration risk where entry_id appears in injection stats but not in entry_classification_meta (entry deleted between queries). The specification does not explicitly state the expected behavior. Test plan assumes "skip orphaned entries" — implementer should confirm.
3. **Settled with zero injections and inactive topic**: R-09/E-19 — an entry with zero injections whose topic is inactive. Per spec, Settled requires "at least one historical injection with success outcome," so zero-injection entries with inactive topics should NOT be Settled. They should fall through to the default (Effective). This seems counterintuitive but matches the spec. Implementer should verify intent.

## Self-Check

- [x] OVERVIEW.md maps risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan — which suites to run, new tests needed
- [x] Per-component test plans match architecture component boundaries (3 components)
- [x] Every high-priority risk has at least one specific test expectation
- [x] Integration tests defined for component boundaries
- [x] All output files within `product/features/crt-018/test-plan/`
