# Test Plan: launcher (`bin/unimatrix.js`: `hook`, `mcp-stdio`, `status`)

Risks: R-01, R-04, R-07, R-10, R-23, R-39, R-32, R-33, R-40. Files: update `cli-routing.test.js`, `shim.test.js` (old string tests updated, not deleted); new `launcher-hook.test.js`, `launcher-status.test.js`.

## hook (AC-03, AC-04 outcomes, R-01)
- `unimatrix hook <EVENT>` with recording stub binary: Node client `main` ran, stub never invoked.
- Exit matrix for every Claude event class x {not project, Linux not installed, macOS/Windows no bundle/no credential, each resolution failure, Linux installed + binary deleted, client throws sync/async, connection error}: exit in {0,1}, never 2; not-a-project: stdout and stderr empty; Linux not installed: stdout empty + exactly one stderr line; macOS/Windows: silent; connection error: existing hook-client code unchanged (D-042). Runs on all three OS via `wiring-e2e` (platform rows parametrised through injected `platform`, plus real-OS row).
- Legacy L1 string `unimatrix hook E` with launcher on PATH: stored, Rust stub not invoked.
- Pass-through (`serve`, daemon): Linux argv + `LD_LIBRARY_PATH` preserved (R-10); darwin/win32 prints "requires the local server (Linux only)", exit 1, stub untouched (D-071).

## mcp-stdio (R-07, R-39, D-043)
Spawn the written MCP argv, send `initialize`: first stdout byte is `{`; from subdir cwd; single process after exec. Inside project not installed -> exit 1, one stderr line, empty stdout (all OS). Outside project (incl. Codex/OpenCode user-scope, headless cwd outside root) -> exit 0, empty stdout/stderr, zero network requests; cloned repo, init never run -> same (D-051).

## status (FR-12g, R-19, R-32, R-33, R-40)
Always exit 0 even when all checks fail; no network, no writes (fixture mtime snapshot). Text output field order exact; `--json` schema keys exact. Cases: `project: none (...)`; each leftover project file -> one `legacy:` line in order `.mcp.json`, `.codex/config.toml`, `opencode.json`, `legacyProjectMcp` same order, none after re-init; `installed: no (missing credential)`; `trust: FAIL <path>`; `mcp node: MISSING <path>` on Windows fallback entry with deleted node; Codex wired never fired -> approval step printed; with `last-hook.json` entry -> time; corrupt `last-hook.json` -> "unknown", no crash. Clone of initialised fixture: no tracked Unimatrix MCP entry; status says not attached + `npx unimatrix init`.

## Rework (D-077): mcp-stdio process model (R-07)
Local (Linux) `mcp-stdio` = spawn Rust with `stdio:'inherit'`, mirror exit code AND signal. Tests (stub binary = `process.execPath` script, POSIX; Windows row uses the D-047 fallback form, see below):
- Zero Node stdout bytes before spawn: spawn the written MCP argv via real `child_process.spawn`, capture stdout; first byte is the stub's first byte, and a stub that writes nothing yields empty stdout (Node wrote none). Also stderr contains no banner on success.
- Inherited stdio: stub echoes stdin line to stdout; bytes round-trip unchanged (large 1 MiB payload, no deadlock); stub's `process.stdin.isTTY`/fd identity equals parent's pipes (no extra buffering layer): assert via `fs.fstatSync(0)` ino equality with the harness-supplied pipe when exposed, else round-trip only.
- Exit code mirrored: stub exits 0, 1, 7, 42 -> launcher exits identically.
- Signal mirrored: stub kills itself with SIGTERM / SIGKILL (POSIX) -> launcher terminates with the same signal (parent observes `signal` set, not an exit code); harness sends SIGTERM/SIGINT to launcher -> forwarded to child, child gets it once, launcher exits after child (no orphan: pgrep child pid gone within 2 s). Documented 128+n fallback asserted if the pseudocode specifies it.
- Spawn failure (ENOENT/EACCES binary) -> exit 1, one stderr line naming the path, stdout empty.
- No-exec equivalence: process trace shows exactly one Node and one stub process alive during the session (launcher stays as the parent; documented per D-077); this is not a failure.
- Windows MCP fallback entry written form `{command:<node>, args:[shim-entry.js,'mcp-stdio']}` executed by `spawn` on every OS using stub node paths: initialize round-trip works through shim-entry -> launcher -> (cloud) mcp-bridge stub.

## Rework: status additions
- `status` reports Linux install with platform binary missing from `pkg/<version>/node_modules` (D-078) as `binary: MISSING <path>`; exit 0.
- effectiveEnv-dependent launcher tests (if any) are SEPARABLE under H1 (see resolve-runtime.md).
