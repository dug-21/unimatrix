# Test Plan: hook-handler (C2)

**File under test**: `crates/unimatrix-server/src/uds/hook.rs`
**Risks covered**: R-02, R-04 (partial), R-09

## Unit Tests

Follow existing pattern: `test_input()` helper, `build_request()` function tests.

### Tool name matching (R-09 -- High Priority)

```
test_build_request_pretooluse_context_cycle_with_prefix
  Arrange: input.extra = {"tool_name": "mcp__unimatrix__context_cycle", "tool_input": {"type": "start", "topic": "col-022"}}
  Act: build_request("PreToolUse", &input)
  Assert: Returns RecordEvent with event_type containing "cycle_start" (or "cycle_begin")
  Assert: event.payload contains feature_cycle = "col-022"

test_build_request_pretooluse_context_cycle_without_prefix
  Arrange: input.extra = {"tool_name": "context_cycle", "tool_input": {"type": "start", "topic": "col-022"}}
  Act: build_request("PreToolUse", &input)
  Assert: Returns RecordEvent with cycle-specific event_type

test_build_request_pretooluse_wrong_server_prefix
  Arrange: input.extra = {"tool_name": "mcp__other_server__context_cycle", "tool_input": {"type": "start", "topic": "col-022"}}
  Act: build_request("PreToolUse", &input)
  Assert: Falls through to generic RecordEvent (NOT cycle-specific handling)
  Note: Only match "context_cycle" when tool_name ends with it or is prefixed by our server name

test_build_request_pretooluse_context_cycle_substring_no_match
  Arrange: input.extra = {"tool_name": "my_context_cycle_thing", ...}
  Act: build_request("PreToolUse", &input)
  Assert: Does NOT match cycle handler (substring false positive prevention)
```

### Cycle start event construction

```
test_build_request_cycle_start_event_type
  Arrange: Valid context_cycle start PreToolUse
  Act: build_request("PreToolUse", &input)
  Assert: event.event_type == "cycle_start" (exact constant match)
  Assert: event.topic_signal == Some("col-022")

test_build_request_cycle_start_with_keywords
  Arrange: PreToolUse with tool_input: {"type":"start","topic":"col-022","keywords":["a","b"]}
  Act: build_request("PreToolUse", &input)
  Assert: event.payload["keywords"] == ["a","b"]
  Assert: event.payload["feature_cycle"] == "col-022"

test_build_request_cycle_stop_event_type
  Arrange: Valid context_cycle stop PreToolUse
  Act: build_request("PreToolUse", &input)
  Assert: event.event_type == "cycle_stop" (or "cycle_end")
```

### Validation failure graceful fallthrough (R-02 -- High Priority)

```
test_build_request_cycle_invalid_type_falls_through
  Arrange: tool_input: {"type":"pause","topic":"col-022"}
  Act: build_request("PreToolUse", &input)
  Assert: Returns generic RecordEvent (not cycle-specific)
  Assert: No panic

test_build_request_cycle_missing_topic_falls_through
  Arrange: tool_input: {"type":"start"}  (missing topic)
  Act: build_request("PreToolUse", &input)
  Assert: Returns generic RecordEvent (fallthrough)

test_build_request_cycle_malformed_tool_input_falls_through
  Arrange: tool_input: "not-an-object" or null
  Act: build_request("PreToolUse", &input)
  Assert: Returns generic RecordEvent
  Assert: No panic (R-02: hook must never crash)

test_build_request_cycle_missing_tool_input_key_falls_through
  Arrange: input.extra = {"tool_name": "context_cycle"}  (no tool_input key)
  Act: build_request("PreToolUse", &input)
  Assert: Returns generic RecordEvent

test_build_request_cycle_topic_too_long_falls_through
  Arrange: tool_input with topic = "a".repeat(129)
  Act: build_request("PreToolUse", &input)
  Assert: Falls through to generic handler (validate_cycle_params rejects)
```

### Session ID propagation

```
test_build_request_cycle_preserves_session_id
  Arrange: input.session_id = Some("sess-42"), valid context_cycle start
  Act: build_request("PreToolUse", &input)
  Assert: event.session_id == "sess-42"

test_build_request_cycle_no_session_id
  Arrange: input.session_id = None, valid context_cycle
  Act: build_request("PreToolUse", &input)
  Assert: event.session_id == "" (or default empty, matching existing convention)
```

## Edge Cases

- PreToolUse for other tools (e.g., "context_store") must NOT trigger cycle handler
- tool_input with extra unexpected fields should not cause deserialization failure
- Unicode in topic field passed to validate_cycle_params (validation handles it)
