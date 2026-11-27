# IMPLEMENTATION-BRIEF — 1036: Harden Per-Machine Harness Wiring Placement

GH #1036. Node-only work in `packages/unimatrix/lib/`. No Rust. Built-ins only (`fs`, `path`, `crypto`, `child_process`, `os`). Re-synthesized after design-review decisions D-025..D-030.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/1036/SCOPE.md |
| Scope Risk Assessment | product/features/1036/SCOPE-RISK-ASSESSMENT.md |
| Decision Register (D-001..D-030, binding) | product/features/1036/DECISIONS.md |
| Architecture (incl. §6a Canonical Contract Names, §8 test infrastructure, §9) | product/features/1036/architecture/ARCHITECTURE.md |
| ADR Index | product/features/1036/architecture/ADR-INDEX.md |
| Specification | product/features/1036/specification/SPECIFICATION.md |
| Risk Strategy | product/features/1036/RISK-TEST-STRATEGY.md |
| Alignment Report (round 2) | product/features/1036/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/1036/ACCEPTANCE-MAP.md |
| Research input | product/research/ass-110/ass-110-findings.md |

Naming authority: ARCHITECTURE §6a. Where any document differs, §6a wins.

## Goal

Keep nan-024's wiring destinations (D-001, D-003) and harden them. `init` / `init --bundle` / `wire` must write each harness's wiring to the file that harness reads under `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`/`XDG_CONFIG_HOME` and `CODEX_HOME`; remove owned entries from abandoned or shadowing nan-024 legacy locations while keeping other live profiles; and route every file Unimatrix writes through one symlink-safe, race-checked, verified writer that enforces modes on credential and trust inputs and contains project-file links to the project root. `status` must name a lost Claude local-scope entry and list other profiles, and init must keep `.claude/settings.local.json` git-ignored through the per-repo exclude file without touching any tracked file.

## Component Map

| Component | Tier | Pseudocode | Test Plan |
|-----------|------|-----------|-----------|
| C1 config-dirs resolver (`lib/config-dirs.js`, new) | hard | pseudocode/config-dirs.md | test-plan/config-dirs.md |
| C2 writer core (`lib/trust.js`, extended) | hard | pseudocode/trust-writer-core.md | test-plan/trust-writer-core.md |
| C3 writer API (`lib/safe-write.js`, new) | hard | pseudocode/safe-write.md | test-plan/safe-write.md |
| C4 wiring plan (`lib/wire-plan.js`, new) | hard | pseudocode/wire-plan.md | test-plan/wire-plan.md |
| C5 git exclude (`lib/git-exclude.js`, new) | hard | pseudocode/git-exclude.md | test-plan/git-exclude.md |
| C6 orchestrator (`lib/wire.js`) | hard | pseudocode/wire.md | test-plan/wire.md |
| C7 Claude MCP (`lib/claude-json.js`) | hard | pseudocode/claude-json.md | test-plan/claude-json.md |
| C8 Claude hooks (`lib/merge-settings.js`) | hard | pseudocode/merge-settings.md | test-plan/merge-settings.md |
| C9 Codex (`lib/codex-install.js`) | hard | pseudocode/codex-install.md | test-plan/codex-install.md |
| C10 OpenCode (`lib/opencode-install.js`) | hard | pseudocode/opencode-install.md | test-plan/opencode-install.md |
| C11 ownership (`lib/hook-ownership.js`) | hard | pseudocode/hook-ownership.md | test-plan/hook-ownership.md |
| C12 install store (`lib/install-store.js`) | hard | pseudocode/install-store.md | test-plan/install-store.md |
| C13 init (`lib/init.js`) | hard | pseudocode/init.md | test-plan/init.md |
| C14 credstore (`lib/hook-client/credstore.js`) | hard | pseudocode/credstore.md | test-plan/credstore.md |
| C15 status (`lib/status.js`) | hard | pseudocode/status.md | test-plan/status.md |
| C16 removed (`lib/atomic-write.js`) | hard | pseudocode/atomic-write-removal.md | test-plan/atomic-write-removal.md |
| C17 test infrastructure (`test/helpers/*`, existing suites) | routine | pseudocode/test-infrastructure.md | test-plan/test-infrastructure.md |

Component file names confirmed by Stage 3a (all 17 pseudocode + test-plan files exist at the listed paths). Stage 3a items, now in ARCH §3.3/§6/§6a: git-exclude leg is harness `claude-code`, surface `git-exclude` (SPEC FR-40/46); `updateFileSync` also returns `before`; `ConfigDirs` carries per-harness `value`. Tier comes from ARCHITECTURE §2.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview (actual, Stage 3a done) | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan (actual, Stage 3a done) | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Resolved Decisions

ADRs: ADR-001 (#5937), ADR-002 (#5960), ADR-003 (#5962), ADR-004 (#5959), ADR-005 (#5955), ADR-006 (#5963), ADR-007 (#5961). nan-024 amendments: ADR-001 (#5945), ADR-003 (#5956), ADR-006 (#5947), ADR-004 (#5948).

| Decision | Resolution | Source | ADR (Unimatrix ID) |
|----------|------------|--------|--------------------|
| Placement rule and exceptions | Project-scoped per-machine surface first (D-046). Named global-file exceptions: Claude MCP `.claude.json`, Codex, OpenCode MCP. Measured `CLAUDE_CONFIG_DIR` layout. Intent gate is store/credential | D-001, D-003, D-046 | ADR-001 (#5937); amends nan-024 ADR-001 (#5945), ADR-003 (#5956) |
| One resolver | `lib/config-dirs.js`: sole path builder; precedence per harness (OpenCode measured); invalid/relative values ignored with a warning; `source`; `legacy` = `~/.claude/settings.json`, `~/.config/opencode`; `alternates` incl. default `~/.claude.json` | SR-09, SR-17, D-019, D-028 | ADR-002 (#5960) |
| One hardened synchronous writer | `lib/safe-write.js` API over a core in `trust.js`: realpath-follow, `wx` random temp, fsync, sha256 CAS, fixpoint verify, win32 retry, 3 attempts then exit 1; `enforceMode` (D-025); `containWithin` (D-026); `atomic-write.js` deleted | D-010, D-013, D-020, D-025, D-026 | ADR-003 (#5962) |
| Writer core in `trust.js` | Single core, no second copy; symlinked `trust.json` exits 1 with canonical message | D-021 | ADR-005 (#5955) |
| Wiring plan table | `lib/wire-plan.js` W/R/X steps identical in both modes; write-before-remove; legacy removal only `~/.claude/settings.json` hooks and `~/.config/opencode` MCP; default `~/.claude.json` kept; same-real-file rule | D-007, D-015, D-019, D-022, D-028, D-029 | ADR-004 (#5959) |
| Git-ignore ensure | Per-repo exclude file via `git rev-parse --git-path info/exclude`; check-ignore first; branches reported; append via writer; re-check | D-011 | ADR-006 (#5963) |
| status and init reporting | Resolved files and sources; local-scope check on resolved `.claude.json` only; `staleLegacy`; `alternateProfiles` incl. default `~/.claude.json`; `opencodeConfig` unreadable; init `[VAR]` and D-013 note | D-013, D-019, D-024, D-028 | ADR-007 (#5961) |

### Decision Traceability (every D-entry)

| ID | Decision (short) | Where it lands |
|----|------------------|----------------|
| D-001 | Keep nan-024 placement; harden only | No destination moves; ADR-001 (#5937) |
| D-002 | MCP failure outside Unimatrix projects accepted; no zero-tool handshake; ADR-004/D-043 unchanged | Out of scope; shim/launcher untouched |
| D-003 | Claude MCP stays in `.claude.json` local scope; named exception | ADR-001 (#5937); C7 |
| D-004 | `.gitignore` append (superseded by D-011) | Not implemented; replaced by D-011 |
| D-005 | Scope In-1..6 and AC-1..AC-10 approved; amended by D-010, D-011, D-026, D-027, D-028 | ACCEPTANCE-MAP.md |
| D-006 | Prior verdicts: D-026(nan-024) revise, D-040 keep, D-046 keep, D-060 revise, D-039 revise | ADR-001 (#5937); SCOPE Prior Decision Verdicts |
| D-007 | Never remove a twin that is the same real file as the target | `sameRealFile` in C1; C4 filter; ADR-004 (#5959) |
| D-008 | Spec defines AC-8 edge cases | SPEC FR-40..FR-46; C5 |
| D-009 | context_cycle topic `1036-harness-wiring` | Tracking label only |
| D-010 | Every file Unimatrix writes uses the one hardened writer; weak path removed | C3, C2; ADR-003 (#5962) |
| D-011 | Ignore via `git rev-parse --git-path info/exclude`; skip and report outside git; supersedes D-004 | C5; ADR-006 (#5963); AC-8 |
| D-012 | File-level Codex/Claude checks accepted; follow-up #1053 | Test plan; skip messages name #1053 |
| D-013 | Lost-update window accepted; synchronous read-modify-rename; init notice; status backstop | FR-21, FR-52, FR-35; ADR-003 (#5962) |
| D-014 | Pre-nan-024 `.opencode/plugins/` shim and package.json dep out of scope | NOT in Scope; C10 |
| D-015 | Owned OpenCode entry at old default location counts as prior opt-in | C10, W5; ADR-004 (#5959) |
| D-016 | Unparseable user `opencode.json` skipped with warning, exit 0 (amended by D-024) | C10; SPEC FR-14 |
| D-017 | AC-5 "every leg" = harness legs + git-exclude; store legs also `unchanged` | C12; FR-26a |
| D-018 | New-file mode per caller (amended by D-023, D-025) | C3 `newMode` |
| D-019 | Remove nan-024 legacy locations only; alternate profiles never removed, `status` lists them | C1 `legacy`/`alternates`; C4; C15; ADR-004 (#5959) |
| D-020 | Writer coverage: all init/bundle/wire files incl. `remote.json`, skills, `bin/*`, root marker; excludes hook-time state; package copy is atomic dir rename | FR-20; C12, C13, C14; ADR-003 (#5962) |
| D-021 | Single writer core in `trust.js`; symlinked `trust.json` exits 1 with fix message | C2; C12; ADR-005 (#5955) |
| D-022 | Byte-for-byte = JSON deep-equal with order, TOML identical outside owned table, untouched not rewritten, indent reused | FR-16; ADR-004 (#5959) |
| D-023 | New config/credential 0600; new `bin/*` 0700 (amended by D-025) | C3 `newMode`; ADR-003 (#5962) |
| D-024 | Unparseable user `opencode.json`: install warns exit 0; `status` names file as leaving OpenCode unwired | C10, C15 `opencodeConfig`; ADR-007 (#5961) |
| D-025 | Enforce modes, re-tightened on every write: `remote.json`, `install.json`, `trust.json`, root marker 0600; `bin/*` 0700. Other existing files keep mode; mode-only drift repaired by chmod, reported `updated` | `enforceMode` in C2/C3; C12, C14; ADR-003 (#5962); R-01 |
| D-026 | Project-file links written through only if real target inside `realpath(root)`; else exit 1 `outside-project` naming the link. User/global links per AC-3. Exclude file exempt | `containWithin` in C3; FR-22a; ADR-003 (#5962); R-03 |
| D-027 | Real-`opencode` check added to AC-2: `opencode debug config --standalone` | AC-2(b); C17; ARCH §8 |
| D-028 | D-019 wins: with `CLAUDE_CONFIG_DIR` set, default `~/.claude.json` (incl. `projects[root]`) is kept and listed by `status`; AC-1 amended; RT-05/R5 retired | C7, C4, C15; ADR-004 (#5959), ADR-007 (#5961); AC-1 |
| D-029 | Unparseable project `opencode.json` during removal stays exit 1 | C10; SPEC FR-14, FR-53; R-16 |
| D-030 | Keep R4/RT-03: with `CLAUDE_CONFIG_DIR` set, remove owned hooks from default `~/.claude/settings.json` (In-3 as written); file stays in `legacy`; foreign hooks untouched. Decided by uni-zero-design under primary authority; hooks live in project `settings.local.json` so nothing goes dark and duplicate firing is prevented (AC-6, #5803) | C4 R4; C1 `legacy`; ADR-004 (#5959); S-6 |

## Files to Create/Modify

All under `packages/unimatrix/`.

| File | Action | Summary |
|------|--------|---------|
| `lib/config-dirs.js` | create | `resolveConfigDirs`, `sameRealFile`; precedence, warnings, `legacy` and `alternates` (~100 lines) |
| `lib/safe-write.js` | create | `updateFileSync`, `writeFileSync`, `SafeWriteError`, `tempPath`, `renameWithRetry`; `enforceMode`, `containWithin` options (~120) |
| `lib/wire-plan.js` | create | `buildWirePlan`, `runWirePlan`; W/R/X step table, gating, legacy filter (~250) |
| `lib/git-exclude.js` | create | `ensureSettingsLocalIgnored` (~100) |
| `lib/trust.js` | modify | Hosts `hardenedWriteSync` core and `renameWithRetry`; built-ins only; mode enforcement and containment in core; trust check and `trust set` unchanged |
| `lib/wire.js` | modify | Shrinks: transport, preflight, installStore, plan build/run, action lines with `[VAR]`, D-013 note. Delete `gitIgnored`, `GITIGNORE_WARNING`, dispatch functions, hardcoded paths |
| `lib/claude-json.js` | modify | Pure mutates: destination + G2 in one mutate; `.mcp.json` removal; `readClaudeMcpState`; never touches the default `~/.claude.json` under an override (D-028); drop `atomicWriteText` re-export and mtime/size CAS |
| `lib/merge-settings.js` | modify | `mergeSettings` / `removeOwnedHooks` compute inside `updateFileSync` (project files with `containWithin`) |
| `lib/codex-install.js` | modify | Writers via `updateFileSync`; `codexHome` delegates to resolver (re-exported); rethrow `SafeWriteError` |
| `lib/opencode-install.js` | modify | Resolved path from resolver; legacy `~/.config/opencode` removal; D-015 opt-in; G3 in same mutate as project `mcp.unimatrix` removal; D-029 exit 1 on unparseable project file; rethrow `SafeWriteError` |
| `lib/hook-ownership.js` | modify | Add `isLegacyOpencodePlugin(el)` |
| `lib/install-store.js` | modify | All writes via safe-write; enforced modes (`bin/*` 0700; `install.json`, `trust.json`, root marker 0600); store legs `created/updated/unchanged`; atomic dir rename for package copy; symlinked `trust.json` pre-check and rewrapped TrustError; prune keys on `.tmp-` |
| `lib/init.js` | modify | `cleanStaleRemoteSubtree` and skill install write via safe-write (skills with `containWithin`). No new logic |
| `lib/hook-client/credstore.js` | modify | `write()` lazily requires safe-write with `enforceMode: 0o600` |
| `lib/status.js` | modify | `collect(ctx)` injectable; paths from resolver only; adds `configDirs`, `claudeMcp`, `opencodeConfig`, `staleLegacy`, `alternateProfiles`; delete private `codexHome` and `home/.claude.json` paths |
| `lib/atomic-write.js` | delete | Weak path removed (D-010) |
| `test/helpers/home-guard.js` | modify | `scrubbedEnv(extra)`; `assertTmpHome` rejects overrides outside tmp |
| `test/helpers/wire-fixture.js` | modify | `seedSymlinked`, `seedLegacy`, `seedAlternates`, real-git `makeProject` (worktree, submodule) |
| `test/*.test.js` (existing) | modify | Extend `merge-settings`, `wire`, `init-integration`, `install-store`, `trust`, `opencode-install`, `codex-install`, `hook-ownership`, `status` suites |
| `test/config-dirs.test.js`, `test/safe-write.test.js`, `test/wire-plan.test.js`, `test/git-exclude.test.js` | create | New modules only (NFR-07) |

## Data Structures

```
ConfigDirs = {
  claude:   {dir, claudeJson, userSettings, source, value},
  opencode: {dir, configJson, source, value},
  codex:    {dir, hooksJson, configToml, source, value},
  legacy:   {userSettings, opencodeJson},                       // removal candidates (D-019, D-028)
  alternates: [{harness, file, key, var}],                      // status-only; incl. default ~/.claude.json
  warnings: string[]
}
source in "CLAUDE_CONFIG_DIR" | "OPENCODE_CONFIG_DIR" | "XDG_CONFIG_HOME" | "CODEX_HOME" | "default"
value  = raw env value used, null when source is "default"; for <VAR>=<value> output (FR-34, FR-50); not in status --json

SafeWriteError extends Error {file, code}   // single class, no subclasses
code in "dangling-symlink" | "not-regular" | "outside-project" | "conflict" | "verify-failed" | "io"

Step    = {id: "W1".., harness, surface, file, source, gate?, run(dryRun) -> WireLeg[]}
WireLeg = {harness, surface, action, path, command?, entry?, reason?, warning?, source?}
  new actions: skipped-dest-unwired, skipped-no-git, skipped-not-repo, skipped-tracked,
               skipped-git-error, appended-not-effective

status --json additions:
  configDirs, claudeMcp {status: wired|missing|foreign|unreadable|n/a, file, key},
  opencodeConfig {status: ok|absent|unreadable, file},
  staleLegacy [{harness,file,key}] (unreadable legacy: key null, unreadable true),
  alternateProfiles [{harness,file,key,var}]
```

Writer fault seam: `opts.fault(stage)`, stage in `"after-read" | "before-rename" | "after-rename"`.

## Function Signatures

```
resolveConfigDirs({env, home, platform}) -> ConfigDirs          // never mutates env, never throws
sameRealFile(a, b) -> boolean                                    // D-007 identity
codexHome(env, home) -> {dir, warning?}                          // kept signature, delegates to resolver

updateFileSync(file, mutate:(raw|null)=>string|null,
  opts?:{dryRun, newMode=0o600, enforceMode, containWithin, verify=true, attempts=3,
         retry:{count,delayMs}, platform, fault})
  -> {action:"created"|"updated"|"unchanged", file, target, before}   // before: raw the deciding mutate call saw
writeFileSync(file, data, opts?) -> {action, file, target}
tempPath(dest) -> "<dir>/.<base>.tmp-<16hex>"
renameWithRetry(from, to, platform, retry?) -> void
hardenedWriteSync(file, data, opts)                              // trust.js core

buildWirePlan({root, home, platform, env, node, dirs, harnessSel, writeMcp, hooksOnly, dryRun}) -> Step[]
runWirePlan(steps, {dryRun}) -> {manifest: WireLeg[], warnings: string[]}
ensureSettingsLocalIgnored(root, {dryRun, run?}) -> WireLeg      // harness "claude-code", surface "git-exclude"
isLegacyOpencodePlugin(el) -> boolean
writeClaudeMcp({file, projectRoot, entry, platform, dryRun}) -> {action, file, key, warning?, removedUserScope}
readClaudeMcpState(file, projectRoot, platform) -> {status, file, key}
writeOpencodeMcp({file, platform, home, node, harnessSelected, dryRun}) -> WireLeg
removeOwnedOpencodeProjectMcp(root, dryRun) -> {actions, warnings, changed, removedPlugin}
status.collect(ctx?:{env, home, platform, cwd}) -> Info
```

Removed: `removeOwnedClaudeLegacy` (no longer exists, D-028). `containWithin` is passed as `<root>` for `settings.local.json`, project `settings.json`, `.mcp.json`, project `opencode.json`, `.codex/*`, `.claude/skills/*`; not for the exclude file or user/global files. Mutate contract: synchronous, pure, `null` means no change; the writer re-runs mutate on read-back and the result must be `null` (fixpoint verify).

Mode rules (D-025, D-023): `enforceMode` re-applies on every write, new or existing: `remote.json`, `install.json`, `trust.json`, `<hash>/root` 0600; `~/.unimatrix/bin/*` 0700 (win32 0600). Equal bytes with a drifted enforced mode: chmod the real target, report `updated`, no temp or rename. All other existing files keep mode; other new files 0600.

## Wiring Plan Order (ARCHITECTURE §3.3)

store, W1 (Claude hooks), W2+R1 (Claude MCP + G2 in one mutate), R2-R4, R6, Codex W3, W4, R7-R9, OpenCode W5, R10-R12, X1 (git exclude). R5 is retired (D-028). Gated removals (R1, R4, R10, X1) run only when the destination leg ended `created`/`updated`/`unchanged`; otherwise they emit `skipped-dest-unwired` in both modes. A thrown `SafeWriteError` ends the run, so no later removal executes. Alternate profiles never produce a step.

## Constraints

- R1: no PATH, profile or repo-layout changes. Env vars are read, never set (git child env scrub excepted). No tracked file edited; the ignore entry goes to the exclude file.
- ADR-004 / D-043 unchanged. Launcher, shim, trust check, latency budgets untouched.
- D-040: entry text identical per OS family; no marker fields; ownership shape-based.
- D-065: a foreign value at a `unimatrix` key is never overwritten at resolved, twin or G2 locations.
- AC-6 uniqueness across resolved and removal locations; alternate profiles (AP-1..AP-3, incl. default `~/.claude.json`) excluded.
- FR-12d / FR-60..63: one planning path for both modes; dry-run writes nothing (no file, temp, dir, mtime, chmod), and runs the D-026 containment check.
- Parse failure: resolved `.claude.json`, project settings/`.mcp.json` and project `opencode.json` (incl. during removal, D-029) exit 1 untouched; user `opencode.json` and Codex files warn, `skipped-malformed`, exit 0.
- D-026: containment is the single gate for project files (the old lexical `isWithinProject` guards are removed); realpath of root resolved first; separator-boundary prefix check; re-checked before rename.
- NFR-03: no `async`, `await`, `.then(`, `setTimeout` in the writer; no await between read and rename.
- NFR-02/PL-10: new modules <= 300 code lines; no production file over 1,000.
- `lib/trust.js` stays built-ins-only and byte-identical to installed `bin/trust.js`. Rewrite `bin/trust.js` by rename only.
- `credstore.js` lazy-requires safe-write so the hook hot path does not load it.
- Each enforced-mode caller (install-store, credstore, root marker, bin copy) must pass `enforceMode`; a missing caller passes content tests and fails only the mode-drift test.
- Real-binary checks: `claude mcp get` (AC-1c) and `opencode debug config --standalone` (AC-2b; `--standalone` mandatory, `opencode mcp list` unusable); skip with a reason naming the binary and #1053 when absent. Codex file-level only.
- Test env: all four override vars scrubbed by default; overrides pass explicitly under tmp (SR-10, #5911).
- Migrate existing `install-store`, `trust` and `credstore` retry/mode/cleanup tests to the new writer and keep them green before deleting private writers (FR-33). Order early.
- Test infrastructure is cumulative. Anti-stub. No pseudocode in this brief.

## Dependencies

- nan-024 base (merged, PR #1033).
- Node built-ins only; `check-zero-deps.js` must pass.
- `git` (optional; absence yields `skipped-no-git`), `claude` and `opencode` (optional; AC-1(c), AC-2(b); both in the devcontainer).
- Existing helpers: `hook-ownership.js`, `claude-json.js` `resolveProjectKey`, `hook-client/config.js` `walkToProjectRoot`/`resolveGitFile`, `opencode-install.js` `detectIndent`/`readJsonSafe`.
- Closes #1035 F2 and the temp-naming item (F4 stays open). Related: #998, #1014, #1034, #1053.

## NOT in Scope

- Shim, trust-check logic, launcher hook path, latency budgets.
- Zero-tool `mcp-stdio` handshake outside a project (D-002).
- OpenCode hooks (#998); Claude plugin packaging; uninstall.
- Moving any destination; Claude hook exec form; OpenCode `project_overlay`; `{env:VAR}` forms.
- Trust-check TOCTOU (#1035 F4); broader `status` diagnostics (#1034) beyond FR-34..FR-38.
- Removing entries at alternate-profile locations (AP-1..AP-3, incl. default `~/.claude.json`) (D-019, D-028).
- Pre-nan-024 `.opencode/plugins/` shim and `.opencode/package.json` dep (D-014).
- Hook-time runtime state writes (D-020).
- Temp files from killed processes; hard-link, ownership and ACL preservation; `opencode.jsonc`; `OPENCODE_CONFIG`.
- Modifying the git index or `.gitignore`.

## Alignment Status

Source: ALIGNMENT-REPORT.md (round 3). PASS 5, WARN 1, VARIANCE 0, FAIL 0. The WARN is the set of Scope Additions below (accepted: FR-50/51 init resolved lines, FR-20a hook-time symlink skip, NFR-04 benchmark), each traced to a scope risk.

Closed: V-1 (file modes) by D-025; V-2 (repo symlinks) by D-026; D-019/AC-1 conflict by D-028; AC-2 real-binary check by D-027; project `opencode.json` posture by D-029; V-3 / RT-03 by D-030 (R4 kept, In-3 as written).

No open items remain for the human. Note: D-030 is recorded as decided by `uni-zero-design` under primary authority, not by the human; it keeps In-3 as written and amends no AC.

Labelling for implementers and testers: the OpenCode real-binary check is AC-2(b); its mechanism is `opencode debug config --standalone`, not `opencode mcp list`. Use ARCHITECTURE §6a values (`foreign`, `unreadable`) and current ADR IDs.

### Other awareness items
- D-029 means an unparseable project `opencode.json` exits 1 while an unparseable user-scope one warns (exit 0); `status` names the user-scope file.
- D-030 means init removes owned hooks from the default `~/.claude/settings.json` under an override (R4 kept); foreign hooks there are untouched.
- AC-1(c) and AC-2(b) skip in CI (D-012, D-027, #1053).
