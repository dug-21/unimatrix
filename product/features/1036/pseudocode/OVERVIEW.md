# Pseudocode Overview — 1036 (harden per-machine harness wiring placement)

Sources: ARCHITECTURE.md (§6 / §6a canonical), SPECIFICATION.md, RISK-TEST-STRATEGY.md, DECISIONS.md D-001..D-033, ADR-001..007 (#5937 #5960 #5962 #5959 #5955 #5943 #5961). Node built-ins only (`fs`, `path`, `crypto`, `child_process`).

## Components

| # | File (pseudocode) | Code file | Why |
|---|---|---|---|
| C1 | config-dirs.md | `lib/config-dirs.js` (new) | Single path builder + `sameRealFile` |
| C2 | trust-writer-core.md | `lib/trust.js` | `hardenedWriteSync` core, `renameWithRetry`, `tempPath` (built-ins only) |
| C3 | safe-write.md | `lib/safe-write.js` (new) | `updateFileSync` / `writeFileSync` / `SafeWriteError` API |
| C4 | wire-plan.md | `lib/wire-plan.js` (new) | Step table W/R/X, gating, executor |
| C5 | git-exclude.md | `lib/git-exclude.js` (new) | `ensureSettingsLocalIgnored` |
| C6 | wire.md | `lib/wire.js` | Orchestrator; output lines; D-013 note |
| C7 | claude-json.md | `lib/claude-json.js` | Pure mutates, `readClaudeMcpState` |
| C8 | merge-settings.md | `lib/merge-settings.js` | Hooks compute inside `updateFileSync` |
| C9 | codex-install.md | `lib/codex-install.js` | Writers via C3; `codexHome` from C1 |
| C10 | opencode-install.md | `lib/opencode-install.js` | Resolved file, D-015, G3 |
| C11 | hook-ownership.md | `lib/hook-ownership.js` | `isLegacyOpencodePlugin` |
| C12 | install-store.md | `lib/install-store.js` | Store writes via C3, enforced modes, dir swap, trust.json symlink check |
| C13 | init.md | `lib/init.js` | Skills + `cleanStaleRemoteSubtree` via C3 |
| C14 | credstore.md | `lib/hook-client/credstore.js` | `write()` via C3 lazily, enforce 0600 |
| C15 | status.md | `lib/status.js` | Resolved-file reporting |
| C16 | atomic-write-removal.md | `lib/atomic-write.js` | Delete |
| C17 | test-infrastructure.md | `test/helpers/*`, suites | Fixtures, seams, real-binary tests |

## Data flow

```
wire(root, o)
  dirs = C1.resolveConfigDirs({env: o.env||process.env, home, platform})   // once (FR-07)
  store legs = C12.installStore(...)  --> each file via C3 --> C2 core
  steps = C4.buildWirePlan({root, home, platform, env, node, dirs, harnessSel, writeMcp, hooksOnly, dryRun})
  {manifest, warnings} = C4.runWirePlan(steps, {dryRun})
        step.run(dryRun) -> harness module (C7/C8/C9/C10/C5) -> C3.updateFileSync(file, mutate, opts)
  C6 renders: resolved lines (FR-50), same-env note (FR-51), action lines, D-013 notice (FR-52)
status.collect(ctx) -> C1 + C7.readClaudeMcpState + read-only scans (no C3)
```

## Shared types (exact names from ARCHITECTURE §6)

- `ConfigDirs = {claude:{dir, claudeJson, userSettings, source}, opencode:{dir, configJson, source}, codex:{dir, hooksJson, configToml, source}, legacy:{userSettings, opencodeJson}, alternates:{harness, file, key, var}[], warnings:string[]}`; `source` ∈ `CLAUDE_CONFIG_DIR|OPENCODE_CONFIG_DIR|XDG_CONFIG_HOME|CODEX_HOME|default`. Each harness block additionally carries `value` (the env value used, or null) so renderers can print `<VAR>=<value>` (FR-34, FR-50) — see Open Question Q3.
- `Mutate = (raw: string|null) => string|null` — sync, pure, `null` = no change.
- `WriteResult = {action: "created"|"updated"|"unchanged", file, target}`.
- `SafeWriteError extends Error {file, code}`; code ∈ `dangling-symlink|not-regular|outside-project|conflict|verify-failed|io`. No subclasses.
- `WireLeg = {harness, surface, action, path, command?, entry?, reason?, warning?, source?}`. New actions: `skipped-dest-unwired`, `skipped-no-git`, `skipped-not-repo`, `skipped-tracked`, `skipped-git-error`, `appended-not-effective`.
- `Step = {id, harness, surface, file, source, gate?, run: (dryRun) => WireLeg[]}`.
- `OK_ACTIONS = {"created","updated","unchanged"}` — defined once in C4, used for gates.

## Mutate + detail convention

Every harness module exposes a pure `compute(raw) -> {text: string|null, detail}`. Mutate = `raw => compute(raw).text`. After `updateFileSync` returns `{action, file, target, before}`, leg detail = `compute(before).detail`. A skip (foreign / malformed-with-warning) is `text: null` + `detail.skip = {action, reason, warning}`; never thrown. Parse failures that must exit 1 throw from `compute`; C3 propagates the original error unchanged.

## Build order (waves)

FR-33 / SR-03: writer migration first, private writers deleted last.

1. **Wave 1 — writer**: C2 (core in trust.js) → C3 (safe-write). Port existing `install-store.test.js` retry/mode/cleanup and `trust.test.js` rename tests to run against C3/C2 *while `atomic-write.js`, install-store `atomicWrite`/`tmpName` and trust `recordTrustIdentity` body still exist*; all green. C11 and C1 can proceed in parallel (no deps).
2. **Wave 2 — consumers migrate**: C12, C14, C13, C7, C8, C9, C10 switch to C3 (C9/C10/C7 also take paths from C1). C5 (new, depends on C3).
3. **Wave 3 — plan + orchestration**: C4 (depends on C1, C5, C7–C10, C11), C6 (depends on C4), C15 (depends on C1, C7).
4. **Wave 4 — deletion + tests**: C16 delete `atomic-write.js`, `claude-json.atomicWriteText`, install-store private writers, `wire.gitIgnored`/`GITIGNORE_WARNING`, `status.codexHome`; grep tests (FR-01, FR-20); C17 fixtures, parity, real-binary AC-1/AC-2.

C17 helper `scrubbedEnv` should land in Wave 1 so every new test uses it.

## File-size budget

All new files < 250 lines; `trust.js` ~375, `codex-install.js` ~337 after change. No file nears 500 lines except existing `merge-settings.js` (478 now; the refactor must be net-neutral — move the read/write out, do not add).

## Open questions (for architect / SM)

- **Q1** `updateFileSync` return gains `before: string|null` (raw the decision used) so callers derive leg detail without side-channel state. Additive to §6 signature.
- **Q2** Git-exclude leg naming: ARCHITECTURE §6 says harness `"git"`, surface `"exclude"`; SPEC FR-40/FR-46 and the Domain Model say harness `claude-code`, surface `git-exclude`. Not in §6a. Pseudocode uses the spec form (user-visible line `claude-code git-exclude: ...`). Architect to confirm and add to §6a.
- **Q3** `ConfigDirs` per-harness `value` field (env value used) is needed to print `<VAR>=<value>` (FR-34, FR-50); not listed in §6. Additive.
- **Q4** FR-05 keeps the D-085 `CODEX_HOME` warning text; FR-06 defines a generic text for all vars. Pseudocode keeps the legacy text for `CODEX_HOME` only.
- **Q5** Symlink loop (S6) and non-regular target share code `not-regular`; spec only says "throws naming the path". Confirm vs `io`.
- **Q6** W1 `unchanged` now means byte-equal (writer) rather than parsed deep-equal (old wire.js). Reformat-only hand edits now yield `updated` + rewrite.
- **Q7** `install-store.writeRootMarker` return changes from path to `{file, action}`; tests adjusted.
