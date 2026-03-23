## ADR-006: SubagentStart stdout uses hookSpecificOutput JSON envelope

### Context

SR-01 in the original architecture was marked "Unconfirmed at architecture time" — the
question was whether Claude Code reads SubagentStart hook stdout and injects it into the
subagent context. Vision alignment review (WARN-2, AC-SR01 confirmed) has resolved this:

Claude Code documentation confirms SubagentStart supports context injection via stdout.
However, SubagentStart does NOT accept plain text on stdout like UserPromptSubmit. It
requires a specific JSON envelope:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "SubagentStart",
    "additionalContext": "injected text here"
  }
}
```

Without this envelope, Claude Code ignores SubagentStart hook stdout — injection silently
fails. The existing `write_stdout()` function used by UserPromptSubmit writes plain text
and would produce no injection for SubagentStart events.

The server's response type (`HookResponse::Entries`) is identical for both event sources.
The difference is purely in how the hook process serializes that response to stdout before
exiting. This is a hook-process-only concern: the server has no awareness of how its
`HookResponse` is rendered to stdout.

SubagentStart input also provides `agent_id` and `agent_type` fields in `input.extra`.
These are available as context within the hook process but require no behavior change in
crt-027 — no routing or filtering based on agent type is introduced in this feature.

### Decision

In `uds/hook.rs`, add a `write_stdout_subagent_inject` helper function alongside the
existing `write_stdout`:

```rust
fn write_stdout_subagent_inject(entries_text: &str) -> io::Result<()> {
    use std::io::Write;
    let envelope = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SubagentStart",
            "additionalContext": entries_text
        }
    });
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", envelope)
}
```

The dispatch path in `hook.rs` (the function that calls `write_stdout` after receiving
a `HookResponse::Entries`) is extended with a branch:

- If `source == "SubagentStart"`: call `write_stdout_subagent_inject(formatted_text)`
- Otherwise: call `write_stdout(formatted_text)` (unchanged plain-text path)

The `additionalContext` value is identical to what `write_stdout` would have written —
the same formatted index/entries text produced from `HookResponse::Entries`. The
formatting pipeline (`format_entries_for_injection` or equivalent) is unchanged; only
the final stdout serialization differs by source.

**Scope boundary:** The server continues returning `HookResponse::Entries` for
SubagentStart-sourced ContextSearch requests. No server-side change is required.
This decision is confined entirely to the hook process (`uds/hook.rs`).

**agent_id / agent_type:** These fields from `input.extra` are parsed and available
within the SubagentStart arm of `build_request` if needed in future features (e.g.,
agent-type-conditioned ranking in W3-1). In crt-027 they are read from `input.extra`
but not acted upon.

### Consequences

- SubagentStart stdout injection now works: Claude Code injects `additionalContext` into
  the subagent context before its first token.
- UserPromptSubmit stdout remains plain text — no change to that path.
- `write_stdout_subagent_inject` is a small, testable function. Unit tests can assert
  that: (a) the output is valid JSON, (b) `hookEventName` equals `"SubagentStart"`,
  (c) `additionalContext` contains the expected entries text.
- The server is not modified. Any future change to how SubagentStart responses are
  formatted requires only a hook-process change.
- SR-01 is now resolved: "Confirmed. SubagentStart stdout injection is supported when
  the `hookSpecificOutput` JSON envelope is used." See updated SR-01 section in
  ARCHITECTURE.md.
