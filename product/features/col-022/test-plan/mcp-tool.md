# Test Plan: mcp-tool (C1)

**File under test**: `crates/unimatrix-server/src/mcp/tools.rs`
**Risks covered**: R-01 (partial), R-08

## Unit Tests

### CycleParams deserialization

Follow existing pattern in tools.rs tests (test_search_params_deserialize, etc.).

```
test_cycle_params_deserialize_start
  Input: {"type": "start", "topic": "col-022"}
  Assert: params.r#type == "start", params.topic == "col-022", params.keywords.is_none()

test_cycle_params_deserialize_with_keywords
  Input: {"type": "start", "topic": "col-022", "keywords": ["attr", "lifecycle"]}
  Assert: params.keywords == Some(vec!["attr", "lifecycle"])

test_cycle_params_deserialize_stop
  Input: {"type": "stop", "topic": "col-022"}
  Assert: params.r#type == "stop"

test_cycle_params_missing_required_type
  Input: {"topic": "col-022"}
  Assert: serde_json::from_str::<CycleParams> returns Err

test_cycle_params_missing_required_topic
  Input: {"type": "start"}
  Assert: serde_json::from_str::<CycleParams> returns Err

test_cycle_params_extra_fields_ignored
  Input: {"type": "start", "topic": "col-022", "unknown": true}
  Assert: deserialization succeeds, extra field silently ignored

test_cycle_params_keywords_empty_array
  Input: {"type": "start", "topic": "col-022", "keywords": []}
  Assert: params.keywords == Some(vec![])

test_cycle_params_keywords_null_vs_absent
  Input A: {"type": "start", "topic": "col-022", "keywords": null}
  Input B: {"type": "start", "topic": "col-022"}
  Assert: both result in params.keywords == None
```

### Response format (R-08)

The MCP tool returns acknowledgment only, not attribution confirmation. The response must NOT contain `was_set` (Variance 2 resolution).

```
test_context_cycle_start_response_is_acknowledgment
  Arrange: Valid CycleParams with type="start"
  Act: Call context_cycle handler (requires mock server or direct function call)
  Assert: Response text contains "noted" or "acknowledged" or "cycle_started"
  Assert: Response text does NOT contain "was_set"

test_context_cycle_stop_response_is_acknowledgment
  Arrange: Valid CycleParams with type="stop"
  Act: Call context_cycle handler
  Assert: Response text contains "cycle_stopped" or equivalent acknowledgment

test_context_cycle_validation_error_response
  Arrange: CycleParams with type="pause" (invalid)
  Act: Call context_cycle handler
  Assert: Response is error with descriptive message mentioning "start" or "stop"
```

### Capability check

```
test_context_cycle_requires_session_write
  Assert: context_cycle tool handler checks for SessionWrite capability
  Note: If tool registration uses the same capability-check pattern as context_store,
        verify the capability annotation or guard exists. This may be a code inspection
        rather than a runtime test, depending on the tool registration pattern.
```

## Integration Tests

R-08 integration: Verify through infra-001 harness that response format is correct (see OVERVIEW.md new integration tests).

## Edge Cases

- `type` field is a reserved keyword in Rust (`r#type`). Verify serde rename or raw identifier works with JSON key `"type"`.
- Keywords with zero-length strings: `["", "valid"]` -- should the empty string be accepted or rejected? Validation owns this (see shared-validation.md).
