# col-019: PostToolUse Response Capture

## Problem Statement

All PostToolUse observation rows have NULL `response_size` and `response_snippet` columns. This affects 5,136+ rows and growing. The root cause is a two-part field name mismatch between what Claude Code sends and what the observation pipeline expects, compounded by col-009's rework interception stripping response data entirely.

## Root Cause Analysis

### Problem 1: Rework Interception Drops Response Data

col-009 (rework tracking) intercepts PostToolUse events for file-mutating tools (Bash, Edit, Write, MultiEdit) in `hook.rs:build_request()`. These are converted to `post_tool_use_rework_candidate` events with a payload containing only `tool_name`, `file_path`, and `had_failure`. The response data from Claude Code is discarded.

In `listener.rs`, the `post_tool_use_rework_candidate` handler (line 522) records to the in-memory `session_registry` for rework detection but does NOT write to the `observations` table. These events are never persisted as observations.

**Impact**: File-mutating tool PostToolUse events (the majority of all PostToolUse events) are completely absent from the observations table. They have no row at all, not just NULL columns.

### Problem 2: Field Name Mismatch for Non-Rework Tools

Non-rework PostToolUse events (e.g., Read, Grep, Glob, MCP tools) pass through `generic_record_event()` which copies `input.extra` as the ImplantEvent payload. Then `extract_observation_fields()` looks for:
- `payload.get("response_size")` -- expects an integer
- `payload.get("response_snippet")` -- expects a string

But Claude Code sends `tool_response` as a JSON object containing the tool's result. The old JSONL pipeline (pre-col-012) computed `response_size` (byte length of serialized tool_response) and `response_snippet` (first 500 chars) from the raw response. The SQLite pipeline never performs this conversion.

**Claude Code PostToolUse input fields** (from docs.anthropic.com):
```json
{
  "session_id": "...",
  "hook_event_name": "PostToolUse",
  "tool_name": "Read",
  "tool_input": { "file_path": "/path/to/file" },
  "tool_response": { "content": "file content here..." }
}
```

The `HookInput` struct parses `session_id`, `hook_event_name`, `cwd`, `transcript_path`, `prompt` as named fields; everything else (including `tool_name`, `tool_input`, `tool_response`) lands in the `extra` catch-all via `#[serde(flatten)]`.

So the payload contains `tool_response` (a JSON object), not `response_size` (an integer) or `response_snippet` (a string).

## Affected Code Locations

| File | Function | Issue |
|------|----------|-------|
| `crates/unimatrix-server/src/uds/hook.rs` | `build_request()` lines 223-278 | Rework interception strips response data and changes event_type |
| `crates/unimatrix-server/src/uds/listener.rs` | Lines 522-557 | Rework handler records to session_registry but not observations table |
| `crates/unimatrix-server/src/uds/listener.rs` | `extract_observation_fields()` lines 1604-1615 | Looks for wrong field names in PostToolUse payload |
| `crates/unimatrix-server/src/uds/hook.rs` | `generic_record_event()` line 285 | Passes raw `input.extra` without converting tool_response |

## Downstream Impact

### Blocked Metrics (unimatrix-observe/src/metrics.rs)
- `total_context_loaded_kb` -- sum of PostToolUse response_size (line 117-123)
- `edit_bloat_total_kb` -- Edit PostToolUse response_size (line 125-133)
- `edit_bloat_ratio` -- edit response_size / total response_size (line 136-143)
- `context_load_before_first_write_kb` -- Read PostToolUse response_size before first write (line 147-162)

### Blocked Detection Rules (unimatrix-observe/src/detection/)
- `ContextLoadRule` -- relies on PostToolUse response_size for Read tools
- `EditBloatRule` -- relies on PostToolUse response_size for Edit tools
- `ContextHeavyReadRule` (agent.rs:42) -- checks response_size on Read PostToolUse
- `LargeEditRule` (agent.rs:421) -- checks response_size on Edit PostToolUse

### Blocked Extraction Rules
- `KnowledgeGapRule` (knowledge_gap.rs:36) -- checks response_size == 0 for zero-result detection
- `DeadKnowledgeRule` (dead_knowledge.rs:60) -- checks response_snippet for pattern matching
- `RecurringFrictionRule` (recurring_friction.rs:92) -- checks response_snippet for "denied" patterns

## Scope

### In Scope

1. **Fix field name mapping**: Convert Claude Code's `tool_response` object to `response_size` (byte length) and `response_snippet` (first 500 chars of serialized response) in `extract_observation_fields()`.

2. **Preserve rework tracking AND observation recording**: PostToolUse events for file-mutating tools must BOTH feed the rework tracker AND persist as observations with response data. Currently they only feed rework tracking.

3. **Handle both event paths**: The fix must handle:
   - Non-rework tools flowing through `generic_record_event()` -> `extract_observation_fields()`
   - Rework-eligible tools flowing through the col-009 interception path

4. **Capture tool_response in hook.rs**: The `build_request()` function must pass `tool_response` data through to the server, not discard it during rework interception.

5. **Tests**: Unit tests verifying response_size and response_snippet are correctly extracted for both rework and non-rework PostToolUse events.

### Out of Scope

- Schema changes (the observations table already has response_size and response_snippet columns)
- Backfilling historical NULL rows (no response data was persisted, so there is nothing to recover)
- Changes to the observation read path (SqlObservationSource already reads these columns correctly)
- Changes to detection rules or metrics computation (they already handle the fields correctly; they just never receive non-NULL values)
- UserPromptSubmit capture (col-018)
- Topic attribution (col-017)

## Success Criteria

- SC-1: PostToolUse observations for non-rework tools (Read, Grep, Glob, MCP tools) have non-NULL `response_size` and `response_snippet` values in the observations table.
- SC-2: PostToolUse observations for rework-eligible tools (Bash, Edit, Write, MultiEdit) are persisted in the observations table with non-NULL `response_size` and `response_snippet`, AND rework tracking continues to function.
- SC-3: `response_size` equals the byte length of the serialized `tool_response` JSON.
- SC-4: `response_snippet` equals the first 500 characters of the serialized `tool_response` JSON (or the full content if shorter).
- SC-5: All existing rework detection tests continue to pass.
- SC-6: New unit tests verify response capture for both rework and non-rework PostToolUse events.

## Key Constraints

- **Hook latency budget**: The hook binary has a 50ms total budget (40ms transport + 10ms startup). Response size computation (byte length of JSON) is O(1) after serialization. Snippet extraction (first 500 chars) is O(1). No latency risk.
- **Backward compatibility**: The observation schema does not change. The rework tracking mechanism must continue to work. No changes to the read path.
- **Fire-and-forget pattern**: Observation writes are fire-and-forget (spawn_blocking). This pattern must be preserved.

## Estimated Complexity

Low. The fix involves:
1. Extracting `tool_response` from `input.extra` in the hook's `build_request()` and passing it through
2. Computing `response_size` and `response_snippet` from `tool_response` in `extract_observation_fields()`
3. Ensuring rework-eligible PostToolUse events also persist as observations
4. ~50-100 lines of code changes + ~100 lines of tests
