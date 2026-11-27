# C17 — Test infrastructure (cumulative)

Extend existing helpers; no new scaffolding dirs.

## `test/helpers/home-guard.js` (Wave 1)
```
OVERRIDE_VARS = ["CLAUDE_CONFIG_DIR","OPENCODE_CONFIG_DIR","XDG_CONFIG_HOME","CODEX_HOME"]
function scrubbedEnv(extra = {}) -> object
  env = {...process.env}; for v in OVERRIDE_VARS: delete env[v]
  for [k,v] of entries(extra):
     if OVERRIDE_VARS.includes(k) and path.isAbsolute(v): assertUnderTmp(v)   // reuse real()/tmpdir logic of assertTmpHome
     env[k] = v
  return env
module.exports adds scrubbedEnv, OVERRIDE_VARS
```
All `wireOpts` and spawned-CLI tests pass `env: scrubbedEnv(...)` (SR-10, Unimatrix #5803/#5911 lesson: never touch the real user config).

## `test/helpers/wire-fixture.js`
```
seedSymlinked(file, target)            // mkdir -p both dirs; write target if not exists; symlinkSync(target, file)
seedLegacy(home, dirs)                 // owned hooks in ~/.claude/settings.json + owned mcp.unimatrix in ~/.config/opencode/opencode.json
seedAlternates(home, env)              // owned entries in default ~/.claude.json (projects[root] + top-level), $XDG/opencode/opencode.json, ~/.codex/*
makeProject({git: true, worktree?: bool, submodule?: bool})  // real `git init` (env scrubbed of GIT_*), returns {root, worktreeRoot?}
snapshot(dir) -> Map<path,{bytes,mtimeMs,mode}>; diffSnapshots(a,b) -> changed paths   // AC-9 / FR-63
listTemps(dir) -> paths matching /\.tmp-[0-9a-f]{16}$/        // AC-10
```

## Writer seam tests (C3) — `test/safe-write.test.js` (new suite file, uses existing helpers)
S1–S12 table, fault stages, CAS/verify exhaustion, mode matrix, containment fixtures, win32 via injected `platform` + stubbed fs (`renameSync` EPERM sequence; dir-fsync open throwing).

## Ported (FR-33, first)
`install-store.test.js` retry/mode/cleanup and `trust.test.js` rename tests run against C2/C3 while private writers still exist; then unchanged after deletion.

## Parity suites
- AC-9: for each fixture (no override, each override, same-file overrides, legacy seeded, foreign seeded, symlinked configs, git/no-git), run `wire({dryRun:true})` then `wire()` on a fresh copy: manifests equal on `{harness, surface, action, path, reason, entry, command}` (except FR-42 step 7); `diffSnapshots` == non-unchanged non-skipped leg paths (store paths included).
- AC-5: second real run → every leg `unchanged`; snapshot mtimes equal (incl. exclude file, store files).
- SR-09: `status.collect({env, home, platform, cwd}).configDirs` paths equal manifest destination paths.

## Real-binary tests
- AC-1 (`c14-claude.test.js` or new `real-claude-config-dir.test.js`): skip with printed reason if `claude` not on PATH. Temp HOME, `CLAUDE_CONFIG_DIR=<tmp>/cfg`, seed owned entry in default `<home>/.claude.json`; run init (scrubbed env + override); `claude mcp get unimatrix` with same env, cwd = project → found; default file still has seeded entry; `status --json` `alternateProfiles` lists it.
- AC-2: skip if `opencode` absent. Two runs: `OPENCODE_CONFIG_DIR=<tmp>/cfg`; `XDG_CONFIG_HOME=<tmp>/xdg`. `opencode debug config --standalone` (mandatory flag) → parse JSON array; doc with `path === resolved` holds `mcp.servers.unimatrix` or `mcp.unimatrix` deep-equal written entry; no doc under `~/.config/opencode` holds an owned entry. Lenient matching.

## D-025 / D-026 fixtures
As ARCHITECTURE §8 (pre-chmod store files; symlinked project files + symlinked `.claude/` dir, both modes).

## Updates to existing tests
- `wire.test.js`: drop `GITIGNORE_WARNING` assertions; assert `claude-code git-exclude` leg instead.
- `opencode-install.test.js`: pass `file` from `resolveConfigDirs`.
- `install-store.test.js`: `writeRootMarker` returns `{file, action}`; store legs not `written`.
