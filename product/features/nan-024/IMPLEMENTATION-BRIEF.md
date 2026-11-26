# nan-024 Implementation Brief: Every Harness on the Node Client — Portable Wiring

GH #1023 · Dogfood follow-up #1025 · OpenCode hooks #998

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/nan-024/SCOPE.md |
| Decision Register | product/features/nan-024/DECISIONS.md |
| Scope Risk Assessment | product/features/nan-024/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-024/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-024/specification/SPECIFICATION.md |
| Risk Strategy | product/features/nan-024/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-024/ALIGNMENT-REPORT.md |
| Codex evidence | product/features/nan-024/research/codex-hooks-evidence.md |
| Acceptance Map | product/features/nan-024/ACCEPTANCE-MAP.md |

DECISIONS.md is binding. Later entries win: D-023 > D-015, D-025 > D-006/D-017, D-026 > D-018/D-019, D-029 > D-020, D-046 > R5 waivers, D-060 > D-026 (MCP portable), D-050 > D-037, D-052 > D-045, D-062/D-063/D-064 amend D-060's MCP set, D-066 > D-057, D-067 > D-061 (ADR IDs).

## Goal

Every shipped Claude and Codex hook runs on the Node hook client; the Rust binary is server/daemon/MCP-stdio only (D-001, D-002). Local `init` and cloud attach write identical config per OS family (D-040), every install per-machine (D-046), pointing hooks at one user-level shim by its absolute resolved path; binary, client, transport and credentials resolve at runtime through one resolver that honours a user-level override (D-013, D-024, D-027). `init` stops provisioning OpenCode plugins; `wire --harness opencode` keeps a per-machine retrieval MCP entry (D-046, D-060, D-063) (D-004). macOS and Windows are supported cloud-only (D-032, D-033).

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| shim (`lib/shim/unimatrix.sh`, `lib/shim/unimatrix.cmd`) | pseudocode/shim.md | test-plan/shim.md |
| shim-entry (`lib/shim-entry.js`) | pseudocode/shim-entry.md | test-plan/shim-entry.md |
| install-store (`lib/install-store.js`) | pseudocode/install-store.md | test-plan/install-store.md |
| resolve-runtime (`lib/resolve-runtime.js`) | pseudocode/resolve-runtime.md | test-plan/resolve-runtime.md |
| trust (`lib/trust.js`) | pseudocode/trust.md | test-plan/trust.md |
| project-state (`lib/project-state.js`: `isUnimatrixProject` D-045, `isInstalled`) | pseudocode/project-state.md | test-plan/project-state.md |
| launcher (`bin/unimatrix.js`, incl. `hook`, `mcp-stdio`, `status` verbs) | pseudocode/launcher.md | test-plan/launcher.md |
| hook-ownership (`lib/hook-ownership.js`) | pseudocode/hook-ownership.md | test-plan/hook-ownership.md |
| merge-settings (`lib/merge-settings.js`) | pseudocode/merge-settings.md | test-plan/merge-settings.md |
| wire-init (`lib/wire.js`, `lib/init.js`) | pseudocode/wire-init.md | test-plan/wire-init.md |
| codex-install (`lib/codex-install.js`) | pseudocode/codex-install.md | test-plan/codex-install.md |
| opencode-install (`lib/opencode-install.js`) | pseudocode/opencode-install.md | test-plan/opencode-install.md |
| hook-client (`lib/hook-client/index.js`, incl. `last-hook.json` write) | pseudocode/hook-client.md | test-plan/hook-client.md |
| ci-and-docs (Q9 Windows MCP proof job, workflows, OQ-6 audit, README, `.gemini/settings.json`, release notes) | pseudocode/ci-and-docs.md | test-plan/ci-and-docs.md |

All paths relative to `packages/unimatrix/` unless noted. Pseudocode and test-plan files are produced in Session 2 Stage 3a.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Retire Rust hook client | Yes, now; #682 soak removed | D-001 | — |
| Topology | Local: hook→Node→UDS→daemon, MCP→Rust stdio. Cloud: hook→Node→HTTPS, MCP→mcp-bridge→HTTPS | D-002 | architecture/ADR-004-launcher-hook-route.md |
| Wiring form | User-level shim, invoked by absolute resolved path; no `$HOME`, no shell expansion in hook destinations | D-003, D-027 | architecture/ADR-001-user-level-shim-wiring-form.md |
| Exit codes (OD-1) | Not a project → 0 silent; project not installed → 0 + one-line hint (Linux); resolution failure → 1 + stderr. Never 2. Connection errors keep current codes | D-023 (supersedes D-015), D-042 | architecture/ADR-004-launcher-hook-route.md |
| Override source (OD-2) | `UNIMATRIX_BINARY` = binary only. Client: redirect > install. No inference from beside binary; no `UNIMATRIX_CLIENT_ROOT` | D-024, D-016, D-013 | architecture/ADR-002-single-runtime-resolver.md |
| Latency budget (OD-3) | p95 ≤ max(`node -e 0` p95 + 15 ms, 50 ms), same CI job; covers not-a-project path; pre-Node shell check iff that path breaches. Cloud + RTT | D-025 (supersedes D-006, extends D-017) | architecture/ADR-005-latency-budget.md |
| Codex shell/merge (OD-4) | Evidence gathered: `$SHELL -lc` / `%COMSPEC% /C`; layers merge additively | D-014 | architecture/ARCHITECTURE.md (Addendum), research/codex-hooks-evidence.md |
| Hook destinations (OD-5) | Claude → `.claude/settings.local.json`; Codex → `~/.codex/hooks.json` only; owned entries removed from other scopes | D-026 (supersedes D-018/D-019) | architecture/ADR-003-ownership-predicate-and-cross-scope-dedupe.md |
| Windows | Kept. `.cmd` shim; quoted absolute path, forward slashes; spaces handled | D-027 | architecture/ADR-001-user-level-shim-wiring-form.md |
| Codex approval | One-time approval accepted; init prints step, writes no trust hashes; hook string byte-fixed across upgrades | D-028 | architecture/ADR-001-user-level-shim-wiring-form.md |
| Trust check | Current-user owned, not g/o-writable, parent 0700; refuse loudly; never skip. Windows: full ACL check at init, identity `{size, mtimeMs, ino}` in `trust.json`, per-event compare, full recheck on mismatch | D-029 (supersedes D-020), D-036 | architecture/ADR-002-single-runtime-resolver.md |
| Node path | Recorded → PATH → loud re-init hint; re-recorded on init/upgrade | D-030 | architecture/ADR-001-user-level-shim-wiring-form.md |
| Stable package copy | `~/.unimatrix/pkg/<version>/`, atomic `install.json` rename, prune to current + previous; local and cloud | D-031 | architecture/ADR-001-user-level-shim-wiring-form.md |
| Platform model | macOS/Windows cloud-only; binary resolved only for local transport; "no binary" normal | D-032 | architecture/ADR-004-launcher-hook-route.md |
| CI DoD | macOS + Windows cloud attach and Claude/Codex hooks through shim proven in CI | D-033 | architecture/ARCHITECTURE.md (Testing Requirements), RISK-TEST-STRATEGY.md (R-26) |
| R5 portability | Dropped; every install per-machine; supersedes R5 waivers in D-018/D-026 | D-046 | architecture/ADR-001-user-level-shim-wiring-form.md |
| "Unimatrix project here" | Local store `~/.unimatrix/<hash>/`, or `~/.unimatrix/<hash>/remote.json`, or headless env (`UNIMATRIX_BUNDLE`/`UNIMATRIX_REMOTE_*`) only with `UNIMATRIX_PROJECT_ROOT` set and realpath(cwd) inside it; one predicate for hooks, MCP bridge, L-4, Q8 | D-045, D-052 | architecture/ADR-004-launcher-hook-route.md |
| Q9 Windows MCP argv | First delivery task: Windows CI proof; on failure swap to fallback and proceed (no stall) | D-034, D-047 | architecture/ADR-001-user-level-shim-wiring-form.md |
| OQ-6 legacy direct-path | Stage 1 audit incl. packed npm tarball and OpenCode plugin dist; escalate any found | D-035 | — |
| Windows MCP fallback | Recorded node path + `<abs home>/.unimatrix/bin/shim-entry.js mcp-stdio`; same CI job pre-proves it. Per-machine scope only (D-060) | D-047 | architecture/ADR-001-user-level-shim-wiring-form.md |
| L-4 package present, no credential | macOS/Windows: package copy present (`install.json` → `pkg/<version>`) but no `UNIMATRIX_BUNDLE` and no credential for this project → hook exit 0 silent; hint only in `status`. Same outcome when not installed at all (no bundle, no credential) | D-037 as amended by D-050 | architecture/ADR-004-launcher-hook-route.md |
| L-5 lost Codex approval | `unimatrix status` (`last-hook.json`) + init-printed step. Server-side signal deferred to #1026 | D-038, D-054 | architecture/ADR-004-launcher-hook-route.md |
| L-6 `.gitignore` | No append; warning in dry-run/init output when `settings.local.json` not ignored | D-039 | architecture/ADR-001-user-level-shim-wiring-form.md |
| L-2 / AC-01 identity | Identical per OS family | D-040 | architecture/ADR-001-user-level-shim-wiring-form.md |
| L-3 PowerShell hook shell | Unsupported, documented | D-041 | architecture/ARCHITECTURE.md |
| `mcp-stdio` no install | Exit 1 inside a D-045 project; silent 0 outside | D-043 | architecture/ADR-004-launcher-hook-route.md |
| Cloned repo, init never run (T-1) | Not a project: hooks and `mcp-stdio` exit 0 silent; `status` reports it | D-051 | architecture/ADR-004-launcher-hook-route.md |
| Headless cross-repo leak (T-2) | Env tier requires `UNIMATRIX_PROJECT_ROOT` containing cwd, in hooks AND `mcp-bridge`/`remote-resolve.js` | D-052 | architecture/ADR-004-launcher-hook-route.md |
| vnc-050 headless contract | `UNIMATRIX_PROJECT_ROOT` required; update `docs/client-setup.md`, `headless-smoke.test.js`; no compat break (unreleased). Amends vnc-050 ADR-006 | D-053 | architecture/ADR-004-launcher-hook-route.md |
| Installed / transport | Linux: valid `install.json` or credential; macOS/Windows: `UNIMATRIX_BUNDLE` ("bundle" means this env var only) or credential. Credential/headless → cloud; else Linux installed → local, unresolvable binary = Broken (exit 1) | D-032, D-042 | architecture/ADR-004-launcher-hook-route.md |
| Repo `.codex/hooks.json` reference config | Reset in #1025, not AC-11 | D-048 | — |
| Alignment rerun | Vision guardian rerun before sign-off; final pass covered D-001..D-068 | D-049, D-055 | — |
| D-038 server signal | Not cheap; deferred to #1026 | D-054 | architecture/ADR-004-launcher-hook-route.md |
| ADR ID drift | ADRs cited by number; Unimatrix IDs only in the snapshot below | D-056, D-059, D-061, D-067 | architecture/ADR-001-user-level-shim-wiring-form.md |
| Fallback node staleness | Fallback MCP entry rewritten whenever node is re-recorded; stale path → `status` reports it (exit 0), init/upgrade preflight exit 1 + re-init hint. Harness spawn ENOENT on a stale path accepted (outside Unimatrix's control) | D-057 as amended by D-066 | architecture/ADR-001-user-level-shim-wiring-form.md |
| OQ-6 audit vs repo `.codex/hooks.json` | Exempt from Stage 1 escalation; listed as exempt | D-058 | architecture/ARCHITECTURE.md |
| Q8 Codex-only drop | Gap rejected; predicate independent of config location | D-044 | architecture/ADR-004-launcher-hook-route.md |
| OpenCode | init writes no provisioning; `wire --harness opencode` writes `mcp.unimatrix` to `~/.config/opencode/opencode.json` | D-004, D-060 | architecture/ADR-006-opencode-init-removal.md |
| C-10 byte-identity | Superseded; tests updated, not deleted | D-008 | architecture/ADR-001-user-level-shim-wiring-form.md |
| MCP placement (V-1/A-5) | Per-machine only: Claude `~/.claude.json` `projects["<abs root>"].mcpServers.unimatrix`; Codex `~/.codex/config.toml` `[mcp_servers.unimatrix]`; OpenCode `~/.config/opencode/opencode.json`. Never a committed `.mcp.json`; init removes owned entries from project `.mcp.json`, `.codex/config.toml` (D-062) and `opencode.json` (D-064), preserving foreign servers. D-040, D-051, D-057 hold as written | D-060 (amends D-026) | architecture/ADR-001-user-level-shim-wiring-form.md, architecture/ADR-003-ownership-predicate-and-cross-scope-dedupe.md |
| Project `.codex/config.toml` MCP | init removes owned entries; Codex MCP user scope only | D-062 | architecture/ADR-003-ownership-predicate-and-cross-scope-dedupe.md |
| OpenCode MCP location | `wire --harness opencode` writes `~/.config/opencode/opencode.json`; silent outside a project (D-043) | D-063 | architecture/ADR-006-opencode-init-removal.md |
| Project `opencode.json` owned `mcp.unimatrix` | init removes it, preserving foreign entries (FR-07i/AC-16) | D-064 | architecture/ADR-003-ownership-predicate-and-cross-scope-dedupe.md |
| `status` legacy scope | Report leftover Unimatrix MCP entry in project `.mcp.json`, `.codex/config.toml` or `opencode.json`, naming the file | D-068 | architecture/ADR-004-launcher-hook-route.md |
| Foreign entry at per-machine `unimatrix` key | Leave in place, never overwrite, write no Unimatrix entry for that harness; init and `--dry-run` warn `unimatrix: <file> key <key> holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written` | D-065 | architecture/ADR-003-ownership-predicate-and-cross-scope-dedupe.md |
| Dogfood | Out (#1025) except override seam | D-009, D-012, D-013 | — |
| No double-firing | At most one owned entry per event across written scopes | D-010 | architecture/ADR-003-ownership-predicate-and-cross-scope-dedupe.md |

Unimatrix ADR ID snapshot (D-067; ADRs are cited by number elsewhere; the active entry is whichever `context_lookup` returns for nan-024): ADR-001 #5888, ADR-002 #5875, ADR-003 #5887, ADR-004 #5886, ADR-005 #5878, ADR-006 #5882; vnc-050 ADR-006 #5869.

## Pending Human Confirmations

None.

## Register Tensions (resolved)

All closed by the human at design-review; delivery implements as stated:

1. D-037 vs D-045/D-023 (hint in every project) → **D-050**: hooks silent; hint only in `unimatrix status`.
2. T-1 cloned repo, init never run → **D-051**: `mcp-stdio` and hooks exit 0 silent; `status` reports it.
3. V-1 (Windows fallback in committed MCP) and A-5 (cloner without shim, exit 127) → **D-060**: MCP per-machine only; committed entries removed.
4. T-2 headless env cross-repo leak → **D-052**: env tier needs `UNIMATRIX_PROJECT_ROOT` containing cwd (hooks + bridge); **D-053**: vnc-050 headless contract amended.

## Delivery Order

1. **First task — Q9 Windows MCP argv proof (D-034, D-047)**: Windows CI spawns the written MCP entry through Claude Code's and Codex's real spawners (USERPROFILE with a space); same job runs the D-047 fallback (recorded node + `shim-entry.js mcp-stdio`). On failure, swap ADR-001's Windows MCP entry to the fallback and proceed; escalate only if the fallback also fails.
2. **Stage 1 — OQ-6 audit (D-035)**: search repo, docs, installers, the packed npm tarball (`npm pack` output) and the OpenCode plugin dist for direct Rust `hook` invocations outside init-written files. Escalate any found, except the repo `.codex/hooks.json` reference config, which is listed as exempt (D-058, D-048 → #1025).
3. **Headless contract task (D-053)**: `docs/client-setup.md` lists `UNIMATRIX_PROJECT_ROOT` as required; `headless-smoke.test.js` sets it and gains a negative case that fails without it; `mcp-bridge.js`/`remote-resolve.js` env tier refuses outside the root (D-052).
4. Remaining components per pseudocode OVERVIEW.

If the Windows `.cmd` hook form (`"<abs>.cmd" hook E`) fails in the D-034 job, there is no named fallback: stop that work and escalate to the human (RTS Failure Modes).

Not in delivery: repo `.codex/hooks.json` reset (#1025, D-048); D-038 server-side signal (#1026, D-054).

## Files to Create/Modify

Under `packages/unimatrix/`:

| Path | Change |
|------|--------|
| `lib/shim/unimatrix.sh` | New. POSIX shim template: recorded node → PATH node → loud hint; optional pre-Node project walk; exec `shim-entry.js` |
| `lib/shim/unimatrix.cmd` | New. Windows batch shim; same logic; exit code and stdin propagation |
| `lib/shim-entry.js` | New. Inline read-only project detection, trust check, read `install.json`, `require(<packageRoot>/bin/unimatrix.js)` in-process |
| `lib/install-store.js` | New. Copy to `~/.unimatrix/pkg/<version>/`, atomic `install.json` swap, prune, record node path, explicit modes/ACLs, `~/.unimatrix/<hash>/root` marker, Windows `trust.json` |
| `lib/resolve-runtime.js` | New. Single resolver; precedence and validation |
| `lib/trust.js` | New. POSIX uid/mode per event; Windows full ACL at init + per-event identity compare (D-036) |
| `lib/project-state.js` | New. `isUnimatrixProject` (D-045/D-052), `isInstalled`, transport; inlined copy in `shim-entry.js` with parity test |
| `lib/hook-ownership.js` | New. `isUnimatrixCommand`, `OWNED_FORMS` |
| `bin/unimatrix.js` | Add `hook`, `mcp-stdio`, `status` routes; pass-through via resolver; Linux-only message elsewhere |
| `lib/merge-settings.js` | Delete local patterns and `normalizeCommandSource` string arm; `buildShimCommand`; write `settings.local.json`; cross-scope removal |
| `lib/wire.js`, `lib/init.js` | Install shim/store; dry-run of user-level edits; Codex approval step; `.gitignore` warning only, never edited (D-039) |
| `lib/codex-install.js` | Hooks to `~/.codex/hooks.json`; remove owned from project `.codex/hooks.json` and `~/.codex/config.toml [hooks]`; MCP to `~/.codex/config.toml [mcp_servers.unimatrix]`; remove owned MCP from project `.codex/config.toml` |
| `lib/claude-json.js` (or within `merge-settings.js`) | Write `~/.claude.json` `projects[<abs root>].mcpServers.unimatrix` only; tmp + rename; abort on parse failure; remove owned entries from project `.mcp.json` preserving foreign servers (`isUnimatrixMcpEntry`) |
| `lib/opencode-install.js` | Delete plugin provisioning; MCP writer to `~/.config/opencode/opencode.json` only |
| `lib/mcp-bridge.js`, `lib/remote-resolve.js` | Env tier honoured only with `UNIMATRIX_PROJECT_ROOT` containing cwd (D-052) |
| `docs/client-setup.md` (repo root), `headless-smoke.test.js` | `UNIMATRIX_PROJECT_ROOT` required; negative smoke case (D-053) |
| `lib/resolve-binary.js` | Thin wrapper over `resolveRuntime().binary` |
| `lib/hook-client/index.js` | Export `main(argv)`; `require.main === module` guard; write `~/.unimatrix/<hash>/last-hook.json` |
| Tests | Update `merge-settings.test.js`, `dogfood-effect.test.js`, `init-remote.test.js`; add tests per component; never delete old-string tests. Test churn: project `.mcp.json` tests (incl. the `.mcp.json` foreign-preservation guard, renamed "removal preserving foreign servers") move fixtures to per-machine MCP destinations; updated, not deleted (AC-12) |
| CI workflows (repo `.github/workflows/`) | Q9 Windows MCP argv proof (first); E2E Linux/macOS/Windows; latency gate; `parity-layer2*` gating |
| `README.md` (~252/254/280), `.gemini/settings.json`, release notes | Remove Rust `unimatrix hook`; OpenCode = retrieval only |

Not modified: `opencode-plugin/lib/emit.js`, Rust `hook.rs`, Rust MCP stdio/daemon.

## Data Structures

- `~/.unimatrix/install.json`: `{"version": string, "packageRoot": "<abs>/pkg/<version>", "node": "<abs node>"}`; written `.tmp` then renamed.
- `~/.unimatrix/redirect.json`: `{"binary"?: abs, "client"?: abs}`.
- `~/.unimatrix/trust.json` (Windows): `{"<abs path>": {"size", "mtimeMs", "ino"}}`.
- `~/.unimatrix/<hash>/root`: project root, one line (pre-Node check input).
- `~/.unimatrix/<hash>/last-hook.json`: `{"<provider>": ISO time}`; read by `status`.
- User-level layout: `~/.unimatrix/` (0700), `bin/` (0700), `bin/unimatrix` (0700), `bin/unimatrix.cmd`, `bin/shim-entry.js`, `install.json` (0600), `pkg/<version>/`.
- `OWNED_FORMS: {name: string, re: RegExp}[]` covering L1–L6 and the absolute shim form (single/double quoted, either slash, `.cmd`, optional `--provider`). Every arm requires `hook\s+<KnownEvent>` or `hook-client/index.js` adjacency.
- Errors: `RuntimeResolutionError{code, source, path}`, `TrustError{path, reason}`.

## Function Signatures and Wire Forms

```
resolveRuntime({env, home, platform, transport})
  -> {binary: string|null, clientRoot: string,
      binarySource: "env"|"redirect"|"install"|null, clientSource: "redirect"|"install"}
checkTrust(paths: string[], platform) -> void            // throws TrustError
isUnimatrixProject(cwd, env, home) -> {hash, kind: "local"|"cloud"|"headless"} | null
isInstalled(project, platform) -> {installed: boolean, transport: "local"|"cloud"|null}
isUnimatrixCommand(cmd: string) -> boolean
buildShimCommand(event, provider?, platform) -> string
main(argv: string[]) -> Promise<void>                    // lib/hook-client/index.js
```

- Hook (Unix/macOS): `'<abs shim>' hook <EVENT>`; refuse init if path contains `'`.
- Hook (Windows): `"<abs shim, forward slashes>.cmd" hook <EVENT>`; refuse if `%` or `"`.
- Codex appends ` --provider codex-cli`.
- MCP (Unix/macOS): `{"command":"sh","args":["-c","exec \"$HOME/.unimatrix/bin/unimatrix\" mcp-stdio"],"env":{}}`.
- MCP (Windows, subject to D-034 proof): `{"command":"cmd","args":["/d","/s","/c","\"\"%USERPROFILE%\\.unimatrix\\bin\\unimatrix.cmd\" mcp-stdio\""],"env":{}}`.
- MCP (Windows fallback, D-047): `{"command":"<recorded abs node>","args":["<abs home>/.unimatrix/bin/shim-entry.js","mcp-stdio"],"env":{}}`. D-057: rewritten whenever node is re-recorded (D-030); a stale path is reported by `unimatrix status` (exit 0) and by the init/upgrade preflight (exit 1 + re-init hint). Spawn-time ENOENT on a stale path is accepted (D-066).
- OpenCode: `mcp.unimatrix = {"type":"local","command":<MCP argv>}` in `~/.config/opencode/opencode.json`.
- MCP destinations (D-060): Claude `~/.claude.json` `projects["<abs root>"].mcpServers.unimatrix`; Codex `~/.codex/config.toml` `[mcp_servers.unimatrix]`. `isUnimatrixMcpEntry(name, entry) -> boolean` decides ownership for removal.
- `unimatrix status [--json]` (ADR-004, FR-12g): evaluated for `process.cwd()`; no other args; no network, no writes; always exit 0, even when every check fails. Text output is one `key: value` line per field, in order:
  - `project:` `local <hash>` | `cloud <hash>` | `headless <root>` | `none`. When `none`: `none (this machine has no Unimatrix store or credential for <root>; run 'npx unimatrix init' to attach)` (D-051).
  - one line per project file holding a leftover owned Unimatrix MCP entry, checked in order `.mcp.json`, `.codex/config.toml`, `opencode.json`: `legacy: <file> has a Unimatrix MCP entry; re-run 'npx unimatrix init' to migrate` (`<file>` project-relative). No lines when none (D-060, D-064, D-068)
  - `installed:` `yes` | `no` + reason (`missing credential`, D-050; `no install.json`)
  - `transport:` `local` | `cloud` | `none`
  - `trust:` `ok` | `FAIL <path>: <reason>`
  - `mcp node:` `ok` | `n/a` (no D-047 fallback entry written) | `MISSING <path>; run 'npx unimatrix init'` (FR-07h, D-057/D-066)
  - `hook claude-code:` / `hook codex-cli:` `wired, last fired <ISO>` | `wired, never fired` | `not wired`; for Codex `wired, never fired` a following line prints the `/hooks` approval step (D-038)
  - `--json`: `{project: {kind, hash, root} | null, legacyProjectMcp: string[] /* project-relative files, same order */, installed: boolean, reason: string | null, transport: "local"|"cloud"|null, trust: {ok, path?, reason?}, mcpNode: {status: "ok"|"n/a"|"missing", path: string|null}, hooks: {"claude-code": {wired, lastFired: string|null}, "codex-cli": {...}}}`. Never reads Codex trust hashes.
- Fire-time order: `isUnimatrixProject` → trust check → `isInstalled` → `resolveRuntime` → `client.main`.

## Constraints

- MCP never written to a committed file. Removal from project `.mcp.json` / `.codex/config.toml` / `opencode.json` preserves foreign servers byte-for-byte, never deletes the file, does not rewrite a file with nothing owned; a foreign command named `unimatrix` is left in place with a stderr warning (AC-16); a foreign entry already at a per-machine `unimatrix` key (`~/.claude.json` project key, `~/.codex/config.toml`, `~/.config/opencode/opencode.json`) is left unchanged, no Unimatrix entry is written for that harness, and init/`--dry-run` print `unimatrix: <file> key <key> holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written` (D-065).
- `~/.claude.json` safety: Claude Code writes it concurrently; change only the one project key, tmp + rename, abort without overwriting on parse failure (exit 1).
- User-scope MCP (Codex, OpenCode) starts in every directory: `mcp-stdio` exits 0 silent and fast outside a project (R-39).

- Node is the only hook client; zero Rust processes per hook event; one Node process per event; no npx on hot path.
- Local server Linux-only; UDS Unix-only; macOS/Windows cloud-only.
- Exit outcomes (ADR-004): not-installed split by platform: Linux → 0 + one stderr hint (D-023); macOS/Windows with no `UNIMATRIX_BUNDLE` and no credential → 0 silent, `status` only (D-050). Not a project / cloned repo → 0 silent. Linux valid `install.json` + missing binary → 1. Resolution failure (incl. Linux installed, binary unresolvable) → 1; connection errors unchanged.
- Headless env is a project only with `UNIMATRIX_PROJECT_ROOT` (absolute, realpath containment, not empty) containing cwd — hooks, `mcp-stdio`, `mcp-bridge`, pre-Node check.
- Exit codes 0 and 1 only for fire-time outcomes; hint goes to stderr, never stdout (R-01).
- `mcp-stdio` writes no stdout bytes before exec (R-07).
- Hook command string byte-fixed across upgrades (D-028).
- User-level writes/removals touch only owned entries and are listed in `--dry-run`; files with nothing owned are not rewritten.
- Project detection runs before trust check; non-project exits 0 silently even with stale install or missing node.
- Pre-Node shim check (enabled only if not-a-project path breaches budget) must be a lossless superset of `isUnimatrixProject` (FR-12a).
- `init` never edits `.gitignore` (D-039).
- No file over 1,000 code lines (PL-10); new logic in new modules.
- Tests use fixture repos/homes only, never this repo's configs or real `~/.unimatrix/` (NFR-05).
- Test count in touched suites does not decrease (NFR-04).

## Dependencies

- Node runtime (no new npm dependencies); `icacls` on Windows.
- Rust daemon (UDS) and remote HTTPS endpoint for E2E.
- CI runners: Linux, macOS, Windows.
- Prior ADRs: nan-016 #5825, #4926; vnc-031 #4941; #682 (soak overridden).

## Risk Focus (RISK-TEST-STRATEGY.md rev 6)

Critical: R-03 ownership predicate variants/foreign matches. High: R-04 in-process client, R-07 MCP stdout, R-08 user-level rewrites, R-17 shim path quoting, R-18 `.cmd` semantics, R-26 CI matrix realism, R-28 predicate parity, R-29 lossy pre-Node check, R-34 headless cross-repo leak, and D-060 risks R-36 removal preserving foreign servers, R-37 concurrent `~/.claude.json` writes, R-38 `projects[root]` key mismatch (symlink/realpath, trailing slash, drive-letter case), R-39 user-scope MCP in every project. Low: R-25 Windows MCP argv (gated by D-034, fallback D-047). Medium: R-32 `last-hook.json`, R-35 headless contract, R-40 migration edge cases (foreign `unimatrix`, leftover legacy entries, duplicate servers).

## NOT in Scope

- Dogfood procedure, redirect management, resetting this repo's configs, removing stray `.opencode/plugins/unimatrix.js` / `.mcp copy.json` (#1025).
- OpenCode hooks/v2 plugin, `emit.js` changes (#998).
- Deleting `hook.rs`; native remote HTTP MCP; uninstall/unwire; macOS/Windows local mode; Gemini writer.
- D-038 server-side signal (MCP calls with zero hook events) — #1026 (D-054).
- F6 soak continuity; Rust MCP stdio/daemon changes; harness firing behaviour; entries written by users or other tools.

## Alignment Status

ALIGNMENT-REPORT.md (rerun against D-001..D-068, incl. this brief): PASS 3, WARN 3, VARIANCE 0, FAIL 0. Vision (`personal-cloud`, Principle 6) and milestone fit PASS; no gaps; no variances requiring approval.

| Finding | Status |
|---------|--------|
| W-1 brief Goal "portable" MCP | Fixed in brief: "per-machine" (D-046/D-060/D-063) |
| W-2 ADR-003 `~/.claude.json` parse failure wording | Fixed: ADR-003 states init exits 1 |
| W-3 RTS header; R-40 scenario 3 | Fixed: RTS header covers D-015..D-068; R-40 scenario 3 restated (`legacy:` line, re-init leaves only per-machine) |
| W-4 SCOPE Goal 6 vs D-024 | Fixed: Goal 6 says `UNIMATRIX_BINARY` governs the binary only; redirect governs binary and client |
| W-5 brief inconsistencies | Fixed in brief: R-25 Low; D-064 row cites ADR-003; D-068 row cell count; `status` order trust → mcp node → hook (ADR-004); this section rebuilt; D-066 > D-057 etc. added to "later entries win" |
| W-6 ADR-001 / spec Domain "Scope" omit `opencode.json` | Fixed: ADR-001 and spec Scope list it |
| Additions: macOS/Windows pass-through exit 1; widened OQ-6 audit | Accepted (derived from D-032 / low risk) |

D-038 server signal deferred to #1026 (D-054). No open items.
