# C6: Postinstall — Pseudocode

## Purpose

Pre-download the ONNX model after `npm install`. Must NEVER cause `npm install` to fail. Every code path exits 0.

## File: packages/unimatrix/postinstall.js

```
#!/usr/bin/env node

FUNCTION main():
    // Entire body wrapped in try/catch -- unconditional exit 0
    TRY:
        LET resolveBinary
        TRY:
            resolveBinary = require("./lib/resolve-binary")
        CATCH:
            // Module resolution failed (shouldn't happen but be defensive)
            console.warn("[unimatrix] postinstall: could not load resolve-binary module, skipping model download")
            process.exit(0)
        END TRY

        LET binaryPath
        TRY:
            binaryPath = resolveBinary()
        CATCH error:
            // Platform binary not available (unsupported OS/arch)
            console.warn("[unimatrix] postinstall: platform binary not available (" + error.message + "), skipping model download")
            process.exit(0)
        END TRY

        // Execute model-download subcommand
        LET { execFileSync } = require("child_process")
        TRY:
            execFileSync(binaryPath, ["model-download"], {
                stdio: ["ignore", "inherit", "inherit"],  // stdin ignored, stdout+stderr forwarded
                timeout: 300000  // 5 minute timeout
            })
            console.log("[unimatrix] postinstall: ONNX model ready")
        CATCH execError:
            // Download failed (network, disk, binary crash) -- warn only
            console.warn("[unimatrix] postinstall: model download failed, will retry on first server start")
            IF execError.stderr:
                console.warn("[unimatrix]   " + execError.stderr.toString().trim())
            END IF
        END TRY

    CATCH outerError:
        // Catch-all: absolutely nothing should break npm install
        console.warn("[unimatrix] postinstall: unexpected error: " + outerError.message)
    END TRY

    process.exit(0)

main()
```

## Guarantees

1. **Always exits 0**. The outer try/catch ensures no uncaught exception can produce a non-zero exit.
2. **No project file modifications**. Only touches `~/.cache/unimatrix-embed/` via the Rust binary.
3. **Timeout protection**. The `execFileSync` has a 5-minute timeout to prevent hanging `npm install` indefinitely.
4. **Graceful degradation**. If binary is missing, network is down, or disk is full, the user sees a warning but `npm install` succeeds. The server's `ensure_model()` will lazy-download on first startup.

## Error Handling

Every error path prints a `[unimatrix]` prefixed warning to stderr/console.warn and exits 0.

| Condition | Behavior |
|-----------|----------|
| resolve-binary module load fails | Warn, exit 0 |
| Platform binary not found | Warn with platform info, exit 0 |
| Binary execution fails | Warn "model download failed", exit 0 |
| Timeout (>5 min) | execFileSync kills child, warn, exit 0 |
| Any uncaught exception | Outer catch: warn, exit 0 |

## Key Test Scenarios

1. Normal: binary found, model downloaded -> exit 0, "ONNX model ready" printed.
2. Binary missing (unsupported platform) -> warn, exit 0.
3. Network unavailable (binary exec fails) -> warn "model download failed", exit 0.
4. Model already cached (model-download exits 0 quickly) -> exit 0.
5. Binary crashes (exit code 1) -> warn, exit 0.
6. Timeout after 5 minutes -> warn, exit 0.
