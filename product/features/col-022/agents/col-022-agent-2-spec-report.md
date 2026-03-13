# Agent Report: col-022-agent-2-spec

## Task
Write SPECIFICATION.md for col-022 (Explicit Feature Cycle Lifecycle).

## Artifacts Produced
- `/workspaces/unimatrix/product/features/col-022/specification/SPECIFICATION.md`

## Key Decisions

1. **FR-19 `was_set` response field**: Specified that the MCP tool response for `type: "start"` includes a `was_set` boolean so callers know whether attribution actually took effect. This addresses SR-08 (callers may not realize attribution failed silently) and satisfies AC-05.

2. **Shared validation function (FR, NFR-05)**: Elevated SR-07 (validation split-brain) into a hard requirement. A single `validate_cycle_params()` function must serve both MCP tool and hook handler. This is a constraint, not a suggestion.

3. **Ordering as mitigation for SR-01**: Rather than adding override/force-set semantics (which would break the #1067 invariant), specified that the ordering constraint is by design -- SM must call `context_cycle(start)` before other tool calls. Protocol integration (follow-up) enforces this. The `was_set: false` response makes detection possible.

4. **Keyword truncation (FR-06)**: Specified that individual keywords exceeding 64 chars are truncated (not rejected), matching the silent-truncation pattern for the array length (AC-13). Consistent UX: no hard failures for keyword formatting.

5. **Follow-up issues as deliverables**: Per SR-04 recommendation, included follow-up GH issue creation in the spec's implied definition of done to prevent shipped-but-unused tool.

## Open Questions for Architect

1. **Keywords storage schema** (SCOPE Open Question 2): New column on sessions table (JSON array) vs. separate keywords table. Spec requires retrievability for future injection (FR-18) but leaves schema choice to architect. SR-05/SR-09 recommend designing with injection query pattern in mind.

2. **Hook tool name matching** (FR-08): The PreToolUse hook's `tool_name` includes the MCP server prefix (e.g., `mcp__unimatrix__context_cycle`). Architect should confirm the exact matching strategy -- prefix match, suffix match, or contains.

3. **`was_set` signal path**: FR-19 requires the MCP tool to report whether attribution succeeded, but the MCP server has no session identity. The hook fires before the MCP call. How does the MCP tool know whether `set_feature_if_absent` succeeded? Options: (a) MCP tool always returns `was_set: true` optimistically (fire-and-forget means it cannot confirm), (b) hook handler injects the result into the PreToolUse response which Claude Code passes to the agent, (c) accept that `was_set` is best-effort. Architect to resolve.

## Self-Check

- [x] SPECIFICATION.md covers all 15 acceptance criteria from SCOPE.md (AC-01 through AC-15)
- [x] Every functional requirement is testable (verification methods in AC table)
- [x] Non-functional requirements include measurable targets (5ms marginal, 50ms total)
- [x] Domain Models section defines 9 key terms
- [x] NOT in scope section is explicit (8 items)
- [x] Output file is in `product/features/col-022/specification/` only
- [x] No placeholder or TBD sections -- unknowns flagged as open questions
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship
- Queried: /query-patterns for feature cycle attribution, MCP tool validation, hook wire protocol -- found #981/#756 (NULL feature_cycle lesson), #1067 (one-session-one-feature), #318/#234 (tool pipeline conventions), #763 (observation intercept pattern), #246 (wire protocol ADR)
