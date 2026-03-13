# Explicit Feature Cycle Lifecycle

## Problem Statement

Feature cycle attribution in the observation pipeline relies on heuristic signals (topic extraction from file paths, feature ID patterns, eager voting with 3+ signals / >60% share). This works when signals are strong but fails in several real-world scenarios:

1. **Worktree-isolated subagents** -- Delivery subagents run inside a coordinator session. All observations land on the coordinator's session_id, attributed to the coordinator's feature. Example: nan-001 delivery ran inside nan-003 coordinator; 521 observations landed with NULL or wrong feature_cycle.

2. **Single-spawn model** -- The workflow change from double-spawn (primary spawns SM, SM spawns specialists) to single-spawn (primary IS SM) means the SM's SessionStart no longer carries `feature_cycle` in `extra`. The only path for early attribution is gone.

3. **Mixed-signal sessions** -- Sessions touching multiple features produce scattered topic signals that never reach the eager threshold (3+ count, >60% share). Example: session `0a9e789b` had signals across `crt-018`, `crt-019`, `col-021` with no winner.

The downstream impact is that `context_retrospective(feature_cycle: "X")` returns "No observation data found" for features that had real work done. This undermines the entire observation pipeline's value.

## Goals

1. Provide an explicit, authoritative mechanism for SM/coordinator agents to declare which feature cycle a session belongs to.
2. Ensure the explicit declaration takes priority over heuristic attribution and cannot be overwritten by weaker signals.
3. Maintain backward compatibility -- existing heuristic attribution (eager voting, majority vote on SessionClose) continues as fallback when no explicit declaration is made.
4. Minimize MCP tool footprint -- single tool with start/stop variants per owner guidance.
5. Accept optional semantic keywords alongside the feature cycle identifier, stored for future use by the context injection pipeline.

## Non-Goals

1. **Multi-feature sessions** -- This feature does not add support for a single session belonging to multiple feature cycles simultaneously. The existing constraint (one session = one feature, Unimatrix #1067) is maintained.
2. **SubagentStart signal weighting** -- Issue #214 raises the question of whether SubagentStart signals should carry more weight in eager voting. That is a separate enhancement to the heuristic pipeline, not part of this feature.
3. **Cross-session feature lifecycle tracking** -- cycle_end does not implement feature-level lifecycle spanning multiple sessions (e.g., "feature X is complete"). It marks a clean observation boundary within the current session.
4. **MCP server-side session state** -- The MCP server remains session-unaware. Cycle lifecycle is an observation pipeline concern, bridged through the hook system which has session identity.
5. **Protocol/agent file changes** -- Updating SM agent definitions and protocol files to call `context_cycle` is integration work that happens after the tool exists, not part of this feature's implementation scope. A follow-up GH issue will be created for this.
6. **Keyword-driven context injection** -- The `keywords` parameter is accepted and stored by this feature, but the injection behavior (using keywords to perform semantic search and inject relevant knowledge on cycle start) is a follow-up feature. A GH issue will be created to track this enhancement.

## Background Research

### Current Attribution Pipeline

The attribution pipeline has three layers operating at different times:

1. **SessionStart** (immediate): `feature_cycle` extracted from `input.extra["feature_cycle"]` in SessionStart hook. Written to `SessionRecord` and `SessionRegistry`. Only works if the spawner sets the field.

2. **Eager attribution** (streaming, bugfix-198): As events arrive, `topic_signal` is accumulated in `SessionRegistry.topic_signals` (TopicTally). After each signal, `check_eager_attribution()` fires: if the leading topic has count >= 3 and > 60% share, it wins. `set_feature_if_absent()` ensures this never overwrites an existing value.

3. **Majority vote** (on close): At `SessionClose` or stale session sweep, `majority_vote()` resolves remaining topic signals. This is the last-chance fallback.

All three layers use the same `set_feature_if_absent` semantic: first writer wins, subsequent attempts are no-ops.

### Wire Protocol (HookRequest)

The UDS wire protocol (`crates/unimatrix-engine/src/wire.rs`) uses `#[serde(tag = "type")]` enum variants:
- `SessionRegister { session_id, cwd, agent_role, feature }`
- `SessionClose { session_id, outcome, duration_secs }`
- `RecordEvent { event: ImplantEvent }`
- `RecordEvents { events: Vec<ImplantEvent> }`
- `ContextSearch`, `Briefing`, `CompactPayload`, `Ping`

Adding a new variant (e.g., `CycleBegin` / `CycleEnd`) follows the established pattern.

### MCP Tool Registration Pattern

11 tools currently registered via `#[rmcp::tool_router]` on `UnimatrixServer`:
context_search, context_lookup, context_store, context_get, context_correct, context_deprecate, context_status, context_briefing, context_quarantine, context_enroll, context_retrospective.

Each tool follows a consistent pattern: Params struct with JsonSchema derive, identity resolution, capability check, validation, business logic, format response, audit/usage recording.

### Hook Handler Architecture

The hook subcommand (`uds/hook.rs`) runs synchronously (no tokio) with a 40ms budget. It:
1. Reads stdin JSON (HookInput)
2. Builds a HookRequest variant based on event type
3. Sends via LocalTransport (UDS) to the server
4. Returns stdout for injection (PreToolUse) or empty

The hook handler extracts topic signals per event type via `extract_event_topic_signal()`. For PreToolUse of MCP tools, the `tool_input` JSON is available -- this is where `context_cycle` parameters would be visible.

### Key Constraint: MCP Tool Calls Fire PreToolUse Hooks

When an agent calls `context_cycle(type: "start", topic: "nan-001")`, Claude Code fires a PreToolUse hook with:
- `tool_name: "context_cycle"`
- `tool_input: { "type": "start", "topic": "nan-001" }`
- `session_id`: the caller's session

The hook handler sees this BEFORE the MCP server processes the tool call. This means the hook side can extract `topic` from `tool_input` and call `set_feature_if_absent()` / `update_session_feature_cycle()` to definitively set the session's feature_cycle. The MCP tool itself can be a no-op that returns acknowledgment.

### Existing Infrastructure That Supports This

- `SessionRegistry::set_feature_if_absent()` -- already implements "first writer wins" semantic
- `update_session_feature_cycle()` -- persists feature_cycle to SQLite sessions table
- `sanitize_metadata_field()` -- input validation for feature_cycle strings
- `is_valid_feature_id()` in unimatrix-observe -- structural validation
- `extract_event_topic_signal()` for PreToolUse -- extracts from `tool_input`

### Lessons Learned (Unimatrix Knowledge Base)

- **#756 / #981**: NULL feature_cycle in sessions breaks downstream retrospective pipeline silently. Defense-in-depth needed -- multiple attribution paths.
- **#1067**: One session = one feature constraint. Eager attribution is immutable by design. `set_feature_if_absent` semantics must be preserved.

## Proposed Approach

### Single MCP Tool: `context_cycle`

Per owner guidance, consolidate to one tool with `type` discriminator:

```
context_cycle(type: "start", topic: "col-022", keywords: ["observation pipeline", "feature attribution", "session tracking"])
context_cycle(type: "stop", topic: "col-022")
```

Parameters:
- `type` (required): "start" or "stop"
- `topic` (required): feature cycle identifier (e.g., "col-022")
- `keywords` (optional): up to 5 semantic keywords describing what the feature is about. Stored with the session for future use by the context injection pipeline.

**MCP side**: Lightweight tool. Validates params, returns acknowledgment. No server-side session state changes (the MCP server has no session identity). The real work happens on the hook side.

**Hook side**: The PreToolUse hook for `context_cycle` fires before the MCP call reaches the server. The hook handler:
1. Detects `tool_name == "context_cycle"` (or the MCP tool name)
2. Extracts `type` and `topic` from `tool_input`
3. For `type: "start"`: sends a HookRequest that calls `set_feature_if_absent()` + `update_session_feature_cycle()` to definitively set the session's feature_cycle
4. For `type: "stop"`: records a cycle-end observation event for retrospective boundary detection

**UDS wire**: Two options (to be decided in architecture):
- **Option A**: New `HookRequest::CycleBegin { session_id, feature_cycle }` / `CycleEnd` variants
- **Option B**: Reuse existing `RecordEvent` with a special `event_type` and extract `feature_cycle` from payload (similar to existing #198 pattern in `RecordEvent` handler)

Option B has lower wire protocol churn but adds implicit coupling. Option A is cleaner but adds wire variants.

### Interaction with Existing Attribution

- `cycle_begin` sets feature_cycle authoritatively via `set_feature_force` -- explicit agent declaration overrides heuristic attribution
- Heuristic attribution (eager voting, majority vote) uses `set_feature_if_absent` -- first-writer-wins applies between heuristic signals only
- If no `cycle_begin` was called, existing heuristic pipeline works unchanged
- `cycle_end` records an observation event -- does not clear or change feature_cycle

## Acceptance Criteria

- AC-01: A new MCP tool `context_cycle` is registered with parameters `type` (required, "start" or "stop"), `topic` (required, feature cycle identifier), and `keywords` (optional, up to 5 semantic keywords).
- AC-02: Calling `context_cycle(type: "start", topic: "X")` from a session causes that session's `feature_cycle` to be set to "X" in both SessionRegistry and the SQLite sessions table.
- AC-03: Explicit `context_cycle(type: "start")` is authoritative -- it overrides any prior heuristic attribution. First-writer-wins (`set_feature_if_absent`) applies only between heuristic signals, not against explicit declarations.
- AC-04: Calling `context_cycle(type: "stop", topic: "X")` records a cycle-end observation event with the session_id and topic, queryable by the retrospective pipeline.
- AC-05: The `context_cycle` tool returns "noted" -- acknowledging parameters were accepted and the event was dispatched. The MCP tool does not confirm attribution outcome (it has no session identity).
- AC-06: The `topic` parameter is validated using the same rules as existing feature_cycle fields (sanitize_metadata_field, non-empty, max 128 chars).
- AC-07: The `type` parameter only accepts "start" or "stop"; any other value returns a validation error.
- AC-08: Existing heuristic attribution (eager voting, majority vote) continues to function unchanged when `context_cycle` is not called.
- AC-09: When `context_cycle(type: "start")` has been called, subsequent heuristic attribution does not overwrite the explicit value (set_feature_if_absent is a no-op when feature_cycle is already set).
- AC-10: The hook handler processes `context_cycle` PreToolUse events within the existing 50ms latency budget.
- AC-11: `context_retrospective(feature_cycle: "X")` correctly finds observations for sessions where `context_cycle(type: "start", topic: "X")` was called.
- AC-12: Wire protocol changes (if any) are backward compatible -- old hook binaries do not crash on new server responses and vice versa.
- AC-13: The `keywords` parameter accepts an array of up to 5 strings, each max 64 characters. Excess entries are silently truncated to 5.
- AC-14: Keywords are stored in the session record (SQLite sessions table) alongside feature_cycle, retrievable for future injection use.
- AC-15: Keywords are passed through the hook/UDS path and persisted via the same fire-and-forget pattern as feature_cycle.

## Constraints

1. **Hook latency budget**: 50ms total (40ms transport timeout + 10ms startup). The PreToolUse interception must not add meaningful overhead.
2. **Wire protocol backward compatibility**: HookRequest/HookResponse use `#[serde(tag = "type")]` -- adding new variants is non-breaking for deserialization (unknown variants error at runtime, not compile time), but old hook binaries must gracefully handle new response shapes.
3. **set_feature_if_absent semantics**: Must be preserved. This is the foundational invariant for one-session-one-feature (Unimatrix #1067).
4. **MCP tool count**: Currently 11 tools. Adding one more (context_cycle) brings to 12. Owner prefers consolidating to a single tool vs. two separate tools.
5. **Fire-and-forget pattern**: Session writes from hooks use spawn_blocking fire-and-forget. The cycle_begin persistence must follow this pattern for consistency.
6. **UDS capabilities**: Hook connections have `[Read, Search, SessionWrite]` capabilities. SessionWrite is required and already available.
7. **No MCP server session state**: The MCP server does not track which session is calling. Feature_cycle setting happens through the hook/UDS path which has session_id from Claude Code.

## Resolved Decisions

1. **Wire protocol approach**: Reuse `RecordEvent` with special event_type extraction (Option B). Lower wire protocol churn, has precedent from bugfix-198 `RecordEvent` payload extraction.

2. **Signal path**: Hook-only (Option A from coupling analysis). The MCP tool is the trigger point; the PreToolUse hook intercepts and does session attribution via `set_feature_if_absent()`. The MCP server has no session identity — hook path is the natural fit, not coupling.

3. **cycle_end behavior**: Record a ground truth boundary event. Session stays attributed to the feature. No state clearing. A subsequent `cycle_start` in the same session is a no-op (first-writer-wins). Post-feature activities in the session remain attributed to the original feature.

4. **Tool naming**: `context_cycle` confirmed. Fits the `context_` prefix convention and aligns with `feature_cycle` field naming.

5. **Multi-session feature cycles**: Each session independently calls `context_cycle(start)`. Design session and delivery session(s) each get their own attribution. No issues — `set_feature_if_absent` is per-session.

## Open Questions

1. **MCP tool response content**: Should the tool return the session_id in its response (useful for debugging) or just a status string? The MCP server does not have session_id in the tool call context.

2. **Keywords storage schema**: New column on sessions table, or a separate keywords table? Single column (JSON array) is simpler; separate table allows indexing. Architect to decide.

## Follow-Up Work

- **Protocol integration**: GH issue to update SM agent definitions and design/delivery/bugfix protocols to call `context_cycle(start)` as first action. (Non-goal for col-022.)
- **Keyword-driven context injection**: GH issue to implement injection behavior — on `context_cycle(start)`, use stored keywords to perform semantic search against Unimatrix knowledge and inject relevant context (patterns, lessons, ADRs) into the agent via hook stdout. This transforms context injection from incidental (prompt-text-based) to intentional (SM-declared topic relevance).

## Tracking

GitHub Issue: #214
