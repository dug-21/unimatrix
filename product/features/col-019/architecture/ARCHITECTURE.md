# col-019: Architecture — PostToolUse Response Capture

## Overview

Fix the PostToolUse observation pipeline so that `response_size` and `response_snippet` are correctly populated for all PostToolUse events. Two changes are required: (1) fix the field name mapping from Claude Code's `tool_response` to computed `response_size`/`response_snippet`, and (2) add observation persistence for rework-intercepted PostToolUse events.

## Architecture Decision: Server-Side Response Processing (ADR-001)

**Context**: Response size and snippet must be computed from Claude Code's `tool_response` JSON object. This computation could happen in the hook binary (before UDS transport) or in the server (after receiving the event).

**Decision**: Compute `response_size` and `response_snippet` server-side in `extract_observation_fields()`.

**Rationale**:
- The hook binary has a strict 50ms latency budget (40ms transport). Adding serialization work risks budget overrun.
- `extract_observation_fields()` runs inside `spawn_blocking`, outside any latency-critical path.
- For non-rework tools, `tool_response` already flows through via `generic_record_event()` which copies `input.extra` as the payload. No hook change needed.
- For rework tools, the hook must include `tool_response` in the payload alongside rework fields. This is a minor addition to the payload construction.

**Consequences**: UDS payload size increases slightly for rework tools (includes tool_response). This is bounded by Claude Code's own response truncation and is well within the 1 MiB MAX_PAYLOAD_SIZE.

## Architecture Decision: Additive Dual-Write for Rework Events (ADR-002)

**Context**: Rework-eligible PostToolUse events (Bash, Edit, Write, MultiEdit) are currently intercepted by the `post_tool_use_rework_candidate` match arm in `dispatch_request()`. This arm records to `session_registry` but does not persist to the observations table. The fix must add observation persistence without breaking rework detection.

**Decision**: Add observation persistence inside the existing rework handler match arm, after the rework event is recorded. The rework handler remains the primary match arm for `post_tool_use_rework_candidate` events.

**Rationale**:
- The rework handler match arm (listener.rs:522) must remain the first match for `post_tool_use_rework_candidate` events.
- Observation persistence is additive: a fire-and-forget `spawn_blocking` call after `session_registry.record_rework_event()`.
- The two writes are intentionally non-transactional. If observation write fails, rework tracking still works. If rework recording fails (in-memory), observations still persist.
- Addresses SR-01 (rework regression risk) and SR-04 (dual-write consistency).

**Consequences**: Rework-eligible PostToolUse events now appear in the observations table. Write volume increases proportionally. The fire-and-forget pattern absorbs the cost.

## Architecture Decision: Rework Event Payload Enhancement (ADR-003)

**Context**: The hook's `build_request()` constructs `post_tool_use_rework_candidate` events with a minimal payload: `{tool_name, file_path, had_failure}`. The original `tool_response`, `tool_name`, and `tool_input` from Claude Code are discarded. The observation writer needs `tool_name`, `tool_input`, and `tool_response` to populate the observation row.

**Decision**: Enhance the `post_tool_use_rework_candidate` payload to include `tool_input` and `tool_response` from the original Claude Code input, alongside the existing rework fields.

**Rationale**:
- The rework handler in listener.rs extracts only `tool_name`, `file_path`, `had_failure` from the payload. Adding `tool_input` and `tool_response` fields does not affect rework extraction -- extra fields in a JSON object are ignored.
- The observation writer needs these fields to populate the observation row correctly.
- Alternative (re-routing through generic_record_event) would require restructuring the dispatch logic and risks SR-01.

**Consequences**: Rework candidate payloads grow in size. Bounded by tool_input + tool_response size. Well within UDS limits.

## Component Changes

### hook.rs — build_request()

**Rework path (PostToolUse for Bash/Edit/Write/MultiEdit)**:

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

For MultiEdit (RecordEvents), each event in the batch gets `tool_input` (the full edits array) and `tool_response`.

**Non-rework path**: No change. `generic_record_event()` already passes `input.extra` which contains `tool_response`.

### listener.rs — dispatch_request() rework handler

After `session_registry.record_rework_event()`, add observation persistence:

```rust
// Persist observation (fire-and-forget)
let obs = extract_observation_fields_for_rework(&event);
let store_clone = Arc::clone(store);
tokio::task::spawn_blocking(move || {
    if let Err(e) = insert_observation(&store_clone, &obs) {
        tracing::error!(error = %e, "rework observation write failed");
    }
});
```

Similarly for the RecordEvents batch handler for MultiEdit.

### listener.rs — extract_observation_fields()

Fix the PostToolUse branch to compute response_size and response_snippet from `tool_response`:

```rust
"PostToolUse" => {
    let tool = event.payload.get("tool_name")...;
    let input = event.payload.get("tool_input")...;
    let (rs, rsnip) = extract_response_fields(&event.payload);
    (tool, input, rs, rsnip)
}
```

New helper function `extract_response_fields()`:
```rust
fn extract_response_fields(payload: &serde_json::Value) -> (Option<i64>, Option<String>) {
    // Try tool_response first (Claude Code's actual field name)
    let response = payload.get("tool_response");
    // Fallback: try legacy field names for backward compatibility
    if response.is_none() {
        let rs = payload.get("response_size").and_then(|v| v.as_i64());
        let rsnip = payload.get("response_snippet")
            .and_then(|v| v.as_str()).map(|s| s.to_string());
        if rs.is_some() || rsnip.is_some() {
            return (rs, rsnip);
        }
        return (None, None);
    }
    let response = response.unwrap();
    let serialized = serde_json::to_string(response).unwrap_or_default();
    let size = serialized.len() as i64;
    let snippet = if serialized.len() > 500 {
        serialized[..500].to_string()
    } else {
        serialized
    };
    (Some(size), Some(snippet))
}
```

Also add a `"post_tool_use_rework_candidate"` arm that extracts the same fields, since rework events have `tool_name`, `tool_input`, and `tool_response` in their payload (after ADR-003).

### listener.rs — RecordEvents batch handler

Add observation persistence for the MultiEdit batch path. After the batch rework recording, persist all events as observations.

## Data Flow (After Fix)

### Non-Rework PostToolUse (Read, Grep, Glob, MCP tools)
```
Claude Code stdin
  -> parse_hook_input() -> HookInput { extra: {tool_name, tool_input, tool_response, ...} }
  -> build_request() -> generic_record_event()
  -> ImplantEvent { event_type: "PostToolUse", payload: input.extra }
  -> UDS transport
  -> dispatch_request() -> generic RecordEvent handler
  -> extract_observation_fields()
     -> tool_response -> response_size (byte length) + response_snippet (first 500 chars)
  -> insert_observation() [spawn_blocking, fire-and-forget]
```

### Rework-Eligible PostToolUse (Bash, Edit, Write, MultiEdit)
```
Claude Code stdin
  -> parse_hook_input() -> HookInput { extra: {tool_name, tool_input, tool_response, ...} }
  -> build_request() -> post_tool_use_rework_candidate
  -> ImplantEvent { event_type: "post_tool_use_rework_candidate",
       payload: {tool_name, file_path, had_failure, tool_input, tool_response} }
  -> UDS transport
  -> dispatch_request() -> rework handler (line 522)
     1. session_registry.record_rework_event() [rework tracking, unchanged]
     2. extract_observation_fields() [NEW]
        -> tool_response -> response_size + response_snippet
     3. insert_observation() [NEW, spawn_blocking, fire-and-forget]
```

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
