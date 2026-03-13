# Agent Report: col-022-agent-3-risk

## Mode
Architecture-Risk

## Artifacts Produced
- `/workspaces/unimatrix/product/features/col-022/RISK-TEST-STRATEGY.md`

## Risk Summary
- **High priority**: 4 risks (R-01, R-02, R-08, R-09)
- **Medium priority**: 6 risks (R-04, R-05, R-06, R-07, R-11, R-12)
- **Low priority**: 2 risks (R-03, R-10)
- **Total**: 12 risks, 29 test scenarios

## Top Risks for Human Attention

1. **R-09 (High)**: Hook tool_name matching may fail due to MCP server prefix. The hook detects `context_cycle` by matching on `tool_name` in PreToolUse events, but Claude Code prefixes tool names with the server name (e.g., `mcp__unimatrix__context_cycle`). If the matching logic uses exact equality instead of contains/suffix, the entire feature is non-functional.

2. **R-01 (High)**: ADR-002's force-set semantic resolves SR-01 but introduces a new risk -- if an agent passes the wrong topic, it overwrites correct heuristic attribution permanently. The `Overridden` variant and warn-level logging are the only defense.

3. **R-08 (High)**: The MCP tool and hook path are inherently disconnected. The MCP tool has no session identity and cannot verify whether hook-side attribution succeeded. Agents will receive success responses even when attribution failed. This is architecturally accepted but must be clearly documented in the tool description.

## Open Questions
1. How does `set_feature_force` behave when the session_id is not in SessionRegistry (session already closed or never registered)? The architecture does not specify this edge case.
2. The MCP tool response includes `was_set` (FR-19), but the MCP server has no session identity. How is this field populated? It can only reflect parameter validation success, not actual attribution state.

## Scope Risk Traceability
All 9 scope risks (SR-01 through SR-09) traced. See Scope Risk Traceability table in RISK-TEST-STRATEGY.md.

## Knowledge Stewardship
- Queried: /knowledge-search for "lesson-learned failures gate rejection" -- no directly relevant lessons for col-022's domain
- Queried: /knowledge-search for "risk pattern" (category: pattern) -- no risk-specific patterns found
- Queried: /knowledge-search for "SQLite migration schema session attribution" -- found #681 (create-new-then-swap pattern) and #836 (new table procedure), informed R-03/R-05 migration risks
- Queried: /knowledge-search for "outcome rework" -- no rework outcomes relevant to col-022
- Stored: nothing novel to store -- risks are feature-specific, no cross-feature pattern visible yet (col-022 is first feature using force-set attribution)
