## ADR-006: Defensive Parsing of Claude Code Hook JSON

### Context

Claude Code provides lifecycle hooks that fire shell commands with JSON piped to stdin. The hook subcommand receives this JSON and must parse fields like `hook_event_name`, `session_id`, `cwd`, and event-specific data. Anthropic documents the hook interface but has not made explicit stability guarantees (SR-05 in the risk assessment).

The parsing strategy must balance two concerns:
1. **Forward compatibility:** Unknown fields added in future Claude Code versions should not break the parser.
2. **Graceful failure:** If required fields are missing or have unexpected types, the hook should degrade gracefully (exit 0, no stdout) rather than crash or show errors to the user.

### Decision

All hook input structs use maximum defensive serde annotations:

```rust
#[derive(Deserialize)]
pub struct HookInput {
    /// Required: which hook fired (e.g., "UserPromptSubmit", "PreCompact")
    #[serde(default)]
    pub hook_event_name: String,

    /// Optional: Claude Code session identifier
    #[serde(default)]
    pub session_id: Option<String>,

    /// Optional: working directory (used for project hash)
    #[serde(default)]
    pub cwd: Option<String>,

    /// Optional: path to transcript file
    #[serde(default)]
    pub transcript_path: Option<String>,

    /// Capture all unknown fields without failing
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
```

Key rules:

1. **`#[serde(default)]` on every field.** Even `hook_event_name` — if absent, it defaults to empty string, and the dispatcher treats empty string as an unknown event (exits 0 silently).

2. **`#[serde(flatten)]` for unknown fields.** Captures any fields Claude Code adds in future versions into a `serde_json::Value`. These are not processed but their presence does not cause parse failure.

3. **`Option<T>` for all fields except `hook_event_name`.** Even fields the current implementation uses (like `cwd`) are optional, because a future Claude Code version might restructure the JSON.

4. **No `#[serde(deny_unknown_fields)]`.** This would break on any new field from Claude Code.

5. **Graceful parse failure.** If `serde_json::from_str` fails entirely (not valid JSON), log to stderr and exit 0. Never exit non-zero on parse failure — non-zero exit codes may display errors to the user.

6. **Session identity fallback.** When `session_id` is `None`, the hook computes a proxy: `ppid-{parent_pid}` using `std::os::unix::process::parent_id()`. This groups hook events by the Claude Code process that spawned them.

7. **cwd fallback.** When `cwd` is `None`, fall back to `std::env::current_dir()`. Claude Code hook processes inherit the working directory from the parent.

### Consequences

**Easier:**
- The hook subcommand survives Claude Code updates that add, rename, or restructure fields.
- Partial JSON (truncated stdin) is handled gracefully — parse failure exits 0.
- Testing is simpler — test fixtures can use minimal JSON (`{"hook_event_name":"Ping"}`) without specifying all fields.

**Harder:**
- Type-checking is weaker — a field that changes type (e.g., `session_id` from string to integer) would deserialize as `None` via serde default, silently losing the value. This is acceptable because the fallback (parent PID proxy) provides session correlation.
- The `extra` field captures all unknown JSON, which could include large objects (e.g., full transcript). The hook process does not process `extra` — it is deserialized into `serde_json::Value` and dropped. Memory impact is bounded by the size of stdin, which Claude Code controls.
