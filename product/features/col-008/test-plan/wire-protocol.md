# Test Plan: wire-protocol

## Component Scope

Changes to `crates/unimatrix-engine/src/wire.rs`: remove dead_code attrs, add session_id to ContextSearch.

## Risk Coverage

| Risk | Test |
|------|------|
| R-09 (wire backward incompatibility) | Backward compat deserialize tests |

## Unit Tests

### Existing Tests (verify unchanged)

- `round_trip_context_search` -- must still pass with new field

### New Tests

#### test_context_search_with_session_id
```
Arrange: HookRequest::ContextSearch { query: "test", session_id: Some("sess-1"), ... }
Act: serialize_request -> deserialize_request
Assert: decoded session_id == Some("sess-1")
```

#### test_context_search_without_session_id
```
Arrange: HookRequest::ContextSearch { query: "test", session_id: None, ... }
Act: serialize_request -> deserialize_request
Assert: decoded session_id == None
```

#### test_context_search_missing_session_id_field_defaults_none
```
Arrange: JSON string without session_id field: {"type":"ContextSearch","query":"test"}
Act: deserialize_request
Assert: decoded session_id == None
```

#### test_compact_payload_round_trip
```
Arrange: HookRequest::CompactPayload { session_id: "s1", injected_entry_ids: vec![1,2,3], role: Some("dev"), feature: None, token_limit: Some(500) }
Act: serialize_request -> deserialize_request
Assert: all fields match
```

#### test_briefing_content_round_trip
```
Arrange: HookResponse::BriefingContent { content: "test content", token_count: 25 }
Act: serialize_response -> deserialize_response
Assert: content == "test content", token_count == 25
```

#### test_compact_payload_empty_entry_ids
```
Arrange: HookRequest::CompactPayload { session_id: "s1", injected_entry_ids: vec![], ... }
Act: serialize -> deserialize
Assert: injected_entry_ids is empty
```

#### test_briefing_content_empty
```
Arrange: HookResponse::BriefingContent { content: "", token_count: 0 }
Act: serialize -> deserialize
Assert: content is empty, token_count is 0
```

## Edge Cases

- Empty session_id string in ContextSearch (Some("")) -- valid, deserialized as Some("")
- Large injected_entry_ids list (1000 entries) -- valid, serialized normally
