# Component 3 — `packages/unimatrix/test/dogfood-effect.test.js` (Effect-verification harness)

## Purpose

`node --test` harness proving AC-02 (switchover repoints by effect) and AC-03 (copy-install
isolation = code freeze) by **real effect**: run the real scripts against a scratch project root
and a test-scoped install, parse resulting settings, and **re-fire a real hook** against the
installed path. Never touches this repo's live settings. Cumulative `node --test` infra (extend
existing helpers under `packages/unimatrix/test/`, do not create isolated scaffolding).

Covers FR-10, FR-11, FR-12, FR-13, NFR-3, NFR-4; addresses SR-04, SR-06, SR-07, SR-08 / the
Critical risks R-01..R-04 and R-07, R-08, R-13, R-15. Non-vacuous: R-01 and R-04 REQUIRE a
negative control that fails when the install/leak is broken.

## Resolved routed open questions

- **OQ-C (re-fire mechanics — PINNED shape):**
  ```
  const res = execFileSync("node", [installedIndexJs, EVENT], {
      cwd: scratchRoot,                     // walkToProjectRoot -> scratchHash, NOT live hash
      input: JSON.stringify(payload),       // synthetic hook stdin
      encoding: "utf8",
      timeout: 15000,
  });
  // execFileSync throws on non-zero exit; success path -> res is stdout string.
  // assert: no throw (exit 0) AND res === "" (empty stdout).  Wrap in try to convert a throw
  //   into an explicit assertion failure carrying status/stderr.
  ```
- **OQ-D / ARCH-OQ-1 (AC-03 edit location):** the behavior-changing edit is applied to a
  **throwaway copy of `packages/unimatrix`** under `os.tmpdir()`, re-packed into a SECOND
  test-scoped install dir; the ORIGINAL installed bytes are then asserted unchanged. The live
  working tree is **never** edited. A teardown asserts the live tree is clean. (R-13)
- **ARCH-OQ-2 (install target):** `before` hook runs `dogfood-install.sh --target <tmp>` into
  `os.tmpdir()`, never `~/.unimatrix/dogfood-client/`. (R-15)

## Shared fixture helpers (functions)

### `makeScratchRoot() -> { root, settingsPath, scratchHash }`  (R-03)
```
makeScratchRoot:
  root <- fs.mkdtempSync(path.join(os.tmpdir(), "dogfood-scratch-"))
  root <- fs.realpathSync(root)                      # mirror config.js realpath (R-03-3): a
                                                      # symlinked os.tmpdir() cannot collapse onto
                                                      # the live root.
  fs.mkdirSync(path.join(root, ".git"))              # a real DIRECTORY -> walkToProjectRoot root
  fs.mkdirSync(path.join(root, ".claude"))
  settingsPath <- path.join(root, ".claude", "settings.json")
  write settingsPath <- SEED_RUST_SHAPE              # "*" PreToolUse Rust commands + 1 foreign hook
  scratchHash <- computeProjectHash(root)            # imported from installed (or in-repo) config.js
  assert scratchHash !== computeProjectHash(repoRoot())   # MUST differ from live (R-03-1)
  assert NOT exists(~/.unimatrix/<scratchHash>/unimatrix.sock)  # daemon-absent precondition (R-03-2)
  return { root, settingsPath, scratchHash }
```

`SEED_RUST_SHAPE` = the live settings *shape* (read live `.claude/settings.json` READ-ONLY, copy
its structure into the fixture — never write live; R-08-3) reduced to: Rust-binary commands with
PreToolUse matcher `"*"` for the events, PLUS one clearly-foreign hook (e.g. a `PreToolUse`
matcher with a non-Unimatrix command) to prove preservation.

### `installToTemp() -> clientDir`  (R-15 / ARCH-OQ-2)
```
installToTemp:
  clientDir <- path.join(os.tmpdir(), "dogfood-client-test-" + rand())
  run dogfood-install.sh with --target=clientDir   (execFileSync, assert exit 0)
  assert clientDir is under os.tmpdir()             # never the real fixed dir
  return clientDir
```

### `tmpdirGuard(p)`  (R-08 — the safety boundary, with its own negative test)
```
tmpdirGuard(p):
  rp <- fs.realpathSync(path.dirname(p)) + basename   # realpath the dir, keep file basename
  assert rp startsWith fs.realpathSync(os.tmpdir())
  assert rp !== LIVE_SETTINGS_PATH                    # /workspaces/unimatrix/.claude/settings.json
  # every script invocation in this suite routes --settings through tmpdirGuard FIRST.
```

## `before` / `after` lifecycle

```
before(suite):
  LIVE_SETTINGS_PATH <- repoRoot()/.claude/settings.json
  LIVE_SETTINGS_HASH_PRE <- sha256(read LIVE_SETTINGS_PATH)          # R-08-2 pre-hash
  REAL_DOGFOOD_DIR <- ~/.unimatrix/dogfood-client
  REAL_DOGFOOD_HASH_PRE <- exists? sha256(tree(REAL_DOGFOOD_DIR)) : null   # R-15-2
  WORKTREE_HC <- packages/unimatrix/lib/hook-client
  WORKTREE_HC_HASH_PRE <- sha256(tree(WORKTREE_HC))                  # R-13 clean-tree baseline
  clientDir <- installToTemp()      # ARCH-OQ-2; if install fails, SKIP suite with clear message (R-05-3)

after(suite):
  cleanup all scratch roots + temp client dirs + temp repo copies
  assert sha256(read LIVE_SETTINGS_PATH) === LIVE_SETTINGS_HASH_PRE  # R-08-2 zero live writes
  if REAL_DOGFOOD_HASH_PRE !== null:
     assert sha256(tree(REAL_DOGFOOD_DIR)) === REAL_DOGFOOD_HASH_PRE # R-15-2 real install untouched
  assert sha256(tree(WORKTREE_HC)) === WORKTREE_HC_HASH_PRE          # R-13 working tree clean
```

## Tests

### T1 — AC-02 switchover by effect (FR-5/6/8/10/11/13)
```
test "promote repoints to installed path + matcher delta + fail-open re-fire":
  { root, settingsPath } <- makeScratchRoot()
  tmpdirGuard(settingsPath)
  run dogfood-switchover.sh promote --settings settingsPath --client clientDir  (assert exit 0)
  s <- JSON.parse(read settingsPath)
  expectedEntry <- buildHookClientCommand(path.join(clientDir,"lib/hook-client/index.js"), <EVENT>)
  for each Unimatrix-owned hook command in s:
     assert command === "node "+clientDir+"/lib/hook-client/index.js <EVENT>"   # === expectedEntry form
  assert PreToolUse matcher in s === PRETOOLUSE_CYCLE_MATCHER     # imported constant, NOT literal (R-09)
  assert foreign hook still present                               # (R-10-3)
  assert no duplicate Unimatrix entries                          # (R-10-2)
  assert registered Unimatrix event count === 8                  # no settings.local.json opt-in (R-10-1)
  # RE-FIRE (OQ-C) — the non-vacuous core (R-01):
  installedIndexJs <- path.join(clientDir, "lib/hook-client/index.js")
  res <- reFire(installedIndexJs, "SessionStart", root, {hook_event_name:"SessionStart"})
  assert res.exitCode === 0 AND res.stdout === ""                # daemon-absent fail-open (R-07, SR-08)
```

### T1b — NEGATIVE CONTROL for the re-fire (MANDATORY, R-01-2)
```
test "re-fire assertion fails when the install is broken":
  brokenIndex <- path.join(os.tmpdir(), "no-such-dir", "index.js")
  assert reFire(brokenIndex, "SessionStart", root, {...}) THROWS / exitCode !== 0
  # proves the re-fire assertion is non-vacuous: it can detect a bad install.
```

### T1c — daemon-absent + malformed input (R-07)
```
test "fail-open on malformed/empty stdin":
  res <- reFire(installedIndexJs, "SessionStart", root, MALFORMED_OR_EMPTY)
  assert res.exitCode === 0 AND res.stdout === ""
  # and assert scratchHash has no socket (precondition already asserted in makeScratchRoot).
```

### T2 — rollback round-trip (FR-7, R-06)
```
test "promote then rollback restores Rust legacy form, idempotent, foreign preserved":
  { settingsPath } <- makeScratchRoot()
  promote(settingsPath, clientDir); rollback(settingsPath, clientDir)
  s <- read
  for each Unimatrix command:
     assert === "LD_LIBRARY_PATH="+repo+"/target/release "+repo+"/target/release/unimatrix hook <EVENT>"
  rollback(settingsPath, clientDir)        # twice
  assert s unchanged (idempotent)
  assert foreign hook preserved; isUnimatrixHook still owns reverted entries
```

### T3 — AC-03 copy-install isolation = code freeze (FR-12, NFR-3, R-04)
```
test "editing in-repo source does not change installed bytes/behavior; entrypoint not a symlink":
  installedIndexJs <- path.join(clientDir, "lib/hook-client/index.js")
  # structural anti-npm-link (C-6, R-04-2):
  assert fs.lstatSync(installedIndexJs).isSymbolicLink() === false
  hcHashBefore <- sha256(tree(clientDir + "/lib/hook-client"))
  preEdit <- reFire(installedIndexJs, "SessionStart", scratchRoot, {...})   # capture pre-edit behavior

  # OQ-D / ARCH-OQ-1: edit a THROWAWAY COPY, never the live tree (R-13):
  tmpRepo <- copy packages/unimatrix -> os.tmpdir()/repo-copy-<rand>
  append behavior-changing marker (process.stderr.write("LEAK")) to
        tmpRepo/lib/hook-client/<somefile>.js
  secondClient <- run dogfood-install.sh against tmpRepo with --target=<tmp2>   # re-pack the EDITED copy
  # assert the ORIGINAL install is invariant:
  assert sha256(tree(clientDir + "/lib/hook-client")) === hcHashBefore         # bytes unchanged (NFR-3)
  postEdit <- reFire(installedIndexJs, "SessionStart", scratchRoot, {...})
  assert postEdit.stdout === preEdit.stdout AND postEdit.exitCode === 0        # behavior unchanged
  # positive proof the marker is real (negative control for the freeze, R-04-1):
  leaked <- reFire(secondClient+"/lib/hook-client/index.js", "SessionStart", scratchRoot, {...})
  assert leaked carries the marker (stderr "LEAK")    # the edit DID change a freshly-packed copy
                                                       # -> the original's invariance is meaningful
  # explicit SR-07 framing (R-04-3): assert this test does NOT require separate {hash} state dirs.
  // documented inline: isolation = code freeze, shared ~/.unimatrix/{hash}/ by design (#4923).
```

### T4 — live-settings guard negative test (R-08-1)
```
test "tmpdirGuard rejects the live settings path":
  assert tmpdirGuard(LIVE_SETTINGS_PATH) THROWS
  # complements the before/after pre/post live-settings hash (zero live writes).
```

## `reFire` helper (the OQ-C shape, one place)
```
reFire(indexJs, event, cwd, payloadObj) -> { exitCode, stdout, stderr }:
  try:
    stdout <- execFileSync("node", [indexJs, event],
                  { cwd, input: JSON.stringify(payloadObj), encoding:"utf8", timeout:15000 })
    return { exitCode:0, stdout, stderr:"" }
  catch e:
    return { exitCode: e.status ?? 1, stdout: e.stdout ?? "", stderr: e.stderr ?? "" }
```

## Data flow

- IN: real `dogfood-install.sh` + `dogfood-switchover.sh`; scratch fixtures; a throwaway repo
  copy for T3; the live settings READ-ONLY (shape source only).
- OUT: pass/fail assertions; no persistent artifacts (all under `os.tmpdir()`, cleaned in `after`).
- Invariants proven by `after`: zero live-settings writes (NFR-4), real dogfood dir untouched
  (R-15), working tree clean (R-13).

## Error handling

| Condition | Behavior |
|-----------|----------|
| install absent / `before` install fails | SKIP suite with a clear message (R-05-3), not opaque crash |
| re-fire non-zero where exit-0 expected | assertion failure carrying status + stderr |
| any test throws mid-run | `after` still asserts live-settings/worktree clean (teardown, not inline) (R-13-2) |

## Key test scenarios → AC / risk map

| Test | Proves | Risks |
|------|--------|-------|
| T1 | AC-02 a–d, matcher delta, 8 events, fail-open re-fire | R-01, R-07, R-09, R-10, SR-04/05/08 |
| T1b | re-fire is non-vacuous (negative control) | R-01 (mandatory) |
| T1c | fail-open on malformed/empty stdin | R-07 |
| T2 | rollback restores exact Rust form, idempotent, foreign preserved | R-06 |
| T3 | AC-03 code-freeze + non-symlink + leak negative control | R-04 (mandatory), R-13, SR-07 |
| T4 + before/after hashes | zero live-settings mutation | R-08, SR-06 |
| before (scratchHash distinct + no socket) | scratch isolation / daemon-absent | R-03 |

## Gaps / flags

- None blocking. The harness imports `computeProjectHash`/`PRETOOLUSE_CYCLE_MATCHER` from the
  **installed** copy (clientDir) to make AC-03's "installed === shipped contract" self-consistent;
  importing the in-repo copy would also work since AC-03 proves they are byte-identical. Pinned to
  installed for honesty (the test asserts the *installed* matcher).
- T3's "re-pack the edited copy" path requires `dogfood-install.sh` to accept an arbitrary repo
  root. Component 1 resolves repo root via `git rev-parse`; running it with `cwd=tmpRepo` only
  works if tmpRepo is itself a git repo. **Resolution:** copy includes a minimal `.git/` dir in
  tmpRepo (or run pack directly on the copied `packages/unimatrix` via a `--pkg-dir` not in scope).
  Simplest honored path: `cp -r` the repo's `packages/unimatrix` into `tmpRepo/packages/unimatrix`
  and `git init` tmpRepo in the `before` of T3. Flagged for the implementer to pick the lighter of
  the two; both keep the live tree untouched.
