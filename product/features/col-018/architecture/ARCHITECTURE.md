# col-018: Architecture

## Overview

col-018 adds an observation write side-effect to the existing `ContextSearch` dispatch arm in the UDS listener. No new modules, no wire protocol changes, no schema changes. The modification is localized to a single function in a single file.

## Architecture Decision: Server-Side Intercept in ContextSearch Dispatch

### Context

UserPromptSubmit with non-empty prompt maps to `HookRequest::ContextSearch` on the hook side. The server-side ContextSearch handler executes the search pipeline and returns results, but does not record the prompt as an observation. col-017 explicitly designed this split: hook-side topic extraction for RecordEvent paths, server-side topic extraction for ContextSearch paths.

### Decision

Add observation persistence and topic signal accumulation as a side effect in the `ContextSearch` dispatch arm of `dispatch_request()` in `listener.rs`. The observation is written fire-and-forget before the search pipeline executes. Topic signal is extracted server-side via `unimatrix_observe::extract_topic_signal(&query)`.

### Rationale

1. **No wire protocol coupling**: col-017 designed the topic attribution split specifically to avoid wire protocol coordination between col-017 and col-018.
2. **All data in hand**: The ContextSearch dispatch arm already has `query` (prompt text) and `session_id`.
3. **Existing infrastructure**: `insert_observation()`, `ObservationRow`, `extract_topic_signal()`, `session_registry.record_topic_signal()` all exist and are already used in listener.rs.
4. **Code path isolation**: UDS dispatch only handles hook-originated requests. MCP tool searches go through `tools.rs`. No risk of recording MCP tool searches as prompt observations.

### Consequences

- The `ContextSearch` dispatch arm gains a side effect (observation write) that is not present in the wire protocol semantics. This is documented in code comments.
- The observation `ObservationRow` is constructed directly in the dispatch arm rather than going through `extract_observation_fields()`, since there is no `ImplantEvent` to extract from. This is a small duplication but is cleaner than constructing a synthetic `ImplantEvent`.

## Data Flow

```
Hook Process (hook.rs)                    Server (listener.rs)
--------------------------               ---------------------------
UserPromptSubmit event
  |
  v
build_request("UserPromptSubmit")
  |
  +-- prompt non-empty -->  ContextSearch { query, session_id, ... }
  |                              |
  +-- prompt empty     -->  RecordEvent   (existing path, unchanged)
                                 |
                                 v
                         dispatch_request()
                                 |
                                 v
                         ContextSearch arm:
                           1. Extract topic_signal = extract_topic_signal(&query)
                           2. Accumulate topic signal in session_registry
                           3. Build ObservationRow { hook="UserPromptSubmit", input=query, topic_signal }
                           4. spawn_blocking insert_observation (fire-and-forget)
                           5. Execute search pipeline (handle_context_search)
                           6. Return search results
```

## Integration Surface

### Modified Function

`dispatch_request()` in `crates/unimatrix-server/src/uds/listener.rs`

The `HookRequest::ContextSearch` match arm (currently lines 635-669) gains 3 operations before the existing `handle_context_search()` call:

1. **Topic extraction**: `unimatrix_observe::extract_topic_signal(&query)` -- already imported and used elsewhere in this file (line 1385).
2. **Topic accumulation**: `session_registry.record_topic_signal(sid, signal, timestamp)` -- same call pattern as RecordEvent arm (lines 583-588).
3. **Observation persistence**: Construct `ObservationRow` and call `insert_observation()` via `spawn_blocking_fire_and_forget` -- same pattern as RecordEvent arm (lines 592-598).

### Unchanged Components

- `hook.rs` `build_request()`: UserPromptSubmit arm unchanged
- `wire.rs` `HookRequest`: No new variants
- `wire.rs` `ImplantEvent`: No changes
- `handle_context_search()`: No signature or behavior changes
- `extract_observation_fields()`: Not used for this path (direct ObservationRow construction)
- `insert_observation()`: Reused as-is
- Database schema: v10 unchanged

### Input Truncation

The `query` string stored in `ObservationRow.input` should be truncated to prevent unbounded storage for extremely long prompts. The existing `MAX_PAYLOAD_SIZE` (1 MiB) provides an upper bound at the transport level, but a tighter application-level limit of 4096 characters for the `input` field is appropriate, consistent with practical prompt sizes.

### Session ID Handling

When `session_id` is `None` on the ContextSearch request, the observation write is skipped. In practice this never happens for hook-originated requests (hook.rs:261 always populates session_id from `input.session_id`), but the guard ensures correctness for edge cases.

## ADRs

### ADR-018-001: Direct ObservationRow Construction (not via extract_observation_fields)

**Context**: The existing `extract_observation_fields()` takes an `ImplantEvent` and maps its fields to an `ObservationRow`. In the ContextSearch path, there is no `ImplantEvent` -- the prompt comes from the `query` field of `ContextSearch`.

**Decision**: Construct `ObservationRow` directly in the ContextSearch dispatch arm rather than creating a synthetic `ImplantEvent` and routing through `extract_observation_fields()`.

**Rationale**: Creating a synthetic `ImplantEvent` would be misleading (it was never sent on the wire) and would require populating fields (`event_type`, `payload`, `timestamp`) that serve no purpose. Direct construction is more honest and readable.

**Consequences**: If `extract_observation_fields()` gains new normalization logic in the future, the ContextSearch path would need manual updating. This is an acceptable trade-off for a single call site.

### ADR-018-002: Skip Observation When session_id is None

**Context**: `ContextSearch.session_id` is `Option<String>`. The `observations` table has `session_id TEXT NOT NULL`.

**Decision**: Skip the observation write entirely when `session_id` is `None` rather than using a fallback value.

**Rationale**: A missing session_id means the request is not from a normal hook flow. Recording an observation with a fabricated session_id would pollute the observation data. The hook path always provides session_id, so this guard only catches edge cases (manual UDS testing, future non-hook callers).

**Consequences**: Non-hook ContextSearch requests via UDS will not be observed. This is intentional.
