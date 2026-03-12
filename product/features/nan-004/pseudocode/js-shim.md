# C2: JS Shim — Pseudocode

## Purpose

Entry point registered in npm `bin` field. Resolves the platform-specific binary and either delegates to JS init logic or exec's the Rust binary with forwarded arguments.

## File: packages/unimatrix/bin/unimatrix.js

Shebang: `#!/usr/bin/env node`

```
IMPORT resolveBinary FROM "../lib/resolve-binary.js"
IMPORT { execFileSync } FROM "child_process"

FUNCTION main():
    args = process.argv.slice(2)

    // Route "init" to JS implementation (ADR-003)
    IF args[0] === "init":
        IMPORT init FROM "../lib/init.js"
        TRY:
            AWAIT init({ dryRun: args.includes("--dry-run") })
        CATCH error:
            process.stderr.write("unimatrix init failed: " + error.message + "\n")
            process.exit(1)
        RETURN
    END IF

    // All other subcommands: resolve binary and exec
    LET binaryPath
    TRY:
        binaryPath = resolveBinary()
    CATCH error:
        process.stderr.write(error.message + "\n")
        process.exit(1)
    END TRY

    TRY:
        execFileSync(binaryPath, args, { stdio: "inherit" })
    CATCH error:
        // execFileSync throws on non-zero exit code
        // error.status contains the exit code from the child process
        IF error.status IS NOT null:
            process.exit(error.status)
        ELSE:
            // Signal death or spawn failure
            process.stderr.write("Failed to execute unimatrix: " + error.message + "\n")
            process.exit(1)
        END IF
    END TRY

main()
```

## Routing Logic

| argv[2] | Destination | Mechanism |
|---------|-------------|-----------|
| `init` | `lib/init.js` | Direct JS import, async |
| `hook` | Rust binary | `execFileSync` |
| `export` | Rust binary | `execFileSync` |
| `import` | Rust binary | `execFileSync` |
| `version` | Rust binary | `execFileSync` |
| `model-download` | Rust binary | `execFileSync` |
| (none) | Rust binary | `execFileSync` (MCP server mode) |
| `--version` | Rust binary | `execFileSync` |

## Error Handling

- **Binary not found**: `resolveBinary()` throws with a message listing supported platforms. Shim prints to stderr, exits 1.
- **Binary execution failure**: `execFileSync` throws. If the child process exited with a code, propagate that code. If the spawn itself failed (ENOENT, EACCES), print diagnostic and exit 1.
- **Init failure**: Caught at top level, printed to stderr, exit 1.

## Key Test Scenarios

1. `npx unimatrix init` routes to JS init logic (does not invoke Rust binary for init).
2. `npx unimatrix hook SessionStart` invokes Rust binary with `["hook", "SessionStart"]`.
3. `npx unimatrix export` invokes Rust binary with `["export"]`.
4. `npx unimatrix` (no args) invokes Rust binary with `[]` (MCP server mode).
5. `npx unimatrix --version` invokes Rust binary with `["--version"]`.
6. Rust binary exits code 1 -> shim exits code 1.
7. Rust binary exits code 0 -> shim exits code 0.
8. Binary not found -> stderr message lists supported platforms, exits 1.
