## ADR-003: Named `mcp_context` Field on `HookInput` for Gemini Payload Access

### Context

Gemini CLI's `BeforeTool` and `AfterTool` hook payloads include a structured
`mcp_context` field that carries MCP server identity and the bare tool name:

```json
{
  "server_name": "unimatrix",
  "tool_name": "context_cycle",
  "url": "http://..."
}
```

`build_cycle_event_or_fallthrough()` reads `tool_name` from `input.extra["tool_name"]`
(Claude Code sends it as `"mcp__unimatrix__context_cycle"` at the payload root). For
Gemini, this field is absent from the top level — it is nested at
`extra["mcp_context"]["tool_name"]`.

Two access patterns were considered:

**Option A — Stringly-typed `extra` access**: Read `input.extra.get("mcp_context")` in
the promotion adapter. No struct change required. Fragile: field name is a magic string
with no type system guidance; the path `extra["mcp_context"]["tool_name"]` is easy to
misread or mistype. `HookInput` already uses `extra` as a catch-all flatten — relying
on it exclusively for named fields compounds the readability problem.

**Option B — Named field `mcp_context: Option<serde_json::Value>` on `HookInput`**:
Deserializes via `#[serde(default)]`. When the Gemini payload contains `"mcp_context"`,
serde populates this field directly (the flatten `extra` still captures other unknown
fields). Access in the promotion adapter is `input.mcp_context.as_ref()` — typed,
named, self-documenting. The `extra` flatten still handles all other unknown fields
including `mcp_context` fields from future providers or Gemini versions. This is the
pattern used by `prompt: Option<String>` (already a named field for `UserPromptSubmit`).

**SR-08 context**: The `mcp_context.tool_name` → top-level `tool_name` promotion is the
single highest-risk integration point in this feature. A stringly-typed implementation
at this exact point is the worst place to introduce ambiguity. Named field access is
lower risk.

### Decision

Add `mcp_context: Option<serde_json::Value>` to `HookInput` in
`unimatrix-engine/src/wire.rs` with `#[serde(default)]`. This named field coexists
with the existing `extra` flatten — serde populates both independently. The named field
provides typed access in the `"PreToolUse"` promotion adapter; `extra` continues to
capture all other unknown fields.

The promotion step in the `"PreToolUse"` arm of `build_request()` reads:
```rust
if let Some(bare_name) = input.mcp_context
    .as_ref()
    .and_then(|v| v.get("tool_name"))
    .and_then(|v| v.as_str())
{
    // Promote bare tool name to extra["tool_name"] so
    // build_cycle_event_or_fallthrough() can find it.
    extra_clone["tool_name"] = serde_json::Value::String(bare_name.to_string());
}
```

The existing `build_cycle_event_or_fallthrough()` matching logic is unchanged. The bare
name `"context_cycle"` satisfies the existing guard `tool_name == "context_cycle"`.

### Consequences

Easier: promotion adapter code is type-safe and self-documenting; SR-08 risk is
mitigated by named access; future Gemini payload fields can be promoted via the same
named-field pattern rather than ad-hoc `extra` access.

Harder: `HookInput` in `wire.rs` (a shared crate) acquires another optional field.
The field is `Option<serde_json::Value>` rather than a typed struct, preserving
flexibility for Gemini version variations in `mcp_context` structure.
