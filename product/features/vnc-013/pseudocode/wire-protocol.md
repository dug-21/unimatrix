# vnc-013 Pseudocode: wire-protocol
## File: `crates/unimatrix-engine/src/wire.rs`

---

## Purpose

Extend `HookInput` and `ImplantEvent` with provider-identity and Gemini-payload fields
so that `hook::run()` can set provider after normalization and `listener.rs` can derive
`source_domain` without inference.

This is a pure struct extension — no logic changes. The framing functions, serialization
helpers, and all other types are untouched.

---

## New/Modified Types

### `HookInput` — add two fields

Current struct (relevant excerpt):
```
pub struct HookInput {
    #[serde(default)] pub hook_event_name: String,
    #[serde(default)] pub session_id: Option<String>,
    #[serde(default)] pub cwd: Option<String>,
    #[serde(default)] pub transcript_path: Option<String>,
    #[serde(default)] pub prompt: Option<String>,
    #[serde(flatten)]  pub extra: serde_json::Value,
}
```

Add BEFORE the `extra` flatten field (ordering matters: flatten must remain last):

```
    /// Originating LLM provider. Populated by hook::run() after normalize_event_name(),
    /// NOT from stdin JSON. #[serde(default)] ensures existing Claude Code hook JSON
    /// (which omits this field) deserializes to None without error (NFR-05, ADR-002).
    ///
    /// Valid values: "claude-code" | "gemini-cli" | "codex-cli" | None
    #[serde(default)]
    pub provider: Option<String>,

    /// Gemini CLI structured MCP context field. Present in BeforeTool and AfterTool
    /// payloads. Structure: { "server_name": str, "tool_name": str, "url": str }.
    /// Also captured by the `extra` flatten, but the named field enables typed access
    /// in build_cycle_event_or_fallthrough() without stringly-typed extra access (ADR-003).
    ///
    /// Claude Code and Codex payloads omit this field; serde(default) → None.
    #[serde(default)]
    pub mcp_context: Option<serde_json::Value>,
```

Field ordering in struct definition:
```
pub struct HookInput {
    #[serde(default)] pub hook_event_name: String,
    #[serde(default)] pub session_id: Option<String>,
    #[serde(default)] pub cwd: Option<String>,
    #[serde(default)] pub transcript_path: Option<String>,
    #[serde(default)] pub prompt: Option<String>,
    #[serde(default)] pub provider: Option<String>,       // NEW
    #[serde(default)] pub mcp_context: Option<serde_json::Value>,  // NEW
    #[serde(flatten)]  pub extra: serde_json::Value,      // must remain last
}
```

NOTE: serde flatten captures ALL unknown fields into `extra`, including `mcp_context`
if Gemini sends it. Since `mcp_context` is now a named field, serde will NOT also
capture it into `extra` — the named field takes priority in serde's deserialization
order. This is the intended behavior (ADR-003).

### `ImplantEvent` — add one field

Current struct (relevant excerpt):
```
pub struct ImplantEvent {
    pub event_type: String,
    pub session_id: String,
    pub timestamp: u64,
    pub payload: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_signal: Option<String>,
}
```

Add after `topic_signal`:

```
    /// Provider identity propagated from HookInput.provider through normalization.
    /// Non-None for all events processed through the normalization layer.
    /// None for events deserialized from wire frames that predate vnc-013 (backward compat).
    ///
    /// skip_serializing_if = "Option::is_none": Claude Code events without --provider
    /// produce wire frames without this field; the listener handles missing field as None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
```

---

## Initialization Sequence

No constructor logic — both fields use `#[serde(default)]` which means:
- Missing JSON key → field value is `None` (via `Option<T>: Default`)
- Explicit `null` in JSON → field value is `None`
- Explicit string in JSON → field value is `Some(string)`

The `provider` field on `HookInput` is populated AFTER deserialization by `hook::run()`.
`run()` uses a two-path dispatch depending on whether `--provider` was supplied:

```
// In run():
let mut hook_input = parse_hook_input(&stdin_content);

// Two-path dispatch:
//   Hint path  (provider.is_some()): caller knows the provider; map event name only.
//   Inference path (provider.is_none()): derive both canonical name and provider from event.

if let Some(ref hint) = provider {
    // Hint path: translate event name, use CLI flag value for provider.
    let canonical_name = map_to_canonical(&event);
    hook_input.provider = Some(hint.clone());
    // canonical_name used for build_request() below
} else {
    // Inference path: normalize_event_name infers both fields.
    let (canonical_name, provider_str) = normalize_event_name(&event);
    hook_input.provider = Some(provider_str.to_string());
    // canonical_name used for build_request() below
}
// provider_str / hint is &'static str or String — .to_string() always valid
```

---

## Backward Compatibility

Both new `HookInput` fields and the new `ImplantEvent` field use `#[serde(default)]`.

Test scenario (existing Claude Code hook JSON without new fields):
```
Input JSON: {"hook_event_name": "PreToolUse", "session_id": "s1", "cwd": "/work"}
Expected: HookInput { provider: None, mcp_context: None, ... }
No deserialization error.
```

Test scenario (Gemini BeforeTool JSON with mcp_context):
```
Input JSON: {
  "hook_event_name": "BeforeTool",
  "mcp_context": { "server_name": "unimatrix", "tool_name": "context_cycle", "url": "..." }
}
Expected: HookInput {
  hook_event_name: "BeforeTool",
  mcp_context: Some(Value::Object({ "server_name": ..., "tool_name": ..., "url": ... })),
  provider: None,  // populated by run() after parse
  extra: Value::Object({}),  // mcp_context consumed by named field, not in extra
}
```

Test scenario (ImplantEvent from pre-vnc-013 wire frame):
```
Input JSON: {"event_type": "PreToolUse", "session_id": "s1", "timestamp": 100, "payload": {}}
Expected: ImplantEvent { provider: None, topic_signal: None, ... }
No deserialization error.
```

---

## Error Handling

No fallible operations. All new fields are `Option<T>` with `#[serde(default)]`.
The `parse_hook_input()` fallback in hook.rs already handles total parse failure
(returns empty `HookInput` with all fields at their defaults).

---

## Key Test Scenarios

All from RISK-TEST-STRATEGY R-13 and backward compatibility requirements:

1. `test_hook_input_deserializes_without_new_fields` (NFR-05, AC-08):
   Deserialize minimal Claude Code JSON (`{"hook_event_name":"PreToolUse"}`).
   Assert `provider == None`, `mcp_context == None`. No error.

2. `test_hook_input_deserializes_with_mcp_context` (AC-14):
   Deserialize Gemini BeforeTool JSON with `mcp_context` field.
   Assert `mcp_context == Some(...)`, `mcp_context.get("tool_name") == Some("context_cycle")`.

3. `test_hook_input_provider_none_when_absent` (NFR-05):
   Deserialize JSON without `provider` field. Assert `provider == None`.

4. `test_implant_event_deserializes_without_provider` (NFR-05):
   Deserialize `ImplantEvent` JSON without `provider` key. Assert `provider == None`.

5. `test_implant_event_provider_round_trip` (AC-05):
   Serialize `ImplantEvent { provider: Some("gemini-cli"), ... }`.
   Assert JSON contains `"provider":"gemini-cli"`.
   Deserialize back. Assert `provider == Some("gemini-cli")`.

6. `test_implant_event_provider_none_omits_field` (wire compat):
   Serialize `ImplantEvent { provider: None, ... }`.
   Assert JSON does NOT contain `"provider"` key (skip_serializing_if behavior).

7. `test_mcp_context_not_duplicated_in_extra` (ADR-003 correctness):
   Deserialize Gemini BeforeTool JSON with `mcp_context` field.
   Assert `input.extra` does NOT also contain a `"mcp_context"` key.
   (Verifies serde named-field-over-flatten priority.)
