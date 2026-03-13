# col-022: hook-handler -- Pseudocode

## Purpose

Extend the PreToolUse path in `build_request()` to detect `context_cycle` MCP tool calls, extract and validate parameters, and construct a specialized `RecordEvent` with `event_type: "cycle_start"` or `"cycle_stop"`. ADR-001 (reuse RecordEvent), ADR-004 (shared validation).

## File: `crates/unimatrix-server/src/uds/hook.rs`

### New Import

```
use crate::infra::validation::{
    validate_cycle_params, CYCLE_START_EVENT, CYCLE_STOP_EVENT, CycleType,
};
```

### Modify: `build_request` function

Add a new match arm for `"PreToolUse"` **before** the existing fallthrough to `generic_record_event`. Currently, `"PreToolUse"` falls through to the `_ =>` arm (line 362). The new arm must be inserted as an explicit case.

The match structure becomes:

```
match event:
    "SessionStart" => ...
    "Stop" | "TaskCompleted" => ...
    "Ping" => ...
    "UserPromptSubmit" => ...
    "PreCompact" => ...
    "PostToolUse" => ...

    // NEW: col-022 -- intercept PreToolUse for context_cycle
    "PreToolUse" => build_cycle_event_or_fallthrough(event, session_id, input)

    _ => generic_record_event(event, session_id, input)
```

### New Function: `build_cycle_event_or_fallthrough`

```
fn build_cycle_event_or_fallthrough(
    event: &str,
    session_id: String,
    input: &HookInput,
) -> HookRequest:

    // Step 1: Check if this is a context_cycle tool call
    let tool_name = input.extra
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")

    // Match by "context_cycle" substring in tool_name.
    // Claude Code sends tool_name as "mcp__unimatrix__context_cycle"
    // (server prefix + tool name). Use .contains("context_cycle") to match
    // regardless of prefix format.
    //
    // R-09 mitigation: also verify the prefix contains "unimatrix" to avoid
    // matching a tool from a different MCP server named "context_cycle".
    if !tool_name.contains("context_cycle"):
        return generic_record_event(event, session_id, input)

    // Additional check: must be from our server (contains "unimatrix" in prefix)
    // If tool_name is exactly "context_cycle" (no prefix), allow it (direct MCP call)
    if tool_name != "context_cycle" && !tool_name.contains("unimatrix"):
        return generic_record_event(event, session_id, input)

    // Step 2: Extract parameters from tool_input
    let tool_input = match input.extra.get("tool_input"):
        Some(v) => v
        None =>
            tracing::warn!("context_cycle PreToolUse missing tool_input")
            return generic_record_event(event, session_id, input)

    let type_str = tool_input.get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")

    let topic_str = tool_input.get("topic")
        .and_then(|v| v.as_str())
        .unwrap_or("")

    let keywords_opt: Option<Vec<String>> = tool_input.get("keywords")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter()
            .filter_map(|item| item.as_str().map(|s| s.to_string()))
            .collect())

    // Step 3: Validate using shared function
    let validated = match validate_cycle_params(
        type_str,
        topic_str,
        keywords_opt.as_deref(),
    ):
        Ok(v) => v
        Err(msg) =>
            // Hook must never fail (FR-03.7). Log warning, fall through.
            tracing::warn!(
                error = %msg,
                tool_name = tool_name,
                "context_cycle validation failed in hook, falling through to generic"
            )
            return generic_record_event(event, session_id, input)

    // Step 4: Build specialized RecordEvent
    let event_type = match validated.cycle_type:
        CycleType::Start => CYCLE_START_EVENT.to_string()
        CycleType::Stop  => CYCLE_STOP_EVENT.to_string()

    // Build payload with feature_cycle and keywords
    // The feature_cycle key in payload is what the #198 extraction path and
    // the new cycle_start handler both look for.
    let mut payload = serde_json::json!({
        "feature_cycle": validated.topic,
    })

    if !validated.keywords.is_empty():
        let keywords_json = serde_json::to_string(&validated.keywords)
            .unwrap_or_else(|_| "[]".to_string())
        payload["keywords"] = serde_json::Value::String(keywords_json)

    // Set topic_signal to the topic value -- strong signal for eager attribution
    // as a secondary attribution path.
    let topic_signal = Some(validated.topic.clone())

    HookRequest::RecordEvent {
        event: ImplantEvent {
            event_type,
            session_id,
            timestamp: now_secs(),
            payload,
            topic_signal,
        },
    }
```

### Design Notes

**tool_name matching (R-09)**: Claude Code prefixes MCP tool names with the server name in the format `mcp__{server}__{tool}`. For Unimatrix, this is `mcp__unimatrix__context_cycle`. The `.contains("context_cycle")` check is resilient to prefix format changes. The additional `.contains("unimatrix")` check prevents matching a `context_cycle` tool from a different MCP server.

**Validation failure fallthrough**: When `validate_cycle_params` returns `Err`, the hook falls through to `generic_record_event`. This records the event as a standard observation (with the raw `input.extra` as payload). The #198 extraction may still pick up `feature_cycle` from the raw tool_input if present. This is defense-in-depth.

**No response injection**: The hook handler for PreToolUse does not inject stdout content. The cycle event is fire-and-forget via UDS. The hook process exits with code 0.

**Keywords in payload**: Keywords are serialized as a JSON string within the payload JSON object. The listener deserializes this string to persist to the `keywords` column. This avoids nested JSON array handling in serde_json::Value.

## Error Handling

- Missing `tool_input`: log warn, fall through to generic. No panic.
- Missing `type` or `topic` fields: `validate_cycle_params` returns Err, fall through.
- Malformed `keywords` (not an array, items not strings): `filter_map` skips non-string items, worst case empty vec. No panic.
- `serde_json::to_string` for keywords: infallible for `Vec<String>` in practice; `unwrap_or_else` returns `"[]"` defensively.
- Hook always returns `HookRequest` (never panics, never returns error).

## Key Test Scenarios

1. **PreToolUse with context_cycle start**: input with `tool_name: "mcp__unimatrix__context_cycle"`, `tool_input: {"type":"start","topic":"col-022","keywords":["kw1"]}`. Verify `RecordEvent` with `event_type: "cycle_start"`, payload contains `feature_cycle: "col-022"` and `keywords: "[\"kw1\"]"`, `topic_signal: Some("col-022")`.

2. **PreToolUse with context_cycle stop**: Verify `event_type: "cycle_stop"`, payload contains `feature_cycle`, no keywords required.

3. **PreToolUse with non-cycle tool**: input with `tool_name: "mcp__unimatrix__context_search"`. Verify falls through to `generic_record_event`.

4. **PreToolUse with other server's tool**: `tool_name: "mcp__other_server__context_cycle"`. Verify falls through (does not contain "unimatrix").

5. **PreToolUse with bare tool name**: `tool_name: "context_cycle"` (no prefix). Verify it IS processed (direct call case).

6. **Missing tool_input**: Verify falls through to generic, warn logged.

7. **Invalid type in tool_input**: `type: "restart"`. Verify falls through after validation failure, warn logged.

8. **Empty topic**: Verify falls through after validation failure.

9. **Keywords with non-string items**: `keywords: [1, "valid", null]`. Verify only "valid" is kept.

10. **No keywords field**: Verify `keywords_opt` is None, validated with empty keywords.

11. **Hook exit code**: regardless of path taken, the hook process exits 0.

12. **Event type constants match listener**: verify `CYCLE_START_EVENT` and `CYCLE_STOP_EVENT` are the same constants imported in both hook.rs and listener.rs.
