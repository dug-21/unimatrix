# col-019: Implementation Brief

## Summary

Fix PostToolUse observation pipeline to populate `response_size` and `response_snippet` columns. Two changes: (1) compute response fields from Claude Code's `tool_response` JSON object in `extract_observation_fields()`, and (2) add observation persistence for rework-intercepted PostToolUse events.

## Implementation Steps

### Step 1: Add extract_response_fields() Helper (listener.rs)

Create a new function that computes response_size and response_snippet from a tool_response JSON value.

**File**: `crates/unimatrix-server/src/uds/listener.rs`

```rust
/// Extract response_size and response_snippet from a PostToolUse event payload.
///
/// Tries `tool_response` first (Claude Code's field name), then falls back to
/// legacy `response_size`/`response_snippet` fields for backward compatibility.
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

    // Fallback: legacy field names (test fixtures, future compatibility)
    let rs = payload.get("response_size").and_then(|v| v.as_i64());
    let rsnip = payload
        .get("response_snippet")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    (rs, rsnip)
}
```

### Step 2: Update PostToolUse Branch in extract_observation_fields() (listener.rs)

Replace the direct field lookups with the new helper.

**File**: `crates/unimatrix-server/src/uds/listener.rs`, lines 1604-1615

**Before**:
```rust
"PostToolUse" => {
    let tool = event.payload.get("tool_name")...;
    let input = event.payload.get("tool_input")...;
    let rs = event.payload.get("response_size").and_then(|v| v.as_i64());
    let rsnip = event.payload.get("response_snippet").and_then(|v| v.as_str()).map(|s| s.to_string());
    (tool, input, rs, rsnip)
}
```

**After**:
```rust
"PostToolUse" | "post_tool_use_rework_candidate" => {
    let tool = event.payload.get("tool_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let input = event.payload.get("tool_input")
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let (rs, rsnip) = extract_response_fields(&event.payload);
    (tool, input, rs, rsnip)
}
```

Note: The hook column for rework candidates must be normalized to "PostToolUse". Add override after the match block:
```rust
// Normalize rework candidate events to PostToolUse for observation consistency
let hook = if hook == "post_tool_use_rework_candidate" {
    "PostToolUse".to_string()
} else {
    hook
};
```

### Step 3: Enhance Rework Candidate Payload (hook.rs)

Add `tool_input` and `tool_response` to the rework candidate payload construction.

**File**: `crates/unimatrix-server/src/uds/hook.rs`

For single-tool rework (Bash, Edit, Write), update the payload at lines 266-277:

**Before**:
```rust
payload: serde_json::json!({
    "tool_name": tool_name,
    "file_path": file_path,
    "had_failure": had_failure,
}),
```

**After**:
```rust
payload: serde_json::json!({
    "tool_name": tool_name,
    "file_path": file_path,
    "had_failure": had_failure,
    "tool_input": input.extra.get("tool_input"),
    "tool_response": input.extra.get("tool_response"),
}),
```

For MultiEdit batch (lines 244-253), add the same fields to each event payload.

### Step 4: Add Observation Persistence in Rework Handler (listener.rs)

In the `post_tool_use_rework_candidate` handler (lines 522-557), add observation write after rework recording.

**After** `session_registry.record_rework_event(&event.session_id, rework_event);` (line 555), add:

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

### Step 5: Add Observation Persistence in RecordEvents Handler (listener.rs)

In the `RecordEvents` handler (lines 584-603), the events are currently ALL treated as rework candidates (MultiEdit only). Add observation batch persistence.

The existing code at line 595 already creates `obs_batch` and writes them. But the rework candidate events in RecordEvents are matched by the RecordEvents handler directly, which already does batch observation writes. Check: does the RecordEvents handler already call `extract_observation_fields`?

Looking at the code (lines 594-600):
```rust
let obs_batch: Vec<ObservationRow> = events.iter().map(extract_observation_fields).collect();
tokio::task::spawn_blocking(move || {
    if let Err(e) = insert_observations_batch(&store_for_obs, &obs_batch) {
        tracing::error!(error = %e, "batch observation write failed");
    }
});
```

Wait -- the RecordEvents handler DOES already call extract_observation_fields and insert_observations_batch. But it handles ALL RecordEvents, not just rework candidates. The issue is that for MultiEdit rework candidates, they are routed via RecordEvents and DO go through the batch observation write -- but `extract_observation_fields()` doesn't handle the `post_tool_use_rework_candidate` event_type. Step 2 above fixes this by adding the match arm.

However, looking more carefully: the RecordEvents handler at line 584 matches ALL RecordEvents batches, not just rework candidates. There is no separate rework handler for RecordEvents. So MultiEdit rework events DO get persisted as observations -- but with the wrong event type and NULL response fields (because extract_observation_fields falls through to the default `_ =>` arm for "post_tool_use_rework_candidate").

So Step 2's fix (adding `"post_tool_use_rework_candidate"` to the match arm + hook normalization) is sufficient for MultiEdit too. The RecordEvents handler already persists observations. No additional change needed in Step 5.

**Actually, wait**: Re-reading the dispatch_request flow. The `post_tool_use_rework_candidate` handler at line 522 matches `HookRequest::RecordEvent { ref event }` with the guard. RecordEvents is `HookRequest::RecordEvents { events }` -- a different variant. MultiEdit goes through `build_request()` which returns `HookRequest::RecordEvents { events }`. This means MultiEdit events go to the RecordEvents handler (line 584), NOT the rework handler (line 522). The RecordEvents handler does NOT call `session_registry.record_rework_event()` -- it just writes observations.

This means MultiEdit rework detection is currently broken? Let me re-check...

Actually, looking at the RecordEvents handler more carefully: it just logs and writes observations. It does NOT do rework recording. But MultiEdit is supposed to produce rework candidates. Let me trace this:

1. `build_request("PostToolUse", &input)` for MultiEdit with non-empty edits returns `HookRequest::RecordEvents { events }` where each event has `event_type: "post_tool_use_rework_candidate"`.
2. In dispatch_request, `HookRequest::RecordEvents { events }` matches the RecordEvents handler at line 584.
3. The RecordEvents handler calls `extract_observation_fields` on each event and writes to observations table.
4. The rework handler at line 522 NEVER sees these events because they're in a RecordEvents, not a RecordEvent.

So MultiEdit rework recording via session_registry was never implemented. That is a pre-existing issue, not something col-019 needs to fix. But col-019 DOES need to ensure MultiEdit PostToolUse events get proper response data in their observation rows. Step 2's fix handles this.

**Revised Step 5**: No additional code needed for RecordEvents. Step 2's match arm fix ensures `extract_observation_fields()` correctly handles `post_tool_use_rework_candidate` events in the existing RecordEvents batch write path.

### Step 6: Add Unit Tests

**File**: `crates/unimatrix-server/src/uds/listener.rs` (test module)

Add tests for `extract_response_fields()` and the updated `extract_observation_fields()`:
- Normal tool_response object
- Missing tool_response
- Null tool_response
- Empty object tool_response
- Large response (>500 chars) truncation
- Legacy field name fallback
- Rework candidate event type -> normalized to PostToolUse

**File**: `crates/unimatrix-server/src/uds/hook.rs` (test module)

Add tests for enhanced rework payloads:
- Edit PostToolUse payload includes tool_input and tool_response
- Bash PostToolUse payload includes tool_input and tool_response
- Missing tool_response in input -> null in payload

### Step 7: Run Full Test Suite

Verify all existing tests pass, including the 8 rework tests in hook.rs.

## File Change Summary

| File | Change Type | Lines (est.) |
|------|------------|-------------|
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | ~40 (new helper + match arm update + rework persistence) |
| `crates/unimatrix-server/src/uds/hook.rs` | Modify | ~15 (payload enhancement for rework candidates) |
| `crates/unimatrix-server/src/uds/listener.rs` | Add tests | ~80 |
| `crates/unimatrix-server/src/uds/hook.rs` | Add tests | ~40 |

**Total**: ~175 lines across 2 files.

## Dependencies

None. No new crate dependencies. All changes use existing serde_json, tokio, and Arc.

## Risks to Watch During Implementation

1. **R-01 (HIGH)**: Verify all 8 existing rework tests pass after hook.rs payload changes. The payload grows but rework field extraction should be unaffected.
2. **R-02 (MEDIUM)**: Test with diverse tool_response shapes. The serialization-based approach should handle all cases, but verify with real-world examples.
3. **MultiEdit rework gap**: MultiEdit events go through RecordEvents, which never calls session_registry.record_rework_event(). This is a pre-existing issue, not in col-019 scope. Document for future fix.
