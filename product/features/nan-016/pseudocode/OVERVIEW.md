# nan-016 Pseudocode — OVERVIEW

> Slice A. Four fixed components per ARCHITECTURE Component Breakdown. Each `.md` below is
> self-contained. This file fixes the shared types, the imported frozen contract, the data
> flow across component boundaries, and the sequencing constraints.

## Components

| # | Component | File (committed) | Pseudocode |
|---|-----------|------------------|-----------|
| 1 | Build + copy-install | `scripts/dogfood-install.sh` | `dogfood-install.md` |
| 2 | Switchover | `scripts/dogfood-switchover.sh` | `dogfood-switchover.md` |
| 3 | Effect-verification harness | `packages/unimatrix/test/dogfood-effect.test.js` | `dogfood-effect.md` |
| 4 | Runbook | `product/features/nan-016/RUNBOOK.md` | `runbook.md` |

## Sequencing constraints (build order)

1. **Component 1 first** — Components 2 and 3 require an installed tree to exist. Component 2
   `require`s the *installed* `lib/merge-settings.js`; Component 3's `before` hook runs
   Component 1 into a test-scoped temp `--target`.
2. **Component 2 second** — Component 3 invokes Component 2's promote/rollback against scratch.
3. **Component 3 third** — depends on 1 + 2.
4. **Component 4 (runbook) any time** — documents 1–3; no code dependency.

## ARCH-OQ-3 RESOLVED — shell-wrapping-Node (not a single Node CLI)

**Decision:** Components 1 and 2 are **POSIX shell scripts (`#!/usr/bin/env sh`, no bashisms)
that wrap Node one-liners** for the two Node-only operations: `npm pack` invocation (shell
calls `npm`, a native CLI — no Node wrapper needed there) and the `mergeSettings` call (a
`node -e` / `node <<'EOF'` one-liner requiring the *installed* `lib/merge-settings.js`).

**Justification:**
- SPEC FR-1/FR-5 and SCOPE name the artifacts as `scripts/dogfood-install.sh` /
  `scripts/dogfood-switchover.sh` — committed `.sh` files. A single `scripts/dogfood.js`
  would rename the committed surface the scope and acceptance map reference.
- The clean-replace safety guard (validate `--target`, `rm`, staged `mv`) is native shell
  filesystem work — keeping it in shell avoids a Node process whose own resolution could
  fail before the guard runs.
- Only the `mergeSettings` step *must* be Node (it consumes the shipped JS contract). Wrapping
  one `node` invocation per script keeps the Node surface minimal and auditable.
- ARCH OQ-3 explicitly permits either and asks the pseudocode agent to pin; shell-wrapping
  keeps the committed filenames, the loud-tooling-error posture (`set -e`; non-zero on
  failure), and the existing repo `scripts/*.sh` convention (OQ-A).

The Node one-liners are passed via a heredoc to `node -` (stdin) or `node -e`, with parameters
passed as `process.argv`/env to avoid shell-quoting injection into JS string literals.

## Shared types / data structures

```
InstalledClientTree            -- the extracted tarball package/ root at the install dir.
  dir:        <target>         -- default ~/.unimatrix/dogfood-client/ ; overridable --target
  contains:   bin/ lib/ skills/ postinstall.js protocols/   (the package.json files[] set)
  excludes:   the platform binary (optionalDependency; never bundled)
  entrypoint: <target>/lib/hook-client/index.js
  mergeApi:   <target>/lib/merge-settings.js                 (consumed by Component 2)
  config:     <target>/lib/hook-client/config.js

ScratchFixture                 -- per harness test, under os.tmpdir(), cleaned up.
  root:           <tmp>/dogfood-scratch-<rand>/
  gitDir:         <root>/.git/                  -- a real DIRECTORY (walkToProjectRoot root)
  settingsPath:   <root>/.claude/settings.json  -- seeded Rust-hook shape ("*" PreToolUse)
  foreignHook:    a non-Unimatrix hook entry, to prove preservation
  scratchHash:    computeProjectHash(realpath(root))   -- MUST differ from this repo's hash
  (no settings.local.json ⇒ SubagentStop opt-out ⇒ 8 registered events, NOT 9)

CommandSource (promote)        -- object arm of mergeSettings:
  { events: HOOK_EVENTS,
    commandForEvent: (event) => buildHookClientCommand(<client>/lib/hook-client/index.js, event) }

CommandSource (rollback)       -- string arm of mergeSettings (legacy):
  "<repo>/target/release/unimatrix"     -- triggers normalizeCommandSource legacy arm
```

## Imported frozen contract (from the INSTALLED `lib/merge-settings.js` + `config.js`, C-8)

Verified against shipped 0.7.2. Components 2 and 3 import these from the **installed** copy
(Component 2) or the in-repo copy for assertion constants (Component 3 may import from the
installed copy too — both are byte-identical post-install, which AC-03 proves).

```
mergeSettings(filePath, commandSource, options)
    -> { actions: string[], content: object }
    commandSource: { events:string[], commandForEvent:(e)=>string }   // promote
                 | string                                              // rollback (legacy)
    options:       { dryRun:boolean }
    NOTE: SubagentStop is opt-in. mergeSettings reads settings.local.json (the sibling of
          filePath at dirname(filePath)=<root>/.claude) and FILTERS SubagentStop out unless
          unimatrix.hooks.subagent_stop === true. On a scratch root with none ⇒ 8 events.

buildHookClientCommand(clientPath, event) -> string
    "node <clientPath> <event>"   bare path; path QUOTED iff it contains whitespace.
    VERIFIED: buildHookClientCommand("/x/lib/hook-client/index.js","SessionStart")
              === 'node /x/lib/hook-client/index.js SessionStart'
              ("/x y/...") === 'node "/x y/lib/hook-client/index.js" Stop'

normalizeCommandSource(string)  legacy arm
    -> emits "LD_LIBRARY_PATH=<binDir> <binary> hook <event>" over HOOK_EVENTS.

isUnimatrixHook(entry) / UNIMATRIX_PATTERNS   (verified 5 patterns)
    matches: ^unimatrix hook , ^unimatrix-server hook , /unimatrix hook , /unimatrix-server hook ,
             node[.exe] <path>/hook-client/index.js  (quoted-double | quoted-single | bare; / or \)
    ⇒ promote updates Rust commands in place (no duplicates); rollback re-owns them.
    ALSO imported by Component 2's one-liner (Stage 3b) as the ownership predicate for the
    post-mergeSettings stale-uni-hook prune (#4930): a uni hook NOT referencing the mode's
    targetToken is removed. mergeSettings keys on EVENT_MATCHERS[event] only, so a uni hook
    under a non-canonical matcher (the live "*" PreToolUse Rust hook) is invisible to it and
    must be pruned by Component 2, not by mergeSettings.

HOOK_EVENTS = ["SessionStart","Stop","UserPromptSubmit","PreToolUse","PostToolUse",
               "PostToolUseFailure","PreCompact","SubagentStart","SubagentStop"]   (9; SubagentStop opt-in)

EVENT_MATCHERS.PreToolUse === PRETOOLUSE_CYCLE_MATCHER
               === "context_cycle|mcp__unimatrix__context_cycle"   (narrowed from "*")

computeProjectHash(realpath(root)) -> 16-hex     // sha256(realpath).slice(0,16)
socketPathFor(hash) / resolve(cwd) walks to .git -> ~/.unimatrix/{hash}/unimatrix.sock
    KEYS ON PROJECT ROOT, not client install location (#4923). Installed client run from a
    given cwd resolves the SAME socket/state as the Rust binary would.
```

Package facts (verified, package.json 0.7.2): `name="@dug-21/unimatrix"`,
`files=["bin/","lib/","skills/","postinstall.js","protocols/"]`, `postinstall="node postinstall.js"`.
`npm pack` tarball name is `dug-21-unimatrix-<version>.tgz` (scope `/` → `-`); resolve by GLOB,
never hardcode the version (the version bumps; #4328 lesson).

## Cross-boundary data flow

```
[1] dogfood-install.sh
      npm pack(packages/unimatrix) -> <staging>/dug-21-unimatrix-*.tgz
      extract package/ -> <staging>/extracted/
      atomic mv -> <target> (InstalledClientTree)        boundary OUT: InstalledClientTree
            |
            v
[2] dogfood-switchover.sh promote --settings S --client <target>
      require(<target>/lib/merge-settings.js)             boundary IN: InstalledClientTree.mergeApi
                                                            (imports mergeSettings + isUnimatrixHook)
      mergeSettings(S, promoteCommandSource(<target>), {dryRun:true})  -> {actions, content}
      pruneStaleUniHooks(content, targetToken, isUnimatrixHook)        -- Stage 3b amendment
         targetToken = <target>/lib/hook-client/index.js (promote) | <repo>/target/release/unimatrix (rollback)
         removes uni-owned hooks NOT referencing targetToken (e.g. the stale "*" PreToolUse Rust
         uni hook mergeSettings leaves behind, #4930); drops emptied matcher groups + event keys
      -> one-liner writes S (unless --dry-run) with `node <target>/lib/hook-client/index.js <EVENT>`
         and NO stale uni hook; foreign hooks preserved      boundary OUT: repointed + pruned settings
            |
            v
[3] dogfood-effect.test.js   (before: runs [1] into temp --target)
      builds ScratchFixture
      runs [2] promote/rollback --settings <scratch> --client <temp-target>
      asserts settings shape (matcher === PRETOOLUSE_CYCLE_MATCHER, 8 events, foreign survives,
              NO stale "*" Rust uni hook survives post-promote — the Stage 3b prune postcondition)
      RE-FIRES: execFileSync("node",[<target>/lib/hook-client/index.js,"SessionStart"],
                             {cwd:scratchRoot, input:JSON}) -> assert exit 0 / empty stdout
            |
            v
[4] RUNBOOK.md   documents [1]=promotion=F6 reset, [2]=switchover/rollback, the matcher delta,
                 fail-open posture, the deferred-flip boundary.
```

## Routed open questions — resolutions (detail in component files)

- **OQ-C (re-fire mechanics):** PINNED to
  `execFileSync("node", [installedIndexJs, EVENT], { cwd: scratchRoot, input: JSON.stringify(payload),
  encoding:"utf8", timeout: <ms> })`; assert `status===0` (or returned object) and empty stdout.
  Detail in `dogfood-effect.md`.
- **OQ-D / ARCH-OQ-1 (AC-03 edit):** the behavior-changing edit is performed in a **throwaway
  copy of the in-repo `packages/unimatrix` tree** under `os.tmpdir()` (never the live working
  tree); a teardown asserts the live working tree is clean even on failure. Detail in
  `dogfood-effect.md`.
- **ARCH-OQ-2 (harness install target):** `before` hook runs Component 1 with
  `--target <os.tmpdir()>/dogfood-client-test-<rand>`; never the real fixed dir. The real
  `~/.unimatrix/dogfood-client/` is never written by the suite. Detail in `dogfood-effect.md`.
- **ARCH-OQ-3 (shell vs Node):** RESOLVED above — shell wrapping Node.

## Clean-replace safety guard (shared invariant, Component 1; honored by Component 3 install)

Before any `rm`, validate `--target`:
1. non-empty after expansion;
2. resolves (realpath of parent + basename) to a path that is **either** under `$HOME/.unimatrix/`
   **or** an explicit absolute path passed as `--target` whose realpath is NOT `$HOME`, `/`,
   `$HOME/.unimatrix` (the parent itself), or any ancestor of the repo;
3. basename is non-empty and not `.`/`..`.
A failure of any check is a loud non-zero exit BEFORE removal. Never degrade to `rm -rf` of a
parent or `$HOME`. (Security Risk: clean-replace `rm -rf`.)
