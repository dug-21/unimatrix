# Test Plan: ToolContext (mcp/context.rs)

## Component Summary

`ToolContext` gains one new field:

```rust
pub client_type: Option<String>
```

This field is populated by `build_context_with_external_identity()` via a `client_type_map`
lookup and consumed by every tool handler when constructing `AuditEvent.agent_attribution`
and `AuditEvent.metadata`.

This component has minimal independent logic — it is a data carrier. Tests focus on:
1. The field's presence and correct propagation from `build_context_with_external_identity`.
2. Correct consumption by tool handlers (attribution population).
3. The `None` / `Some` distinction and its downstream effects.

---

## Unit Tests

### TC-U-01: `ToolContext` with `client_type = Some(...)` correctly flows to `agent_attribution`

**Risk**: R-03, IR-03
**Arrange**: Construct a `ToolContext` with `client_type = Some("codex-mcp-client".to_string())`.
**Assert**:
- `ctx.client_type.clone().unwrap_or_default() == "codex-mcp-client"` — the pattern used at
  `AuditEvent` construction sites

---

### TC-U-02: `ToolContext` with `client_type = None` produces empty `agent_attribution`

**Risk**: R-03, AC-03
**Arrange**: Construct a `ToolContext` with `client_type = None`.
**Assert**:
- `ctx.client_type.clone().unwrap_or_default() == ""` — sentinel value, no panic

---

### TC-U-03: `ToolContext` with `client_type = Some("")` produces empty `agent_attribution`

**Risk**: AC-02 (edge: empty string stored then retrieved)
**Note**: Per AC-02, an empty `clientInfo.name` is NOT inserted into `client_type_map`.
However if somehow `Some("")` were constructed, the downstream behavior must be `metadata = "{}"`.
**Arrange**: Construct a `ToolContext` with `client_type = Some("".to_string())`.
**Assert**:
- `ctx.client_type.as_deref().filter(|s| !s.is_empty())` returns `None`
- `metadata` construction produces `"{}"`, not `{"client_type":""}` (the filter guards this)

---

### TC-U-04: `metadata` construction — `Some("codex-mcp-client")` produces correct JSON

**Risk**: R-08, R-06, SEC-02
**Arrange**: `client_type = Some("codex-mcp-client".to_string())`.
**Act**: Apply the metadata construction pattern from FR-10:
```rust
serde_json::json!({"client_type": ct}).to_string()
```
**Assert**:
- Result equals `r#"{"client_type":"codex-mcp-client"}"#` (or semantically equivalent)
- Parses as valid JSON: `serde_json::from_str::<serde_json::Value>(&result).is_ok()`
- `result["client_type"] == "codex-mcp-client"`

---

### TC-U-05: `metadata` construction — `None` produces `"{}"`

**Risk**: R-06
**Arrange**: `client_type = None`.
**Act**: Apply metadata pattern: when `client_type` is `None` or empty, use `"{}".to_string()`.
**Assert**:
- Result equals `"{}"`
- Parses as valid JSON object with no keys

---

### TC-U-06: `ToolContext` does NOT use `AuditContext.session_id` as attribution source

**Risk**: Session namespace warning (IMPLEMENTATION-BRIEF.md section on session ID namespaces)
**Arrange**: Construct a `ToolContext` where `client_type` differs from the agent-declared
`session_id`.
**Assert**:
- `AuditEvent.agent_attribution` is populated from `ctx.client_type`, NOT from
  `ctx.audit_ctx.session_id` or `ctx.audit_ctx.agent_id`
- This is a code-inspection assertion, confirmed by the tools.md tests showing attribution
  origin

---

## Integration Boundary Tests

### TC-I-01: `build_context_with_external_identity` → `ToolContext.client_type` round-trip

**Risk**: R-03, R-05
Tested in `server.md` (SRV-U-09 and SRV-U-10). Cross-reference confirms that
`ToolContext.client_type` is populated correctly from the map lookup.

---

### TC-I-02: `client_type` field does not appear in MCP tool parameter schema

**Risk**: NFR-08, C-05
**Assert**: No `#[tool]` input struct in `tools.rs` contains a `client_type` or
`agent_attribution` field. Attribution is server-internal only.

Verified by code inspection: the agent-facing parameter structs for all 12 tools remain
unchanged by vnc-014.
