# Component: Tool Handlers (mcp/tools.rs)

## Purpose

Migrate all 12 tool handlers from `build_context()` to
`build_context_with_external_identity()` (Seam 2). At every `AuditEvent`
construction site, populate the four new fields using `ctx.client_type`.

The migration is mechanical (O(n)) — each handler substitutes one function
call and extends its `AuditEvent` struct literal.

**File modified:** `crates/unimatrix-server/src/mcp/tools.rs`

---

## Migration Pattern

Every tool handler currently follows this pattern:

```
// BEFORE (current):
let ctx = self
    .build_context(&params.agent_id, &params.format, &params.session_id)
    .await?;
// ...
self.audit_fire_and_forget(AuditEvent {
    event_id:   0,
    timestamp:  0,
    session_id: ctx.audit_ctx.session_id.clone().unwrap_or_default(),
    agent_id:   ctx.agent_id.clone(),
    operation:  "context_X".to_string(),
    target_ids: target_ids.clone(),
    outcome:    Outcome::Success,
    detail:     "...".to_string(),
});
```

Replace with:

```
// AFTER (vnc-014):
let ctx = self
    .build_context_with_external_identity(
        &params.agent_id,
        &params.format,
        &params.session_id,
        &tool_call_context.request_context,
        None,   // always None in vnc-014; W2-3 wires the Some arm
    )
    .await?;
// ...
self.audit_fire_and_forget(AuditEvent {
    event_id:   0,
    timestamp:  0,
    session_id: ctx.audit_ctx.session_id.clone().unwrap_or_default(),
    agent_id:   ctx.agent_id.clone(),
    operation:  "context_X".to_string(),
    target_ids: target_ids.clone(),
    outcome:    Outcome::Success,
    detail:     "...".to_string(),
    // vnc-014 additions:
    credential_type:   "none".to_string(),
    capability_used:   Capability::X.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata: match ctx.client_type.as_deref().filter(|s| !s.is_empty()) {
        Some(ct) => serde_json::json!({"client_type": ct}).to_string(),
        None     => "{}".to_string(),
    },
});
```

**CRITICAL (Unimatrix #4363):** `AuditEvent.session_id` is always populated
from `ctx.audit_ctx.session_id` (agent-declared, `mcp::`-prefixed). It is
NEVER the rmcp `Mcp-Session-Id` UUID. The rmcp UUID is only used as a lookup
key for `client_type_map` inside `build_context_with_external_identity`.

**CRITICAL (FR-10, SEC-02):** `metadata` construction MUST use
`serde_json::json!`. Format-string concatenation is prohibited.

---

## The `tool_call_context` Parameter

rmcp tool handlers in this codebase receive a `Parameters<T>` extractor.
Some handlers also receive a `RequestContext<RoleServer>` through the
`#[tool]` mechanism.

The delivery agent must verify the exact parameter list for each handler.
`RequestContext` may be accessed as:
- A direct parameter on the handler function if rmcp exposes it there, or
- `tool_call_context.request_context` if the handler receives a wrapper type

Check the rmcp 0.16.0 `#[tool]` attribute documentation and the
`ServerHandler` dispatching to confirm how `RequestContext<RoleServer>` is
made available to individual tool functions. This is the IR-02 integration
risk — the exact call pattern must be verified empirically.

---

## Per-Tool Capability and AuditEvent Template

### context_search

```
capability gate: Capability::Search
capability_used: Capability::Search.as_audit_str()  -> "search"

AuditEvent {
    ...,
    operation:         "context_search".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Search.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_lookup

```
capability gate: Capability::Read
capability_used: Capability::Read.as_audit_str()    -> "read"

AuditEvent {
    ...,
    operation:         "context_lookup".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Read.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

NOTE: Specification domain model maps `context_lookup` to `Capability::Search`
("search"). The current handler uses `Capability::Read` in `require_cap`.
Delivery agent must inspect the current `require_cap` call in `context_lookup`
and use whatever capability is actually gated there. Document the choice in
the PR.

### context_get

```
capability gate: Capability::Read
capability_used: Capability::Read.as_audit_str()    -> "read"

AuditEvent {
    ...,
    operation:         "context_get".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Read.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_store

```
capability gate: Capability::Write
capability_used: Capability::Write.as_audit_str()   -> "write"

AuditEvent {
    ...,
    operation:         "context_store".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Write.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_correct

```
capability gate: Capability::Write
capability_used: Capability::Write.as_audit_str()   -> "write"

AuditEvent {
    ...,
    operation:         "context_correct".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Write.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_deprecate

```
capability gate: Capability::Write
capability_used: Capability::Write.as_audit_str()   -> "write"

AuditEvent {
    ...,
    operation:         "context_deprecate".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Write.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_status

```
capability gate: Capability::Read
capability_used: Capability::Read.as_audit_str()    -> "read"

AuditEvent {
    ...,
    operation:         "context_status".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Read.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_briefing

```
capability gate: Capability::Search
capability_used: Capability::Search.as_audit_str()  -> "search"

AuditEvent {
    ...,
    operation:         "context_briefing".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Search.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_quarantine

```
capability gate: Capability::Write
capability_used: Capability::Write.as_audit_str()   -> "write"

AuditEvent {
    ...,
    operation:         "context_quarantine".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Write.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_enroll

```
capability gate: Capability::Admin
capability_used: Capability::Admin.as_audit_str()   -> "admin"

AuditEvent {
    ...,
    operation:         "context_enroll".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Admin.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_retrospective

```
capability gate: Capability::Read
capability_used: Capability::Read.as_audit_str()    -> "read"

AuditEvent {
    ...,
    operation:         "context_retrospective".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Read.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

### context_cycle

```
capability gate: Capability::Write
capability_used: Capability::Write.as_audit_str()   -> "write"

AuditEvent {
    ...,
    operation:         "context_cycle".to_string(),
    credential_type:   "none".to_string(),
    capability_used:   Capability::Write.as_audit_str().to_string(),
    agent_attribution: ctx.client_type.clone().unwrap_or_default(),
    metadata:          <json_or_empty>,
}
```

---

## metadata Construction Helper (inline)

For clarity, the `metadata` field construction can be written as a helper
expression. This is the same expression at all 12 sites:

```
let metadata_json = match ctx.client_type.as_deref().filter(|s| !s.is_empty()) {
    Some(ct) => serde_json::json!({"client_type": ct}).to_string(),
    None     => "{}".to_string(),
};
```

The `serde_json::json!` macro correctly serializes `ct` as a JSON string
value regardless of content (backslashes, quotes, newlines, control chars).
This is the SEC-02 mitigation.

---

## Non-Tool-Call AuditEvent Construction Sites

`background.rs` (lines ~1197, ~1252, ~2267) and `uds/listener.rs` also
construct `AuditEvent` directly without `ToolContext`. These sites are NOT
on the tool-call path and do not have `RequestContext` available.

They must be updated using `..AuditEvent::default()` struct update syntax:

```
AuditEvent {
    // existing fields for this site
    session_id: "".to_string(),
    agent_id:   "system".to_string(),
    operation:  "background_tick".to_string(),
    target_ids: vec![],
    outcome:    Outcome::Success,
    detail:     "...".to_string(),
    // Four new fields via default:
    ..AuditEvent::default()
    // yields: credential_type="none", capability_used="",
    //         agent_attribution="", metadata="{}"
}
```

Note: `..AuditEvent::default()` must come LAST in the struct literal.
`event_id` and `timestamp` are set to 0 by default and overwritten by
`log_audit_event` — this is the existing pattern.

The delivery agent must enumerate all construction sites in `background.rs`
and `uds/listener.rs` (OQ-2 in architecture). If any additional sites exist
beyond the three confirmed in background.rs, they must receive the same
`..AuditEvent::default()` treatment.

---

## Error Handling

No new error paths introduced. The `build_context_with_external_identity` call
propagates errors via `?` exactly as `build_context` did. The new `metadata`
construction using `serde_json::json!` and `.to_string()` is infallible for
any valid string input.

---

## Key Test Scenarios

1. **Seam 2 migration completeness (AC-12, R-05)**: After removing
   `build_context`, `cargo build --workspace` compiles without error.
   No remaining call to `build_context` exists in production code.

2. **Attribution per tool (AC-11)**: For each of the 12 tools, an integration
   test that:
   - Populates `client_type_map` with `("<session-uuid>", "codex-mcp-client")`
   - Calls the tool with the matching session header
   - Reads the resulting audit row
   - Asserts `capability_used` matches the canonical value for that tool
   - Asserts `agent_attribution = "codex-mcp-client"`
   - Asserts `metadata` parses as JSON with `client_type = "codex-mcp-client"`

3. **No session context (AC-03, NFR-03)**: Each tool called with no matching
   `client_type_map` entry produces `agent_attribution = ""`, `metadata = "{}"`,
   `credential_type = "none"`. No error returned.

4. **metadata JSON safety (R-08, EC-06)**: Inject adversarial `clientInfo.name`
   values:
   - Contains `"` (embedded quotes)
   - Contains `\` (backslash)
   - Contains newline `\n`
   - Contains `"}` (closing sequence)
   All four produce valid, parseable JSON in `metadata`. The deserialized
   `client_type` value equals the original string exactly.

5. **credential_type constant (FR-09)**: All 12 tools produce
   `credential_type = "none"`. Not empty string, not `null`.

6. **Non-tool-call sites (R-12)**: Background tick audit rows have
   `credential_type="none"`, `capability_used=""`, `agent_attribution=""`,
   `metadata="{}"`. Verify INSERT does not fail on the NOT NULL constraints.

7. **serde_json import**: Confirm `serde_json` is already in `Cargo.toml`
   for `unimatrix-server` (it is an existing dep per IMPLEMENTATION-BRIEF.md).
   The `json!` macro is used via `serde_json::json!` without additional imports.
