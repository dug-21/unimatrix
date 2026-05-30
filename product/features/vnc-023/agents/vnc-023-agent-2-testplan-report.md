# Agent Report: vnc-023-agent-2-testplan

## Task
Design per-component test plans for vnc-023 (rmcp 0.16 to 1.7 migration) rooted in the Risk Strategy.

## Output Files

- `/workspaces/unimatrix/product/features/vnc-023/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/vnc-023/test-plan/cargo-version-bump.md`
- `/workspaces/unimatrix/product/features/vnc-023/test-plan/server-struct-migration.md`
- `/workspaces/unimatrix/product/features/vnc-023/test-plan/server-test-migration.md`
- `/workspaces/unimatrix/product/features/vnc-023/test-plan/config-allowed-origins.md`
- `/workspaces/unimatrix/product/features/vnc-023/test-plan/router-origin-wiring.md`
- `/workspaces/unimatrix/product/features/vnc-023/test-plan/main-call-site.md`
- `/workspaces/unimatrix/product/features/vnc-023/test-plan/initialize-signature.md`

## Risk Coverage Mapping

| Risk | Priority | Covered By | Test Type |
|------|----------|-----------|-----------|
| R-01 (Critical) | Extension propagation | server-struct-migration, server-test-migration + infra-001 security suite | Integration + compile |
| R-02 (Critical) | initialize signature | initialize-signature | Compile gate + unit + integration |
| R-03 (High) | Struct migration logic | server-struct-migration (T-01 through T-06) | Unit (get_info assertions) |
| R-04 (High) | Config wiring 4-hop | config-allowed-origins + router-origin-wiring + main-call-site | Unit + compile |
| R-05 (High) | CVE resolution | cargo-version-bump (V-03, V-04) + router-origin-wiring (T-05) | Verification |
| R-06 (Medium) | Behavioral defaults | initialize-signature (existing tests) | Existing test pass |
| R-07 (Medium) | UDS transport | cargo-version-bump (V-05, V-06) | Compile gate |
| R-08 (Medium) | serve_client renamed | server-test-migration (T-01, T-03) | Compile gate |
| R-09 (Medium) | Config deserialization | config-allowed-origins (T-01 through T-05) | Unit |
| R-10 (High) | http crate mismatch | cargo-version-bump (V-04) + R-01 coverage | Verification + integration |
| R-11 (Medium) | ErrorData signature | cargo-version-bump (V-07) | Compile + diff review |
| R-12 (Low) | Description string | server-struct-migration (T-03) | Unit |
| R-13 (High) | Origins vs hosts | router-origin-wiring (T-05) + code review | Code review + unit |

## Integration Suite Plan

| Suite | Run | Reason |
|-------|-----|--------|
| smoke | MANDATORY | Minimum gate |
| protocol | YES | MCP handshake validates ServerInfo (R-02, R-03) |
| tools | YES | Tool invocations exercise extension propagation (R-01) |
| security | YES | Capability enforcement depends on ResolvedIdentity (R-01) |
| lifecycle | YES | Session lifecycle validates keep_alive behavior (R-06) |
| confidence | NO | No changes to confidence logic |
| contradiction | NO | No changes to contradiction logic |
| volume | NO | No schema/storage changes |
| edge_cases | OPTIONAL | Lower priority, run if time permits |
| adaptation | NO | No changes to adaptation logic |

New integration tests: None required. Existing security suite implicitly validates extension propagation (ADR-003). If gap confirmed in Stage 3c, add one test to test_security.py.

## Open Questions

1. **get_info() testability**: The existing test module may not have a direct test for `get_info()` field values. Stage 3b may need to add one. The test plan specifies what to assert (T-01 through T-06 in server-struct-migration) but the test function may not exist yet.

2. **StreamableHttpServerConfig field visibility**: If `allowed_origins` on `StreamableHttpServerConfig` is not publicly readable (only settable), router-origin-wiring T-04 cannot directly assert the value. Fall back to compile-only verification that the field assignment compiles.

3. **allowed_hosts default inspection**: router-origin-wiring T-05 depends on being able to read `StreamableHttpServerConfig::default().allowed_hosts`. If the field is private, verify via code review only.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 17 entries; relevant: #4700 (ADR-001 compile-first), #4701 (ADR-002 allowed_origins), #4702 (ADR-003 extension propagation test), #3814 (MCP param deser requires transport-level validation)
- Queried: mcp__unimatrix__context_search (decision, topic vnc-023) -- 3 ADRs retrieved
- Queried: mcp__unimatrix__context_search (rmcp testing patterns) -- #3814 confirmed pattern that rmcp dispatch needs integration-level validation
- Stored: nothing novel to store -- test plan follows established patterns from prior features; no new testing infrastructure or technique discovered
