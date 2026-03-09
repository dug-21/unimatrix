# col-019: Architecture -- PostToolUse Response Capture

## Overview

Fix the PostToolUse observation pipeline so that `response_size` and `response_snippet` are correctly populated for all PostToolUse events. Two changes are required: (1) fix the field name mapping from Claude Code's `tool_response` to computed `response_size`/`response_snippet` in the server, and (2) add observation persistence for rework-intercepted PostToolUse events while preserving col-017 topic signal accumulation.

## Architecture Decision: Server-Side Response Processing (ADR-001)

**Context**: Response size and snippet must be computed from Claude Code's `tool_response` JSON object. This computation could happen in the hook binary (before UDS transport) or in the server (after receiving the event).

**Decision**: Compute `response_size` and `response_snippet` server-side in `extract_observation_fields()`.

**Rationale**:
- The hook binary has a strict 50ms latency budget (40ms transport). Adding serialization work risks budget overrun.
- `extract_observation_fields()` runs inside `spawn_blocking`, outside any latency-critical path.
- For non-rework tools, `tool_response` already flows through via the inline RecordEvent (hook.rs line 293) which copies `input.extra` as the payload. No hook change needed for non-rework.
- For rework tools, the hook must include `tool_input` and `tool_response` in the payload alongside existing rework fields. This is a minor addition.
- Addresses SR-05 (hook vs server processing) and SR-06 (payload size increase).

**Consequences**: UDS payload size increases for rework tools (includes tool_input + tool_response). This is bounded by Claude Code's own response truncation and is well within the 1 MiB MAX_PAYLOAD_SIZE.

## Architecture Decision: Additive Dual-Write for Rework Events (ADR-002)

**Context**: Rework-eligible PostToolUse events are currently intercepted by the `post_tool_use_rework_candidate` match arm in `dispatch_request()` (listener.rs line 522). This arm records to `session_registry` (line 555) and accumulates topic signals (line 558, added by col-017), but does not persist to the observations table. The fix must add observation persistence without breaking rework detection or topic signal attribution.

**Decision**: Add observation persistence inside the existing rework handler match arm, AFTER both `session_registry.record_rework_event()` and the topic signal accumulation block. The rework handler remains the primary match arm for `post_tool_use_rework_candidate` events.

**Rationale**:
- The rework handler match arm (line 522) must remain the first match for `post_tool_use_rework_candidate` events.
- Observation persistence is additive: a fire-and-forget `spawn_blocking` call after all synchronous operations.
- The two writes are intentionally non-transactional. If observation write fails, rework tracking still works. If rework recording fails (in-memory), observations still persist.
- Placement after topic signal accumulation ensures SR-05 (col-017 interaction) is addressed -- the synchronous topic signal path completes before the async observation write is spawned.
- Addresses SR-01 (rework regression) and SR-04 (dual-write consistency).

**Consequences**: Rework-eligible PostToolUse events now appear in the observations table. Write volume increases proportionally. The fire-and-forget pattern absorbs the cost.

## Architecture Decision: Rework Event Payload Enhancement (ADR-003)

**Context**: The hook's `build_request()` constructs `post_tool_use_rework_candidate` events (line 343) with a payload of `{tool_name, file_path, had_failure}` plus `topic_signal` on the ImplantEvent (col-017). The original `tool_input` and `tool_response` from Claude Code are discarded. The observation writer needs these fields to populate the observation row.

**Decision**: Enhance the `post_tool_use_rework_candidate` payload to include `tool_input` and `tool_response` from the original Claude Code input, alongside existing rework fields.

**Rationale**:
- The rework handler in listener.rs extracts only `tool_name`, `file_path`, `had_failure` from the payload. Adding `tool_input` and `tool_response` fields does not affect rework extraction -- extra fields in a JSON object are ignored.
- The observation writer needs these fields to populate the observation row correctly.
- The `topic_signal` field already flows correctly via `ImplantEvent.topic_signal` (col-017) and does not need to be in the payload.
- Alternative (re-routing through generic RecordEvent) would require restructuring the dispatch logic and risks SR-01.

**Consequences**: Rework candidate payloads grow in size. Bounded by tool_input + tool_response size. Well within UDS limits.

## Component Changes

### hook.rs -- build_request() PostToolUse handler (lines 280-356)

**Non-rework path (line 291-301)**: No change needed. The inline `HookRequest::RecordEvent` already copies `input.extra` as the payload, which contains `tool_response`. The `topic_signal` field is set from `extract_event_topic_signal()`. The server-side `extract_observation_fields()` will compute response_size/snippet from the `tool_response` in the payload.

**Single-tool rework path (Bash/Edit/Write, line 343-355)**: Add `tool_input` and `tool_response` to the payload.

Current payload:
```json
{"tool_name": "Edit", "file_path": "src/foo.rs", "had_failure": false}
```

New payload:
```json
{
  "tool_name": "Edit",
  "file_path": "src/foo.rs",
  "had_failure": false,
  "tool_input": {"path": "src/foo.rs", "old_string": "a", "new_string": "b"},
  "tool_response": {"success": true}
}
```

**MultiEdit batch path (line 318-332)**: Add `tool_input` and `tool_response` to each event payload.

Current per-event payload:
```json
{"tool_name": "MultiEdit", "file_path": "src/foo.rs", "had_failure": false}
```

New per-event payload:
```json
{
  "tool_name": "MultiEdit",
  "file_path": "src/foo.rs",
  "had_failure": false,
  "tool_input": {"edits": [...]},
  "tool_response": {"success": true}
}
```

Note: For MultiEdit, every per-path event gets the same `tool_input` (full edits array) and `tool_response`. This is a simplification -- the per-edit breakdown is not available from Claude Code.

### listener.rs -- extract_observation_fields() (line 1803)

**PostToolUse branch (line 1817-1828)**: Replace the direct field lookups with computed response fields.

Current code looks for:
```rust
let rs = event.payload.get("response_size").and_then(|v| v.as_i64());
let rsnip = event.payload.get("response_snippet").and_then(|v| v.as_str()).map(|s| s.to_string());
```

New code computes from tool_response:
```rust
let (rs, rsnip) = extract_response_fields(&event.payload);
```

Add `"post_tool_use_rework_candidate"` to the match arm so rework events can also be extracted:
```rust
"PostToolUse" | "post_tool_use_rework_candidate" => { ... }
```

The hook column must be normalized to `"PostToolUse"` for rework candidates to maintain consistency with the observation read path.

### listener.rs -- New helper: extract_response_fields()

```rust
fn extract_response_fields(payload: &serde_json::Value) -> (Option<i64>, Option<String>) {
    // Primary: compute from tool_response (Claude Code's actual field)
    if let Some(response) = payload.get("tool_response") {
        if !response.is_null() {
            let serialized = serde_json::to_string(response).unwrap_or_default();
            let size = serialized.len() as i64;
            let snippet: String = serialized.chars().take(500).collect();
            return (Some(size), Some(snippet));
        }
    }
    // Fallback: legacy field names
    let rs = payload.get("response_size").and_then(|v| v.as_i64());
    let rsnip = payload.get("response_snippet")
        .and_then(|v| v.as_str()).map(|s| s.to_string());
    (rs, rsnip)
}
```

### listener.rs -- Rework handler observation persistence (after line 564)

After the existing rework handler completes its synchronous operations (rework recording on line 555, topic signal accumulation on lines 558-563), add observation persistence:

```rust
// col-019: Persist rework PostToolUse as observation (fire-and-forget)
let store_for_obs = Arc::clone(store);
let obs = extract_observation_fields(&event);
tokio::task::spawn_blocking(move || {
    if let Err(e) = insert_observation(&store_for_obs, &obs) {
        tracing::error!(error = %e, "rework observation write failed");
    }
});
```

The `extract_observation_fields()` call handles hook type normalization (rework_candidate -> PostToolUse) and response field extraction from the enhanced payload.

### listener.rs -- RecordEvents handler (line 603)

No changes needed. The RecordEvents handler already calls `extract_observation_fields()` on each event in the batch (line 625) and persists them via `insert_observations_batch()`. The match arm fix in `extract_observation_fields()` (adding `"post_tool_use_rework_candidate"`) is sufficient for MultiEdit batch events to get correct response fields and hook normalization.

## Data Flow (After Fix)

### Non-Rework PostToolUse (Read, Grep, Glob, MCP tools)
```
Claude Code stdin
  -> parse_hook_input() -> HookInput { extra: {tool_name, tool_input, tool_response, ...} }
  -> build_request() -> inline RecordEvent (line 293)
  -> ImplantEvent { event_type: "PostToolUse", payload: input.extra, topic_signal }
  -> UDS transport
  -> dispatch_request() -> generic RecordEvent handler (line 569)
     1. topic_signal accumulation (line 583) [col-017, unchanged]
     2. extract_observation_fields()
        -> tool_response -> response_size (byte length) + response_snippet (first 500 chars)
     3. insert_observation() [spawn_blocking, fire-and-forget]
```

### Rework-Eligible PostToolUse (Bash, Edit, Write)
```
Claude Code stdin
  -> parse_hook_input() -> HookInput { extra: {tool_name, tool_input, tool_response, ...} }
  -> build_request() -> post_tool_use_rework_candidate (line 343)
  -> ImplantEvent { event_type: "post_tool_use_rework_candidate",
       payload: {tool_name, file_path, had_failure, tool_input, tool_response},
       topic_signal }
  -> UDS transport
  -> dispatch_request() -> rework handler (line 522)
     1. session_registry.record_rework_event() [line 555, unchanged]
     2. session_registry.record_topic_signal() [line 558, col-017, unchanged]
     3. extract_observation_fields() [NEW - col-019]
        -> normalizes hook to "PostToolUse"
        -> tool_response -> response_size + response_snippet
     4. insert_observation() [NEW - col-019, spawn_blocking, fire-and-forget]
```

### MultiEdit PostToolUse
```
Claude Code stdin
  -> build_request() -> RecordEvents { events } (line 332)
  -> Each ImplantEvent: { event_type: "post_tool_use_rework_candidate",
       payload: {tool_name: "MultiEdit", file_path, had_failure, tool_input, tool_response},
       topic_signal }
  -> UDS transport
  -> dispatch_request() -> RecordEvents handler (line 603)
     1. topic_signal accumulation for each event [line 614, col-017, unchanged]
     2. extract_observation_fields() on each event [line 625, enhanced]
        -> normalizes hook to "PostToolUse"
        -> tool_response -> response_size + response_snippet
     3. insert_observations_batch() [spawn_blocking, fire-and-forget]
```

Note: MultiEdit rework events go through RecordEvents handler, which does NOT call `session_registry.record_rework_event()`. This is a pre-existing gap in MultiEdit rework recording, not in col-019 scope.

## Integration Surface

- **Input**: Claude Code hook stdin JSON (PostToolUse events)
- **Internal**: hook.rs build_request() -> UDS -> listener.rs dispatch_request()
- **Output**: observations table rows with populated response_size and response_snippet
- **Dependencies**: No new crate dependencies. Uses existing serde_json, spawn_blocking.
- **Consumers**: unimatrix-observe metrics, detection rules, extraction rules (all read via SqlObservationSource)

## Non-Goals

- No schema changes
- No changes to the observation read path
- No backfill of historical data
- No changes to PreToolUse, SubagentStart, SubagentStop handling
- No changes to col-017 topic signal logic
