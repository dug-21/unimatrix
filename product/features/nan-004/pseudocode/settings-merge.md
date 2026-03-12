# C5: Settings Merge — Pseudocode

## Purpose

Structure-aware merge of Unimatrix hook configuration into `.claude/settings.json`. Isolated module for testability. Implements ADR-004 prefix-match identification.

## File: packages/unimatrix/lib/merge-settings.js

```
CONST UNIMATRIX_PATTERNS = [
    /^unimatrix\s+hook\s/,
    /^unimatrix-server\s+hook\s/,
    /\/unimatrix\s+hook\s/,
    /\/unimatrix-server\s+hook\s/
]

CONST HOOK_EVENTS = [
    "SessionStart", "Stop", "UserPromptSubmit",
    "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"
]

// Matcher per event: "" for session-level, "*" for tool/agent-level
CONST EVENT_MATCHERS = {
    SessionStart: "",
    Stop: "",
    UserPromptSubmit: "",
    PreToolUse: "*",
    PostToolUse: "*",
    SubagentStart: "*",
    SubagentStop: "*"
}

FUNCTION isUnimatrixHook(hookEntry) -> boolean:
    IF NOT hookEntry OR NOT hookEntry.command OR typeof hookEntry.command !== "string":
        RETURN false
    END IF
    RETURN UNIMATRIX_PATTERNS.some(pattern => pattern.test(hookEntry.command))

FUNCTION mergeSettings(filePath, binaryPath, options) -> { actions: string[], content: object }:
    LET dryRun = options.dryRun || false
    LET actions = []
    LET content = {}

    // Step 1: Read existing file
    IF fs.existsSync(filePath):
        LET raw = fs.readFileSync(filePath, "utf8").trim()

        IF raw === "":
            // Empty file: treat as {}
            content = {}
            actions.push("settings.json was empty, initializing")
        ELSE:
            TRY:
                content = JSON.parse(raw)
            CATCH parseError:
                THROW Error(
                    "Malformed .claude/settings.json: " + parseError.message + "\n" +
                    "Fix the JSON syntax manually and re-run 'npx unimatrix init'.\n" +
                    "File: " + filePath
                )
            END TRY
        END IF
    ELSE:
        // File does not exist: start with empty object
        actions.push("Created .claude/settings.json")
    END IF

    // Step 2: Ensure hooks key exists
    IF NOT content.hooks:
        content.hooks = {}
    END IF

    // Validate hooks is an object
    IF typeof content.hooks !== "object" OR Array.isArray(content.hooks):
        THROW Error(
            ".claude/settings.json 'hooks' key is not an object.\n" +
            "Expected: { \"hooks\": { \"EventName\": [...] } }\n" +
            "File: " + filePath
        )
    END IF

    // Step 3: For each hook event, merge the unimatrix entry
    FOR EACH event IN HOOK_EVENTS:
        LET hookCommand = binaryPath + " hook " + event
        LET matcher = EVENT_MATCHERS[event]

        LET newHookEntry = {
            type: "command",
            command: hookCommand
        }

        // The settings format is: hooks.EventName = [ { matcher, hooks: [...] } ]
        IF NOT content.hooks[event]:
            content.hooks[event] = []
        END IF

        LET eventArray = content.hooks[event]
        LET merged = false

        FOR EACH matcherGroup IN eventArray:
            IF matcherGroup.matcher === matcher:
                // Found a matcher group for our matcher value
                IF NOT matcherGroup.hooks:
                    matcherGroup.hooks = []
                END IF

                // Look for existing unimatrix hook to update
                LET existingIndex = -1
                LET duplicateIndices = []
                FOR i = 0 TO matcherGroup.hooks.length - 1:
                    IF isUnimatrixHook(matcherGroup.hooks[i]):
                        IF existingIndex === -1:
                            existingIndex = i
                        ELSE:
                            duplicateIndices.push(i)  // Duplicate to remove
                        END IF
                    END IF
                END FOR

                // Remove duplicates (dedup on re-run, per ADR-004)
                FOR EACH dupIdx IN duplicateIndices REVERSED:
                    matcherGroup.hooks.splice(dupIdx, 1)
                    actions.push("Removed duplicate unimatrix hook for " + event)
                END FOR

                IF existingIndex >= 0:
                    // Update in place
                    matcherGroup.hooks[existingIndex] = newHookEntry
                    actions.push("Updated hook: " + event)
                ELSE:
                    // Append
                    matcherGroup.hooks.push(newHookEntry)
                    actions.push("Added hook: " + event)
                END IF

                merged = true
                BREAK
            END IF
        END FOR

        IF NOT merged:
            // No matcher group found for our matcher value; create one
            eventArray.push({
                matcher: matcher,
                hooks: [newHookEntry]
            })
            actions.push("Added hook: " + event + " (new matcher group)")
        END IF
    END FOR

    // Step 4: Write file
    IF NOT dryRun:
        LET dir = path.dirname(filePath)
        fs.mkdirSync(dir, { recursive: true })
        fs.writeFileSync(filePath, JSON.stringify(content, null, 2) + "\n", "utf8")
    ELSE:
        actions = actions.map(a => "[dry-run] " + a)
    END IF

    RETURN { actions, content }
```

## Merge Algorithm Summary

1. Parse existing JSON (or start with `{}`).
2. Preserve ALL existing top-level keys (permissions, etc.).
3. For each of the 7 hook events:
   a. Find the matcher group matching our expected matcher ("" or "*").
   b. Within that group's hooks array, find unimatrix entries by prefix pattern.
   c. If found: update command in place, remove duplicates.
   d. If not found: append new entry to the hooks array.
   e. If no matcher group: create new matcher group with our entry.
4. Write JSON with 2-space indentation.

## Unimatrix Identification Patterns (ADR-004)

A hook entry is "owned by Unimatrix" if its `command` field matches any of:
- `^unimatrix\s+hook\s` — bare name, current
- `^unimatrix-server\s+hook\s` — bare name, pre-rename
- `\/unimatrix\s+hook\s` — absolute path, current
- `\/unimatrix-server\s+hook\s` — absolute path, pre-rename

This handles upgrades from pre-rename configurations without duplication.

## Error Handling

| Condition | Behavior |
|-----------|----------|
| File does not exist | Start with `{}`, create file |
| Empty file (0 bytes) | Start with `{}` |
| Malformed JSON | Throw with diagnostic, do NOT modify file |
| `hooks` key is not an object | Throw with diagnostic |
| Non-unimatrix hooks present | Preserved exactly, never modified |
| Permissions block present | Preserved exactly |
| Other top-level keys | Preserved exactly |
| Duplicate unimatrix hooks in same event | Deduplicated (keep first, remove rest) |

## Key Test Scenarios

1. **Empty/absent file**: Creates full hooks structure with 7 events.
2. **Permissions only, no hooks key**: Permissions preserved, hooks section added.
3. **Existing non-unimatrix hooks**: Custom hooks preserved, unimatrix appended.
4. **Pre-rename commands** (`unimatrix-server hook ...`): Updated in place, not duplicated.
5. **Absolute path commands** from prior init: Paths updated in place.
6. **Extra top-level keys**: All keys preserved after merge.
7. **Idempotency**: merge -> read back -> merge again -> output identical.
8. **Malformed JSON**: Error thrown, file not modified.
9. **Hooks key is array instead of object**: Error thrown.
10. **Duplicate unimatrix hooks in same event**: Deduplicated to one.
11. **Mixed matchers**: Empty matcher for SessionStart/Stop/UserPromptSubmit, "*" for PreToolUse/PostToolUse/SubagentStart/SubagentStop.
