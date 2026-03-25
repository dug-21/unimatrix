# Component: hook-registration

**File:** `.claude/settings.json`
**Wave:** 1 (no dependencies)
**Action:** Modify

---

## Purpose

Register `PostToolUseFailure` with Claude Code so that when a tool call fails, the hook binary is
invoked with the failure payload on stdin. Without this registration, the hook binary is never
called — the entire feature is a no-op at runtime regardless of all other code changes.

---

## Existing Pattern (reference for the new entry)

The existing `PreToolUse` and `PostToolUse` entries use `matcher: "*"` (all tools) and an absolute
path to the release binary:

```json
"PreToolUse": [
  {
    "matcher": "*",
    "hooks": [
      {
        "type": "command",
        "command": "/workspaces/unimatrix/target/release/unimatrix hook PreToolUse"
      }
    ]
  }
],
"PostToolUse": [
  {
    "matcher": "*",
    "hooks": [
      {
        "type": "command",
        "command": "/workspaces/unimatrix/target/release/unimatrix hook PostToolUse"
      }
    ]
  }
]
```

---

## New Entry to Add

Add `"PostToolUseFailure"` as a top-level key in the `"hooks"` object, immediately after the
`"PostToolUse"` entry (lifecycle order: PreToolUse → PostToolUse → PostToolUseFailure):

```json
"PostToolUseFailure": [
  {
    "matcher": "*",
    "hooks": [
      {
        "type": "command",
        "command": "/workspaces/unimatrix/target/release/unimatrix hook PostToolUseFailure"
      }
    ]
  }
]
```

### Required Properties

| Property | Value | Rationale |
|----------|-------|-----------|
| Top-level key | `"PostToolUseFailure"` | Exact casing per Claude Code hook event name (R-14: casing mismatch means hook never fires) |
| `matcher` | `"*"` | All tools — same as PreToolUse and PostToolUse (FR-01.1) |
| `type` | `"command"` | Same as all other entries |
| `command` | `"/workspaces/unimatrix/target/release/unimatrix hook PostToolUseFailure"` | Absolute path to release binary + event name arg (FR-01.2) |

---

## Initialization Sequence

No initialization logic. This is a static JSON configuration file read by Claude Code at session
start. The entry takes effect immediately for the next Claude Code session started after the file
is modified.

---

## Data Flow

```
Claude Code reads settings.json at session start
  --> for each tool failure:
        Claude Code fires "PostToolUseFailure" hook
        Invokes: /workspaces/unimatrix/target/release/unimatrix hook PostToolUseFailure
        Passes payload as JSON on stdin:
          { "tool_name": "...", "tool_input": {...}, "error": "...", "is_interrupt": true|false }
        Hook binary reads stdin, calls build_request("PostToolUseFailure", ...)
```

---

## Error Handling

No runtime errors for this component. Failure modes:

| Failure | Effect | Detection |
|---------|--------|-----------|
| Key casing wrong (e.g., `"postToolUseFailure"`) | Claude Code never fires the hook | R-14: AC-01 JSON inspection test |
| `matcher` absent or wrong value | Hook fires only for some tools | AC-01 inspection |
| Command path wrong or binary not built | Hook invoked but fails at process spawn | Hook exits non-zero; inspect shell errors |
| `"PostToolUseFailure"` key absent entirely | Entire feature is a no-op at runtime | AC-01 inspection |

---

## Key Test Scenarios

### T-HR-01: settings.json contains PostToolUseFailure key with correct casing (AC-01, R-14)

```
test settings_json_has_posttoolusefailure:
  content = read_file(".claude/settings.json")
  parsed = parse_json(content)
  hooks = parsed["hooks"]
  assert hooks.contains_key("PostToolUseFailure")
  entry = hooks["PostToolUseFailure"][0]
  assert entry["matcher"] == "*"
  assert entry["hooks"][0]["type"] == "command"
  assert entry["hooks"][0]["command"].contains("unimatrix hook PostToolUseFailure")
```

### T-HR-02: Command path pattern is consistent with PreToolUse and PostToolUse entries

```
test settings_json_command_path_consistent:
  pre_cmd  = hooks["PreToolUse"][0]["hooks"][0]["command"]
  post_cmd = hooks["PostToolUse"][0]["hooks"][0]["command"]
  fail_cmd = hooks["PostToolUseFailure"][0]["hooks"][0]["command"]
  // All three must use the same binary path prefix
  assert pre_cmd.starts_with("/workspaces/unimatrix/target/release/unimatrix hook")
  assert post_cmd.starts_with("/workspaces/unimatrix/target/release/unimatrix hook")
  assert fail_cmd.starts_with("/workspaces/unimatrix/target/release/unimatrix hook")
  // Event name suffix is different for each
  assert fail_cmd.ends_with("PostToolUseFailure")
```

---

## Anti-Patterns to Avoid

- Do NOT use `matcher: ""` (empty string) — that matches no tools; PreToolUse/PostToolUse use `"*"`
- Do NOT normalize the key name to `"PostToolUse"` or any other value
- Do NOT add stdout injection configuration — `PostToolUseFailure` is observation-only (FR-03.7)
- Do NOT add a `timeout` override — the existing 40ms default is sufficient
