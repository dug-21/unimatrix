# Agent Report: vnc-021-agent-2-testplan

## Task
Design per-component test plans for vnc-021 HTTPS transport + static token auth, mapping 17 risks and 51 test scenarios to concrete test cases.

## Output Files

| File | Tests | Lines |
|------|-------|-------|
| test-plan/OVERVIEW.md | -- | ~95 |
| test-plan/token-manager.md | 11 | ~78 |
| test-plan/config-extensions.md | 12 | ~72 |
| test-plan/static-token-auth.md | 14 + 4 CR | ~82 |
| test-plan/tls-config.md | 9 | ~55 |
| test-plan/health-handler.md | 9 | ~60 |
| test-plan/path-router.md | 14 | ~78 |
| test-plan/http-listener.md | 15 | ~75 |
| test-plan/lifecycle-integration.md | 17 | ~85 |

**Total: 101 test cases + 4 code review checkpoints**

## Risk Coverage Summary

All 17 risks from RISK-TEST-STRATEGY.md have test coverage:
- 4 Critical risks (R-01, R-03, R-04, R-07): 19 test cases
- 7 High risks (R-02, R-05, R-08, R-09, R-10, R-11, R-18): 27 test cases
- 4 Medium risks (R-06, R-13, R-14, R-15): 16 test cases
- 2 Low risks (R-12, R-16): 3 test cases

51 risk scenarios mapped; 101 total test cases (includes edge cases beyond risk scenarios per lesson #3386).

## Integration Suite Plan

- **Mandatory gate**: smoke + tools + protocol + lifecycle + security suites via infra-001
- **No new infra-001 tests**: HTTP-specific behavior tested via Rust integration tests (infra-001 uses stdio, cannot test HTTP transport)
- **New Rust integration test file**: `tests/http_integration.rs` for full-stack HTTP tests

## Open Questions

1. **T-TM-11 (uppercase hex)**: Should `load_or_generate_token` accept uppercase hex? The generator writes lowercase, but a manually-created token file might contain uppercase. Behavior must be defined.
2. **T-HH-08 (query params on /health)**: Does `GET /health?foo=bar` bypass auth? Path comparison semantics must be defined — does the bypass compare URI path only (query stripped) or full URI?
3. **T-PR-12 (GET on MCP path)**: How does the router handle GET requests to MCP paths? rmcp may reject non-POST, or the router could reject at the routing layer.
4. **Test fixture strategy for TLS**: Should tests use `rcgen` (dev-dependency) to generate self-signed certs at test time, or ship pre-generated PEM fixtures? rcgen is cleaner but adds a dev-dependency.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- found 6 vnc-021 ADRs (#4665-4670), lesson #3386 (edge-case test omission pattern), pattern #729 (cross-crate integration tests)
- Stored: nothing novel to store -- test plan design followed established patterns; no new test infrastructure techniques discovered at design time
