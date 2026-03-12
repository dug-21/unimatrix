# C4: Init Command — Pseudocode

## Purpose

Deterministic, non-interactive, idempotent project wiring. Configures MCP server, hooks, skills, and pre-creates the database. Implemented in JavaScript per ADR-003.

## File: packages/unimatrix/lib/init.js

```
IMPORT resolveBinary FROM "./resolve-binary.js"
IMPORT mergeSettings FROM "./merge-settings.js"
IMPORT { execFileSync } FROM "child_process"
IMPORT fs, path FROM "node:fs", "node:path"

CONST HOOK_EVENTS = [
    "SessionStart", "Stop", "UserPromptSubmit",
    "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"
]

ASYNC FUNCTION init(options):
    LET dryRun = options.dryRun || false
    LET actions = []  // Accumulator for summary output

    // Step 1: Resolve project root
    LET projectRoot = detectProjectRoot(process.cwd())
    actions.push("Project root: " + projectRoot)

    // Step 2: Resolve binary path
    LET binaryPath = resolveBinary()
    actions.push("Binary: " + binaryPath)

    // Step 3: Write/merge .mcp.json
    LET mcpActions = writeMcpJson(projectRoot, binaryPath, dryRun)
    actions.push(...mcpActions)

    // Step 4: Merge hooks into .claude/settings.json
    LET settingsPath = path.join(projectRoot, ".claude", "settings.json")
    LET settingsResult = mergeSettings(settingsPath, binaryPath, { dryRun })
    actions.push(...settingsResult.actions)

    // Step 5: Copy skill files
    LET skillActions = copySkills(projectRoot, dryRun)
    actions.push(...skillActions)

    // Step 6: Pre-create database (exec Rust binary)
    IF NOT dryRun:
        TRY:
            execFileSync(binaryPath, ["version", "--project-dir", projectRoot], {
                stdio: "pipe"
            })
            actions.push("Database: pre-created at ~/.unimatrix/{hash}/")
        CATCH error:
            THROW Error("Database creation failed: " + error.stderr?.toString() || error.message)
        END TRY
    ELSE:
        actions.push("[dry-run] Would pre-create database via: unimatrix version --project-dir " + projectRoot)
    END IF

    // Step 7: Validate binary
    IF NOT dryRun:
        TRY:
            LET versionOutput = execFileSync(binaryPath, ["version"], {
                stdio: "pipe", encoding: "utf8"
            }).trim()
            actions.push("Validation: " + versionOutput)
        CATCH error:
            THROW Error("Binary validation failed: " + error.stderr?.toString() || error.message)
        END TRY
    ELSE:
        actions.push("[dry-run] Would validate binary via: unimatrix version")
    END IF

    // Step 8: Print summary
    printSummary(actions, dryRun)

module.exports = init
```

### detectProjectRoot(startDir) -> string

```
FUNCTION detectProjectRoot(startDir):
    LET current = path.resolve(startDir)
    LOOP:
        IF fs.existsSync(path.join(current, ".git")):
            RETURN current
        END IF
        LET parent = path.dirname(current)
        IF parent === current:
            // Reached filesystem root
            THROW Error(
                "Could not find project root (.git directory).\n" +
                "Run this command from within a git repository."
            )
        END IF
        current = parent
    END LOOP
```

### writeMcpJson(projectRoot, binaryPath, dryRun) -> string[]

```
FUNCTION writeMcpJson(projectRoot, binaryPath, dryRun):
    LET mcpPath = path.join(projectRoot, ".mcp.json")
    LET actions = []
    LET existing = {}

    IF fs.existsSync(mcpPath):
        TRY:
            existing = JSON.parse(fs.readFileSync(mcpPath, "utf8"))
        CATCH parseError:
            THROW Error(
                "Malformed .mcp.json at " + mcpPath + ": " + parseError.message + "\n" +
                "Fix the JSON syntax and re-run 'npx unimatrix init'."
            )
        END TRY
        actions.push("Updated .mcp.json (preserved existing servers)")
    ELSE:
        actions.push("Created .mcp.json")
    END IF

    // Ensure mcpServers key exists
    IF NOT existing.mcpServers:
        existing.mcpServers = {}
    END IF

    // Add/update the unimatrix entry with absolute path
    existing.mcpServers.unimatrix = {
        command: binaryPath,
        args: [],
        env: {}
    }

    IF NOT dryRun:
        fs.writeFileSync(mcpPath, JSON.stringify(existing, null, 2) + "\n", "utf8")
    ELSE:
        actions[actions.length - 1] = "[dry-run] " + actions[actions.length - 1]
    END IF

    RETURN actions
```

### copySkills(projectRoot, dryRun) -> string[]

```
FUNCTION copySkills(projectRoot, dryRun):
    LET actions = []
    LET targetDir = path.join(projectRoot, ".claude", "skills")
    LET sourceDir = path.join(__dirname, "..", "skills")

    // Ensure .claude/skills/ exists
    IF NOT dryRun:
        fs.mkdirSync(targetDir, { recursive: true })
    END IF

    // Enumerate skill directories in the bundled skills/
    LET skillDirs = fs.readdirSync(sourceDir, { withFileTypes: true })
        .filter(d => d.isDirectory())
        .map(d => d.name)

    FOR EACH skillDir IN skillDirs:
        LET src = path.join(sourceDir, skillDir)
        LET dst = path.join(targetDir, skillDir)

        IF NOT dryRun:
            // Create skill directory
            fs.mkdirSync(dst, { recursive: true })

            // Copy all files in the skill directory
            LET files = fs.readdirSync(src)
            FOR EACH file IN files:
                LET srcFile = path.join(src, file)
                LET dstFile = path.join(dst, file)

                // Security: reject path traversal
                IF file.includes(".."):
                    THROW Error("Path traversal detected in skill file: " + file)
                END IF

                fs.copyFileSync(srcFile, dstFile)
            END FOR

            actions.push("Copied skill: " + skillDir)
        ELSE:
            actions.push("[dry-run] Would copy skill: " + skillDir)
        END IF
    END FOR

    RETURN actions
```

### printSummary(actions, dryRun)

```
FUNCTION printSummary(actions, dryRun):
    IF dryRun:
        console.log("\n--- Dry Run Summary ---\n")
    ELSE:
        console.log("\n--- Unimatrix Init Complete ---\n")
    END IF

    FOR EACH action IN actions:
        console.log("  " + action)
    END FOR

    console.log("")
    IF NOT dryRun:
        console.log("Next step: start a Claude Code session and run /unimatrix-init")
    END IF
```

## Error Handling

| Condition | Behavior |
|-----------|----------|
| No .git found | Throw with "Could not find project root" |
| Binary not found | Throw from resolveBinary() (C3) |
| .mcp.json malformed JSON | Throw with file path and parse error, do not modify |
| .claude/settings.json malformed | Delegated to mergeSettings (C5): throw with diagnostic |
| DB creation fails (binary exec) | Throw with binary's stderr output |
| Binary validation fails | Throw with binary's stderr output |
| Path traversal in skill file name | Throw, abort skill copy |

## State Machine

Init has no persistent state. It is a single-pass, top-to-bottom execution. Idempotency comes from the merge semantics of each step (overwrite-or-update, never duplicate).

## Key Test Scenarios

1. Clean project (no .mcp.json, no .claude/): all files created, DB initialized, summary printed.
2. Existing project with .mcp.json containing other servers: servers preserved, unimatrix added.
3. Re-run init: no duplicates in settings.json, .mcp.json has one unimatrix entry, skills overwritten.
4. --dry-run: no files created or modified, all actions printed with "[dry-run]" prefix.
5. No .git directory: error with diagnostic message.
6. Binary missing: error from resolveBinary propagated.
7. Init from subdirectory: walks up to .git, wires project root.
8. Malformed .mcp.json: error with diagnostic, no file modification.
9. DB creation failure: error includes binary stderr.
10. Summary suggests running /unimatrix-init as next step.
