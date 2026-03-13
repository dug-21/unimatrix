# Agent Report: col-022-agent-0-scope-risk

## Task
Scope-level risk assessment for col-022 (Explicit Feature Cycle Lifecycle).

## Artifacts Produced
- `/workspaces/unimatrix/product/features/col-022/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- **High severity**: 2 (SR-01, SR-07)
- **Medium severity**: 5 (SR-02, SR-04, SR-05, SR-08, SR-09)
- **Low severity**: 1 (SR-06)
- **Total**: 9 risks identified (0 critical, 2 high, 5 medium, 1 low)

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-01** (High/Med): Hook-side attribution may race with eager attribution. If early file-path signals trigger `set_feature_if_absent` before `context_cycle(start)` fires, the explicit declaration is silently rejected. This undermines the feature's core value proposition.

2. **SR-07** (High/Med): Split validation between MCP tool (param validation) and hook handler (attribution logic) creates divergence risk. A single shared validation function is essential.

3. **SR-04** (Med/High): The tool ships but is inert until SM agents/protocols are updated to call it. High likelihood of a gap between tool availability and actual usage. Spec should mandate follow-up issue creation as a deliverable.

## Knowledge Stewardship
- Queried: /knowledge-search for "lesson-learned failures gate rejection" -- found #1067 (one session = one feature constraint), #384 (silent event loss accepted)
- Queried: /knowledge-search for "outcome rework" -- no rework outcomes found relevant to this feature
- Queried: /knowledge-search for "risk pattern" -- no directly applicable risk patterns found
- Queried: /knowledge-search for "hook latency UDS wire protocol" -- found ADR-005 (#246), ADR-002 (#243), UDS capability pattern (#300)
- Queried: /knowledge-search for "session attribution feature_cycle" -- found #759 (HashMap accumulator), #1067 (immutable attribution), #980 (eager attribution bugfix)
- Stored: nothing novel to store -- first scope-risk assessment for observation pipeline features, no cross-feature pattern visible yet
