# col-022 Researcher Report

## Agent ID
col-022-researcher

## Task
Explore the problem space for explicit feature cycle lifecycle management and produce SCOPE.md.

## Findings

### Current Attribution Pipeline (Three Layers)
1. **SessionStart**: feature_cycle from `input.extra["feature_cycle"]` -- only works if spawner sets it
2. **Eager attribution** (bugfix-198): TopicTally accumulation + threshold check (3+ count, >60% share) via `check_eager_attribution()` + `set_feature_if_absent()`
3. **Majority vote on close**: Last-chance fallback at SessionClose or stale session sweep

All layers share `set_feature_if_absent` semantic (first writer wins). This invariant is critical and must be preserved.

### Key Architectural Insight: Hook-Side Attribution
MCP tool calls fire PreToolUse hooks before the MCP server processes the call. The hook handler receives `tool_name` and `tool_input` with the caller's `session_id`. This means the hook side can extract `feature_cycle` from `context_cycle`'s `tool_input` and call `set_feature_if_absent()` + `update_session_feature_cycle()` without the MCP server needing session awareness.

### Existing Infrastructure Ready for Reuse
- `set_feature_if_absent()` in SessionRegistry -- already implements first-writer-wins
- `update_session_feature_cycle()` -- persists to SQLite sessions table
- `sanitize_metadata_field()` -- input validation
- `extract_event_topic_signal()` for PreToolUse -- extracts from `tool_input`
- #198 pattern in RecordEvent handler -- already extracts `feature_cycle` from event payload

### Tool Count
Currently 11 MCP tools. Owner requests consolidation to single `context_cycle` tool with `type: start|stop` discriminator.

### Wire Protocol Options
- Option A: New HookRequest variants (CycleBegin/CycleEnd) -- clean, more churn
- Option B: Reuse RecordEvent with special event_type -- less churn, has precedent from #198

## Artifacts Produced
- `/workspaces/unimatrix/product/features/col-022/SCOPE.md`

## Open Questions (for human)
1. Wire protocol approach: new variants vs RecordEvent reuse?
2. Belt-and-suspenders: should MCP tool also attempt attribution, or rely solely on hook path?
3. cycle_end: simple observation event or richer behavior (signal drain, status marking)?
4. Tool naming confirmation: `context_cycle` acceptable?
5. MCP tool response: include session_id for debugging or just status?

## Risks
- **Hook dependency**: If Claude Code changes hook behavior (e.g., stops firing PreToolUse for MCP tools), the attribution path breaks. Mitigation: belt-and-suspenders with MCP-side fallback.
- **Race condition on concurrent sessions**: Two sessions calling `context_cycle(start)` for the same session_id (unlikely but possible in theory). Mitigated by `set_feature_if_absent` Mutex semantics.
- **Adoption gap**: The tool only works if SM/coordinator agents actually call it. Until protocols are updated, existing heuristic attribution must remain functional (AC-08).

## Knowledge Stewardship
- Queried: /query-patterns for "feature cycle lifecycle management observation pipeline attribution" -- found #383 (ObservationSource trait), #981/#756 (NULL feature_cycle lessons), #866 (attribution metadata), #1067 (one session = one feature constraint)
- Stored: nothing novel to store -- agent lacks Write capability; recommend storing "PreToolUse hook interception enables session-scoped side effects from MCP tools" as a pattern on topic observation-pipeline
