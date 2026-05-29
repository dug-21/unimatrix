# Agent Report: vnc-022-agent-0-scope-risk

## Task
Scope-level risk assessment for vnc-022 (Remote observation transport — /observe endpoint).

## Artifacts Produced
- `/workspaces/unimatrix/product/features/vnc-022/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 3 (SR-01, SR-03, SR-07, SR-09) |
| Medium | 5 (SR-02, SR-04, SR-05, SR-06, SR-08) |
| Low | 0 |

**Total: 9 risks identified**

## Top 3 Risks for Architect Attention

1. **SR-07** (High/Med): PathRouter cannot reach UnimatrixServer service handles — primary structural problem that blocks the /observe handler implementation. Architect must solve handle-passing without breaking rmcp encapsulation.

2. **SR-01** (High/Med): dispatch_request's 10-parameter signature is being extended with a capabilities param across two transports — wide integration seam prone to drift. Recommend a context struct.

3. **SR-09** (High/Low): Making dispatch_request pub(crate) and adding capability parameter changes UDS call sites — risk of silent regression on local hook path. Must be tested atomically.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for lesson-learned failures (#4473 warn+continue masking, #4147 spec-phase testability), outcome rework (#367 two-phase migration), risk patterns (no directly applicable patterns), dispatch/transport (#4691 dispatch_request transport-agnostic, #763 observation intercept), vnc-021 infra (#4669 TLS, #4670 static_token auth)
- Stored: nothing novel to store -- first scope-risk pass for vinculum remote transport; patterns will emerge after architecture-risk mode completes

## Status
COMPLETE
