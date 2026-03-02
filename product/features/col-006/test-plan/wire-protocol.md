# Test Plan: wire-protocol

## Risks Covered

| Risk | Severity | Test Coverage |
|------|----------|--------------|
| R-07 | High | Framing errors: partial header, partial payload, broken pipe |
| R-08 | High | Oversized payload rejection (> 1 MiB) |
| R-09 | Medium | Malformed JSON: empty object, unknown type, missing fields, invalid JSON, non-object |
| R-12 | Medium | HookInput defensive parsing |

## Unit Tests

Location: `crates/unimatrix-engine/src/wire.rs` (within `#[cfg(test)]` module)

### Framing (R-07)

1. **test_write_read_frame_roundtrip**: Write a frame with known payload, read it back. Assert payload matches.

2. **test_write_frame_empty_payload_accepted**: Write a 0-byte payload. `write_frame` succeeds (the function does not reject empty; `read_frame` rejects empty).

3. **test_read_frame_partial_header**: Provide a reader with only 2 bytes, then EOF. Assert `TransportError::Transport` with "connection closed" message.

4. **test_read_frame_partial_payload**: Write a 4-byte header claiming 100 bytes, but provide only 50 bytes then EOF. Assert `TransportError::Transport`.

5. **test_read_frame_zero_length**: Write a 4-byte header with value 0. Assert `TransportError::Codec` with "empty payload".

6. **test_write_frame_oversized_rejected** (R-08): Attempt to write a payload of `MAX_PAYLOAD_SIZE + 1` bytes. Assert `io::Error` with `InvalidInput`.

7. **test_read_frame_oversized_rejected** (R-08): Write a 4-byte header claiming `MAX_PAYLOAD_SIZE + 1` bytes. Assert `TransportError::Codec` with "exceeds max".

8. **test_read_frame_max_size_boundary**: Write a header claiming exactly `MAX_PAYLOAD_SIZE` bytes with valid payload. Assert success.

9. **test_read_frame_u32_max**: Write a 4-byte header with value `u32::MAX`. Assert `TransportError::Codec`.

### Serialization Round-trips

10. **test_serialize_deserialize_ping**: Round-trip `HookRequest::Ping`. Assert match.

11. **test_serialize_deserialize_session_register**: Round-trip with all fields. Assert all fields preserved.

12. **test_serialize_deserialize_session_close**: Round-trip with all fields.

13. **test_serialize_deserialize_record_event**: Round-trip with ImplantEvent containing nested JSON payload.

14. **test_serialize_deserialize_pong**: Round-trip `HookResponse::Pong` with version string.

15. **test_serialize_deserialize_ack**: Round-trip `HookResponse::Ack`.

16. **test_serialize_deserialize_error**: Round-trip `HookResponse::Error` with code and message.

### Malformed JSON Deserialization (R-09)

17. **test_deserialize_empty_object**: `{}` -> deserialization fails (missing `type` tag). Assert `TransportError::Codec`.

18. **test_deserialize_unknown_type_tag**: `{"type":"FutureVariant"}` -> deserialization fails. Assert error contains variant name.

19. **test_deserialize_missing_required_field**: `{"type":"SessionRegister"}` without `session_id`. Verify behavior: either serde default fills it or deserialization fails (depends on whether `session_id` has `#[serde(default)]` on HookRequest -- it does NOT, it is required. So this should fail).

20. **test_deserialize_json_array**: `[1,2,3]` -> deserialization fails. Assert Codec error.

21. **test_deserialize_invalid_json**: `{broken` -> deserialization fails. Assert Codec error.

22. **test_deserialize_non_utf8**: Bytes `[0xFF, 0xFE]` -> deserialization fails.

### HookInput Defensive Parsing (R-12)

23. **test_hook_input_minimal**: `{"hook_event_name":"SessionStart"}` -> parses successfully. `session_id` is `None`, `cwd` is `None`.

24. **test_hook_input_full**: All known fields populated -> all fields parsed correctly.

25. **test_hook_input_unknown_fields**: `{"hook_event_name":"Ping","new_field":42}` -> parses without error. `extra` contains `{"new_field":42}`.

26. **test_hook_input_empty_json**: `{}` -> parses with `hook_event_name` defaulting to empty string.

27. **test_hook_input_missing_event_name**: `{"session_id":"abc"}` -> `hook_event_name` defaults to empty string.

28. **test_hook_input_session_id_as_integer**: `{"hook_event_name":"X","session_id":123}` -> `session_id` defaults to `None` (type mismatch silently handled by serde default).

29. **test_hook_input_not_json**: `"hello world"` -> deserialization fails (not an object).

### Error Code Constants

30. **test_error_code_values**: Verify `ERR_UID_MISMATCH == -32001`, `ERR_LINEAGE_FAILED == -32002`, etc.

## Edge Cases

- Trailing bytes after JSON payload in a frame: ignored (connection closes after one request)
- Two requests on the same connection (pipelining): server processes only the first per the single-request-per-connection model -- tested in uds-listener
- JSON with trailing whitespace or newlines: serde handles this (standard behavior)

## Assertions

All assertions use exact equality for serialized/deserialized round-trips. Error assertions check variant type and relevant error message content.
