# C3: Binary Resolution — Pseudocode

## Purpose

Determine which platform package is installed and return the absolute path to its binary. Shared by the JS shim (C2), init command (C4), and postinstall (C6).

## File: packages/unimatrix/lib/resolve-binary.js

```
CONST PLATFORMS = {
    "linux-x64": "@dug-21/unimatrix-linux-x64"
}

FUNCTION resolveBinary() -> string:
    // 1. Check environment variable override (development/testing)
    IF process.env.UNIMATRIX_BINARY:
        LET envPath = process.env.UNIMATRIX_BINARY
        IF NOT fs.existsSync(envPath):
            THROW Error("UNIMATRIX_BINARY points to non-existent file: " + envPath)
        END IF
        RETURN fs.realpathSync(envPath)
    END IF

    // 2. Determine current platform key
    LET platformKey = process.platform + "-" + process.arch
    // Normalize: node reports "x64" for arch, "linux" for platform
    // platformKey will be "linux-x64" on the target system

    // 3. Look up package name for this platform
    LET packageName = PLATFORMS[platformKey]
    IF NOT packageName:
        THROW Error(
            "Unsupported platform: " + platformKey + "\n" +
            "Supported platforms: " + Object.keys(PLATFORMS).join(", ") + "\n" +
            "Set UNIMATRIX_BINARY environment variable to use a custom binary path."
        )
    END IF

    // 4. Resolve binary path via require.resolve
    LET binaryPath
    TRY:
        binaryPath = require.resolve(packageName + "/bin/unimatrix")
    CATCH:
        THROW Error(
            "Could not find platform binary package '" + packageName + "'.\n" +
            "Run 'npm install @dug-21/unimatrix' to install.\n" +
            "If using pnpm or yarn, set UNIMATRIX_BINARY to the binary path."
        )
    END TRY

    // 5. Resolve through symlinks to get the real absolute path (ADR-001)
    RETURN fs.realpathSync(binaryPath)

module.exports = resolveBinary
```

## Platform Key Mapping

Node.js `process.platform` + `process.arch` -> platform key:
- `linux` + `x64` -> `"linux-x64"` -> `"@dug-21/unimatrix-linux-x64"`

Future additions (not in scope):
- `darwin` + `arm64` -> `"darwin-arm64"`
- `darwin` + `x64` -> `"darwin-x64"`

## Error Handling

| Condition | Error Message | Exit Behavior |
|-----------|--------------|---------------|
| UNIMATRIX_BINARY set but file missing | "UNIMATRIX_BINARY points to non-existent file: {path}" | Throw |
| Platform not in PLATFORMS map | "Unsupported platform: {key}\nSupported: ..." | Throw |
| require.resolve fails | "Could not find platform binary package..." | Throw |

All errors are thrown, not caught here. Callers (C2, C4, C6) handle them.

## Key Test Scenarios

1. On linux-x64 with package installed: returns absolute real path to `bin/unimatrix`.
2. `UNIMATRIX_BINARY` env set to valid path: returns that path (resolved through symlinks).
3. `UNIMATRIX_BINARY` set to non-existent path: throws with diagnostic.
4. Unsupported platform (e.g., win32-x64): throws listing supported platforms.
5. Platform package not installed (require.resolve fails): throws with install instructions.
6. Returned path is absolute (starts with `/`).
7. Returned path is resolved through symlinks (no symlink components).
