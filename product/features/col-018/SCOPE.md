# col-018: UserPromptSubmit Dual-Route

## Problem Statement

When a `UserPromptSubmit` hook fires with a non-empty prompt, the hook process (`hook.rs:253-268`) maps it exclusively to `HookRequest::ContextSearch`. The server-side dispatch (`listener.rs:635-669`) handles `ContextSearch` by running the search pipeline and returning results. No observation is persisted.

User prompts are the richest topic/intent signal available but are completely discarded from the observation record. Every other hook event type goes through `RecordEvent` and gets persisted to the `observations` table via `insert_observation()`. UserPromptSubmit with a non-empty prompt is the sole exception.

For UserPromptSubmit with an empty prompt, the hook falls through to `generic_record_event()` (hook.rs:257), which does get recorded. So empty prompts are observed but meaningful prompts are not.

## Design Decision: Server-Side Intercept

col-017's scope (line 148-151) explicitly resolved the col-018 design approach:

> col-018 uses a server-side intercept pattern. The observation write happens in the ContextSearch dispatch arm with the prompt text already in-hand. So for UserPromptSubmit events, the server extracts the topic signal from the query text (calling `extract_topic_signal(&query)`) when writing the observation.

This means:
- No wire protocol changes. The hook continues to send `HookRequest::ContextSearch` exactly as today.
- No hook.rs changes. The `build_request()` UserPromptSubmit arm stays unchanged.
- Server-side only. The `ContextSearch` dispatch arm in `listener.rs` gains an observation write as a side effect before executing the search pipeline.
- Server-side topic extraction. The server calls `unimatrix_observe::extract_topic_signal(&query)` on the prompt text to populate `ObservationRow.topic_signal`. This is different from the RecordEvent path where topic extraction happens hook-side.

### Why server-side intercept (not a new wire variant)

1. col-017 designed the topic attribution system with this split in mind: hook-side extraction for RecordEvent paths (tool use, subagent), server-side extraction for ContextSearch paths (UserPromptSubmit).
2. No wire protocol coordination needed between col-017 and col-018.
3. The ContextSearch dispatch arm already has the prompt text (`query` field) in hand -- no additional data needed.
4. `unimatrix_observe::extract_topic_signal` is already a dependency of `unimatrix-server` and already used in `listener.rs` (line 1385) for retrospective attribution.

### Why NOT a new wire variant

A new `ContextSearchWithObservation` variant would require hook-side changes, wire protocol extension, and coordination that col-017 explicitly designed away. The milestone proposal (ass-018) left both options open, but col-017's scope resolved it to server-side intercept.

## Scope

### In Scope

1. **Server-side observation write in ContextSearch dispatch**: Add observation persistence logic to the `HookRequest::ContextSearch` dispatch arm in `listener.rs`. Before executing the search pipeline, construct an `ObservationRow` and persist it via `insert_observation()` in a fire-and-forget `spawn_blocking` task.

2. **Server-side topic extraction**: Call `unimatrix_observe::extract_topic_signal(&query)` on the prompt text to populate `ObservationRow.topic_signal`. The prompt is the richest signal source for topic detection.

3. **Server-side topic signal accumulation**: Call `session_registry.record_topic_signal()` with the extracted topic signal (when present), matching the pattern used in the `RecordEvent` dispatch arm (listener.rs:583-588).

4. **Observation field values**:
   - `session_id`: from the ContextSearch request's `session_id` field (with fallback)
   - `ts_millis`: current time (same pattern as `extract_observation_fields`)
   - `hook`: `"UserPromptSubmit"`
   - `tool`: `None`
   - `input`: prompt text (the `query` string, truncated to existing limits)
   - `response_size`: `None`
   - `response_snippet`: `None`
   - `topic_signal`: result of `extract_topic_signal(&query)` -- populated, not None

### Out of Scope

- Wire protocol changes (no new HookRequest variants)
- Hook-side changes (hook.rs `build_request()` unchanged)
- Topic extraction logic changes (col-017 owns `extract_topic_signal`)
- Schema changes (observations table already has all needed columns at v10)
- Changes to other hook event types
- Query logging for the search itself (nxs-010/col-021 territory)
- Changes to ContextSearch response format or search pipeline behavior
- MCP tool-originated ContextSearch (MCP tool searches go through `tools.rs`, not the UDS dispatch)

## Key Constraints

1. **No latency impact on search response**: The observation write must be fire-and-forget (`spawn_blocking` with no `.await`). The search pipeline must not block on the observation insert.

2. **Session ID availability**: The `ContextSearch` request has `session_id: Option<String>`. The hook-side always populates `session_id` for UserPromptSubmit (hook.rs:261), so in practice it will always be `Some`. When `None`, the observation can use a placeholder or be skipped.

3. **No schema changes**: Schema is v10 (from col-017). The observations table has 8 columns: `session_id, ts_millis, hook, tool, input, response_size, response_snippet, topic_signal`. All are already present.

4. **Existing `insert_observation` reuse**: The existing `insert_observation()` function and `ObservationRow` struct are reused directly. No new persistence functions needed.

5. **Discriminating hook vs MCP origin**: Only hook-originated `ContextSearch` requests need observation recording. This is naturally handled because MCP tool searches go through `tools.rs`, not through the UDS dispatch arm. All `ContextSearch` arriving via UDS are hook-originated.

## Success Criteria

1. Every `UserPromptSubmit` event with a non-empty prompt creates one row in the `observations` table with `hook = "UserPromptSubmit"` and `input` containing the prompt text.
2. The `topic_signal` column is populated via server-side `extract_topic_signal(&query)` -- not set to None.
3. Topic signals from UserPromptSubmit observations are accumulated in the session registry for col-017 attribution (via `record_topic_signal()`).
4. `ContextSearch` results are still returned to stdout for Claude Code injection (search pipeline unaffected).
5. The observation write does not add latency to the search response (fire-and-forget).
6. Empty-prompt UserPromptSubmit continues to work as today (generic RecordEvent via hook fallback).
7. MCP tool-originated searches are unaffected (different code path via tools.rs).

## Dependencies

- col-017 (merged): topic_signal column on observations, extract_topic_signal in unimatrix-observe, session_registry.record_topic_signal()
- col-012 (merged): observation persistence infrastructure (insert_observation, ObservationRow)
- col-007/col-008 (merged): hook handler and ContextSearch pipeline

## Estimated Complexity

Small. Touches 1 file:
- `crates/unimatrix-server/src/uds/listener.rs` -- add observation write + topic extraction + topic signal accumulation in the ContextSearch dispatch arm

Tests:
- Unit test: ContextSearch dispatch records observation (verify observation row in DB after dispatch)
- Unit test: topic_signal populated from prompt text containing feature IDs
- Unit test: search results still returned correctly with observation side effect
- Integration test: round-trip UserPromptSubmit with non-empty prompt produces both search results and observation row
