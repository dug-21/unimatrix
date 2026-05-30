# Agent Report: vnc-023-agent-3-risk

## Task
Produce architecture-risk RISK-TEST-STRATEGY.md for vnc-023 (rmcp 0.16 to 1.7 migration).

## Status: COMPLETE

## Artifacts Produced
- `/workspaces/unimatrix/product/features/vnc-023/RISK-TEST-STRATEGY.md`

## Risk Summary
- **Critical**: 2 risks (R-01 extension propagation, R-02 initialize signature)
- **High**: 4 risks (R-03 struct construction, R-04 config wiring, R-05 CVE resolution, R-10 http version)
- **Medium**: 5 risks (R-06 behavioral defaults, R-07 UDS transport, R-08 serve_client, R-09 config compat, R-11 ErrorData)
- **Low**: 2 risks (R-12 description, R-13 origin/host interaction)
- **Total**: 13 risks, 25 test scenarios

## Top 3 Risks
1. **R-01 (Critical)**: Extension propagation regression -- ResolvedIdentity silently lost through rmcp 1.7 internals. No compile-time signal. Failure degrades all tool calls to anonymous.
2. **R-02 (Critical)**: ServerHandler::initialize trait signature may have changed from `impl Future` to `async fn`. Compile gate catches this but fix must preserve client_type_map population logic.
3. **R-04 (High)**: allowed_origins config wiring has 4 hops (config.toml -> HttpConfig -> ProjectRouter -> McpAdapter -> StreamableHttpServerConfig). Value dropped at any hop silently disables CSRF defense.

## Scope Risk Traceability
All 10 SR-XX risks traced. SR-01, SR-02, SR-04, SR-07 resolved pre-architecture. SR-03, SR-05, SR-06, SR-08, SR-09, SR-10 mapped to architecture-level risks R-01 through R-13.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" -- #4473 (warn+continue masks missing tests), #4452 (gate-fix test must use broken trust class) informed R-01 test design
- Queried: /uni-knowledge-search for "risk pattern dependency upgrade migration" -- #4699 (rmcp migration scope pattern), #4367 (rmcp 0.16 traps) directly informed R-01, R-02, R-08
- Queried: /uni-knowledge-search for "rmcp ServerHandler extension propagation" -- #4368, #4699 confirmed extension propagation as highest integration risk
- Queried: /uni-knowledge-search for "config deserialization backward compatible" -- #646 (serde default pattern) confirmed R-09 mitigation approach
- Stored: nothing novel to store -- risk patterns identified are specific to this migration, not cross-feature patterns yet
