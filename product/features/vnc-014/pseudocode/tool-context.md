# Component: ToolContext (mcp/context.rs)

## Purpose

Add a single new field `client_type: Option<String>` to `ToolContext`.
This field carries the transport-attested `clientInfo.name` from
`client_type_map` into each tool handler so it can populate
`AuditEvent.agent_attribution` and `AuditEvent.metadata`.

**File modified:** `crates/unimatrix-server/src/mcp/context.rs`

---

## Modified Type

### `ToolContext` struct

```
/// Pre-validated context available to every MCP tool handler.
///
/// Constructed via `UnimatrixServer::build_context_with_external_identity()`.
/// Capability checking is a separate `UnimatrixServer::require_cap()` call
/// because different tools require different capabilities.
pub(crate) struct ToolContext {
    /// Resolved agent identity.
    pub agent_id: String,
    /// Agent trust level.
    pub trust_level: TrustLevel,
    /// Parsed response format.
    pub format: ResponseFormat,
    /// Pre-built audit context for service calls.
    pub audit_ctx: AuditContext,
    /// Typed caller identity for rate limiting.
    pub caller_id: CallerId,
    /// Transport-attested client name from MCP initialize handshake.
    ///
    /// Populated from `client_type_map` keyed on the rmcp session ID.
    /// None when no entry exists (no initialize called, or stdio with no
    /// registered client name).
    ///
    /// Used to populate AuditEvent.agent_attribution and AuditEvent.metadata.
    /// MUST NOT be confused with agent_id (which is agent-declared, spoofable).
    pub client_type: Option<String>,
}
```

---

## Construction

`ToolContext` is constructed only in `build_context_with_external_identity()`
in `server.rs`. The `client_type` field is populated there by the
`client_type_map` lookup. No other construction site creates `ToolContext` with
`client_type` set.

Tool handlers that construct `AuditEvent` read `ctx.client_type` directly.
They do not need to import anything new — `client_type` is a plain field
on the struct they already have.

---

## Usage Pattern at AuditEvent Construction

Every tool handler that builds an `AuditEvent` uses this pattern:

```
AuditEvent {
    // existing fields
    session_id:  ctx.audit_ctx.session_id.clone().unwrap_or_default(),
    agent_id:    ctx.agent_id.clone(),
    // ... other existing fields ...

    // vnc-014 fields
    credential_type:   "none".to_string(),
    capability_used:   Capability::X.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata: match ctx.client_type.as_deref().filter(|s| !s.is_empty()) {
        Some(ct) => serde_json::json!({"client_type": ct}).to_string(),
        None     => "{}".to_string(),
    },
}
```

**SESSION ID NAMESPACE WARNING (#4363):**
`AuditEvent.session_id` is populated from `ctx.audit_ctx.session_id`
(the agent-declared `mcp::`-prefixed parameter). It is NEVER the rmcp
`Mcp-Session-Id` UUID. The rmcp UUID is the `client_type_map` lookup key
only — an internal routing key that must not appear in audit records.

---

## Error Handling

No new error paths. `client_type: Option<String>` is infallible to populate
(map lookup returns `Option`, cloned to `Option<String>`).

---

## Key Test Scenarios

1. **Field presence**: `ToolContext` compiles with the new field. All
   construction sites (only `build_context_with_external_identity`) supply
   the field — confirmed by the fact that struct literal construction in Rust
   requires all fields.

2. **None propagation (NFR-03)**: When `client_type = None`, tool handler
   produces `agent_attribution = ""` and `metadata = "{}"`.

3. **Some propagation (AC-01)**: When `client_type = Some("codex-mcp-client")`,
   tool handler produces:
   - `agent_attribution = "codex-mcp-client"`
   - `metadata` parses as JSON with `client_type = "codex-mcp-client"`

4. **Empty string sentinel**: `client_type = Some("")` behaves like `None` —
   `filter(|s| !s.is_empty())` gates the metadata construction.
   `agent_attribution = ""`, `metadata = "{}"`.
