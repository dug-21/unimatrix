# nan-024 Pseudocode Overview

Binding inputs: DECISIONS.md D-001..D-075 (later wins), ADR-001..006, ARCHITECTURE.md, SPECIFICATION.md, RISK-TEST-STRATEGY.md rev 6, research/codex-hooks-evidence.md.
D-072: Windows MCP entry is ALWAYS the D-047 fallback (`{command:<recorded abs node>, args:[<abs home>/.unimatrix/bin/shim-entry.js,"mcp-stdio"], env:{}}`); D-057/D-066 apply to every Windows install. D-073: Codex passes the hook string raw to `cmd.exe /C`; Windows hook form `"<abs>.cmd" hook E` stands.
All paths are under `packages/unimatrix/` unless noted. No component touches `crates/`; Rust (`hook.rs`, daemon, stdio, store hash) is read-only oracle.

## Components (14) and language

| # | Component | Files | Language |
|---|---|---|---|
| 1 | shim | `lib/shim/unimatrix.sh`, `lib/shim/unimatrix.cmd` (templates) | POSIX sh + batch (packages/ only) |
| 2 | shim-entry | `lib/shim-entry.js` (+ installed sibling copy of `lib/trust.js`) | JS |
| 3 | install-store | `lib/install-store.js` | JS |
| 4 | resolve-runtime | `lib/resolve-runtime.js`, `lib/resolve-binary.js` | JS |
| 5 | trust | `lib/trust.js` | JS |
| 6 | project-state | `lib/project-state.js` | JS |
| 7 | launcher | `bin/unimatrix.js`, new `lib/hook-route.js`, `lib/status.js` | JS |
| 8 | hook-ownership | `lib/hook-ownership.js` | JS |
| 9 | merge-settings | `lib/merge-settings.js`, new `lib/claude-json.js`, new `lib/mcp-entry.js` | JS |
| 10 | wire-init | `lib/wire.js`, `lib/init.js` | JS |
| 11 | codex-install | `lib/codex-install.js` | JS |
| 12 | opencode-install | `lib/opencode-install.js` | JS |
| 13 | hook-client | `lib/hook-client/index.js`, `lib/hook-client/config.js` (env sanitize hook-up) | JS |
| 14 | ci-and-docs | `.github/workflows/*.yml`, README, `.gemini/settings.json`, `docs/client-setup.md`, release notes; Part B headless contract (`lib/hook-client/mcp-bridge.js`, `lib/hook-client/remote-resolve.js`, `test/headless-smoke.test.js`) | yml/md, PLUS JS in Part B |

Language flags: no component spans crates/ and packages/. Two mixed-language items: (a) shim = sh AND cmd files in one component (both templates, no JS; assign to one agent); (b) ci-and-docs Part B (headless-contract, D-053) is JS + md while the rest is yml/md; recommend dispatching Part B to the JS developer in the same wave as project-state consumers, and the remainder to a docs/CI agent. Headless-contract is named explicitly in ci-and-docs.md Part B.
Note: the brief writes `lib/mcp-bridge.js` / `lib/remote-resolve.js`; actual files are `lib/hook-client/mcp-bridge.js` and `lib/hook-client/remote-resolve.js`.

## Data flow (fire time)
harness shell -> `<abs>/.unimatrix/bin/unimatrix[.cmd] hook E` (shim) -> node `bin/shim-entry.js`: inline `isUnimatrixProject` (null -> exit 0) -> trust check -> inline installed decision -> read `install.json` -> `require(<packageRoot>/bin/unimatrix.js).run(args, {preflight})` -> launcher `hook-route` -> `resolveRuntime` -> `require(<clientRoot>/lib/hook-client/index.js).main(argv)` -> UDS/HTTPS -> `last-hook.json`.
Legacy `unimatrix hook E` (PATH): launcher performs the same preflight itself (no `preflight` ctx).
init/wire time: `wire-init` -> `install-store` (pkg copy, shim, shim-entry, install.json, root marker, trust) -> `merge-settings.buildShimCommand` + `mergeSettings` (Claude hooks), `codex-install` (Codex hooks + MCP), `claude-json` (Claude MCP), `opencode-install` (wire only) -> removal of owned entries via `hook-ownership` across scopes.

## Shared contracts (every component file uses these exactly)

```
Platform = "linux" | "darwin" | "win32"          // process.platform
Project  = {hash: string, kind: "local"|"cloud"|"headless", root: string}
InstallJson = {version: string, packageRoot: abs, node: abs}        // ~/.unimatrix/install.json
Redirect    = {binary?: abs, client?: abs}                          // ~/.unimatrix/redirect.json
TrustJson   = {"<abs path>": {size: number, mtimeMs: number, ino: number}}  // Windows only
class RuntimeResolutionError extends Error {code, source, path}     // resolve-runtime.js
class TrustError extends Error {path, reason}                       // trust.js
class UnsafeShimPathError extends Error {path, ch}                  // defined in install-store.js, imported by merge-settings.js (init exit 1)
resolveRuntime({env, home, platform, transport, allowSelf?}) -> {binary|null, clientRoot, binarySource, clientSource}
checkTrust(paths, platform, opts?) -> void          // throws TrustError
isUnimatrixProject(cwd, env, home) -> Project | null
isInstalled(project, platform, {env, home}) -> {installed, transport: "local"|"cloud"|null, reason?}
effectiveEnv(env, cwd, home) -> env copy with UNIMATRIX_BUNDLE/UNIMATRIX_REMOTE_* removed unless headless-valid (D-052)
isUnimatrixCommand(cmd) -> boolean ; isUnimatrixMcpEntry(name, entry) -> boolean
buildShimCommand(event, provider, platform, home) -> string
buildMcpEntry(platform, home, node) -> {command, args, env}         // lib/mcp-entry.js
main(argv) -> Promise<void>                          // hook-client; argv[2] = event (existing convention)
```
Signature extensions beyond the brief (flagged, additive): `resolveRuntime` optional `allowSelf` (init-time, before an install exists); `buildShimCommand` takes `home` (needed for round-trip property test with spaces); `effectiveEnv` (closes cross-repo leak when a store credential exists in another repo); `isInstalled` takes `{env, home}`.

Constants: `UHOME(home) = join(home, ".unimatrix")`; shim path `UHOME/bin/unimatrix` (win32 `.cmd`); `UHOME/bin/shim-entry.js`; `UHOME/install.json`; `UHOME/redirect.json`; `UHOME/trust.json`; `UHOME/pkg/<version>/`; `UHOME/<hash>/root`, `remote.json`, `last-hook.json`. `OWNED_USER_FILES` list for dry-run: `~/.unimatrix/**`, `~/.claude/settings.json`, `~/.claude.json`, `~/.codex/hooks.json`, `~/.codex/config.toml`, `~/.config/opencode/opencode.json`.
Exit codes at fire time: 0 and 1 only (never 2). Hint/errors to stderr, never stdout.

## Recommended wave plan (dependency order; components inside a wave are parallel)
Wave 0 (tasks, no code deps): Q9 proof is DONE (D-072, workflow exists); OQ-6 audit (ci-and-docs Part A, read-only; results D-074/D-075 recorded); headless docs edits (ci-and-docs Part B docs half).
Wave 1 (leaves): hook-ownership, trust, project-state, resolve-runtime (+resolve-binary wrapper), hook-client (`main(argv)`, `last-hook.json`, config env sanitize needs project-state only as an interface; stub-free because project-state signature is fixed here).
Wave 2: shim (templates), shim-entry (inlines predicate, requires trust sibling), install-store (needs trust + shim templates + project hash), merge-settings (+ claude-json, mcp-entry; needs hook-ownership), headless-contract JS (ci-and-docs Part B; needs project-state `effectiveEnv`).
Wave 3: launcher (hook-route, status, mcp-stdio; needs project-state, trust, resolve-runtime, hook-client, hook-ownership, claude-json readers), codex-install (needs hook-ownership, merge-settings, mcp-entry), opencode-install (needs mcp-entry, hook-ownership).
Wave 4: wire-init (orchestrates all writers; needs install-store, merge-settings, codex-install, opencode-install).
Wave 5: ci-and-docs Part C (E2E matrix Linux/macOS/Windows, latency gate, `parity-layer2*` gating, README/.gemini/release notes). Latency result may flip `PRENODE_ENABLED` in install-store (feedback edge to Wave 2).
Test updates (`merge-settings.test.js`, `dogfood-effect.test.js`, `init-remote.test.js`, `codex-install.test.js`, `opencode-install.test.js`, `wire.test.js`, `resolve-binary.test.js`, `cli-routing.test.js`, `shim.test.js`) travel with the component that changes the asserted behaviour; update, never delete.

## Cross-cutting design points (decisions made in pseudocode; see Open Questions)
1. shim-entry is self-contained for pre-trust logic: it inlines `isUnimatrixProject` and the minimal installed decision (Node builtins only), and loads `trust.js` from a sibling copy in `~/.unimatrix/bin/` written by install-store. After the trust check passes it loads everything else from `packageRoot`.
2. `bin/unimatrix.js` exports `run(args, ctx)` and only auto-runs under `require.main === module`, so shim-entry can call it in-process (R-04).
3. `mcp-stdio` local transport: Node cannot `exec`; launcher spawns the Rust binary with `stdio: "inherit"` and mirrors its exit code (R-07 "no stdout before spawn").
4. Env tier (D-052) is enforced in one function (`effectiveEnv`), applied at the top of `remote-resolve.resolveRemote` (so hook-client config and mcp-bridge cannot forget it) and by the launcher. `project-state.js` lazy-requires `hook-client/config.js` to avoid a require cycle.
