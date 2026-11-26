# nan-024 Architecture: Every Harness on the Node Client

## System Overview
Hook config points at the user-level shim by its absolute resolved path (D-027). Every install, local or cloud, is per-machine (D-046). The shim, launcher and client resolve runtime state through one resolver and one project predicate (D-045). Linux local: hook -> shim -> launcher (in-process) -> Node client -> UDS -> Rust daemon; MCP -> shim -> Rust stdio. Cloud (Linux, macOS, Windows): hook -> shim -> Node client -> HTTPS; MCP -> shim -> mcp-bridge -> HTTPS (D-002). macOS/Windows are cloud-only; no binary there is normal (D-032). Rust `hook` is never invoked by shipped wiring.

## Component Breakdown
| Component | File | Responsibility | ADR |
|---|---|---|---|
| Shim script | `lib/shim/unimatrix.sh`, `lib/shim/unimatrix.cmd` -> `~/.unimatrix/bin/unimatrix[.cmd]` | Recorded node -> PATH node -> loud hint; optional pre-Node check (ADR-005: root marker, headless env, ancestor `.git` file); exec `shim-entry.js` | 001, 005 |
| Shim entry | `lib/shim-entry.js` -> `~/.unimatrix/bin/shim-entry.js` | Project predicate (+ inlined `effectiveEnv`), trust check via sibling `./trust.js`, read `install.json`, require launcher in-process | 001, 002, 004 |
| Stable copy | `lib/install-store.js` (new) | `pkg/<version>/` (Linux: incl. platform binary package in `node_modules/`, D-078), atomic `install.json` swap, prune, record node path (init exits 1 if it cannot, D-082), write `root` markers | 001, 005 |
| Runtime resolver | `lib/resolve-runtime.js` (new) | Client: redirect > install. Binary (local only): env > redirect > install | 002 |
| Trust check | `lib/trust.js` (new; source) -> `~/.unimatrix/bin/trust.js` (installed copy, D-090) | Self-contained (Node built-ins only, no require from the package dir); atomic tmp+rename refresh on init/upgrade; loaded by `shim-entry.js` before anything from `packageRoot`; in the trust set. POSIX uid/mode per event; Windows full ACL at init + identity compare per event | 002 |
| Project state | `lib/project-state.js` (new) | `isUnimatrixProject` (D-045), `isInstalled`, transport | 004 |
| Launcher | `bin/unimatrix.js` | Routes `hook`, `mcp-stdio` (new; local = spawn Rust with inherited stdio, mirror exit code/signal, this is ADR-004's "exec", D-077), `status` (new), init/wire/mcp-bridge; pass-through (Linux) | 004 |
| Ownership predicate | `lib/hook-ownership.js` (new) | Single owned-command test; table of forms | 003 |
| Settings merge | `lib/merge-settings.js` | Shim-form hooks to `settings.local.json`; cross-scope removal | 003 |
| Wire/init | `lib/wire.js`, `lib/init.js` | Install shim + store; dry-run lists user-level edits; gitignore warning; print Codex approval step | 001, 003 |
| Codex writer | `lib/codex-install.js` | Hooks to `$CODEX_HOME/hooks.json` (default `~/.codex`) when Codex is detected (D-081); project-scope removal; MCP to `$CODEX_HOME/config.toml` | 001, 003 |
| OpenCode | `lib/opencode-install.js` | Provisioning removed from init; MCP only | 006 |
| Hook client | `lib/hook-client/index.js` (canonical: `packages/unimatrix/lib/hook-client/index.js`; bare `hook-client/index.js` in ADR-001/003 is the suffix matched in legacy commands) | Export `main(argv)`; guard auto-run; write `last-hook.json` | 004 |

## Component Interactions
1. `init`/`wire` -> `install-store` copies package, records node path, writes shim + `shim-entry.js` + `trust.js` (atomic tmp+rename, D-090) + `install.json` + `~/.unimatrix/<hash>/root` with explicit modes/ACLs (Windows: full ACL check, write `trust.json`) -> `buildShimCommand(event, provider?, platform, home)` -> `mergeSettings` removes all `isUnimatrixCommand` entries across managed scopes, inserts one per event in the destination (D-026).
2. Harness -> shell runs `'<abs>/.unimatrix/bin/unimatrix' hook E` (Windows `"<abs>/.unimatrix/bin/unimatrix.cmd" hook E`) -> shim -> [pre-Node check if enabled] -> node `shim-entry.js` -> `isUnimatrixProject` (null: exit 0 silent) -> trust check -> `isInstalled` -> `resolveRuntime` -> launcher `hook` -> `client.main(argv)` over UDS or HTTPS -> `last-hook.json`.
3. Exits: ADR-004 table.

## Integration Surface
| Point | Signature / shape | Source |
|---|---|---|
| Pre-Node check (shim) | Filter only, builtins only; starts Node on: (a) headless env set and `UNIMATRIX_PROJECT_ROOT` non-empty (no containment test), (b) cwd equal/inside a `root` marker, (c) store dir without `root`, (d) ancestor `.git` regular file (worktrees). Else exit 0. Must be a lossless superset of `isUnimatrixProject`; superset proof test required (D-079). Node's predicate is unchanged (D-052) | `lib/shim/unimatrix.sh`/`.cmd`, ADR-005 |
| `isUnimatrixProject` | `(cwd, env, home) -> {hash, kind: "local"\|"cloud"\|"headless"} \| null`; project iff `~/.unimatrix/<hash>/` or `~/.unimatrix/<hash>/remote.json` exists, or `UNIMATRIX_BUNDLE`/`UNIMATRIX_REMOTE_*` set with `UNIMATRIX_PROJECT_ROOT` containing cwd (D-052) | `lib/project-state.js`, inlined in `shim-entry.js` |
| `isInstalled` | `(project, platform, {env, home}) -> {installed: boolean, transport: "local"\|"cloud"\|null}`. macOS/Windows: bundle or credential store entry (D-032). Linux: valid `install.json` or credential. Mode: credential/headless -> cloud; else Linux and installed -> local, unresolvable binary = Broken (exit 1) | `lib/project-state.js` |
| `resolveRuntime` | `({env, home, platform, transport, allowSelf?}) -> {binary: string\|null, clientRoot: string, binarySource: "env"\|"redirect"\|"install"\|null, clientSource: "redirect"\|"install"}`; throws `RuntimeResolutionError{code, source, path}`. `allowSelf` is additive and init-time only (resolve before an install exists; D-080). Additive `allowSelf` is not in ADR-002's signature text; contract text lives here. | `lib/resolve-runtime.js` |
| `effectiveEnv` | `(env, cwd, home) -> env'`: copy of `env` minus `UNIMATRIX_BUNDLE` and all `UNIMATRIX_REMOTE_*` unless `UNIMATRIX_PROJECT_ROOT` is absolute and realpath(cwd) is inside realpath(it). Governs hooks (config/remote resolution, `isInstalled`, transport) and the MCP bridge env tier; no other code reads shell cloud variables (D-052, D-091). Inlined copy in `shim-entry.js`; parity test | `lib/project-state.js` (ADR-004) |
| `checkTrust` | `(paths: string[], platform) -> void`; throws `TrustError{path, reason}`. Installed copy at `~/.unimatrix/bin/trust.js` is what runs; self-contained; test: installed copy == package source, no non-builtin `require` (D-090) | `lib/trust.js` -> `~/.unimatrix/bin/trust.js` (ADR-002) |
| `status` cloud-env field (D-091) | Text, after `transport:`: `cloud env: not set` \| `in effect` \| `IGNORED <NAME>[, <NAME>...]: UNIMATRIX_PROJECT_ROOT is unset; set it to the project root to use them` \| `IGNORED <NAME>[, <NAME>...]: cwd is not inside UNIMATRIX_PROJECT_ROOT <root>; set it to the project root to use them`. `--json`, between `transport` and `trust`: `cloudEnv: {status: "unset"\|"in-effect"\|"ignored", vars: string[], reason: "project-root-unset"\|"cwd-outside-root"\|null, projectRoot: string\|null}`. Names only, never values; exit 0 unchanged | `bin/unimatrix.js` (ADR-004) |
| Redirect | `~/.unimatrix/redirect.json` = `{"binary"?: abs, "client"?: abs}` | ADR-002 |
| Install pointer | `~/.unimatrix/install.json` = `{"version", "packageRoot": abs pkg/<version>, "node": abs}`; replaced by rename | ADR-001 |
| Trust cache (Windows) | `~/.unimatrix/trust.json` = `{"<abs path>": {"size", "mtimeMs", "ino"}}` | ADR-002 |
| Root marker | `~/.unimatrix/<hash>/root` = project root, one line | ADR-005 |
| Hook evidence | `~/.unimatrix/<hash>/last-hook.json` = `{"<provider>": ISO time}` | ADR-004 |
| Env | `UNIMATRIX_BINARY` (binary only); `UNIMATRIX_PROJECT_ROOT` (required with headless env, hooks and bridge) | ADR-002, ADR-004 |
| Package copy (Linux) | `pkg/<version>/node_modules/<platform binary pkg>`; the resolver's installed-binary source (D-078) | `lib/install-store.js` |
| Hook command (Unix/macOS) | `'<abs shim>' hook <EVENT>` (+ ` --provider codex-cli`) | `buildShimCommand(event, provider, platform, home)` in `merge-settings.js` (`home` additive, D-080) |
| Hook command (Windows) | `"<abs shim, forward slashes>.cmd" hook <EVENT>` (+ ` --provider codex-cli`) | same |
| MCP placement | Claude `~/.claude.json` `projects[<abs root>].mcpServers.unimatrix`; Codex `$CODEX_HOME/config.toml` `[mcp_servers.unimatrix]` (default `~/.codex`; D-081); OpenCode `~/.config/opencode/opencode.json`. Owned entries removed from project `.mcp.json` / `.codex/config.toml` / `opencode.json`, foreign servers preserved (D-060) | ADR-003 |
| MCP ownership | `isUnimatrixMcpEntry(name, entry): boolean`; a foreign entry at a per-machine `unimatrix` key is left in place with a warning naming file and key, and no Unimatrix entry is written (D-065) | `lib/hook-ownership.js`, ADR-003 |
| MCP (Unix/macOS) | `command: "sh", args: ["-c", "exec \"$HOME/.unimatrix/bin/unimatrix\" mcp-stdio"]` | ADR-001 |
| MCP (Windows) | `command: "cmd", args: ["/d", "/s", "/c", "\"\"%USERPROFILE%\\.unimatrix\\bin\\unimatrix.cmd\" mcp-stdio\""]` (proven by D-034); fallback `command: <recorded node>, args: ["<abs home>/.unimatrix/bin/shim-entry.js", "mcp-stdio"]` (D-047) | ADR-001 |
| Expansion rule | D-027 no-expansion applies to hook destinations only | ADR-001 |
| Hook destinations | Claude `.claude/settings.local.json`; Codex `$CODEX_HOME/hooks.json` (default `~/.codex`), written whenever Codex is detected: project `.codex/` exists, `$CODEX_HOME` dir exists, `codex` on PATH, or `--harness codex-cli`; no XDG (D-081) | ADR-003, D-026 |
| Launcher verbs | `hook <EVENT> [--provider p]`, `mcp-stdio`, `status`, init/wire/mcp-bridge, pass-through (Linux) | `bin/unimatrix.js` |
| Ownership | `isUnimatrixCommand(cmd): boolean`, `OWNED_FORMS: {name, re}[]` | `lib/hook-ownership.js` |
| Client entry | `main(argv: string[]): Promise<void>` | `lib/hook-client/index.js` |
| Exit codes (all OS) | Not a project / cloned repo: 0 silent. Resolution failure: 1. Connection errors unchanged | ADR-004 |
| Exit codes, project not installed: Linux | 0 + one-line stderr hint (D-023) | ADR-004 |
| Exit codes, project not installed: macOS/Windows | 0 silent; hint only in `status` (D-050) | ADR-004 |

## Entry-Point Coverage
| Entry point | Mechanism |
|---|---|
| `init` local / cloud attach | Same `buildShimCommand` + MCP form, per-machine files; identical per OS family (AC-01, D-040, D-060) |
| Re-run over committed `.mcp.json` entry | Owned entry removed, foreign servers preserved (D-060) |
| Re-run over legacy | `isUnimatrixCommand` table; cross-scope removal (AC-05/13) |
| Harness-fired hook | Shim -> in-process Node; ADR-004 outcomes |
| Codex hooks without project `.codex/` | Written to `$CODEX_HOME/hooks.json` when Codex is detected (D-081) |
| Codex in non-Unimatrix project | Silent exit 0 for hook and `mcp-stdio`; in budget (ADR-005, D-043) |
| Codex-only project, MCP relocated | Still a project via D-045 predicate (D-044) |
| Legacy `unimatrix hook` | Launcher `hook` route (AC-03) |
| Cache clean / reinstall / upgrade | Stable copy; string unchanged (D-028, D-031) |
| `UNIMATRIX_BINARY` / redirect | Resolver (AC-10/14) |
| Lost Codex approval | `unimatrix status` + init-printed step (D-038) |
| `wire --harness opencode` | ADR-006 (AC-09) |
| Headless `npx -y ... mcp-bridge` | Route unchanged (AC-12). Env tier honoured only with `UNIMATRIX_PROJECT_ROOT` set and cwd inside it (D-052/D-053) |

## Delivery Order
1. First task: Windows CI run proving Claude Code and Codex pass the Windows MCP argv verbatim (Q9, D-034). On failure, swap to the D-047 fallback (recorded node + `shim-entry.js mcp-stdio`) and proceed.
2. Delivery task: `docs/client-setup.md` and `headless-smoke.test.js` updated for required `UNIMATRIX_PROJECT_ROOT` (D-053; amends vnc-050 ADR-006).
3. Repo `.codex/hooks.json` reference config reset is #1025, not AC-11 (D-048).
4. Stage 1 includes the OQ-6 audit (repo `.codex/hooks.json` exempt, D-058): search repo, docs and installers for direct Rust `hook` invocations outside init-written files; escalate any found (D-035).

## Testing Requirements
- E2E (AC-07): run the written command through the harness shell (`sh -c`, `$SHELL -lc`, `cmd.exe /C`, Git Bash) with temp `HOME`/`USERPROFILE`, incl. a path with a space; assert observation + provider.
- Project predicate table: {local store, remote.json, headless env with `UNIMATRIX_PROJECT_ROOT` containing cwd, headless env without it, headless env with cwd outside it, none}; same table for the MCP bridge env tier (D-052) x {root cwd, subdir cwd, nested repo}; inlined copy == `project-state.js` == `config.js` hash.
- Exit matrix: {not a project, cloned repo (silent), macOS/Windows without bundle/credential (silent), Linux store without install (hint), Linux installed without binary (exit 1), each resolution failure, connection error} x {hook, `mcp-stdio`} x {Linux, macOS, Windows}.
- Override matrix (AC-14): {env only, redirect only, env + redirect, none, invalid redirect, malformed redirect} x {hook, `mcp-stdio`, pass-through}; assert binary and client separately.
- Trust: POSIX umask 002 init passes; group-writable / foreign-owned refused. Windows: extra-writer ACE refused at init; identity mismatch triggers full recheck; matching path spawns no process.
- `status`: wired-never-fired Codex hook prints approval step; bundle without credential reported.
- Byte-fixed Codex string across simulated upgrade (D-028).
- Pre-Node check superset proof (D-079): fixture matrix {local, remote.json, headless valid/without root/outside root, worktree cwd, nested subdir, prefix sibling, legacy store without `root`, none} against the real shim with a stub node; `isUnimatrixProject != null` implies Node started.
- Preflight (D-082): `wire` and upgrade-without-re-record exit 1 on a missing recorded node; init exits 1 if node cannot be recorded.
- Codex detection (D-081): each trigger writes `$CODEX_HOME/hooks.json`; none -> not written; custom `$CODEX_HOME` honoured.
- `mcp-stdio` local (D-077): exit code and signal mirrored; no Node stdout bytes before spawn (R-07).
- Linux install: `pkg/<version>/node_modules` contains the platform binary; resolver works after the npx cache is removed (D-078).
- Latency: ADR-005.
- CI DoD (D-033): Windows and macOS runners: cloud attach, plus Claude and Codex hooks firing through the shim, observation stored. Linux: local + cloud. `parity-layer2*` as a gate (AC-08). Fixture repos only.

## Modularity (PL-10)
No file exceeds 1,000 code lines: `init.js` 763 (shrinks), `codex-install.js` 621, `opencode-install.js` 608 (shrinks), `merge-settings.js` 428, `bin/unimatrix.js` 283 (+~120 incl. `status`). New logic in new modules.

## Resolved Decisions
| Item | Resolution | Register |
|---|---|---|
| OD-1 exit codes | 0 silent / 0 hint / 1 resolution failure; connection errors unchanged | D-023, D-042 |
| OD-2 override | `UNIMATRIX_BINARY` binary only; client redirect > install | D-024 |
| OD-3 latency | Absolute p95 budgets (ms, cloud / not-a-project): Linux 74/50, macOS 126/85, Windows 164/81, incl. not-a-project; CI report-only; lossless pre-Node check ships disabled on every OS | D-094, D-096, D-088 (D-025 numbers superseded) |
| OD-4 Codex shell/merge | Addendum | D-014 |
| OD-5 destinations | Claude `settings.local.json`, Codex `$CODEX_HOME/hooks.json` | D-026, D-081 |
| R5 | Dropped; all installs per-machine | D-046 |
| R-06 trust | POSIX per event; Windows full ACL at init + identity compare | D-029, D-036 |
| R-09 node | Recorded -> PATH -> loud hint | D-030 |
| Windows | Absolute quoted shim path; `.cmd` shim | D-027 |
| Platform model | macOS/Windows cloud-only | D-032 |
| "Unimatrix project here" | Single predicate | D-045 |
| L-1 Windows trust cost | Identity compare per event | D-036 |
| L-2 / AC-01 | Identical per OS family | D-040 |
| MCP placement (V-1/A-5) | Per-machine only; owned entries removed from project `.mcp.json`, `.codex/config.toml`, `opencode.json` | D-060, D-064 |
| Foreign per-machine `unimatrix` key | Left in place, warned, never overwritten; no Unimatrix entry written | D-065 |
| `status` legacy scope | One `legacy:` line per project file with a leftover entry | D-068 |
| L-3 PowerShell | Unsupported, documented | D-041 |
| L-4 bundle, no credential | Exit 0 silent; reported in `status` | D-037, D-050 |
| Cloned repo, no init | Silent; `status` reports it | D-051 |
| Headless env cross-repo leak | Requires `UNIMATRIX_PROJECT_ROOT` containing cwd (hooks + bridge) | D-052, D-053 |
| Windows MCP fallback | Recorded node + `shim-entry.js mcp-stdio` | D-047 |
| Repo `.codex/hooks.json` reset | #1025; exempt from OQ-6 escalation | D-048, D-058 |
| Windows fallback node staleness | Rewritten on node re-record; spawn-time ENOENT accepted; `status` reports it, init/upgrade preflight exits 1 | D-057, D-066 |
| D-038 server signal | Deferred to #1026 | D-054 |
| L-5 lost Codex approval | `status` + printed step; server signal follow-up | D-038 |
| L-6 gitignore | No append; dry-run warning | D-039 |
| `mcp-stdio` no install | Exit 1 inside project, silent outside | D-043 |
| Q8 Codex-only drop | Gap rejected; predicate independent of config location | D-044 |
| Q9 Windows MCP argv | Proof failed; D-047 fallback form for every Windows install | D-034, D-072 |
| `mcp-stdio` "exec" | Spawn, inherited stdio, mirrored code/signal counts as ADR-004's exec | D-077 |
| Binary in stable copy | Linux platform binary package copied into `pkg/<version>/node_modules` | D-078 |
| Pre-Node check scope | Adds ancestor `.git` file clause and loosened headless clause, pre-check only; superset test | D-079 |
| Additive signatures | `resolveRuntime.allowSelf`, `buildShimCommand home`, `isInstalled {env, home}`; `effectiveEnv` (accepted, D-091) | D-080, D-091 |
| Codex hook destination | `$CODEX_HOME/hooks.json` when Codex detected | D-081 |
| D-066 preflight | wire + upgrade-without-re-record; init exits 1 if node unrecorded | D-082 |
| Gemini/README Rust `hook` replacement | Absolute-shim form, see below | D-083 |
| OQ-6 legacy direct-path | Stage 1 audit | D-035 |

## Reference configs and docs: replacement for Rust `unimatrix hook` (D-083, AC-11)
`.gemini/settings.json` (four commands; Gemini writer stays out of scope, D-007) becomes, per event:
`"command": "\"$HOME/.unimatrix/bin/unimatrix\" hook BeforeTool --provider gemini-cli"` (same for `AfterTool`, `SessionStart`, `SessionEnd`). No `_comment` key (JSON has no comments and Gemini's schema tolerance is unverified); the explanation lives in the README. The path resolves to the Node shim, which routes to the Node client (ADR-001, ADR-004). The form is Unix reference text; Windows users substitute the absolute `unimatrix.cmd` path (README states this). It is a documented reference config, not init-written.
README (lines ~252, ~254, ~280) never writes the literal `unimatrix hook`; it names the verb as the launcher's `hook` verb:
- ~252: "...the `--provider codex-cli` flag on the launcher's `hook` verb disambiguates attribution. ... Reference configurations are provided at `.gemini/settings.json` (absolute-shim form, `"$HOME/.unimatrix/bin/unimatrix" hook <Event> --provider gemini-cli`; on Windows use the absolute path of `unimatrix.cmd`) and `.codex/hooks.json`; ..." The `hook` verb routes to the Node hook client.
- ~254: "...maps its typed hooks and event bus to Claude-shaped frames and invokes the launcher's `hook <EVENT> --provider opencode` verb (routed to the Node client)..."
- ~280: "...OpenCode events pass `--provider opencode` from the plugin shim..., and may carry `--model <providerID/modelID>` on the launcher's `hook` verb..."
**AC-11 check (unchanged wording; a grep over README and `.gemini/settings.json`, not `isUnimatrixCommand`, because the absolute shim form is itself owned):** fail on any match of `(?<!\.unimatrix/bin/)(?<![\w.-])unimatrix(-server)?["']?\s+hook\b`, and on any line containing both `LD_LIBRARY_PATH=` and `hook`. The shim forms pass: `.unimatrix/bin/unimatrix" hook` is excluded by the lookbehind, and `unimatrix.cmd" hook` does not match. The check may be added as a test; it must not narrow AC-11 (it enforces the AC, it does not redefine it). Require the PowerShell-unsupported statement (D-041) in docs.

## Limitations
None open.

## Follow-ups (outside nan-024)
- Server-side "session with MCP calls but zero hook events" signal in `context_status`/briefing (D-038 optional): #1026 (D-054). Not cheap: MCP stdio has no harness session id, so correlation needs Rust server session plumbing.

## Addendum: Codex hook execution and scope merging (OD-4)
Evidence: `research/codex-hooks-evidence.md` (openai/codex @ 06f334aa).

- **Execution.** Codex runs hook `command` via `$SHELL -lc` (fallback `/bin/sh`) on Unix, `%COMSPEC% /C` (fallback `cmd.exe`) on Windows, line wrapped in quotes. The absolute quoted shim path needs no expansion on either (D-027); Codex hooks are supported on Windows. Single quotes are valid in fish.
- **Merge.** Layers merge additively; project `.codex/` disabled when untrusted. Hooks go to `$CODEX_HOME/hooks.json` (default `~/.codex`) only; owned entries removed from project `.codex/hooks.json` and `$CODEX_HOME/config.toml` `[hooks]` (D-026, ADR-003).
- **Trust.** Each hook needs approval; init prints the step; `unimatrix status` reports a wired hook that has not fired (D-038); no hash reads or writes; spec must not name an approval key (unverified).
- **String stability.** Byte-fixed across upgrades (D-028); test asserts it.
- **Cross-project firing.** Hook and `mcp-stdio` exit 0 silently outside a D-045 project, in budget, even with stale install or missing node (D-025, D-043, ADR-005).
- **MCP.** argv, not shell; unaffected by hook trust. User scope `$CODEX_HOME/config.toml` (D-060, D-081); silent outside a project (D-043).
