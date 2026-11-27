# Architecture — 1036: Harden per-machine harness wiring placement

Inputs: `SCOPE.md` (approved, D-005), `DECISIONS.md` D-001..D-032, `SCOPE-RISK-ASSESSMENT.md`, `product/research/ass-110/ass-110-findings.md`, `packages/unimatrix/lib/`.
ADRs: `ADR-INDEX.md` (Unimatrix #5937, #5960, #5962, #5959, #5955, #5963, #5961). Reconciled with D-001..D-032.

## 1. System Overview

`unimatrix init`, `init --bundle` and `wire` all run one flow: `wire()` in `lib/wire.js` (nan-024 D-040). This feature keeps every destination (D-001, D-003) and changes how `wire()` finds, writes and reports them:

```
init / init --bundle / wire  (bin/unimatrix.js -> lib/init.js -> wire())
        |
        v
 config-dirs.resolveConfigDirs(env, home, platform)        <-- also used by status.collect, mcpNode
        |
        v
 wire-plan.buildWirePlan(ctx) -> ordered steps (W*, R*, X1)
        |
        v
 wire-plan.runWirePlan(steps, {dryRun})  -- same steps, same mutate, both modes
        |      each step -> harness module mutate fn -> safe-write.updateFileSync
        |                                                  |
        |                                                  v
        |                                    trust.js hardened core (built-ins only)
        v
 manifest (WireLeg[]) -> action lines (with [VAR] source), warnings, D-013 note
```

Measured facts this design depends on (SR-01, Claude Code 2.1.332, temp HOME and CLAUDE_CONFIG_DIR, real user config untouched):

| Fact | Result |
|---|---|
| `CLAUDE_CONFIG_DIR=<d> claude mcp add-json -s local` | wrote `<d>/.claude.json` `projects[<root>].mcpServers` |
| `-s user` with the override | wrote `<d>/.claude.json` top-level `mcpServers` |
| `claude mcp get` with the override | found the local entry; without the override (same HOME) "No MCP server named" |
| User settings with the override | SessionStart hook fired only from `<d>/settings.json`; `<d>/.claude/settings.json` and `~/.claude/settings.json` were ignored |
| Relative `CLAUDE_CONFIG_DIR=cfg` | Claude resolved it against its own cwd and created `<cwd>/cfg` (SR-17 confirmed) |
| OpenCode 2.4.1, `opencode debug config --standalone` (temp HOME) | `OPENCODE_CONFIG_DIR` wins over `XDG_CONFIG_HOME`, which wins over `~/.config/opencode`. The output lists the source document path and its `mcp.servers` |
| `opencode mcp list` without `--standalone` | Connects to the user's running background service, which uses the real config, so tests must not use it. `mcp list --standalone` printed "No MCP servers configured" even for a populated config, so it is not a usable check |

## 2. Component Breakdown

| # | Component | File | Responsibility | Tier |
|---|---|---|---|---|
| C1 | Config-dir resolver | `lib/config-dirs.js` (new, ~100 lines) | Only builder of harness config paths. Owns precedence, relative-value warnings and `source`. Lists the nan-024 `legacy` locations and the D-019 `alternates` (ADR-002) | hard |
| C2 | Hardened writer core | `lib/trust.js` (extended, ~+150) | The single built-ins-only write primitive: realpath, O_EXCL random temp, fsync, hash CAS, rename retry, verify, temp cleanup (ADR-003, ADR-005, D-021). No second copy exists | hard |
| C3 | Writer API | `lib/safe-write.js` (new, ~120) | `updateFileSync` / `writeFileSync`, mutate + fixpoint verify + dryRun + 3-attempt loop, `SafeWriteError`, `tempPath`, `renameWithRetry` | hard |
| C4 | Wiring plan | `lib/wire-plan.js` (new, ~250) | Removal/destination table, legacy-location identity filter, ordering, gating, executor (ADR-004) | hard |
| C5 | Git exclude ensure | `lib/git-exclude.js` (new, ~100) | D-011 ensure via `git rev-parse --git-path info/exclude` (ADR-006) | hard |
| C6 | Orchestrator | `lib/wire.js` (shrinks) | Transport, preflight, installStore, then `buildWirePlan` + `runWirePlan`; action lines with source; D-013 note. Deletes `gitIgnored`, `GITIGNORE_WARNING`, dispatch functions, hardcoded paths | hard |
| C7 | Claude MCP | `lib/claude-json.js` | Pure mutates: dest + G2 in one mutate; `.mcp.json` removal; never touches the default `~/.claude.json` under `CLAUDE_CONFIG_DIR` (D-028); read-only `readClaudeMcpState` for status. Drops `atomicWriteText` re-export and mtime/size CAS | hard |
| C8 | Claude hooks | `lib/merge-settings.js` | `mergeSettings`, `removeOwnedHooks` compute inside `updateFileSync` | hard |
| C9 | Codex | `lib/codex-install.js` | Writers through `updateFileSync`; `codexHome` delegated to C1 (re-exported); `maybeWireCodex` rethrows `SafeWriteError` | hard |
| C10 | OpenCode | `lib/opencode-install.js` | Resolved user path from C1; legacy `~/.config/opencode` removal; D-015 prior-opt-in rule; G3 removal in the same mutate as project `mcp.unimatrix` removal; rethrows `SafeWriteError` | hard |
| C11 | Ownership | `lib/hook-ownership.js` | Adds `isLegacyOpencodePlugin(spec)` (pure) | hard |
| C12 | Install store | `lib/install-store.js` | All file writes go through C3. Modes are enforced on every write (D-025): `bin/*` 0700; `install.json`, `trust.json` and `<hash>/root` 0600. Store legs report `created/updated/unchanged` (D-017). The package copy is swapped by an atomic dir rename (§3.8, D-020). The symlinked-trust-file check and its fix message live here (§3.7, D-021). prune keys on `.tmp-` | hard |
| C13 | Init | `lib/init.js` | `cleanStaleRemoteSubtree` and skill install write via C3. No new logic | hard |
| C14 | Credential store write | `lib/hook-client/credstore.js` | `write()` uses `require("../safe-write.js")` lazily with `enforceMode: 0o600`, so `remote.json` is re-tightened to 0600 on every write (D-020, D-025). Reads unchanged | hard |
| C15 | Status | `lib/status.js` | `collect(ctx)` is injectable and reads paths from C1 only. Adds `configDirs`, `claudeMcp`, `opencodeConfig` (D-024), `staleLegacy` and `alternateProfiles` (D-019). Deletes the private `codexHome` and `home/.claude.json` paths (ADR-007) | hard |
| C16 | Removed | `lib/atomic-write.js` | Deleted (D-010) | hard |
| C17 | Test infrastructure | `test/helpers/home-guard.js`, `test/helpers/wire-fixture.js`, existing suites | Env scrub of the four vars by default, symlink, legacy-location, alternate-profile and exclude fixtures, writer fault seam tests, real-`claude` AC-1 and real-`opencode` AC-2 tests (D-027) | routine |

Modularity (PL-10): no production file approaches 1,000 code lines. Largest after change: `lib/init.js` (~391, unchanged size), `lib/trust.js` (~375), `lib/codex-install.js` (~337). No file is pushed over the cap. New logic is in C1, C3, C4, C5.

## 3. Component Interactions

### 3.1 Resolver (C1)

`resolveConfigDirs({env, home, platform})` — `env` is never mutated (R1). An env value is used only if it is a non-empty, absolute path without NUL. A relative value is ignored and produces a warning (it names the var and value, and says the harness resolves it against its own cwd). Resolution then falls to the next tier.

| Harness | Tier 1 | Tier 2 | Default | Files |
|---|---|---|---|---|
| Claude | `CLAUDE_CONFIG_DIR=<d>` | — | `~` | `<d>/.claude.json`, `<d>/settings.json`; default `~/.claude.json`, `~/.claude/settings.json` |
| OpenCode | `OPENCODE_CONFIG_DIR=<d>` | `$XDG_CONFIG_HOME/opencode` | `~/.config/opencode` | `<dir>/opencode.json` |
| Codex | `CODEX_HOME=<d>` | — | `~/.codex` | `<dir>/hooks.json`, `<dir>/config.toml` |

`legacy` lists the fixed nan-024 locations. They are removal candidates only when an override moves the destination elsewhere:
- `~/.claude/settings.json` (owned hooks; In-3, kept by D-030; spec RT-03)
- `~/.config/opencode/opencode.json`

`alternates` lists the other live profiles. Status lists them; wiring never touches them (D-019, D-028):
- the default `~/.claude.json`, both `projects[<root>]` and the top-level `mcpServers.unimatrix`, when `CLAUDE_CONFIG_DIR` is set
- `$XDG_CONFIG_HOME/opencode/opencode.json` when `OPENCODE_CONFIG_DIR` wins
- `~/.codex/{hooks.json,config.toml}` when `CODEX_HOME` is set

### 3.2 Writer (C2 + C3)

The full contract is in ADR-003 (#5962). What callers rely on:
- `mutate(raw: string|null) -> string|null` is synchronous and pure. `null` means no change.
- `unchanged` is returned whenever the text and mode are already correct. Nothing is written, so mtime stays the same (AC-5).
- A symlink is followed and the change goes to its target; the link stays a link (AC-3). A dangling link or a non-regular target throws `SafeWriteError` (code `dangling-symlink` / `not-regular`). The exception is `~/.unimatrix/trust.json`, which install-store refuses before writing (§3.7, D-021).
- Modes:
  - Default (D-023): an existing file keeps its current mode. A new file gets the caller's `newMode` (0600 for config files), applied with fchmod so umask does not matter.
  - Enforced (D-025): the `enforceMode` option re-tightens the mode on every write, whether the file is new or existing. It applies to `remote.json`, `install.json`, `trust.json` and `<hash>/root` (0600), and to every `~/.unimatrix/bin/*` file (0700).
  - If the content is equal but an enforced mode differs, the writer chmods the resolved target and reports `updated`.
- Containment (D-026): project files are written with `containWithin: <root>`. These are `settings.local.json`, `.mcp.json`, project `opencode.json`, `.codex/*` and `.claude/skills/*`.
  - The writer takes `realpath(<root>)` first. It then resolves the target's real path: the existing file, or else the nearest existing ancestor plus the remainder. This also catches symlinked parent directories.
  - If that real path is not inside the root, the writer throws `SafeWriteError` code `outside-project` naming the link and its target, and init exits 1.
  - Dry-run runs the same check. User/global links are not contained and follow AC-3 unchanged.
  - The lexical `isWithinProject` guards on these writes are replaced by this check.
- The temp file is `<targetdir>/.<base>.tmp-<16hex>`, opened `wx`. It is unlinked on every failure path (AC-10).
- CAS: before the rename, the target is re-read and its sha256 compared with the read hash. A mismatch retries (AC-7).
- Verify: after the rename, the file is re-read through the original path and `mutate(readBack) === null` must hold. If not, the whole attempt is retried. After 3 attempts the writer throws `SafeWriteError{code: "conflict" | "verify-failed", file}`, and init/wire exit 1 naming the file.
- `opts.fault(stage)` is the test seam. Stages: `"after-read"`, `"before-rename"`, `"after-rename"`. It mirrors install-store's `_fault`.
- `dryRun: true` runs the read and the mutate, writes nothing, and returns the same action.

### 3.3 Wiring plan (C4) — removal/destination table (SR-08 baseline; the spec finalizes it)

`R` = resolved file from C1; `L` = nan-024 legacy location from `legacy`; `same(a,b)` = D-007 identity (equal `dev`+`ino` when both exist, else equal `realpath.native` of the nearest existing ancestor plus the remainder).

| Id | Harness | Role | File | Key / entries | Runs when | Gate |
|---|---|---|---|---|---|---|
| W1 | claude-code | dest hooks | `<root>/.claude/settings.local.json` | owned hook per event | claude-code wanted | — |
| R2 | claude-code | remove | `<root>/.claude/settings.json` | owned hooks | after W1 | W1 ok |
| R3 | claude-code | remove | `R.claude.userSettings` | owned hooks | after W1 | W1 ok |
| R4 | claude-code | remove legacy | `L ~/.claude/settings.json` | owned hooks | override in effect, `!same(R3)` | W1 ok |
| W2 | claude-code | dest mcp | `R.claude.claudeJson` | `projects[key].mcpServers.unimatrix` | wanted and `writeMcp` | — |
| R1 | claude-code | remove (G2) | same file as W2, **same mutate** | top-level `mcpServers.unimatrix` (owned) | with W2 | W2 not skipped-foreign |
| R6 | claude-code | remove project | `<root>/.mcp.json` | owned `mcpServers.*` | `!hooksOnly` | none (D-060) |
| W3 | codex-cli | dest hooks | `R.codex.hooksJson` | owned hook per event | Codex detected (D-081) | — |
| W4 | codex-cli | dest mcp | `R.codex.configToml` | `[mcp_servers.unimatrix]` | Codex detected | — |
| R7 | codex-cli | remove | `R.codex.configToml` | owned `[hooks.*]` tables | after W4 | none (existing) |
| R8 | codex-cli | remove project | `<root>/.codex/hooks.json` | owned hooks | after W3; not same file as W3 | none (existing) |
| R9 | codex-cli | remove project | `<root>/.codex/config.toml` | owned `[mcp_servers.unimatrix]` | after W4; not same file as W4 | none (existing) |
| W5 | opencode | dest mcp | `R.opencode.configJson` | `mcp.unimatrix` | detected or `--harness opencode`. A new entry needs `--harness opencode` or an owned entry at R or `L ~/.config/opencode` (D-015) | — |
| R10 | opencode | remove legacy | `L ~/.config/opencode/opencode.json` | owned `mcp.unimatrix` | override in effect, `!same(W5)` | W5 ok |
| R11 | opencode | remove project | `<root>/opencode.json` | owned `mcp.unimatrix` | `!hooksOnly` | none (D-064) |
| R12 | opencode | remove project (G3) | same file as R11, **same mutate** | owned `plugin[]` entries (`isLegacyOpencodePlugin`) | `!hooksOnly` | none (legacy) |
| X1 | claude-code | ensure (surface `git-exclude`) | resolved `info/exclude` | `/.claude/settings.local.json` | claude-code wired | W1 ok |

"ok" means the leg action is `created`, `updated` or `unchanged`. If the gate fails, the leg is `skipped-dest-unwired`. A thrown error (`SafeWriteError`, parse abort) ends the run, so no later step runs (write-before-remove, SR-11). Steps that resolve to the same real file are merged or deduplicated. A legacy location that is the same real file as the destination produces no step and no leg (D-007). `alternates` never produce a step (D-019). D-014: the pre-nan-024 `.opencode/plugins/` shim and the `.opencode/package.json` dep are out of scope.

Order: store, W1, W2(+R1), R2-R4, R6, then Codex W3, W4, R7-R9, then OpenCode W5, R10-R12, then X1.

### 3.4 Per-file parse-failure posture (SR-05)

| File | Unparseable |
|---|---|
| resolved `.claude.json` | exit 1 naming the file, not modified (FR-07j) |
| `settings.local.json`, project/user/legacy `settings.json`, `.mcp.json`, project `opencode.json` (incl. during removal, D-029) | exit 1 naming the file (existing throw) |
| Codex `hooks.json` / `config.toml` (user or project) | `skipped-malformed` leg plus a stderr warning, exit code unchanged |
| resolved and legacy OpenCode `opencode.json` (incl. JSONC) | `skipped-malformed` leg plus a stderr warning, exit 0 (D-016). Status names the resolved file as leaving OpenCode unwired (D-024) |

"Foreign entries preserved byte-for-byte" (AC-4, D-022): a file with no owned change is never rewritten. In a rewritten JSON file, every foreign value stays deep-equal and keeps its key order. Every JSON rewrite detects and reuses the file's existing indentation (`detectIndent`; 2 for a new file). A TOML edit leaves the bytes outside the owned table identical.

### 3.5 Git exclude (C5)

See ADR-006 (#5963). Every git call runs with `cwd = <root>`, the same root `wire()` uses. Each outcome is a WireLeg: `unchanged`, `created`/`updated` (append), `skipped-no-git`, `skipped-not-repo`, `skipped-tracked`, `skipped-git-error`, `appended-not-effective`. The append goes through C3. Dry-run runs the read-only git calls and the mutate. Edge cases (D-008): the spec.

### 3.6 Status (C15)

See ADR-007 (#5961). The `claudeMcp` check reads only the resolved `.claude.json` and resolves its key through `resolveProjectKey`. Owned entries left at nan-024 legacy locations are reported as `stale` (`staleLegacy`), never as wired. Owned entries at `alternates` are listed as other profiles that are left in place (D-019). An unparseable resolved `opencode.json` is named as leaving OpenCode unwired (D-024).

### 3.7 Error boundaries

| Origin | Error | Propagation |
|---|---|---|
| C2/C3 | `SafeWriteError{file, code}` | Always up to `init`/`wire`, which exit 1 with `unimatrix init failed: <msg naming file>`. The Codex/OpenCode wrappers rethrow it |
| C7 | `ClaudeJsonError` | Unchanged: exit 1 |
| C1 | none (never throws) | Warnings go to `warnings[]` and stderr, and status repeats them |
| C5 | none (never throws) | Outcome legs; warnings on stderr |
| Trust check after store | `TrustError` | Exit 1. When the reason is `symlink not allowed`, install-store rethrows it with the D-021 fix message (§3.7) |
| install-store pre-check | symlinked `~/.unimatrix/trust.json` | Exit 1 before any write to it (§3.7) |
| trust.js hook-time `recordTrustIdentity` | any | Swallowed (unchanged posture) |

### 3.7 Symlinked trust files (D-021)

Before the trust step, install-store lstats `~/.unimatrix/trust.json`. If it is a symlink, init exits 1 without writing to it and prints:

`unimatrix: <path> is a symlink (-> <link text>); trust state is never written through a symlink. Remove it ('rm "<path>"') and re-run 'npx unimatrix init'`

`<link text>` is `fs.readlinkSync(path)`, so a dangling link is named too. Other trust-set files (`install.json`, `bin/*`) are written through their links (AC-3). The unchanged trust check then refuses them as `symlink not allowed`, and install-store rethrows that error as:

`unimatrix: <path> is a symlink (-> <link text>); trust files must be regular files. Remove it ('rm "<path>"') and re-run 'npx unimatrix init'` The trust check itself is unchanged.

### 3.8 Package-copy swap (D-020)

The package copy is swapped by an atomic directory rename:
1. Build the copy in `pkg/.<version>.tmp-<16hex>`. `mkdirSync` fails on EEXIST, so the name is exclusive.
2. Copy the files. On POSIX, apply `chmodTree` (this is a new tree).
3. Write `.complete` last. fsync the marker and the directory (best-effort).
4. `renameWithRetry(tmp, pkg/<version>)` onto an absent destination.
5. If the destination is already complete (a concurrent init), keep it and discard the tmp.
6. If the destination is incomplete, rename it aside to a fresh temp name (atomic, never deleted in place), retry step 4, then remove the aside copy.

`install.json` points to the new packageRoot only after the rename succeeds, so readers see either the old version or the complete new one.

### 3.9 Release-history check for user-scope Claude hooks (D-032)

D-032 makes D-030 conditional: did any release write Unimatrix hooks to the user-scope `~/.claude/settings.json` (or `$CLAUDE_CONFIG_DIR/settings.json`)?

**Finding: none did.** R4 removes owned entries only and keeps foreign hooks byte-identical (AC-4). It needs no discovery of other projects and no status flag. No design change.

Evidence (checked 2026-11-27):

| Source | Result |
|---|---|
| npm registry: `npm view @dug-21/unimatrix versions` | 26 published versions, 0.5.5 … 0.11.3; `latest` = 0.11.3. Every one has a git tag. Tags v0.5.0–v0.5.4 and v0.6.1 were never published |
| Published tarballs (all 26, fetched by `dist.tarball`, scanned without execution) | Zero matches for a home-anchored `.claude` path (`homedir()`/`home`/`HOME`/`USERPROFILE`/`CLAUDE_CONFIG_DIR` within 60 chars of `".claude"`) in any `.js`. Zero `CLAUDE_CONFIG_DIR` references. Positive control: the same pattern matches today's `lib/wire.js:95` |
| Hook writer in published code | `mergeSettings` is only ever called with a project path. 0.5.5: `lib/init.js:213` `path.join(projectRoot, ".claude", "settings.json")`. 0.8.0: `lib/init.js:387`, `:495`. 0.11.3: `lib/init.js:503`, `:615`. Credentials and opt-in go to project `.claude/settings.local.json` (0.8.0 `init.js:213`, 0.11.3 `init.js:290`) |
| All 32 git tags (`git grep` over `packages/`, `npm/`, `scripts/`, `crates/`, tests and docs excluded) | `settings.json` references are project-relative only (init.js, merge-settings.js, scripts/dogfood-switchover.sh). Crates reference `settings.json` only in observation text (`unimatrix-observe` report/friction strings, a `hook.rs:556` comment). No Rust writer |
| Unreleased history (`git log --all -G 'homedir\(\).*\.claude\|home, ".claude", "settings'` over `packages/`) | Only nan-024 commits `600e43c2` and `7c7a573e` (2026-11-26). These add the user-scope **removal** (`wire.js` `removeOwnedHooks` over `path.join(c.home, ".claude", "settings.json")`), not a write. nan-023 `ac163873` writes project `.claude/settings.json` (`wire.js:125`). Neither is in any tag (`git merge-base --is-ancestor` fails against v0.11.3) |
| `scripts/dogfood-switchover.sh` (nan-016) | Repo-only dev tooling, not in the npm package. Default target is `<repo>/.claude/settings.json` (line 76). A developer could hand-pass `--settings ~/.claude/settings.json`. Entries written that way use owned forms and are removed by R4 like any other owned entry; this is not a release path |

Downstream: the spec, RTS, brief and ACCEPTANCE-MAP need no AC or test change. They may cite this subsection as the D-032 evidence.

## 4. Technology Decisions

| ADR | Unimatrix | Decision |
|---|---|---|
| ADR-001 | #5937 | D-046 placement rule; named exceptions Claude MCP, Codex, OpenCode MCP; measured Claude layouts; intent gate is store/credential |
| ADR-002 | #5960 | One resolver `lib/config-dirs.js`; relative values ignored with warning; source attribution |
| ADR-003 | #5962 | One hardened synchronous writer; coverage set; enforced modes (D-025); project-file containment (D-026); fixpoint verify; errors exit 1 |
| ADR-004 | #5959 | Plan table drives writes/removals/dry-run; gating; nan-024 legacy removal limited to `~/.claude/settings.json` and `~/.config/opencode` (D-019, D-028); D-015 opt-in; D-022 AC-4; D-029; G3 ownership |
| ADR-005 | #5955 | One writer core inside trust.js (D-021); symlinked trust.json fails with fix message; trust check unchanged |
| ADR-006 | #5963 | Exclude-file ensure mechanism |
| ADR-007 | #5961 | status/init resolved-file reporting; Claude local-scope check; stale legacy; alternate profiles listed; unparseable opencode.json named |

Amended nan-024 ADRs (rationale/paths only, decisions kept): #5896→#5945, #5897→#5956, #5894→#5947, #5901→#5948. Only Node built-ins (`fs`, `path`, `crypto`, `child_process`) are used, so `check-zero-deps.js` passes.

## 5. Integration Points

- `lib/init.js` `init()` / `initRemote()` call `wire()` without change. `initRemote` is async, but `wire()` and every writer are synchronous (D-013).
- `bin/unimatrix.js` `wire` verb and `status` verb: unchanged routing.
- `~/.unimatrix/bin/trust.js` is the installed copy of `lib/trust.js`. It stays byte-identical to its source and built-ins-only (`test/trust-selfcontained.test.js` unchanged).
- `lib/hook-client/credstore.js` `write()` is called only by init. The lazy require keeps the hook hot path free of the writer.
- Real `claude` binary (devcontainer, `~/.local/bin/claude`) for AC-1. In CI the test is skipped with a reason when `claude` is absent (D-012, #1053).

## 6. Integration Surface

| Integration point | Type / signature | Source |
|---|---|---|
| `resolveConfigDirs` | `({env: object, home: string, platform: string}) -> ConfigDirs` | new `lib/config-dirs.js` |
| `ConfigDirs` | `{claude: {dir, claudeJson, userSettings, source, value}, opencode: {dir, configJson, source, value}, codex: {dir, hooksJson, configToml, source, value}, legacy: {userSettings, opencodeJson}, alternates: {harness, file, key, var}[], warnings: string[]}`; `source` ∈ `"CLAUDE_CONFIG_DIR" \| "OPENCODE_CONFIG_DIR" \| "XDG_CONFIG_HOME" \| "CODEX_HOME" \| "default"`; `value` = the raw env value used (null when `source` is `"default"`), for `<VAR>=<value>` output (FR-34, FR-50); not in `status --json` | new |
| `codexHome` | `(env, home) -> {dir, warning?}` (kept signature; delegates to C1) | `lib/codex-install.js` re-export |
| `sameRealFile` | `(a: string, b: string) -> boolean` (D-007 identity) | new `lib/config-dirs.js` |
| `updateFileSync` | `(file: string, mutate: (raw: string\|null) => string\|null, opts?: {dryRun?: boolean, newMode?: number = 0o600, enforceMode?: number, containWithin?: string, verify?: boolean = true, attempts?: number = 3, retry?: {count, delayMs}, platform?: string, fault?: (stage) => void}) -> {action: "created"\|"updated"\|"unchanged", file: string, target: string, before: string\|null}`; `before` = the raw passed to mutate on the deciding call of the successful attempt (leg detail is derived from it, not from the verify re-run) | new `lib/safe-write.js` |
| `writeFileSync` | `(file, data: string\|Buffer, opts?) -> {action, file, target}` (whole-file replace; byte-equal verify; no CAS unless `opts.expectHash`) | new `lib/safe-write.js` |
| `SafeWriteError` | `extends Error {file: string, code: "dangling-symlink"\|"not-regular"\|"outside-project"\|"conflict"\|"verify-failed"\|"io"}`; message names the file (and link target for `dangling-symlink` / `outside-project`) | new `lib/safe-write.js` |
| `tempPath` | `(dest: string) -> string` = `<dir>/.<base>.tmp-<16hex>` | new `lib/safe-write.js` (from core) |
| `renameWithRetry` | `(from, to, platform, retry?) -> void` | moved from install-store into trust.js core, re-exported by safe-write |
| trust.js core export | `hardenedWriteSync(file, data, opts)`; built-ins only | `lib/trust.js` |
| `buildWirePlan` | `(ctx: {root, home, platform, env, node, dirs: ConfigDirs, harnessSel, writeMcp, hooksOnly, dryRun}) -> Step[]` | new `lib/wire-plan.js` |
| `Step` | `{id: "W1"\|..., harness, surface, file, source, gate?: string, run: (dryRun) -> WireLeg[]}` | new |
| `runWirePlan` | `(steps: Step[], {dryRun}) -> {manifest: WireLeg[], warnings: string[]}` | new |
| `WireLeg` (extended) | `{harness, surface, action, path, command?, entry?, reason?, warning?, source?}`; new actions `removed` (existing), `skipped-dest-unwired`, `skipped-no-git`, `skipped-not-repo`, `skipped-tracked`, `skipped-git-error`, `appended-not-effective` | nan-023 ADR-001 #5763 |
| `ensureSettingsLocalIgnored` | `(root: string, {dryRun, run?: (args) => {status, stdout}}) -> WireLeg` (harness `"claude-code"`, surface `"git-exclude"`) | new `lib/git-exclude.js` |
| `isLegacyOpencodePlugin` | `(el: unknown) -> boolean`; true iff `el` is the string `@dug-21/unimatrix-opencode-plugin`, or that name + `@` + a non-empty version spec containing no `/` | `lib/hook-ownership.js` |
| `writeClaudeMcp` | `(o: {file, projectRoot, entry, platform, dryRun}) -> {action, file, key, warning?, removedUserScope: boolean}`; `file` from C1, no `home` path building | `lib/claude-json.js` |
| `readClaudeMcpState` | `(file, projectRoot, platform) -> {status, file, key}` (read-only) | `lib/claude-json.js` (new fn) |
| `writeOpencodeMcp` | `(o: {file, platform, home, node, harnessSelected, dryRun}) -> WireLeg` (takes resolved `file`) | `lib/opencode-install.js` |
| `removeOwnedOpencodeProjectMcp` | `(root, dryRun) -> {actions, warnings, changed, removedPlugin: boolean}` (G3 in the same mutate) | `lib/opencode-install.js` |
| `status.collect` | `(ctx?: {env, home, platform, cwd}) -> Info` plus `configDirs`, `claudeMcp: {status: "wired"\|"missing"\|"foreign"\|"unreadable"\|"n/a", file, key}`, `opencodeConfig: {status: "ok"\|"absent"\|"unreadable", file}`, `staleLegacy: {harness, file, key}[]`, `alternateProfiles: {harness, file, key, var}[]` | `lib/status.js` |
| Removed | `lib/atomic-write.js` (`atomicWrite`, `existingMode`), `claude-json.atomicWriteText`, `wire.GITIGNORE_WARNING`, `wire.gitIgnored`, `status.codexHome` | — |

## 6a. Canonical Contract Names

Where the spec and this document differed, this document is the authority (R-10). The spec aligns to this table.

| Item | Canonical | Replaces |
|---|---|---|
| Legacy plugin predicate | `isLegacyOpencodePlugin(el)` in `lib/hook-ownership.js`. The accepted forms are spec FR-15 (exact name, or `name@<non-empty spec without "/">`) | `isUnimatrixOpencodePlugin` |
| `status --json` Claude MCP | `claudeMcp: {status, file, key}`, where `status` ∈ `"wired"` \| `"missing"` \| `"foreign"` \| `"unreadable"` \| `"n/a"`. `n/a` = not a Unimatrix project; `file`/`key` are null when n/a | `present` / `unknown` |
| `status --json` OpenCode config | `opencodeConfig: {status, file}`, where `status` ∈ `"ok"` \| `"absent"` \| `"unreadable"`, for the resolved `opencode.json` (D-024) | `opencodeMcp` with `ok`/`missing`/`unparseable` |
| `status --json` leftover owned entries | `staleLegacy: [{harness, file, key}]` for the nan-024 legacy locations init removes from (`~/.claude/settings.json`, `~/.config/opencode/opencode.json`; text `stale: ...`). `alternateProfiles: [{harness, file, key, var}]` for the D-019/D-028 profiles left in place, including the default `~/.claude.json` `projects[<root>]` and top-level entries (text `other profile: ...`). A legacy file that cannot be parsed is listed in `staleLegacy` with `key: null` and `unreadable: true` | `otherLocations` with `kind` |
| `status --json` resolved files | `configDirs: {claude: {claudeJson, userSettings, source}, opencode: {configJson, source}, codex: {hooksJson, configToml, source}, warnings: string[]}` | — |
| Text status values | `mcp claude-code: wired (<file>)` \| `MISSING ...` \| `foreign ...` \| `unreadable (<file> is not valid JSON)` \| `n/a`. The spec fixes the full wording, using these status words | `unknown (...)` |
| Symlinked `trust.json` message (D-021, before any write) | `unimatrix: <path> is a symlink (-> <link text>); trust state is never written through a symlink. Remove it ('rm "<path>"') and re-run 'npx unimatrix init'` (spec FR-20a text adopted) | arch §3.7 earlier text |
| Other symlinked trust-set file (rewrapped TrustError) | `unimatrix: <path> is a symlink (-> <link text>); trust files must be regular files. Remove it ('rm "<path>"') and re-run 'npx unimatrix init'` | — |
| Dangling-link / writer errors | One class, `SafeWriteError extends Error`, with `{file, code}`. `code` ∈ `"dangling-symlink"` \| `"not-regular"` \| `"conflict"` \| `"verify-failed"` \| `"io"` \| `"outside-project"` (D-026, spec FR-22a). The message names the link and its target for `dangling-symlink` and `outside-project`. There are no subclasses, so wrappers rethrow one type (R-12) | `DanglingSymlinkError` |
| Writer fault seam stages | `opts.fault(stage)`, where `stage` ∈ `"after-read"` \| `"before-rename"` \| `"after-rename"` | — |
| Git-exclude leg (X1) | harness `"claude-code"`, surface `"git-exclude"` (SPEC FR-40, FR-46) | harness `git` / surface `exclude` |
| New wire leg actions | `skipped-dest-unwired`, `skipped-no-git`, `skipped-not-repo`, `skipped-tracked`, `skipped-git-error`, `appended-not-effective` | — |
| Enforced modes (D-025) | `enforceMode`: `remote.json`, `install.json`, `trust.json`, `<hash>/root` 0600; `bin/*` 0700, re-tightened on every write. All other existing files keep their mode; new files per D-023 | D-023-only wording |

## 7. User-Facing Entry Points → Mechanism

| Entry point | Outcome | Mechanism |
|---|---|---|
| `init`, no overrides | Each harness wired, abandoned/shadowing entries gone, foreign unchanged | C1 defaults; table W/R; `claude mcp get` test |
| `init` with CLAUDE_CONFIG_DIR / OPENCODE_CONFIG_DIR / XDG_CONFIG_HOME / CODEX_HOME | A harness run with the same env sees the entry. The nan-024 legacy locations (`~/.claude/settings.json` hooks, `~/.config/opencode`) are cleared unless they are the same real file. Alternate profiles, including the default `~/.claude.json`, are kept and listed by status | C1 tiers; R4/R10 with `sameRealFile`; AC-1 real `claude`; AC-2 real `opencode` |
| `init` with a relative override | Warning names the var; outcome = no-override case | C1 warning; status repeats it |
| `init` on a symlinked config | User/global link: still a symlink, the target changed, no temp left. Project-file link to a target outside the project root: exit 1 naming the link (D-026) | C2 realpath + target-dir temp + cleanup |
| `init` that cannot verify | Exit 1 naming the file; old or new wiring remains | C3 CAS/verify loop; gating; throw ends run |
| second `init` | All legs `unchanged`, no mtime change incl. exclude and store files | C3 unchanged short-circuit; C5 check-ignore 0; C12 per-file action |
| `init --dry-run` | Exactly the real run's writes, removals (incl. legacy locations) and exclude append; nothing written | C4 same steps, C3 `dryRun`, C5 read-only git |
| `init` in git / outside git | settings.local.json never shows in `git status`; no tracked file changed; outside git the skip is reported | C5 |
| `status` | Prints every resolved file. Names a missing Claude local-scope entry with the init hint, and names an unparseable opencode.json. Lists alternate profiles. Exits 0 | C15 via C1 + `readClaudeMcpState` |
| `init` while Claude runs | D-013 note printed | C6 note on Claude MCP write/remove legs |

Fail loud: if an override value is given but cannot be honoured (relative), the run warns and names it (§3.1). If a destination cannot be written or verified, the run exits 1. No entry point silently no-ops.

## 8. Test Infrastructure (cumulative)

- `test/helpers/home-guard.js`: add `scrubbedEnv(extra)`. It returns `process.env` minus `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`, `XDG_CONFIG_HOME`, `CODEX_HOME`, plus `extra` (opt-in overrides, which are asserted to live under tmp). `wireOpts` and spawned-CLI tests use it (SR-10, #5911).
- `test/helpers/wire-fixture.js`: add `seedSymlinked(file, target)`, `seedLegacy(home, dirs)`, `seedAlternates(home, env)`, and a git-repo variant of `makeProject` (real `git init`, worktree option).
- Carry the existing install-store retry/mode tests and trust.js rename tests over to C3 before the private writers are deleted (SR-03).
- Win32 branches (rename retry, no dir fsync) are tested with injected `platform` and a stubbed `fs` (SR-04).
- AC-9 parity: for every fixture, the dry-run manifest equals the real-run manifest, and the real-run file diff equals the listed paths.
- SR-09 parity: for each override, `status.collect(ctx).configDirs` equals the paths in init's manifest.
- AC-1 real `claude` (amended by D-028): run with a temp HOME and `CLAUDE_CONFIG_DIR=<tmp>/cfg`, cwd = project. `claude mcp get unimatrix` shows the entry from `<d>/.claude.json`. A seeded owned entry in the default `~/.claude.json` is still present afterwards and appears in status `alternateProfiles`.
- AC-2 real `opencode` (D-027): run with a temp HOME, cwd = project, once with `OPENCODE_CONFIG_DIR=<tmp>/cfg` and once with only `XDG_CONFIG_HOME=<tmp>/xdg`.
  - Run `opencode debug config --standalone`. `--standalone` is mandatory; without it the CLI attaches to the user's background service and real config. Parse the JSON array.
  - Assert that a document whose `path` equals the resolved `opencode.json` holds `mcp.servers.unimatrix` (or `mcp.unimatrix`) equal to the written entry.
  - Assert that no document from `~/.config/opencode` still holds an owned entry.
  - If `opencode` is absent, skip with the reason printed. The output format is unversioned, so match the path and entry leniently.
- D-026: seed `.claude/settings.local.json`, `.mcp.json`, `opencode.json` and `.codex/config.toml` as symlinks to targets outside the root, plus a symlinked `.claude/` dir. Assert exit 1 `outside-project` naming the link, with the target untouched, in both dry-run and real run. A link to a target inside the root is written through.
- D-025: pre-chmod `remote.json`, `install.json`, `trust.json`, the root marker and `bin/*` to 0644/0755. After init they are 0600/0700. A foreign config file at 0644 keeps 0644.

## 9. Open Questions

Resolved:
- D-014..D-024 settle writer coverage (D-020), Codex and XDG twins (D-019), OpenCode JSONC (D-016/D-024), byte-for-byte (D-022) and ADR-005 (D-021).
- D-025 amends D-023: modes are enforced on every write for the credential and trust-input set (§3.2).
- D-026 amends AC-3: project-file symlinks are contained to the project root (§3.2).
- D-027: AC-2 is also checked with the real `opencode` binary (§8).
- D-028 amends AC-1: under `CLAUDE_CONFIG_DIR` the default `~/.claude.json` is an alternate profile, kept and listed. The former R5 removal is gone.
- D-029: an unparseable project `opencode.json` during removal still exits 1 (§3.4).
- D-032: release history shows no user-scope hook writes (§3.9), so D-030 holds unconditionally.
- D-030: R4 / spec RT-03 stays. Under `CLAUDE_CONFIG_DIR`, owned hooks are removed from the default `~/.claude/settings.json` (In-3 as written), and that file stays in `legacy`.

No open questions remain.
