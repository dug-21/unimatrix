# vnc-013 Pseudocode: reference-configs
## Files: `.gemini/settings.json` + `.codex/hooks.json`

---

## Purpose

Provide operator-ready reference hook configurations so that Gemini CLI and Codex CLI
users can connect their LLM client to Unimatrix without guessing the registration format.
These are documentation artifacts, not compiled code.

---

## `.gemini/settings.json`

### Context

Gemini CLI v0.31+ supports hook registration via `.gemini/settings.json`. Four hook
events map to Unimatrix lifecycle events:

| Gemini Event | Canonical (post-normalization) | Unimatrix Action |
|---|---|---|
| `BeforeTool` | `PreToolUse` | Intercepts `context_cycle` calls; records all Unimatrix tool calls |
| `AfterTool` | `PostToolUse` | Records Unimatrix tool completions |
| `SessionStart` | `SessionStart` | Registers session |
| `SessionEnd` | `Stop` | Closes session |

The `BeforeTool` and `AfterTool` events are scoped to Unimatrix tools via the
`matcher` regex `mcp_unimatrix_.*`. This prevents the hook from firing for Gemini's
built-in tools (file operations, shell execution, etc.).

`SessionStart` and `SessionEnd` have no `matcher` (session lifecycle events are not
tool-scoped in Gemini CLI).

### File Structure

```json
{
  "hooks": {
    "BeforeTool": [
      {
        "hooks": [
          {
            "matcher": "mcp_unimatrix_.*",
            "command": "unimatrix hook BeforeTool"
          }
        ]
      }
    ],
    "AfterTool": [
      {
        "hooks": [
          {
            "matcher": "mcp_unimatrix_.*",
            "command": "unimatrix hook AfterTool"
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "hooks": [
          {
            "command": "unimatrix hook SessionStart"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "command": "unimatrix hook SessionEnd"
          }
        ]
      }
    ]
  }
}
```

Note: No `--provider gemini-cli` flag is required for Gemini CLI hook events because
Gemini-unique event names (`BeforeTool`, `AfterTool`, `SessionEnd`) allow unambiguous
inference. `SessionStart` is a shared name and defaults to `"claude-code"` when
`--provider` is absent — this is a known semantic imprecision documented in
RISK-TEST-STRATEGY edge cases. Operators who want accurate `source_domain` for
`SessionStart` events may add `--provider gemini-cli` to the `SessionStart` command.

Recommended: include `--provider gemini-cli` on all four commands for precise attribution:
```
"command": "unimatrix hook BeforeTool --provider gemini-cli"
```

### Validation Requirements (AC-10, R-12)

- File must be valid JSON (parseable by `serde_json` or any JSON parser)
- `matcher` field value must be `"mcp_unimatrix_.*"` (confirmed for Gemini CLI v0.31+)
- Pattern `mcp_unimatrix_.*` must match all 12 Unimatrix tool names:
  - `mcp_unimatrix__context_search` (note: Gemini uses `_` not `__` as separator? Confirm.)
  - Implementer must verify the exact tool name format in Gemini's hook payload during
    implementation. ASS-049 FINDINGS-HOOKS.md should contain this information.
  - If Gemini uses `mcp__unimatrix__context_search` (double underscore), the regex must
    be `mcp__unimatrix_.*` or `mcp_+unimatrix_.*`. Adjust accordingly.
- All four Gemini hook events must be present: `BeforeTool`, `AfterTool`, `SessionStart`, `SessionEnd`

### Gemini CLI v0.31+ Schema Note

The exact schema for `.gemini/settings.json` was confirmed against Gemini CLI v0.31+
documentation. The nested `hooks` structure shown above is the confirmed format.
If the format differs from what is shown, the implementer must use the correct format
from Gemini CLI documentation or ASS-049 findings.

---

## `.codex/hooks.json`

### Context

Codex CLI uses an identical hook registration schema to Claude Code's `.claude/settings.json`
(confirmed by ASS-049 FINDINGS-HOOKS.md). The file location is `.codex/hooks.json`
(project-level) or `~/.codex/hooks.json` (global).

Codex CLI shares all event names with Claude Code (`PreToolUse`, `PostToolUse`,
`SessionStart`, `Stop`). The `--provider codex-cli` flag is MANDATORY on every hook
invocation — without it, events fall through to the `"claude-code"` default in
`normalize_event_name()`, producing incorrect `source_domain` attribution (ADR-006,
R-03, SR-01).

CRITICAL: Live Codex MCP hook support is blocked by Codex upstream bug #16732.
This configuration file is non-functional until that bug is resolved. Unit tests
use synthetic Codex events with `--provider codex-cli` to verify code paths.

### File Structure

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "mcp__unimatrix__.*",
        "hooks": [
          {
            "type": "command",
            "command": "unimatrix hook PreToolUse --provider codex-cli"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "mcp__unimatrix__.*",
        "hooks": [
          {
            "type": "command",
            "command": "unimatrix hook PostToolUse --provider codex-cli"
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "unimatrix hook SessionStart --provider codex-cli"
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "unimatrix hook Stop --provider codex-cli"
          }
        ]
      }
    ]
  }
}
```

NOTE: The exact schema format (field names, nesting structure) must match what Codex
CLI v0.x actually parses. ASS-049 confirmed the schema is identical to Claude Code.
The Claude Code schema uses `"matcher"` and `"hooks"` arrays. If the actual schema
differs, use the correct structure from ASS-049 FINDINGS-HOOKS.md.

### Mandatory Caveat Text

The file MUST include a note (via JSON comment if supported, or via a companion
`README.md` or `# comment` if the format supports it) stating:

```
NOTE: Live Codex CLI MCP hook support is currently non-functional due to Codex
upstream bug #16732. This configuration file is provided for forward-compatibility
only. The --provider codex-cli flag on every hook invocation is REQUIRED to ensure
correct source attribution when #16732 is resolved. Without this flag, all Codex
events will be mislabeled as "claude-code" in Unimatrix.
```

JSON does not support comments. Options:
1. Add a `"_comment"` key at the top level of the JSON object (common convention)
2. Create `.codex/HOOKS-README.md` alongside `.codex/hooks.json` with the caveat text
3. Both: `"_comment"` in JSON + README

Recommended: use `"_comment"` key in the JSON file AND create a companion README.
The `"_comment"` key will be ignored by Codex CLI's hook parser (unknown field).

```json
{
  "_comment": "IMPORTANT: --provider codex-cli is REQUIRED on all hook commands. Without it, events are mislabeled as claude-code. Live MCP hooks are non-functional until Codex bug #16732 is resolved.",
  "hooks": { ... }
}
```

### Validation Requirements (AC-19, R-03)

- File must be valid JSON
- ALL four hook event commands must include `--provider codex-cli` (AC-19 config review check)
- Caveat text about Codex bug #16732 must be present (AC-19)
- matcher must cover all Unimatrix tool names (same regex as Claude Code: `mcp__unimatrix__.*`)

---

## Error Handling

These are JSON configuration files — no runtime logic.

Failure mode: if `mcp_unimatrix_.*` regex is invalid for the target Gemini CLI version,
no hooks fire and no error is surfaced (R-12). The only mitigation is config review (AC-10)
and operator testing.

Failure mode: if `--provider codex-cli` is omitted from the Codex config, Codex events
are mislabeled as `"claude-code"` (R-03). Mitigation: caveat text + AC-19 config review.

---

## Key Test Scenarios

### Config validation — AC-10, AC-19

```
// test_gemini_config_exists_and_valid_json (AC-10):
// Assert .gemini/settings.json exists at repo root.
// Parse as JSON. Assert no parse error.
// Assert all four events present: BeforeTool, AfterTool, SessionStart, SessionEnd.
// Assert matcher == "mcp_unimatrix_.*" for BeforeTool and AfterTool.

// test_codex_config_exists_and_valid_json (AC-19):
// Assert .codex/hooks.json exists at repo root.
// Parse as JSON.
// Assert all four events present: PreToolUse, PostToolUse, SessionStart, Stop.
// Assert each command contains "--provider codex-cli".
// Assert caveat text about Codex bug #16732 is present (in _comment or README).

// test_codex_config_provider_flag_on_all_commands (AC-19, R-03):
// For each hook in .codex/hooks.json, extract the command string.
// Assert command.contains("--provider codex-cli").
// This is the primary guard against R-03 silent mislabel risk.
```

### Synthetic Codex event tests — AC-17, AC-19

These are NOT config file tests — they test the normalization code with synthetic
Codex-like input. Included here for traceability:

```
// test_normalize_codex_pretooluse_with_provider_hint (AC-17):
// normalize_event_name("PreToolUse", Some("codex-cli")) == ("PreToolUse", "codex-cli")

// test_normalize_codex_posttooluse_with_provider_hint:
// normalize_event_name("PostToolUse", Some("codex-cli")) == ("PostToolUse", "codex-cli")

// test_normalize_shared_name_without_hint_defaults_claude_code (AC-18, R-03/SC-2):
// normalize_event_name("PreToolUse", None) == ("PreToolUse", "claude-code")
// Documents the silent mislabel risk explicitly: without --provider, Codex events
// are attributed as "claude-code".
```

---

## No Pseudocode

These files contain only JSON data. There is no algorithmic logic to describe.
The implementation is: write the JSON files to the repository root at the specified paths.
