# col-019: Specification — PostToolUse Response Capture

## Domain Model

### Observation Record (existing, unchanged)
```
ObservationRow {
    session_id: String,       -- Claude Code session UUID
    ts_millis: i64,           -- event timestamp in milliseconds
    hook: String,             -- "PostToolUse" (normalized from "post_tool_use_rework_candidate")
    tool: Option<String>,     -- tool name (Bash, Edit, Read, etc.)
    input: Option<String>,    -- serialized tool_input JSON
    response_size: Option<i64>,    -- byte length of serialized tool_response
    response_snippet: Option<String>, -- first 500 chars of serialized tool_response
}
```

### Claude Code PostToolUse Input (external, read-only)
```
HookInput {
    session_id: String,
    hook_event_name: "PostToolUse",
    tool_name: String,           -- in extra
    tool_input: Object,          -- in extra
    tool_response: Object|null,  -- in extra (THE KEY FIELD)
    exit_code: Option<i64>,      -- in extra (Bash only)
    interrupted: Option<bool>,   -- in extra (Bash only)
}
```

### Response Field Computation
```
Given tool_response: Option<serde_json::Value>:
  if tool_response is Some(value) and value is not Null:
    serialized = serde_json::to_string(value)
    response_size = serialized.len() as i64
    response_snippet = serialized[..min(500, serialized.len())]
  else:
    response_size = None
    response_snippet = None
```

## Functional Requirements

### FR-01: Response Field Extraction from tool_response

`extract_observation_fields()` MUST compute `response_size` and `response_snippet` from the `tool_response` field in the event payload for all PostToolUse events.

- `response_size` = byte length of `serde_json::to_string(tool_response)`
- `response_snippet` = first 500 characters of the serialized string (char boundary safe)
- If `tool_response` is absent or null: both fields are None

### FR-02: Legacy Field Name Fallback

For backward compatibility with any events that may contain `response_size` and `response_snippet` as direct fields (e.g., test fixtures, future changes), `extract_observation_fields()` SHOULD check for legacy field names when `tool_response` is absent.

### FR-03: Rework Payload Enhancement

`build_request()` in hook.rs MUST include `tool_input` and `tool_response` from the Claude Code input in the `post_tool_use_rework_candidate` payload, alongside existing `tool_name`, `file_path`, and `had_failure` fields.

### FR-04: Rework Observation Persistence

The `post_tool_use_rework_candidate` handler in `dispatch_request()` MUST persist the event as an observation in the observations table, in addition to recording the rework event in session_registry.

- The observation hook type MUST be stored as `"PostToolUse"` (not `"post_tool_use_rework_candidate"`) for consistency with the observation read path.
- The observation write MUST be fire-and-forget via `spawn_blocking`.
- The rework recording MUST happen before the observation write.

### FR-05: MultiEdit Batch Observation Persistence

The `RecordEvents` handler for MultiEdit batches MUST persist each event as an observation, in addition to existing rework recording.

- Each event in the batch becomes a separate observation row.
- All observations in the batch use hook type `"PostToolUse"`.

### FR-06: Existing Rework Detection Unchanged

All existing rework detection behavior MUST be preserved:
- `post_tool_use_rework_candidate` events still match the rework handler first.
- `session_registry.record_rework_event()` receives the same ReworkEvent struct.
- had_failure detection for Bash (exit_code, interrupted) is unchanged.
- file_path extraction for Edit/Write/MultiEdit is unchanged.

## Non-Functional Requirements

### NFR-01: Hook Latency Budget

No additional serialization or computation in the hook binary's critical path. The only hook-side change is adding `tool_input` and `tool_response` to the rework payload, which is a copy of existing JSON values (O(1) serde_json::Value clone).

### NFR-02: Fire-and-Forget Observation Writes

All observation writes MUST use the existing fire-and-forget pattern: `tokio::task::spawn_blocking` with error logging. Observation write failures MUST NOT affect the hook response.

### NFR-03: Snippet Character Safety

The 500-character snippet truncation MUST respect UTF-8 character boundaries. Use `.chars().take(500).collect::<String>()` or equivalent, not byte slicing.

## Acceptance Criteria

### AC-01: Non-Rework PostToolUse Response Capture
**Given** a PostToolUse event for a non-rework tool (e.g., Read) with a `tool_response` object in the payload
**When** the event is processed by extract_observation_fields()
**Then** `response_size` equals the byte length of the serialized tool_response JSON AND `response_snippet` equals the first 500 characters of the serialized JSON

### AC-02: Rework PostToolUse Observation Persistence
**Given** a PostToolUse event for a rework-eligible tool (e.g., Edit) processed by build_request()
**When** the resulting post_tool_use_rework_candidate event is dispatched
**Then** (a) the rework event is recorded in session_registry AND (b) an observation row is inserted into the observations table with hook="PostToolUse", non-NULL response_size, and non-NULL response_snippet

### AC-03: MultiEdit Batch Observation Persistence
**Given** a PostToolUse event for MultiEdit with 2+ edits
**When** the resulting RecordEvents batch is dispatched
**Then** each edit produces a separate observation row with hook="PostToolUse" and the shared tool_response data

### AC-04: Missing tool_response Handling
**Given** a PostToolUse event where tool_response is absent or null
**When** the event is processed
**Then** response_size and response_snippet are NULL (not zero, not empty string)

### AC-05: Large Response Truncation
**Given** a PostToolUse event where the serialized tool_response exceeds 500 characters
**When** the event is processed
**Then** response_snippet is exactly 500 characters (at a valid UTF-8 boundary) AND response_size reflects the full serialized byte length

### AC-06: Rework Detection Preserved
**Given** a PostToolUse event for Bash with exit_code=1
**When** processed through the full pipeline
**Then** the ReworkEvent recorded in session_registry has had_failure=true (unchanged from current behavior)

### AC-07: Hook Type Normalization
**Given** a post_tool_use_rework_candidate event persisted as an observation
**When** the observation row is read from the database
**Then** the hook column contains "PostToolUse" (not "post_tool_use_rework_candidate")

### AC-08: Existing Tests Pass
**When** the full test suite is run
**Then** all existing tests in hook.rs (8 rework tests) and listener.rs pass without modification

## Test Plan

### Unit Tests (hook.rs)

1. `posttooluse_rework_payload_includes_tool_response` -- Verify that a rework-eligible PostToolUse event's payload includes tool_input and tool_response.
2. `posttooluse_rework_payload_missing_tool_response` -- Verify graceful handling when tool_response is absent.
3. `posttooluse_multiedit_payload_includes_tool_response` -- Verify MultiEdit batch events include tool_response.

### Unit Tests (listener.rs)

4. `extract_observation_fields_posttooluse_tool_response` -- Verify response_size and response_snippet computed from tool_response.
5. `extract_observation_fields_posttooluse_missing_tool_response` -- Verify None/None when tool_response absent.
6. `extract_observation_fields_posttooluse_null_tool_response` -- Verify None/None when tool_response is JSON null.
7. `extract_observation_fields_posttooluse_large_response_truncated` -- Verify 500-char truncation.
8. `extract_observation_fields_posttooluse_legacy_fields_fallback` -- Verify legacy response_size/response_snippet fields still work.
9. `extract_observation_fields_rework_candidate_as_posttooluse` -- Verify post_tool_use_rework_candidate events are extracted with hook="PostToolUse" and correct response fields.

### Integration Tests

10. Verify that rework-eligible PostToolUse events produce both a rework event in session_registry AND an observation row in the database.

## Constraints

- No schema changes to the observations table
- No changes to the observation read path (SqlObservationSource)
- No changes to detection rules, metrics computation, or extraction rules
- No backfill of historical NULL rows
- Snippet truncation at 500 characters (matching the original JSONL pipeline's behavior)
