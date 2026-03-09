# col-018: UserPromptSubmit Dual-Route

## Problem Statement

The `UserPromptSubmit` hook currently dispatches the user prompt text to `ContextSearch` for context injection but **discards it from the observation record**. When a prompt has text, `build_request()` in `hook.rs` returns a `HookRequest::ContextSearch` variant. The server-side `handle_context_search()` performs the search and returns results -- but never writes the prompt to the `observations` table.

This means:
- The richest signal for topic detection and user intent is lost from the activity data
- Retrospective analysis cannot see what users actually asked
- Topic attribution (col-017) loses its most valuable signal source
- Search quality analysis (Wave 3 crt-019) cannot correlate prompts with search results

When the prompt is **empty**, `build_request()` falls back to `generic_record_event()` which does write a `RecordEvent` to observations -- but with no useful content (empty payload).

## Current Behavior

```
UserPromptSubmit (prompt present)
  hook.rs:build_request() -> HookRequest::ContextSearch { query: prompt, ... }
  listener.rs:dispatch() -> handle_context_search() -> HookResponse::Entries
  Result: Search results returned to Claude Code. Prompt NOT stored in observations.

UserPromptSubmit (prompt empty)
  hook.rs:build_request() -> HookRequest::RecordEvent { event: generic }
  listener.rs:dispatch() -> insert_observation()
  Result: Empty observation stored. No search performed.
```

## Desired Behavior

```
UserPromptSubmit (prompt present)
  1. Store prompt as observation (hook="UserPromptSubmit", input=prompt text)
  2. Dispatch ContextSearch and return results to Claude Code
  Both happen. Neither blocks the other.

UserPromptSubmit (prompt empty)
  Same as current: generic RecordEvent stored, no search.
```

## Scope

### In Scope

1. **Hook-side change**: Modify `build_request()` to produce a request that carries BOTH the observation data AND the search query, so the server can do both.
2. **Server-side change**: Modify `dispatch_request()` in `listener.rs` to persist the prompt as an observation AND perform the ContextSearch, returning search results.
3. **Observation storage**: Store UserPromptSubmit with `hook="UserPromptSubmit"`, `tool=NULL`, `input=<prompt text or JSON>`. The `input` column stores the prompt text for downstream analysis.
4. **Latency budget**: The 50ms hook budget must be preserved. Observation write is fire-and-forget (spawn_blocking), so it should not add latency to the search response path.
5. **Tests**: Unit tests for the new build_request behavior. Integration tests for the dual-route dispatch.

### Out of Scope

- **Topic signal extraction from prompts** -- that is col-017's responsibility. col-018 stores the raw prompt; col-017 extracts topic signals from it.
- **Query logging** -- that is nxs-010 (Wave 2). col-018 stores prompts as observations, not in a query_log table.
- **Prompt truncation policy** -- prompts can be very long. For MVP, store the full prompt text in `input`. A truncation policy can be added later if storage becomes a concern.
- **Schema changes** -- no new columns or tables needed. The existing `observations` table schema is sufficient.
- **Search behavior changes** -- the ContextSearch dispatch remains identical. Only observation persistence is added.

## Technical Analysis

### Approach Options

**Option A: New wire variant `PromptAndSearch`**

Add a new `HookRequest` variant that carries both the observation event and search parameters. The server dispatches both.

- Pro: Clean separation, explicit intent
- Con: New wire variant = serialization changes, more code, backward compatibility concern

**Option B: Server-side intercept in `ContextSearch` handler**

Keep hook.rs producing `ContextSearch` as today. In `dispatch_request()`, when handling `ContextSearch`, also write the query as a UserPromptSubmit observation.

- Pro: No wire protocol change, minimal hook-side change
- Con: ContextSearch handler gains observation-write responsibility (coupling); other ContextSearch callers (e.g., future Briefing) might not want observation writes

**Option C: Hook sends TWO requests (RecordEvent + ContextSearch)**

Modify hook.rs to produce a `RecordEvents` batch containing the observation, followed by a `ContextSearch`. But the current wire protocol is one-request-one-response.

- Pro: Conceptually clean
- Con: Wire protocol doesn't support multi-request; would require significant transport changes

**Recommended: Option B** -- server-side intercept in the ContextSearch handler. The observation write is a fire-and-forget `spawn_blocking` call that adds no latency. The coupling is acceptable because UserPromptSubmit is the only hook that routes to ContextSearch, and the intercept can be gated on the presence of a `session_id` (ContextSearch from MCP tools won't have the UDS session context). This is the smallest, safest change.

### Wire Protocol Impact

Option B requires **no wire protocol changes**. The hook continues to produce `HookRequest::ContextSearch` exactly as today. The server-side change is entirely within `dispatch_request()`.

### Observation Record Format

The observation row for a UserPromptSubmit prompt:

| Column | Value |
|--------|-------|
| `session_id` | From ContextSearch.session_id |
| `ts_millis` | Current time (server-side) |
| `hook` | `"UserPromptSubmit"` |
| `tool` | `NULL` |
| `input` | Prompt text (raw string, not JSON-wrapped) |
| `response_size` | `NULL` |
| `response_snippet` | `NULL` |

### Latency Impact

None. The observation write uses `tokio::task::spawn_blocking` (fire-and-forget), same pattern as all other observation writes in the dispatch handler. The ContextSearch response is returned immediately; the observation write completes asynchronously.

### Interaction with Wave 1 Siblings

- **col-017 (Topic Attribution)**: col-017 will extract topic signals from observation records including UserPromptSubmit. col-018 ensures the prompt data is available for col-017 to consume. No code dependency -- col-017 reads from the observations table that col-018 writes to.
- **col-019 (PostToolUse Response Capture)**: Independent. Different hook type, different code path.

## Open Questions

1. **Prompt storage format**: Store as raw text string or JSON-wrapped (`{"prompt": "..."}`)? Raw text is simpler and matches how SubagentStart stores prompt_snippet. JSON-wrapped is more extensible. Recommendation: raw text for consistency with SubagentStart pattern.

2. **Should empty prompts still generate observations?** Currently they fall through to generic_record_event. Should we keep that behavior or skip observation entirely for empty prompts? Recommendation: keep current behavior (generic RecordEvent for empty prompts) for backward compatibility.

## Acceptance Criteria

- AC-01: UserPromptSubmit with non-empty prompt stores an observation row with `hook="UserPromptSubmit"` and `input=<prompt text>`.
- AC-02: UserPromptSubmit with non-empty prompt still returns ContextSearch results (existing behavior preserved).
- AC-03: UserPromptSubmit with empty prompt behavior unchanged (generic RecordEvent, no search).
- AC-04: Observation write does not add latency to the ContextSearch response (fire-and-forget).
- AC-05: Observation row has correct `session_id` and `ts_millis` values.
- AC-06: Observation row has `tool=NULL`, `response_size=NULL`, `response_snippet=NULL`.
- AC-07: No wire protocol changes required (HookRequest/HookResponse enums unchanged).
- AC-08: Existing hook.rs and listener.rs tests continue to pass.

## Estimated Complexity

Small. ~30-50 lines of production code changes (mostly in `listener.rs` dispatch handler). ~50-80 lines of new tests. No schema migration. No wire protocol changes.
