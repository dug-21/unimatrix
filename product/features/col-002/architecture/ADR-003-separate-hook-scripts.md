## ADR-003: Four Separate Hook Scripts

### Context

Claude Code hooks are registered per event type in `.claude/settings.json`. The hook API delivers different JSON schemas for different event types (PreToolUse has `tool_name` + `tool_input`, PostToolUse adds `tool_response`, SubagentStart/Stop have `agent_type`). A single script could handle all types by inspecting the JSON, or four scripts could each handle one type.

The existing research prototype (`product/research/ass-011/hooks/observe.sh`) uses a single script registered for selected tool types only.

### Decision

Four separate hook scripts, one per event type:
- `observe-pre-tool.sh` (PreToolUse)
- `observe-post-tool.sh` (PostToolUse)
- `observe-subagent-start.sh` (SubagentStart)
- `observe-subagent-stop.sh` (SubagentStop)

Each script is registered as a separate hook in `.claude/settings.json`. Each writes its hook-type-specific fields directly to JSONL — PreToolUse/PostToolUse write `tool` + `input` (+ `response_size`/`response_snippet` for Post), SubagentStart writes `agent_type` + `prompt_snippet`, SubagentStop writes `agent_type` (empty string — platform constraint). Field normalization into the uniform `ObservationRecord` struct happens in the Rust parser, not in the shell scripts.

### Consequences

- **Easier**: Each script is simpler (no type dispatch). Testing one script does not require all event types. Registration is explicit and clear. Adding a new event type means adding a new script without modifying existing ones.
- **Harder**: Four files to maintain instead of one. `.claude/settings.json` configuration is slightly more verbose (four entries instead of one).
