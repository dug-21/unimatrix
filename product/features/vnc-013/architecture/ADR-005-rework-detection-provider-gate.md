## ADR-005: Rework Detection Gated by `provider != "claude-code"` (Not Tool-Name Guard)

### Context

The rework candidate path in `build_request()` under the `"PostToolUse"` arm calls
`is_rework_eligible_tool()` to check whether the tool is `Bash`, `Edit`, `Write`, or
`MultiEdit` — tools specific to Claude Code's file/shell ecosystem. Gemini's `AfterTool`
(normalized to `"PostToolUse"`) covers MCP tool calls only (restricted by the
`mcp_unimatrix_.*` matcher in `.gemini/settings.json`). Unimatrix MCP tool names
(`context_search`, `context_store`, etc.) do not appear in `is_rework_eligible_tool()`'s
allowlist, so today's tool-name guard would happen to exclude Gemini events.

However, the tool-name guard's exclusion of Gemini events is accidental, not
contractual. If Gemini adds a built-in tool named `Bash` or `Edit` in a future version,
or if the `mcp_unimatrix_.*` matcher ever changes, Gemini events could silently enter
the rework path.

OQ-1 from the SCOPE.md is resolved: use `provider != "claude-code"` as the explicit
gate. The `provider` field is threaded through the entire normalization architecture
precisely to enable this discrimination. Using it here is consistent with the design
intent and documents the contract.

Two gate approaches:
**Option A — Tool-name guard only**: Rely on `is_rework_eligible_tool()` returning
`false` for MCP tool names. Works today; fails silently if tool names overlap.

**Option B — Provider gate**: Add `if provider.as_deref() != Some("claude-code") { return generic_record_event(...); }` before `is_rework_eligible_tool()` in the
`"PostToolUse"` arm. Explicit contract; survives tool-name changes; self-documenting.

### Decision

Use provider gate (Option B). In the `"PostToolUse"` arm of `build_request()`:

```rust
"PostToolUse" => {
    // Rework detection is Claude Code-specific (ADR-005 vnc-013).
    // Provider gate ensures Gemini AfterTool and Codex PostToolUse never
    // enter the rework candidate path, regardless of tool names.
    if provider.as_deref() != Some("claude-code") {
        let topic_signal = extract_event_topic_signal(canonical_event, input);
        return HookRequest::RecordEvent {
            event: ImplantEvent {
                event_type: canonical_event.to_string(),
                provider: Some(provider.to_string()),
                ...
            },
        };
    }
    // Existing rework detection logic follows (Claude Code only)
    ...
}
```

The tool-name guard (`is_rework_eligible_tool()`) remains for Claude Code events —
it is a valid secondary filter for non-rework Claude Code tools (e.g., `Read`,
`WebFetch`) and does not need removal.

### Consequences

Easier: rework tracking exclusion is explicit and robust against future tool-name
additions; intent is documented in code comments; AC-04 and AC-12 are directly
testable via `provider` value.

Harder: `provider` must be available in scope at the `"PostToolUse"` arm. Since
`run()` now carries `provider` and passes it to `build_request()`, this is a
parameter threading concern already solved by ADR-002.
