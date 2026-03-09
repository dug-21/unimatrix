# col-019: Implementation Brief

## Summary

Fix PostToolUse observation pipeline to populate `response_size` and `response_snippet` columns. Two changes: (1) compute response fields from Claude Code's `tool_response` JSON object in `extract_observation_fields()`, and (2) add observation persistence for rework-intercepted PostToolUse events while preserving col-017 topic signal accumulation.

## Implementation Steps

### Step 1: Add extract_response_fields() Helper (listener.rs)

Create a new function that computes response_size and response_snippet from a tool_response JSON value.

**File**: `crates/unimatrix-server/src/uds/listener.rs`
**Location**: After `extract_observation_fields()` (after line 1856)

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

Replace the direct field lookups with the new helper and add support for rework candidate events.

**File**: `crates/unimatrix-server/src/uds/listener.rs`
**Location**: Lines 1817-1828 in `extract_observation_fields()`

**Before** (current code on main):
```rust
"PostToolUse" => {
    let tool = event.payload.get("tool_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let input = event.payload.get("tool_input")
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let rs = event.payload.get("response_size")
        .and_then(|v| v.as_i64());
    let rsnip = event.payload.get("response_snippet")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
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

Also add hook type normalization after the match block. Before constructing ObservationRow:

```rust
// Normalize rework candidate hook type to PostToolUse for observation consistency (col-019)
let hook = if hook == "post_tool_use_rework_candidate" {
    "PostToolUse".to_string()
} else {
    hook
};
```

### Step 3: Enhance Rework Candidate Payload (hook.rs)

Add `tool_input` and `tool_response` to the rework candidate payload construction.

**File**: `crates/unimatrix-server/src/uds/hook.rs`

**3a: Single-tool rework (Bash, Edit, Write) -- line 348**

**Before** (current payload construction):
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

**3b: MultiEdit batch -- line 324**

**Before** (current per-event payload):
```rust
payload: serde_json::json!({
    "tool_name": "MultiEdit",
    "file_path": file_path,
    "had_failure": had_failure,
}),
```

**After**:
```rust
payload: serde_json::json!({
    "tool_name": "MultiEdit",
    "file_path": file_path,
    "had_failure": had_failure,
    "tool_input": input.extra.get("tool_input"),
    "tool_response": input.extra.get("tool_response"),
}),
```

Note: `input.extra.get("tool_input")` and `input.extra.get("tool_response")` return `Option<&Value>`. When serialized into a `serde_json::json!()` macro, `None` becomes `null`, which is correct -- `extract_response_fields()` handles null gracefully.

### Step 4: Add Observation Persistence in Rework Handler (listener.rs)

In the `post_tool_use_rework_candidate` handler (line 522), add observation write AFTER the existing synchronous operations.

**File**: `crates/unimatrix-server/src/uds/listener.rs`
**Location**: After topic signal accumulation (line 563), before `HookResponse::Ack` (line 566)

**Current code** (lines 555-566):
```rust
session_registry.record_rework_event(&event.session_id, rework_event);

// col-017: Accumulate topic signal from rework candidate events
if let Some(ref signal) = event.topic_signal {
    session_registry.record_topic_signal(
        &event.session_id,
        signal.clone(),
        event.timestamp,
    );
}

HookResponse::Ack
```

**After** (insert between topic signal block and HookResponse::Ack):
```rust
session_registry.record_rework_event(&event.session_id, rework_event);

// col-017: Accumulate topic signal from rework candidate events
if let Some(ref signal) = event.topic_signal {
    session_registry.record_topic_signal(
        &event.session_id,
        signal.clone(),
        event.timestamp,
    );
}

// col-019: Persist rework PostToolUse as observation (fire-and-forget)
let store_for_obs = Arc::clone(store);
let obs = extract_observation_fields(&event);
tokio::task::spawn_blocking(move || {
    if let Err(e) = insert_observation(&store_for_obs, &obs) {
        tracing::error!(error = %e, "rework observation write failed");
    }
});

HookResponse::Ack
```

### Step 5: Add Unit Tests

**File**: `crates/unimatrix-server/src/uds/listener.rs` (test module)

Tests for `extract_response_fields()`:
- Normal tool_response object -> correct size and snippet
- Missing tool_response -> (None, None)
- Null tool_response -> (None, None)
- Empty object tool_response -> (Some(2), Some("{}"))
- Large response (>500 chars) -> truncation at char boundary
- Legacy field name fallback
- Multi-byte UTF-8 -> char-safe truncation

Tests for `extract_observation_fields()` with rework candidates:
- Rework candidate event type -> normalized hook="PostToolUse"
- Rework candidate with tool_response -> correct response fields
- Rework candidate with topic_signal -> preserved in ObservationRow

**File**: `crates/unimatrix-server/src/uds/hook.rs` (test module)

Tests for enhanced rework payloads:
- Edit PostToolUse payload includes tool_input and tool_response
- Bash PostToolUse payload includes tool_input and tool_response
- MultiEdit batch payload includes tool_input and tool_response
- Missing tool_response in input -> null in payload

### Step 6: Run Full Test Suite

Verify all existing tests pass, including rework tests in hook.rs and all listener.rs tests.

## File Change Summary

| File | Change Type | Lines (est.) |
|------|------------|-------------|
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | ~30 (new helper + match arm update + normalization + rework persistence) |
| `crates/unimatrix-server/src/uds/hook.rs` | Modify | ~10 (payload enhancement for rework candidates) |
| `crates/unimatrix-server/src/uds/listener.rs` | Add tests | ~100 |
| `crates/unimatrix-server/src/uds/hook.rs` | Add tests | ~50 |

**Total**: ~190 lines across 2 files.

## Dependencies

None. No new crate dependencies. All changes use existing serde_json, tokio, and Arc.

## Risks to Watch During Implementation

1. **R-01 (HIGH)**: Verify all existing rework tests pass after hook.rs payload changes. The payload grows but rework field extraction is unaffected (extra JSON fields are ignored).
2. **R-02 (MEDIUM)**: Test with diverse tool_response shapes. The serialization-based approach handles all cases, but verify with real-world examples.
3. **R-03 (MEDIUM)**: Verify topic signal accumulation in the rework handler still works after adding observation persistence. The observation write is async (spawn_blocking) and cannot interfere with the synchronous topic signal path.
4. **MultiEdit rework gap**: MultiEdit events go through RecordEvents, which never calls session_registry.record_rework_event(). This is a pre-existing issue, not in col-019 scope. Document for future fix.
