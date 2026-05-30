# Agent Report: vnc-023-agent-0-scope-risk

## Deliverable
- `/workspaces/unimatrix/product/features/vnc-023/SCOPE-RISK-ASSESSMENT.md` (40 lines)

## Risk Summary
- **High severity**: 3 (SR-01 feature flags, SR-03 trait signature, SR-08 extension propagation)
- **Medium severity**: 4 (SR-02 MSRV, SR-04 http crate, SR-06 origins/hosts interaction, SR-07 schemars, SR-09 UDS blanket impl, SR-10 behavioral defaults)
- **Low severity**: 1 (SR-05 bundled enhancements)
- **Total**: 10 risks

## Top 3 Risks for Architect Attention
1. **SR-08** (High/Med) — Extension propagation regression. `ResolvedIdentity` survival through rmcp 1.7 internals is unverified. Highest integration risk.
2. **SR-01** (High/Low) — Feature flag existence. If any of 6 Cargo features are renamed in 1.7, scope estimate is invalid. Verify before architecture.
3. **SR-03** (High/Med) — `ServerHandler::initialize` trait signature. If changed to `async fn`, the `std::future::ready()` pattern breaks. Mechanical fix but must be designed for.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection -- 5 results, none directly applicable to dependency upgrade scope (gate validation and test-path lessons)
- Queried: `/uni-knowledge-search` for outcome rework dependency upgrade -- no results
- Queried: `/uni-knowledge-search` for risk pattern dependency upgrade transport rmcp -- found #4699 (rmcp migration scope pattern), directly relevant and used to validate assumptions
- Stored: nothing novel to store -- pattern #4699 already captures the transport isolation migration pattern for this exact feature
