# nan-024 Pseudocode Overview

Binding inputs: DECISIONS.md D-001..D-083 (later wins), ADR-001..006, ARCHITECTURE.md, SPECIFICATION.md, RISK-TEST-STRATEGY.md rev 6, research/codex-hooks-evidence.md.
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
| 14A | ci-and-docs Part A (OQ-6 audit) and Part C (workflows, README, `.gemini/settings.json`, release notes, AC-11 grep rule) | `.github/workflows/*.yml`, README, `.gemini/settings.json` | yml/md/CI (CI/docs agent) |
| 14B | ci-and-docs Part B (headless contract, D-053) | `lib/hook-client/remote-resolve.js`, `lib/hook-client/mcp-bridge.js`, `docs/client-setup.md`, `test/headless-smoke.test.js` | JS + md (uni-js-dev) |

Language flags: no component spans crates/ and packages/. Mixed-language items: (a) shim = sh AND cmd files in one component (no JS; one agent); (b) ci-and-docs is SPLIT into two work packages: Part B (headless JS, D-053) -> uni-js-dev; Parts A/C (audit, yml, md, CI) -> CI/docs agent. The two packages touch disjoint files (`docs/client-setup.md` is Part B only; README, `.gemini/settings.json`, workflows are Part A/C only).
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
effectiveEnv(env, cwd, home) -> env copy with UNIMATRIX_BUNDLE/UNIMATRIX_REMOTE_* removed unless headless-valid (D-052)   // PENDING H1
codexHome(env, home) -> {dir, warning?} ; detectsCodex({root, env, home, harnessSel}) -> boolean     // codex-install.js (D-081; relative/invalid CODEX_HOME rule is DEFAULT, flagged)
isUnimatrixCommand(cmd) -> boolean ; isUnimatrixMcpEntry(name, entry) -> boolean
buildShimCommand(event, provider, platform, home) -> string
buildMcpEntry(platform, home, node) -> {command, args, env}         // lib/mcp-entry.js
main(argv) -> Promise<void>                          // hook-client; argv[2] = event (existing convention)
```
Signature extensions (additive, ACCEPTED by D-080; contract text in ARCHITECTURE.md): `resolveRuntime` optional `allowSelf` (init-time, before an install exists); `buildShimCommand` takes `home`; `isInstalled` takes `{env, home}`.
**PENDING H1 (not in the architecture contract):** `effectiveEnv(env, cwd, home)`: isolated in one function in project-state.md with three listed call sites (remote-resolve, hook-client config, launcher hook-route/mcp-stdio) and a removal recipe. Also PENDING H1: ADR-002 trust bootstrap (sibling `~/.unimatrix/bin/trust.js`) in shim-entry, install-store, trust.

Constants: `UHOME(home) = join(home, ".unimatrix")`; shim path `UHOME/bin/unimatrix` (win32 `.cmd`); `UHOME/bin/shim-entry.js`; `UHOME/install.json`; `UHOME/redirect.json`; `UHOME/trust.json`; `UHOME/pkg/<version>/`; `UHOME/<hash>/root`, `remote.json`, `last-hook.json`. `OWNED_USER_FILES` list for dry-run: `~/.unimatrix/**`, `~/.claude/settings.json`, `~/.claude.json`, `$CODEX_HOME/hooks.json`, `$CODEX_HOME/config.toml` (default `~/.codex`, D-081), `~/.config/opencode/opencode.json`.
Exit codes at fire time: 0 and 1 only (never 2). Hint/errors to stderr, never stdout.

## Recommended wave plan (dependency order; components inside a wave are parallel)
Wave 0 (tasks, no code deps): Q9 proof is DONE (D-072, workflow exists); OQ-6 audit (ci-and-docs Part A, read-only, CI/docs agent; results D-074/D-075 recorded).
Wave 1 (leaves): hook-ownership, trust, project-state, resolve-runtime (+resolve-binary wrapper), hook-client (`main(argv)`, `last-hook.json`, config env sanitize needs project-state only as an interface; stub-free because project-state signature is fixed here).
Wave 2: shim (templates), shim-entry (inlines predicate, requires trust sibling), install-store (needs trust + shim templates + project hash), merge-settings (+ claude-json, mcp-entry; needs hook-ownership), ci-and-docs Part B headless contract (uni-js-dev; needs project-state `headlessEnvSet`/`headlessRootContains`, and `effectiveEnv` only if H1 keeps it; includes the `docs/client-setup.md` edit).
Wave 3: launcher (hook-route, status, mcp-stdio; needs project-state, trust, resolve-runtime, hook-client, hook-ownership, claude-json readers), codex-install (needs hook-ownership, merge-settings, mcp-entry), opencode-install (needs mcp-entry, hook-ownership).
Wave 4: wire-init (orchestrates all writers; needs install-store, merge-settings, codex-install, opencode-install).
Wave 5: ci-and-docs Parts C (CI/docs agent; E2E matrix Linux/macOS/Windows, latency gate, `parity-layer2*` gating, README/.gemini/release notes). Latency result may flip `PRENODE_ENABLED` in install-store (feedback edge to Wave 2).
Test updates (`merge-settings.test.js`, `dogfood-effect.test.js`, `init-remote.test.js`, `codex-install.test.js`, `opencode-install.test.js`, `wire.test.js`, `resolve-binary.test.js`, `cli-routing.test.js`, `shim.test.js`) travel with the component that changes the asserted behaviour; update, never delete.

## Cross-cutting design points (decisions made in pseudocode; see Open Questions)
1. shim-entry is self-contained for pre-trust logic: it inlines `isUnimatrixProject` and the minimal installed decision (Node builtins only), and loads `trust.js` from a sibling copy in `~/.unimatrix/bin/` written by install-store. After the trust check passes it loads everything else from `packageRoot`.
2. `bin/unimatrix.js` exports `run(args, ctx)` and only auto-runs under `require.main === module`, so shim-entry can call it in-process (R-04).
3. `mcp-stdio` local transport (D-077): Node cannot `exec`; launcher spawns the Rust binary with inherited stdio and mirrors exit code AND signal; no Node stdout bytes before the spawn (R-07). This counts as ADR-004's "exec"; no ADR change.
3a. Pre-Node shell check (D-079): clause (d) ancestor `.git` file and the loosened headless clause exist ONLY in the shell pre-check; Node's predicate stays D-052 exact; superset parity test specified in shim.md.
3b. Stable copy (D-078): install-store copies the Linux platform binary package into `pkg/<version>/node_modules`; size measured in Stage 3c.
3c. Codex (D-081): one detection rule (`detectsCodex`) governs hooks and MCP; destination `$CODEX_HOME` (default `~/.codex`).
3d. Node record (D-082): init exits 1 if node cannot be recorded (install-store); `wire`/upgrade-without-re-record use a separate path-exists preflight (wire-init).
4. Env tier (D-052) is enforced by `isUnimatrixProject` (containment) everywhere; PENDING H1 adds an extra single-function strip (`effectiveEnv`), applied at the top of `remote-resolve.resolveRemote` (so hook-client config and mcp-bridge cannot forget it) and by the launcher. `project-state.js` lazy-requires `hook-client/config.js` to avoid a require cycle.

## Open questions and resolutions (rework)
Resolved (by decision, applied in the files named):
- OQ-4 gemini/README replacement: D-083 architect forms and AC-11 grep rule used verbatim (ci-and-docs Part C).
- OQ-5 Codex detection / `$CODEX_HOME`: D-081 (codex-install `detectsCodex`, `codexHome`; wire-init).
- OQ-2 D-066 preflight scope: D-082 (wire-init; install-store `NodeNotRecordableError`).
- Prior items 6 (worktree/headless pre-check), 7 (binary in stable copy), 8 (additive signatures), 9 (`mcp-stdio` spawn): D-079, D-078, D-080, D-077.
Still pending a human decision (H1), isolated so either answer has no ripple:
- OQ-1 ADR-002 trust bootstrap (sibling `~/.unimatrix/bin/trust.js`): shim-entry.md, install-store.md, trust.md.
- `effectiveEnv`: project-state.md (definition + removal recipe + three call sites).
Open, flagged DEFAULT (conservative defaults chosen; unspecified by D-081):
- Relative/invalid `$CODEX_HOME` -> ignored, `~/.codex` used, warning (codex-install.md).
- Codex MCP write follows the same detection rule as hooks; the old `--harness codex-cli` opt-in gate for a new MCP entry is dropped (codex-install.md).
Still open (no answer yet):
- OQ-3: confirm `test/check-zero-deps.js` and `check-hook-client-size.js` accept `config.js`/`remote-resolve.js` requiring `../project-state.js`.
- R-38: exact `~/.claude.json` project-key spelling (symlink/realpath, Windows drive case) unverified; CI check specified.
- Trust check refuses symlinked redirect/install files (conservative); may affect #1025 dogfood.
- Hook-client exit code: D-042 read as "unchanged" (client never sets nonzero for connection errors).
- Windows pre-check: junction/symlinked cwd residual (R-29); keep pre-check disabled on Windows unless latency forces it.
- Gemini reference form uses `$HOME` (Unix text only); Windows Gemini shell semantics unverified (architect flag).
