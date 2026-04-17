# Component Test Plan: wire-protocol
## `crates/unimatrix-engine/src/wire.rs`

Validating ACs: **AC-05, AC-14**
Risk coverage: **R-02 (partial), R-13**

---

## Component Responsibility

`wire.rs` defines `HookInput` and `ImplantEvent`. This feature adds three new fields:
- `HookInput.provider: Option<String>` — `#[serde(default)]`
- `HookInput.mcp_context: Option<serde_json::Value>` — `#[serde(default)]`
- `ImplantEvent.provider: Option<String>` — `#[serde(default, skip_serializing_if = "Option::is_none")]`

The tests here verify: deserialization behavior with and without the new fields,
serialization round-trips, and the `skip_serializing_if` behavior on `ImplantEvent.provider`.

---

## Unit Test Expectations

### R-13: Backward Deserialization Regression (AC-08, NFR-05)

**`test_hook_input_deserializes_without_new_fields`**

```rust
// Arrange: minimal Claude Code hook JSON — omits provider and mcp_context
let json = r#"{"hook_event_name":"PreToolUse","session_id":"sess-1"}"#;

// Act
let input: HookInput = serde_json::from_str(json).expect("deserialize");

// Assert
assert_eq!(input.provider, None);
assert_eq!(input.mcp_context, None);
assert_eq!(input.hook_event_name, "PreToolUse");
```

No deserialization error. `#[serde(default)]` must be present on both fields.

---

**`test_hook_input_deserializes_with_provider_field`**

```rust
// Arrange: Codex-style payload with explicit provider
let json = r#"{
    "hook_event_name": "PreToolUse",
    "provider": "codex-cli"
}"#;

// Act
let input: HookInput = serde_json::from_str(json).expect("deserialize");

// Assert
assert_eq!(input.provider, Some("codex-cli".to_string()));
assert_eq!(input.mcp_context, None);
```

---

**`test_hook_input_deserializes_gemini_payload_with_mcp_context`**

```rust
// Arrange: Gemini BeforeTool payload with mcp_context structured field
let json = r#"{
    "hook_event_name": "BeforeTool",
    "mcp_context": {
        "server_name": "unimatrix",
        "tool_name": "context_cycle",
        "url": "http://localhost:3000"
    }
}"#;

// Act
let input: HookInput = serde_json::from_str(json).expect("deserialize");

// Assert
assert!(input.mcp_context.is_some());
let mcp = input.mcp_context.as_ref().unwrap();
assert_eq!(
    mcp.get("tool_name").and_then(|v| v.as_str()),
    Some("context_cycle")
);
assert_eq!(input.provider, None);  // not in payload — inference applies
```

This test is the serde foundation for R-01 (mcp_context.tool_name promotion). If it
fails, no promotion test can succeed.

---

**`test_hook_input_mcp_context_non_object_deserializes`** (NFR-04 edge case)

```rust
// Arrange: mcp_context present but is a string, not an object
let json = r#"{
    "hook_event_name": "BeforeTool",
    "mcp_context": "unexpected-string"
}"#;

// Act
let input: HookInput = serde_json::from_str(json).expect("deserialize — must not error");

// Assert
assert!(input.mcp_context.is_some());
// as_object() would return None — promotion adapter must handle gracefully
```

---

### R-02: ImplantEvent.provider Deserialization and Serialization

**`test_implant_event_deserializes_without_provider`** (R-13 secondary, R-02 degraded path)

```rust
// Arrange: legacy ImplantEvent JSON (before vnc-013) — no provider field
let json = r#"{
    "event_type": "PreToolUse",
    "session_id": "sess-1",
    "timestamp": 1700000000,
    "payload": {}
}"#;

// Act
let event: ImplantEvent = serde_json::from_str(json).expect("deserialize");

// Assert
assert_eq!(event.provider, None);
assert_eq!(event.event_type, "PreToolUse");
```

---

**`test_implant_event_provider_present_serializes`**

```rust
// Arrange
let event = ImplantEvent {
    event_type: "PreToolUse".to_string(),
    session_id: "sess-1".to_string(),
    timestamp: 1700000000,
    payload: serde_json::json!({}),
    topic_signal: None,
    provider: Some("gemini-cli".to_string()),
};

// Act
let json = serde_json::to_string(&event).expect("serialize");
let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

// Assert: provider field IS present when Some
assert_eq!(parsed.get("provider").and_then(|v| v.as_str()), Some("gemini-cli"));
```

---

**`test_implant_event_provider_none_not_serialized`** (skip_serializing_if)

```rust
// Arrange: provider is None
let event = ImplantEvent {
    provider: None,
    // ... other fields
};

// Act
let json = serde_json::to_string(&event).expect("serialize");
let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

// Assert: provider key ABSENT from JSON when None
assert!(parsed.get("provider").is_none(),
    "provider: None must not be serialized (skip_serializing_if)");
```

This validates the `skip_serializing_if = "Option::is_none"` annotation. If omitted,
existing Claude Code hook JSON consumers that don't expect a `provider` key may fail.

---

## Integration Test Expectations

No cross-crate integration tests needed for wire-protocol in isolation. The integration
boundary (UDS wire frame deserialization in listener.rs) is covered by the
`normalization.md` integration tests that exercise the full dispatch path.

The `test_implant_event_provider_present_serializes` test above covers the serialization
side. The listener's deserialization of the wire frame is exercised by AC-06's
integration test.

---

## Assertions Summary

| Test | Assertion | AC |
|------|-----------|-----|
| `test_hook_input_deserializes_without_new_fields` | `provider == None`, `mcp_context == None`, no error | AC-08, NFR-05 |
| `test_hook_input_deserializes_with_provider_field` | `provider == Some("codex-cli")` | AC-17 |
| `test_hook_input_deserializes_gemini_payload_with_mcp_context` | `mcp_context.tool_name == "context_cycle"` | AC-14 |
| `test_hook_input_mcp_context_non_object_deserializes` | No error on non-object mcp_context | NFR-04 |
| `test_implant_event_deserializes_without_provider` | `provider == None`, no error | AC-08, R-13 |
| `test_implant_event_provider_present_serializes` | `provider` key present in JSON | AC-05 |
| `test_implant_event_provider_none_not_serialized` | `provider` key absent from JSON | NFR-05 |

---

## Edge Cases

- `mcp_context` with extra unknown fields: must deserialize without error (`serde_json::Value` absorbs them)
- `provider: ""` (empty string): deserializes to `Some("")`, not `None` — document as degenerate but non-crashing
- `mcp_context` as JSON array (not object): `as_object()` returns None in promotion adapter — no panic
