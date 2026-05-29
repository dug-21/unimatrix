# compact-payload-wire: CompactPayload transcript_excerpt Field

## Purpose

Add forward-compatible `transcript_excerpt: Option<String>` to the `CompactPayload` variant of `HookRequest` in `wire.rs`. Day 1: ignored by dispatch_request. Enables future clients to send transcript data for PreCompact restoration (#670).

## File: `crates/unimatrix-engine/src/wire.rs`

### Modified Enum Variant: HookRequest::CompactPayload

**Current** (line 151):
```
CompactPayload {
    session_id: String,
    injected_entry_ids: Vec<u64>,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
}
```

**After**:
```
CompactPayload {
    session_id: String,
    injected_entry_ids: Vec<u64>,
    role: Option<String>,
    feature: Option<String>,
    token_limit: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transcript_excerpt: Option<String>,
}
```

### Serde Annotations Explained

- `#[serde(default)]`: When field is absent in JSON input, deserialize as `None`. This preserves backward compatibility -- existing UDS callers that omit the field continue to work.
- `skip_serializing_if = "Option::is_none"`: When value is `None`, omit from serialized JSON. Prevents empty `"transcript_excerpt": null` in responses that serialize CompactPayload.

### Impact on dispatch_request CompactPayload Arm

The destructuring pattern in `dispatch_request` (listener.rs line 1164) must add the new field to avoid a compile error:

```
HookRequest::CompactPayload {
    session_id,
    injected_entry_ids: _,
    role,
    feature,
    token_limit,
    transcript_excerpt: _,   // NEW: ignored Day 1 (ADR-005)
} => { ... }
```

Note: This change is in `listener.rs`, documented in dispatch-request-refactor.md, but listed here for completeness. The field is bound to `_` because Day 1 does not use it.

### No Other Code Changes

- `handle_compact_payload` signature: unchanged. Does not receive `transcript_excerpt`.
- No new functions. No new types. No new imports.

## Error Handling

None. This is a purely additive serde field. Deserialization cannot fail due to this field -- `serde(default)` guarantees it.

## Key Test Scenarios

1. **Round-trip without field**: Serialize CompactPayload without `transcript_excerpt`, deserialize back. Field must be `None`.
2. **Round-trip with field**: Serialize CompactPayload with `transcript_excerpt: Some("text")`, deserialize back. Field must be `Some("text")`.
3. **Serialization omits None**: Serialize CompactPayload with `transcript_excerpt: None`. JSON output must NOT contain the key `transcript_excerpt`.
4. **Backward compat**: Deserialize JSON string `{"type":"CompactPayload","session_id":"x","injected_entry_ids":[]}` (no transcript_excerpt key). Must succeed with `transcript_excerpt: None`.
