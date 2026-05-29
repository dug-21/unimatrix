# Test Plan: compact-payload-wire

Component: `crates/unimatrix-engine/src/wire.rs` — CompactPayload transcript_excerpt field addition

Covers: AC-09, R-07

## Unit Tests

Location: `crates/unimatrix-engine/src/wire.rs` (extend existing `#[cfg(test)]` module)

### test_compact_payload_with_transcript_excerpt_round_trip

Arrange: Construct `HookRequest::CompactPayload` with `transcript_excerpt: Some("excerpt text".to_string())` and all other fields populated.
Act: `serialize_request(&req)` then `deserialize_request(&bytes)`.
Assert:
- Deserialized variant is `CompactPayload`
- `transcript_excerpt == Some("excerpt text")`
- All other fields unchanged

### test_compact_payload_without_transcript_excerpt_defaults_to_none

Arrange: JSON string `{"type":"CompactPayload","session_id":"s1","injected_entry_ids":[],"role":null,"feature":null,"token_limit":null}` — no `transcript_excerpt` key.
Act: `serde_json::from_str::<HookRequest>(&json)`
Assert:
- Deserialized successfully (no error)
- `transcript_excerpt == None`

### test_compact_payload_none_transcript_excerpt_omitted_from_json

Arrange: Construct `HookRequest::CompactPayload` with `transcript_excerpt: None`.
Act: `serde_json::to_string(&req)`
Assert:
- Output JSON does NOT contain the string `"transcript_excerpt"`
- Confirms `skip_serializing_if = "Option::is_none"` works

### test_compact_payload_transcript_excerpt_null_deserializes_to_none

Arrange: JSON string with `"transcript_excerpt": null`.
Act: `serde_json::from_str::<HookRequest>(&json)`
Assert:
- `transcript_excerpt == None`

### test_compact_payload_transcript_excerpt_empty_string

Arrange: JSON with `"transcript_excerpt": ""`.
Act: `serde_json::from_str::<HookRequest>(&json)`
Assert:
- `transcript_excerpt == Some("")` (empty string is valid, not None)

## Edge Cases

### test_compact_payload_backward_compat_existing_round_trip_unchanged

Arrange: Rerun existing `round_trip_compact_payload` test.
Assert: Still passes after field addition — confirms no regression on existing tests.

## Risk Trace

| Risk | Scenario | Test |
|------|----------|------|
| R-07 | Missing serde(default) breaks existing callers | test_compact_payload_without_transcript_excerpt_defaults_to_none |
| R-07 | Missing skip_serializing_if leaks field to old consumers | test_compact_payload_none_transcript_excerpt_omitted_from_json |
| R-07 | null value handling | test_compact_payload_transcript_excerpt_null_deserializes_to_none |
