# Agent Report: vnc-021-agent-0-scope-risk

## Mode
Scope-Risk

## Output
- `/workspaces/unimatrix/product/features/vnc-021/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- **High severity**: 3 (SR-01, SR-02, SR-08)
- **Medium severity**: 4 (SR-03, SR-05, SR-07, SR-09)
- **Low severity**: 3 (SR-04, SR-06, SR-10)
- **Total**: 10 risks

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-01 / SR-02** — rmcp 0.16.0 `StreamableHttpService` is lightly-adopted and the tower middleware extension propagation pattern is unproven in this codebase. Design an adapter boundary so rmcp issues can be isolated.

2. **SR-08** — HTTP listener sharing the tokio runtime with UDS, background ticks, and ML inference creates resource contention risk. Connection limits (AC-22) must be enforced early in the tower stack, before TLS handshake.

3. **SR-09** — `build_context_with_external_identity` seam has never been exercised in production. First activation via HTTP auth needs dedicated integration test coverage through to audit log verification.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for `lesson-learned failures gate rejection` -- found 5 results; #4473 (warn+continue masking missing tests) and #4367 (rmcp 0.16 traps) were relevant to informing SR-01/SR-02
- Queried: /uni-knowledge-search for `outcome rework` -- found 5 results; #367 (two-phase migration) tangentially relevant but no direct risk pattern
- Queried: /uni-knowledge-search for `risk pattern` -- found 5 results; no directly applicable patterns for HTTP transport
- Queried: /uni-knowledge-search for `HTTP transport TLS authentication` -- found #4661 (rmcp dep landscape), directly informed SR-04
- Queried: /uni-knowledge-search for `rmcp streamable HTTP` -- found #1897 (UDS bridge pattern) and #4367 (rmcp traps), directly informed SR-01/SR-02
- Stored: nothing novel to store -- no cross-feature risk patterns visible yet; vnc-021 is the first HTTP transport feature
