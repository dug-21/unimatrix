# Agent Report: vnc-021-agent-3-risk

## Role
Risk Strategist (architecture-risk mode)

## Artifact Produced
`product/features/vnc-021/RISK-TEST-STRATEGY.md`

## Risk Summary
- **Critical**: 4 risks (R-01, R-03, R-04, R-07)
- **High**: 7 risks (R-02, R-05, R-08, R-09, R-10, R-11, R-18)
- **Medium**: 5 risks (R-06, R-13, R-14, R-15, R-17)
- **Low**: 2 risks (R-12, R-16)
- **Total**: 18 risks, 51 test scenarios

## Top Risks Requiring Attention

1. **R-01 (Critical)**: rmcp extension propagation — ResolvedIdentity inserted by StaticTokenAuth may be dropped by rmcp's StreamableHttpService before reaching build_context_with_external_identity. This is the single highest-risk integration point. ADR-003 adapter boundary provides a fallback but the primary path must be spike-tested before full build-out.

2. **R-03 (Critical)**: build_context_with_external_identity first real activation — this seam was designed in vnc-005/vnc-014 but never exercised in production. Bearer-token callers have different identity characteristics (static agent_id, no resolve_agent lookup) that may expose untested assumptions.

3. **R-07 (Critical)**: Health endpoint auth bypass scope — if the path-match in StaticTokenAuth uses `starts_with("/health")` instead of exact match `== "/health"`, any path beginning with "/health" bypasses authentication. ADR-002 specifies exact match but implementation must be verified.

4. **R-04 (Critical)**: Connection flood starvation — HTTP listener shares the tokio runtime with all other server components. Pre-TLS semaphore (ADR-004) mitigates but must be tested under load to verify UDS isolation.

## Open Questions

1. **rmcp HTTP/2 support**: Architecture doc notes uncertainty about whether StreamableHttpService requires HTTP/1.1 for SSE. If HTTP/2 clients connect, behavior is undefined. Should the listener explicitly reject HTTP/2 upgrades?

2. **Concurrent token generation**: Two server processes starting in the same data volume simultaneously could race on token file creation. Is file-level locking needed, or is the atomic create-or-load pattern sufficient?

3. **Rate limiting coverage**: R-17 flags that CallerId::HttpBearer must not be rate-limit-exempt. The compiler enforces the match arm exists, but the semantic behavior (rate-limited vs exempt) is a logic error only detectable by code review or integration test. Is there a rate-limiting integration test fixture?

## Scope Risk Traceability
All 10 scope risks (SR-01 through SR-10) traced. 7 map to architecture risks; 3 are resolved at documentation/specification level with no residual architecture risk.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" -- found 8 results; #4473 (warn+continue masks missing failure-path tests) and #4177 (tautological assertions) informed emphasis on assertion quality in test scenarios
- Queried: /uni-knowledge-search for "risk pattern" -- found 5 pattern results; none directly applicable to HTTP transport risk
- Queried: /uni-knowledge-search for "rmcp StreamableHttpService extension propagation" -- found #4367 (rmcp 0.16 traps) which directly elevated R-01 severity
- Queried: /uni-knowledge-search for "connection limiting semaphore" -- found #735 (spawn_blocking saturation) and #1915 (UDS accept loop) which informed R-04 and R-09
- Stored: nothing novel to store -- risks are feature-specific; no cross-feature pattern visible yet (this is the first HTTP transport feature)
