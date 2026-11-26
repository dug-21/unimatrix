# nan-024 Specification: Every Harness on the Node Client — Portable Wiring

## Objective
Every shipped Claude and Codex hook runs on the Node hook client. Local and cloud installs write identical harness config, and the client resolves binary, transport and credentials at runtime (D-001, D-002). `init` stops provisioning OpenCode plugins (D-004). The released launcher honours a user-level override (D-013, D-024).

## Domain Model
| Term | Definition |
|---|---|
| Harness | Host tool that fires hooks and launches MCP: Claude, Codex, OpenCode. |
| Node client | `packages/unimatrix/lib/hook-client/index.js` (ARCHITECTURE.md). The only hook client. |
| Rust binary | `unimatrix` server/daemon/MCP stdio. Never a hook client. Exists on Linux only; macOS and Windows are cloud-only (D-032). |
| Launcher | `bin/unimatrix.js`. Routes subcommands to JS or execs Rust. |
| Shim | `~/.unimatrix/bin/unimatrix` (sh) and `~/.unimatrix/bin/unimatrix.cmd` (Windows), plus `shim-entry.js` (D-027, ADR-001). Hook entries reference it by absolute resolved path. |
| Install pointer | `~/.unimatrix/install.json`: records the current stable package copy and the recorded Node path. |
| Stable package copy | Versioned package dirs under `~/.unimatrix/`, selected by an atomic pointer swap; old versions pruned (D-031). Used in local and cloud. |
| Wiring form | Literal command string written into harness config (D-003, ADR-001). Called `<FORM>` below. Hook `<FORM>` = absolute resolved shim path + `hook <EVENT>` (+ provider); no `$HOME`, no shell expansion (D-027). |
| Unimatrix entry | A harness hook/MCP entry matched by the ownership predicate. |
| Foreign entry | Any entry not matched by the ownership predicate. |
| Ownership predicate | Single function deciding whether an entry is a Unimatrix entry. Covers every legacy form and `<FORM>`. |
| Legacy forms | L1 `unimatrix hook <E>`; L2 `<path>/unimatrix hook <E>` incl. `LD_LIBRARY_PATH=<dir>` prefix; L3 `node <abs>/hook-client/index.js <E>`; L4 npx-cache absolute (`~/.npm/_npx/<hash>/...`); L5 `npx ... unimatrix ... hook` forms; L6 prior `"$HOME/.unimatrix/bin/unimatrix" hook` form. |
| Hook destination | Where hook entries are written: Claude `settings.local.json`; Codex `~/.codex/hooks.json` (D-026). |
| MCP destination | Per-machine only (D-060): Claude `~/.claude.json` `projects["<abs root>"].mcpServers.unimatrix`; Codex `~/.codex/config.toml` `[mcp_servers.unimatrix]`; OpenCode (`wire` only) `~/.config/opencode/opencode.json`, silent outside a project (D-063). Never a committed `.mcp.json`. |
| Legacy project MCP | An owned Unimatrix entry in project `.mcp.json`, project `.codex/config.toml`, or project `opencode.json` `mcp.unimatrix`; removed by `init` and reported by `status` (D-060, D-062, D-064, D-068). |
| Scope | A config file `init`/`wire` writes or cleans for a harness (Claude project `settings.json`, `settings.local.json`, user settings; Codex `~/.codex/hooks.json`, project `.codex/hooks.json`, `~/.codex/config.toml` `[hooks]`; MCP: `~/.claude.json`, project `.mcp.json`, `~/.codex/config.toml`, project `.codex/config.toml`, project `opencode.json`). |
| Mode | Headless env (only with `UNIMATRIX_PROJECT_ROOT` set and cwd inside it) or credential => cloud; else Linux and Installed => local, where an unresolvable binary is Broken (exit 1); else not installed. |
| Override | `UNIMATRIX_BINARY` env (binary only) or a redirect file in `~/.unimatrix/` (binary and client) (D-024). |
| Resolver | One routine that resolves binary and Node client, shared by hook, MCP and pass-through paths. |
| Unimatrix project here | True when any holds for cwd: local store `~/.unimatrix/<hash>/` exists, OR cloud credential `~/.unimatrix/<hash>/remote.json` exists, OR headless env (`UNIMATRIX_BUNDLE` / `UNIMATRIX_REMOTE_*`) is set together with `UNIMATRIX_PROJECT_ROOT` and cwd is inside that root (D-052). `<hash>` is a local directory key only; cloud identity is the bundle's slug. One definition shared by hooks, MCP bridge, L-4 and Q8 (D-045, D-052). A cloned repo where `init` never ran is not a project (D-051). |
| Installed (D-032, ADR-004) | Linux: valid `install.json`, or a credential (`~/.unimatrix/<hash>/remote.json`, or headless env under D-052). macOS/Windows: a bundle (`UNIMATRIX_BUNDLE`, honoured only under D-052) or a credential store entry (`~/.unimatrix/<hash>/remote.json`). Bundle always means `UNIMATRIX_BUNDLE`; the package copy is the stable package copy, never "bundle". Every install is per-machine (D-046). |
| Trust check | Ownership/permission validation of shim, install pointer, redirect file and resolved `packageRoot` (D-029). |
| R1–R4 | Wiring requirements: R1 no PATH/profile/repo-layout change; R2 performance/no hot-path npx; R3 survive cache clean/reinstall/upgrade or fail loud; R4 touch only Unimatrix entries. R5 (portability) is dropped (D-046). |

## Functional Requirements

### Launcher and routing
- FR-01: `bin/unimatrix.js hook <EVENT>` executes the Node client in-process or as the single Node process; it never execs the Rust binary. (AC-03)
- FR-02: Node-routed verbs are `init`, `wire`, `mcp-bridge`, `mcp-stdio`, `status` and `hook`. Every other verb is a Rust pass-through: on Linux it execs the Rust binary with the existing `LD_LIBRARY_PATH` pass-through; on macOS/Windows it prints `unimatrix: <verb> requires the local server (Linux only)` and exits 1 (ADR-004). (AC-12, SR-12)
- FR-03: Binary precedence: `UNIMATRIX_BINARY` > redirect file > installed package. Client precedence: redirect file > installed package. `UNIMATRIX_BINARY` never affects client resolution, and the client is never inferred from beside the binary (D-024). (AC-14, SR-18)
- FR-04: Hook, MCP and pass-through paths all call the one Resolver. The Resolver resolves the binary only when local transport is selected; on platforms without a binary, "no binary" is a normal state, not an error (D-032). (AC-10, AC-14, SR-16, SR-19, SR-20) Exception: Linux pass-through verbs always resolve the binary (`UNIMATRIX_BINARY` → redirect → installed); if unresolvable they exit 1 with stderr naming the sources tried.
- FR-05: An override that is present but invalid (target missing, not executable, client absent, malformed redirect, trust check failed) is a Broken outcome (FR-12) with stderr naming the source and target. Only an absent override falls back to the installed package. (SR-18, SR-20)
- FR-06: Removing the override reverts resolution to the installed package with no change to repo files. (D-005, D-013)
- FR-06a: Node path resolution: recorded path in `install.json`, then PATH fallback, then a Broken outcome with a loud re-init hint. `init` and upgrade re-record the Node path (D-030).
- FR-06b: The package is run from a stable package copy: versioned dirs under `~/.unimatrix/`, atomic pointer swap on install/upgrade, old versions pruned. Applies to local and cloud (D-031). Survives npx-cache clean (R3).

### Wiring
- FR-07: `init` (local), cloud attach and `wire` write byte-identical `<FORM>` for Claude and Codex hook entries and the same MCP entry across modes on a given machine. No mode, transport or credential appears in written config. MCP entries go only to the MCP destination, including cloud `writeMcpBridgeEntry` (D-060). Config is per-machine; identity holds per OS family, except where the D-047 Windows MCP fallback is in use (D-040, D-046). Windows MCP form: `cmd /d /s /c` with quoted `%USERPROFILE%\.unimatrix\bin\unimatrix.cmd` (ADR-001); argv pass-through is proven in Windows CI as the first delivery task (D-034); on failure delivery swaps to the named fallback, recorded node path + `shim-entry.js mcp-stdio` (D-047). Hook entries contain the absolute resolved shim path. (AC-01, SR-06)
- FR-07a: Hook destinations: Claude hooks → `settings.local.json`; Codex hooks → `~/.codex/hooks.json` only. Owned Codex entries in project `.codex/hooks.json` and `~/.codex/config.toml` `[hooks]` are removed (D-026). (AC-04, AC-13)
- FR-07b: Hook `<FORM>` uses the absolute resolved shim path on every platform: no `$HOME`, no shell expansion (D-027). Unix/macOS: `'<abs shim>' hook <EVENT>`; Windows: `"<abs shim, forward slashes>.cmd" hook <EVENT>`; Codex appends ` --provider codex-cli`. Quoting handles spaces under `sh -c`, `$SHELL -lc`, `cmd.exe /C` and Git Bash (ADR-001). (AC-04)
- FR-07c: On Windows the `.cmd` shim is installed; Codex and Claude hooks are supported on Windows (cloud). (AC-04, AC-07)
- FR-07d: The hook command string is byte-for-byte fixed across upgrades; upgrades change only the stable copy and `install.json` (D-028). (AC-04)
- FR-07f: `init` never edits `.gitignore`. If `.claude/settings.local.json` is not git-ignored, the dry-run and `init` output carry a warning (D-039).
- FR-07g: `init` refuses (exit 1, stderr) a shim path it cannot quote safely: one containing `'` on Unix/macOS, or `%` or `"` on Windows (ADR-001). (AC-04)
- FR-07h: When the Node path is re-recorded (D-030), the Windows fallback MCP entry (D-047) is rewritten. A stale recorded Node path is reported by `unimatrix status` and makes the `init`/upgrade preflight exit 1 with a re-init hint; never silent (D-057). (AC-04)
- FR-07i: `init` removes owned Unimatrix entries (predicate `isUnimatrixMcpEntry(name, entry)`) from project `.mcp.json`, project `.codex/config.toml` and project `opencode.json` (`mcp.unimatrix`, D-064). Foreign servers are preserved byte-for-byte, the file is never deleted, and it is not rewritten when nothing owned is present. A foreign command under the name `unimatrix` is left in place with a warning (D-060, D-062, ADR-003). When a foreign entry already holds the per-machine Unimatrix key (`~/.claude.json` `projects[root].mcpServers.unimatrix`, `~/.codex/config.toml` `[mcp_servers.unimatrix]`, OpenCode user config `mcp.unimatrix`), `init` leaves it in place, never overwrites, writes no Unimatrix MCP entry for that harness, and prints `unimatrix: <file> key <key> holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written`; the `init` exit code is unaffected (D-065, ADR-003). (AC-16)
- FR-07j: `~/.claude.json` edits change only the single `projects["<abs root>"].mcpServers.unimatrix` key, written via tmp file + rename. On parse failure `init` aborts without overwriting (exit 1, stderr). (AC-16)
- FR-07e: `init` prints the one-time Codex hook approval step and writes no Codex trust hashes (D-028). (AC-04)
- FR-08: No written file contains an invocation of the Rust binary as a hook client. (AC-02)
- FR-09: The legacy string arm of `normalizeCommandSource` is removed; one command source feeds Claude and Codex. (D-008)
- FR-10: `<FORM>` satisfies R1, R3, R4 (D-046). (AC-04)
- FR-11: When `<FORM>` cannot resolve the client, Node or (in local transport) the binary at fire time, outcome is Broken (FR-12). (AC-04 R3, SR-15)
- FR-12: Fire-time outcomes (D-023):

  | Outcome | Condition | Exit | stderr |
  |---|---|---|---|
  | Not a Unimatrix project | not "Unimatrix project here" (D-045) | 0 | empty |
  | Not installed (Linux) | project here, not Installed | 0 | one-line hint |
  | Not installed (macOS/Windows) | project here, no bundle and no credential | 0 | empty; hint only in `unimatrix status` (D-050) |
  | Broken | resolution failure: trust, Node, install pointer, override, binary (local) | 1 | message naming cause, source and path |

  Exit 1 applies only to binary/client/install resolution failures; connection errors keep their current codes (D-042). Exit 2 is never used. Causes are distinguished by stderr text. macOS/Windows project with no bundle and no credential (not installed): exit 0 silent; hint shown only by `unimatrix status` (D-050). Cloned repo without `init`: hooks and `mcp-stdio` exit 0 silent; reported by `unimatrix status` (D-051). `mcp-stdio` inside a Unimatrix project but not installed: exit 1 with a one-line hint on Linux and macOS/Windows; outside a project: exit 0 silent (D-043, ADR-004). Hooks and MCP use the same D-045 test. (AC-04, AC-07, SR-03)
- FR-12a0: Project detection runs before the trust check; not-a-project exits 0 silently even with stale install or missing Node (ADR-002, ADR-004).
- FR-12a: The Not-a-Unimatrix-project path is measured (NFR-01). The pre-Node shim check is enabled only if that path breaches on any OS (D-025, ADR-005). The pre-Node check treats as store dirs only direct children of `~/.unimatrix/` whose name is exactly 16 characters, all in `[0-9a-f]`; all other entries (`bin/`, `pkg/`, files) are ignored. It starts Node if: (a) `UNIMATRIX_BUNDLE` or any `UNIMATRIX_REMOTE_*` is set and `UNIMATRIX_PROJECT_ROOT` is set with cwd equal to or inside it; or (b) a store dir's `root` file names a path that cwd equals or is inside (path-component match, not string prefix); or (c) a store dir has no `root` file. Otherwise it exits 0 silently. Shell builtins only, no subprocess. It never silences a D-045 project. (AC-06)

### Trust
- FR-12b: Before use, shim, `install.json`, redirect file (if present) and the resolved `packageRoot` must be owned by the current user and not group- or world-writable, with parent dir mode 0700. Failure is Broken, refused loudly; never skipped (D-029).
- FR-12c: Windows equivalent (ADR-002, D-036): owner SID == current user SID; DACL grants write/modify/full control to no SID other than current user, SYSTEM, Administrators; inheritance disabled on `~/.unimatrix` and `bin`. `init` sets ACLs explicitly, runs the full ACL check and records each file's identity `{size, mtimeMs, ino}` in `~/.unimatrix/trust.json`. Each event compares identity via `statSync`; on mismatch or missing `trust.json` it reruns the full check and, on pass, rewrites `trust.json`. No process spawn on the matching path. Silent skip is forbidden (D-029).
- FR-12c1: `trust.json` is itself trust-checked: POSIX per event (FR-12b rules); Windows via the init-time ACL plus a per-event identity compare of its parent dir. Missing or malformed `trust.json` forces the full re-check.
- FR-12e: Codex approval (D-038): `init` prints the approval step; `unimatrix status` reports a Codex hook that is wired but has never fired (from `last-hook.json`) and prints the approval step; it does not read approval state. The server-side signal is deferred to #1026 (D-054).
- FR-12g: New launcher verb `unimatrix status`: reports a wired hook that has never fired (prints the Codex approval step), a stale recorded fallback Node path (FR-07h) as a text line after `trust:`: `mcp node: ok` | `mcp node: n/a` (no D-047 fallback entry written) | `mcp node: MISSING <path>; run 'npx unimatrix init'`; JSON `mcpNode: {status: "ok"|"n/a"|"missing", path: string|null}` (ADR-004), a macOS/Windows project with no bundle or credential (D-050), and a project where this machine has no store or credential: `project:` line reads `none (this machine has no Unimatrix store or credential for <root>; run 'npx unimatrix init' to attach)` (D-051); legacy contract (D-068, ADR-004): text prints one line per file with a leftover owned entry, in order `.mcp.json`, `.codex/config.toml`, `opencode.json`: `legacy: <file> has a Unimatrix MCP entry; re-run 'npx unimatrix init' to migrate` (`<file>` project-relative; no lines if none); JSON `legacyProjectMcp: string[]` in the same order, `[]` when none (D-038). The hook client writes `~/.unimatrix/<hash>/last-hook.json` = `{"<provider>": ISO time}` after each handled event; `status` reads it (ADR-004).
- FR-12h: `isUnimatrixProject(cwd, env, home)` implements D-045 once in `lib/project-state.js` and is inlined in `shim-entry.js`; a parity test asserts both give the same result over the detection table. `init` writes `~/.unimatrix/<hash>/root` (project root, one line).
- FR-12f: A Unimatrix project here (D-045) never loses hook events silently, including Codex-only projects with MCP in user scope (D-044).

### User-level footprint
- FR-12d: User-level writes and removals (`~/.unimatrix/`, `~/.claude/settings.json`, `~/.codex/hooks.json`, `~/.codex/config.toml`, `~/.claude.json`, `~/.config/opencode/opencode.json`) touch only Unimatrix-owned entries and are listed in a dry-run before applying (D-026).

### Ownership and dedupe
- FR-13: One ownership predicate module matches L1–L6 and `<FORM>`, used by Claude and Codex merges. (SR-08)
- FR-14: The predicate does not match foreign commands, including those containing the substring `unimatrix` (e.g. `my-unimatrix-tool`, `echo unimatrix hook`). (AC-04 R4, SR-09)
- FR-15: Re-running `init` over any legacy form replaces it with `<FORM>`: exactly one Unimatrix entry per event, foreign entries byte-preserved. (AC-05)
- FR-16: After any sequence of `init`/`wire` runs, across all scopes for a harness, at most one Unimatrix entry exists per event, located in the hook destination (FR-07a). (AC-13, SR-17)
- FR-17: L2 direct-path Rust entries are rewritten by `init` (launcher routing cannot catch them).

### OpenCode
- FR-18: `init` writes no `.opencode/plugins/`, no `.opencode/package.json`, and leaves `opencode.json` `plugin[]` unchanged. (AC-09)
- FR-19: `wire --harness opencode` writes the `mcp.unimatrix` entry to `~/.config/opencode/opencode.json` (user scope) under the MCP rules of FR-07, silent outside a project (D-043, D-063). (AC-09)
- FR-20: `opencode-plugin/lib/emit.js` is not modified. Its `unimatrix hook` emission lands on Node via FR-01 when invoked through the launcher. (SR-11)

### Docs
- FR-21: README (lines ~252/254/280) and `.gemini/settings.json` carry no Rust `unimatrix hook` references; release notes state OpenCode = retrieval only. The repo's `.codex/hooks.json` reference config is reset in #1025, not here (D-048). Docs state PowerShell is unsupported as a hook shell (D-041). (AC-11)
- FR-21a: Stage 1 audit (D-035) covers the repo, docs, installers, the packed npm tarball and the OpenCode plugin dist for direct Rust `hook` invocations; findings are escalated, except the repo `.codex/hooks.json` (D-048, D-058).
- FR-22: Headless contract amendment (D-053): `UNIMATRIX_PROJECT_ROOT` joins the required headless variables (vnc-050). `docs/client-setup.md` and `headless-smoke.test.js` are updated. (AC-15)

## Non-Functional Requirements
- NFR-01 Latency (D-025): hook p95 per-event wall time ≤ max(Node baseline p95 + 15 ms, 50 ms). Node baseline = p95 of `node -e 0` measured in the same CI job on the same runner (ADR-005). Applies to every path, including Not-a-Unimatrix-project. Measured in CI on the assembled install path, executing the written `<FORM>` literally, including shim, trust check, Resolver and override lookup. Cloud: ≤ local budget + measured RTT. (AC-06, SR-02)
- NFR-02 Process count: one Node process per hook event (zero on the Not-a-Unimatrix-project path if FR-12a moves the check into the shim); no npx resolution on the hot path; zero Rust processes spawned per hook event. (AC-02)
- NFR-03 Platform (D-032): local mode Linux only; UDS Unix only. macOS and Windows are cloud-only and supported for cloud attach and Claude/Codex hooks. (AC-07)
- NFR-04 Test integrity: test count in touched suites does not decrease; tests pinning old strings are updated. (AC-12, SR-10)
- NFR-05 Isolation: all tests use fixture repos/homes, never this repo's tracked configs or the real `~/.unimatrix/`. (SR-07)
- NFR-06 Security: shim, `install.json`, redirect file and resolved `packageRoot` pass the trust check (FR-12b/12c) before being read or executed; override binary target is validated (exists, executable) before exec; redirect file is read only from `~/.unimatrix/`; failures are Broken and loud, never silent fallback. (SR-18, D-029)

## Acceptance Criteria
| AC | Requirement | Verification |
|---|---|---|
| AC-01 | On a given machine, local `init` and cloud attach write byte-identical Claude and Codex hook and MCP entries in the per-machine destinations (`settings.local.json`, `~/.codex/hooks.json`, `~/.claude.json` project key, `~/.codex/config.toml`); identity holds per OS family, D-047 fallback excepted (D-040, D-046, D-060) | Integration test per OS: clean fixture repo + temp HOME, run both modes, byte-compare written entries; assert no project `.mcp.json` written |
| AC-02 | No written file contains Rust `unimatrix hook`; zero Rust hook processes per event | Scan written files against predicate for L1/L2; process-spawn trace asserts no Rust exec during a fired event |
| AC-03 | `unimatrix hook <EVENT>` via launcher runs Node client | Test invokes launcher, asserts Node client executed and Rust binary not exec'd (stub binary records invocation) |
| AC-04 | R1, R3, R4 met (R5 dropped, D-046) | R1: PATH lacking unimatrix, no profile/repo changes; R3: npx-cache clean, reinstall, upgrade — succeeds or Broken (exit 1, stderr); R4: foreign-entry preservation tests. Hooks: Claude entries only in `settings.local.json`; Codex entries only in `~/.codex/hooks.json`. Hook command is the absolute resolved shim path (no `$HOME`, no shell expansion) on Linux, macOS, Windows; test asserts it byte-for-byte and unchanged across an upgrade; a space-containing HOME fixture runs correctly (Windows quoting). `init` stdout has the Codex approval step, no trust hashes written. Exit outcomes per FR-12, project per D-045 (local store, `remote.json`, or headless env): not a project → 0, empty stderr; project without install → Linux: 0, one-line hint; macOS/Windows: 0, empty stderr, hint only in `unimatrix status` (D-050); resolution broken → 1 with stderr; connection error → current code (D-042). `mcp-stdio` no install: inside project → 1 + one-line hint on every OS, outside → 0 silent (D-043). init refuses shim path with `'` (Unix) or `%`/`"` (Windows) → 1. Node re-recorded → Windows fallback MCP entry rewritten; stale recorded Node path → `status` reports it, init/upgrade preflight exits 1 (D-057). Trust check: g/o-writable or foreign-owned shim/`install.json`/redirect, or parent not 0700, or (Windows) extra-writer ACE → 1 with stderr; umask 002 init passes. Windows: identity unchanged → no full check; identity changed → full check reruns (D-036). Not-a-project exits 0 before trust check. macOS/Windows project with no bundle and no credential → 0 silent, shown only in `unimatrix status` (D-050). Cloned repo without init → hook and `mcp-stdio` 0 silent, shown in `status` (D-051). `.gitignore` unchanged by init; warning when `settings.local.json` not ignored (D-039). `unimatrix status` reports a wired-never-fired hook from `last-hook.json` and prints the Codex approval step (D-038). `isUnimatrixProject` parity test (shim-entry vs project-state). Pre-Node check, if enabled, never silences a D-045 project; sibling dir sharing a string prefix with a recorded root is not matched (path-component match); non-hex/non-16-char entries under `~/.unimatrix/` ignored. `trust.json` tampered/malformed/missing → full re-check. Linux pass-through with unresolvable binary → 1 naming sources tried. Codex-only project with MCP in user scope still stores events (D-044) |
| AC-05 | Re-init over each legacy form leaves one entry per event; foreign preserved | Table-driven test, one case per L1–L6, plus foreign-substring negative cases |
| AC-06 | Hook p95 ≤ max(Node baseline p95 + 15 ms, 50 ms), incl. not-a-project path (D-025) | CI benchmark on assembled install path executing literal written command; `node -e 0` baseline measured in same job; cases: Unimatrix project and non-Unimatrix cwd; fails build over budget |
| AC-07 | Real hook event for Claude and Codex ends in stored observation with correct provider; Windows and macOS cloud attach proven (D-033) | CI E2E executing literal written command read from the hook destination: Linux 2 harnesses × {UDS, HTTPS}; macOS and Windows 2 harnesses × HTTPS after cloud attach, firing through the shim. Codex trust bypassed in CI. Shells: `sh -c`, `$SHELL -lc`, `cmd.exe /C`, Git Bash; temp HOME/USERPROFILE with a space. Each platform adds a non-Unimatrix-cwd run: exit 0, no stderr, no observation |
| AC-08 | `parity-layer2*` (UDS, PreCompact, concurrency) run in CI and pass | CI job present and gating (not advisory) (SR-04) |
| AC-09 | `init` writes no OpenCode provisioning; `wire --harness opencode` writes the MCP entry | Fixture with `.opencode/` + `opencode.json`; assert absent dirs/files, project `opencode.json` `plugin[]` unchanged, `mcp.unimatrix` in `~/.config/opencode/opencode.json` equals FR-07 MCP form |
| AC-10 | `UNIMATRIX_BINARY` governs the binary in hook and MCP paths | Two independent tests, one per path, with stub binary recording invocation; client path unchanged by env |
| AC-11 | Docs carry no Rust hook references; release notes state OpenCode = retrieval only; docs state PowerShell unsupported as hook shell (D-041) | Grep test over README and `.gemini/settings.json`; grep docs for the PowerShell statement; release-notes review |
| AC-12 | Regression-guard suites green; old-string tests updated not deleted | CI runs listed suites (`.mcp.json` guard renamed "removal preserving foreign servers"; fixtures use per-machine MCP destinations); test-count check on touched suites |
| AC-13 | At most one Unimatrix entry per event across all written scopes per harness; Codex: exactly one per event, in `~/.codex/hooks.json` | Per-harness test seeding Unimatrix entries in every scope, running init/wire sequences, counting entries across scopes. Claude: all owned hook entries in `settings.local.json`. Codex: seed `~/.codex/hooks.json`, project `.codex/hooks.json`, `~/.codex/config.toml` `[hooks]`; assert exactly one owned entry per event, all in `~/.codex/hooks.json`; foreign entries unchanged; dry-run lists every user-level edit |
| AC-14 | Redirect resolves binary and client; `UNIMATRIX_BINARY` resolves binary only; none → installed package; for hook, MCP, pass-through | Matrix {redirect, `UNIMATRIX_BINARY`, both, none, invalid redirect, malformed redirect} × {hook, MCP, pass-through}; assert executed binary and client paths (env: binary from env, client from redirect or install; both: binary from env, client from redirect); invalid/malformed → exit 1 + stderr. Non-Linux cloud: no binary resolved, no error |
| AC-15 | Headless env counts as a project only with `UNIMATRIX_PROJECT_ROOT` set and cwd inside it, for hooks and MCP bridge (D-052, D-053) | Test: export `UNIMATRIX_BUNDLE`/`UNIMATRIX_REMOTE_*` + `UNIMATRIX_PROJECT_ROOT=<repo A>`; from cwd in repo B fire a hook and start `mcp-bridge` → not a project, exit 0 silent, zero requests reach the stub remote; without `UNIMATRIX_PROJECT_ROOT` → same; cwd inside repo A → event sent. `headless-smoke.test.js` and `docs/client-setup.md` updated to require `UNIMATRIX_PROJECT_ROOT` |
| AC-16 | `init` migrates MCP to per-machine scope: owned entries removed from project `.mcp.json` / `.codex/config.toml` / `opencode.json` (D-062, D-064), foreign servers preserved; foreign `unimatrix` warned, incl. at per-machine keys (D-065); `~/.claude.json` parse failure aborts (D-060) | Fixtures: project `.mcp.json`, `.codex/config.toml` and `opencode.json` with owned + foreign servers → owned removed, foreign byte-equal, file kept; only-foreign file → not rewritten (mtime/bytes unchanged); foreign command under name `unimatrix` → left in place, warning on stderr; foreign entry pre-seeded at each per-machine key (`~/.claude.json`, `~/.codex/config.toml`, OpenCode user config) → byte-unchanged, no Unimatrix MCP entry written for that harness, stderr has `unimatrix: <file> key <key> holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written`, init exit code unaffected; `status` legacy lines per FR-12g for leftover entries in each project file; `~/.claude.json` only the target project key changes; malformed `~/.claude.json` → exit 1, file byte-unchanged, no tmp left |

## User Workflows
1. Local install (Linux): `npm install` → `npx unimatrix init` → dry-run of user-level edits → shim, stable copy, `install.json` written → harness fires `<FORM>` → shim → Node client → UDS → daemon → observation stored.
2. Cloud attach (Linux/macOS/Windows): attach → same config → Node client → HTTPS → observation stored. No binary resolved.
3. Upgrade over legacy: re-run `init` → legacy entries replaced, foreign kept, one entry per event in the hook destination; hook command string unchanged; Node path re-recorded.
4. Cache clean/reinstall: hook fires → works (stable copy) or Broken (exit 1, loud).
5. Override: set redirect file (binary + client) or `UNIMATRIX_BINARY` (binary only) → hook, MCP, pass-through use it → remove → reverts.
6. Codex in a non-Unimatrix project: user-level hook fires → exit 0, silent, within budget.
7. Codex first run: user performs the one-time approval printed by `init`.
8. OpenCode: `wire --harness opencode` → retrieval MCP only.

## Constraints
- Node is the only hook client; hooks never invoke Rust.
- Wiring form ADR-001 (user-level shim) with hook path absolute per D-027. Every install is per-machine; R5 dropped (D-046).
- PowerShell as hook shell is unsupported and documented (D-041).
- Claude merges hooks across scopes; Unimatrix entries per event exist only in `settings.local.json`.
- Codex hooks in `~/.codex/hooks.json` only; one-time approval required (D-028).
- `hook.rs` retained as parity oracle.
- C-10 byte-identity (nan-023 AC-16) superseded (D-008).
- Exit codes limited to 0 and 1 for fire-time outcomes (D-023).

## Definition of Done (CI)
- AC-06 latency benchmark gating, incl. not-a-project path.
- AC-07 E2E: Linux Claude/Codex × UDS/HTTPS; Windows and macOS cloud attach plus Claude and Codex hooks firing through the shim (D-033).
- AC-08 `parity-layer2*` gating.
- AC-12 regression-guard suites green.
- AC-15 cross-repo leak test green.
- AC-16 MCP migration-removal test green.
- First delivery task: Windows CI run proving MCP argv pass-through (D-034).

## Dependencies
- `packages/unimatrix/bin/unimatrix.js` (adds `hook`, `mcp-stdio`, `status` verbs), `lib/wire.js`, `lib/init.js`, `lib/codex-install.js`, `lib/opencode-install.js`, `lib/merge-settings.js`, `lib/resolve-binary.js`, `lib/hook-client/`; new modules per architecture (shim, resolver, ownership).
- Tests: `merge-settings.test.js`, `dogfood-effect.test.js`, `init-remote.test.js`, `parity-layer2*`, `[hc]`/`[pkg]`/`[oc]`/orphan-audit suites.
- Rust daemon (UDS), remote HTTPS endpoint, CI runners for Linux, macOS, Windows.
- ADRs nan-016 #5825, #4926; vnc-031 #4941; #682 (soak overridden by D-001).

## NOT in Scope
- Dogfood procedure, managing the redirect, resetting this repo's configs, removing stray `.opencode/plugins/unimatrix.js` / `.mcp copy.json` (#1025).
- OpenCode hooks/v2 plugin and `emit.js` changes (#998).
- Deleting `hook.rs`; native remote HTTP MCP; uninstall/unwire; macOS/Windows local mode (Rust binary); Gemini writer.
- F6 soak continuity; Rust MCP stdio/daemon changes; harness firing behaviour; entries written by users or other tools.

## Open Questions
None. All closed:
- OQ-1 ADR-001, D-026/D-027; OQ-2 D-026; OQ-3 D-025; OQ-4 ADR-002 `redirect.json`; OQ-5 D-023/D-042; OQ-6 delivery Stage 1 audit, escalate any finding (D-035); OQ-7 D-045; OQ-8 D-036.
- L-1 D-036; L-2 D-040/D-060; V-1/A-5 D-060; L-3 D-041; L-4 D-037/D-050; T-1 D-051; T-2 D-052/D-053; L-5 D-038; L-6 D-039; Q8 D-044; Q9 D-034 (first delivery task).
