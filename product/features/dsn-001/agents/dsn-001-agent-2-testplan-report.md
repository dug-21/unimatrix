# Agent Report: dsn-001-agent-2-testplan

Phase: Test Plan Design (Stage 3a)
Status: COMPLETE

## Files Produced

| File | Lines | Coverage |
|------|-------|---------|
| `test-plan/OVERVIEW.md` | 127 | Risk-to-test mapping, AC coverage, integration harness plan |
| `test-plan/config-loader.md` | 375 | All 17 ConfigError variants, size cap, permissions, merge, freshness AC-25, weight sum, named preset immunity, security validation |
| `test-plan/confidence-params.md` | 215 | SR-10 test (verbatim), 9-field struct, all 4 preset rows, load-bearing weight tests, freshness score |
| `test-plan/category-allowlist.md` | 95 | new() delegation, from_categories custom list, empty list, existing tests unaffected |
| `test-plan/search-service.md` | 110 | grep gate, HashSet lookup tests, empty set no-panic, lesson-learned not hardcoded |
| `test-plan/agent-registry.md` | 120 | session_caps None/Some paths, permissive flag, propagation not silently None |
| `test-plan/server-instructions.md` | 80 | None/Some paths, MCP initialize integration, doc comment update checklist |
| `test-plan/tool-rename.md` | 125 | grep static gate with all excluded dirs, classify_tool, test_protocol.py, completeness checklist |
| `test-plan/startup-wiring.md` | 115 | load_config ordering, hook/bridge exclusion, home_dir None, background tick params |

## Risk Coverage Summary

| Priority | Risks | Coverage |
|----------|-------|---------|
| Critical (R-01–R-06) | 6 | Full — each has named unit tests with exact assertions |
| High (R-07–R-14) | 8 | Full — security ordering test, Admin exclusion, sum invariant |
| Med (R-15–R-22) | 8 | Full — grep audits + unit tests for all |
| Integration (IR-01–IR-05) | 5 | Full — cargo test gate + targeted behavior tests |
| Edge Cases (EC-01–EC-08) | 8 | Full — all documented and assigned to components |
| Security (SR-SEC-01–SR-SEC-05) | 5 | Full — length-before-scan ordering is explicit test |

## Mandatory Pre-PR Gates Documented

1. SR-10 test with exact comment: `confidence-params.md`
2. `grep -r "context_retrospective" .` sweep: `tool-rename.md` (with full excluded-dir list)
3. Four AC-25 named unit tests: `config-loader.md` (all four named functions)
4. Weight sum uses `(sum - 0.92).abs() < 1e-9`: `config-loader.md` (0.95 regression detector)
5. Named preset immunity to `[confidence]`: `config-loader.md`

## Integration Harness Plan

Suites to run: `smoke` (mandatory), `protocol`, `tools`, `security`, `lifecycle`.

New integration tests needed:
- `test_cycle_review_renamed_tool_responds` — tools suite
- `test_tool_discovery_includes_cycle_review` — protocol suite (update existing)
- `test_server_instructions_in_initialize_response` — tools suite (requires config fixture)
- `test_agent_enrollment_strict_session_caps` — tools suite (requires config fixture)

Harness fixture gap identified: AC-05/AC-06/AC-07 require config-injection fixture.
Recommendation: unit test covers logic path; integration path deferred to GH Issue if
fixture changes are too large for this PR.

## Open Questions

1. **`resolve_server_instructions` helper**: Does `server.rs` expose a testable helper
   for the None/Some logic, or is `UnimatrixServer::new()` the only test surface?
   Implementation should expose or make the logic independently testable.

2. **Empty categories minimum**: The spec does not set a minimum category count. The
   config-loader test `test_empty_categories_documented_behavior` expects `Ok(())` based
   on the `> 64` threshold, but this needs explicit confirmation from the implementer.

3. **`AgentRegistry::new_with_session_caps` vs single constructor**: The agent-registry
   test plan assumes a constructor that accepts `session_caps`. If the constructor
   signature differs, adjust `test_agent_registry_session_caps_propagated_to_store`.

4. **Harness config-injection fixture**: AC-05, AC-06, AC-07 MCP-level integration
   tests require starting the server with a config file. This is not currently supported
   by the harness. Implementer should either add a `config_server` fixture or accept
   unit-level coverage for these ACs and file a GH Issue.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "dsn-001 architectural decisions" (category: decision) — returned 4 ADRs (#2284–#2287). Queried "config validation testing toml security" — returned existing test gateway pattern (#315, #264) and OWASP convention (#146).
- Stored: nothing novel to store — all test patterns are feature-specific to dsn-001. The config validation test approach (standalone `validate_config`, temp file fixtures, grep static gates for literal removal) follows existing project conventions. The "SCOPE.md config comment (`<= 1.0`) contradicts ADR invariant (`= 0.92`)" anti-pattern was already documented in RISK-TEST-STRATEGY.md Knowledge Stewardship and is a candidate for future storage if it recurs across features.
