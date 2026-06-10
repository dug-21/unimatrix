# Test Plan — `packages/unimatrix/test/dogfood-effect.test.js`

> The `node --test` effect-verification harness. This file **IS** the AC-02/AC-03 harness — it
> drives the real install + switchover scripts against scratch fixtures and the real installed
> path, then asserts on resulting settings state and a **re-fired** hook's runtime behavior.
> It is the non-vacuous core of the feature (SR-04). Primary risks: **R-01, R-03, R-04
> (Critical)**, plus R-07, R-08, R-09, R-10, R-13, R-15. Covers AC-02, AC-03, and the rollback
> half of AC-04.
>
> **Non-negotiable:** this harness MUST re-fire a real hook via `execFileSync` and MUST include
> negative controls for R-01 and R-04. A green harness with only settings-file string
> assertions and no re-fire / no negative control is a FAILED gate.

## Fixtures & Setup

- **Scratch project root** (per test, under `os.tmpdir()`, `realpath`-resolved): a temp dir
  with a real `.git/` **directory** (so `walkToProjectRoot` treats it as a real root and hashes
  it to its own `~/.unimatrix/{scratchHash}/`) and a scratch `.claude/settings.json` seeded with
  the **live-shaped** Rust-hook shape — INCLUDING the real legacy `"*"` PreToolUse Rust uni hook
  (the shape this repo's actual `.claude/settings.json` carries) — plus a **foreign hook** to
  prove preservation. This live-shaped seed is the load-bearing fixture for the prune: an
  unpruned promote would leave the stale `"*"` Rust uni hook alongside the new
  `PRETOOLUSE_CYCLE_MATCHER` group (see #4930), which the CLEAN post-state now forbids.
- **Rework note (clean-switch):** the switchover is extended with a stale-uni-hook PRUNE so the
  soak is CLEAN. The prior "8-of-9 / documented stale-`"*"` delta" reality (#4930) is REPLACED:
  AC-02 now asserts the CLEAN post-state — every uni-owned hook (per the shipped `isUnimatrixHook`)
  points at the installed entrypoint and the count of stale `"*"` Rust uni hooks == 0. Foreign
  hooks are still preserved untouched.
- **Installed client**: `before`-hook runs `dogfood-install.sh --target <test-scoped temp dir>`
  (R-15 — NEVER the real `~/.unimatrix/dogfood-client/`). If install cannot be staged, the
  harness **skips with a clear message** (R-05) — never hard-crashes.
- **Imports from the INSTALLED copy**: `PRETOOLUSE_CYCLE_MATCHER`, `HOOK_EVENTS`,
  `buildHookClientCommand`, `computeProjectHash` — so assertions track shipped semantics, not
  literals (R-09 / R-03).
- **Teardown**: `afterEach`/`after` cleans all temp dirs AND restores any in-repo edit; the
  working tree must be clean even on assertion failure (R-13).

## R-01 — Non-vacuous effect verification (CRITICAL — negative control MANDATORY)

- `refire_installed-entrypoint_exit-0-empty-stdout`  ← R-01 core (positive)
  - Act: `execFileSync("node",[installedIndexJs,"SessionStart"],
    {cwd:scratchRoot, input:JSON.stringify(payload)})`.
  - Assert: exit 0 AND empty stdout — a REAL invocation of the installed entrypoint, not a parse
    of settings.
- `promote_live-shaped-seed_every-uni-hook-points-at-installed-entrypoint-clean`  ← AC-02 CLEAN post-state
  - Arrange: live-shaped seed (real `"*"` PreToolUse Rust uni hook + foreign hook).
  - Act: run the real `promote`.
  - Assert: (a) EVERY uni-owned command (those for which the shipped `isUnimatrixHook` returns
    true, enumerated over ALL hook groups/events, NOT scoped to one matcher group) ==
    `node <installed>/lib/hook-client/index.js <EVENT>`, and `<installed>` is the real
    test-scoped install dir (not a placeholder/literal); (b) the count of stale `"*"` Rust uni
    hooks == 0 — the pre-existing legacy `"*"` PreToolUse Rust uni hook has been PRUNED, not left
    alongside the new group (inverts #4930's "stale `"*"` survives" reality); (c) the PreToolUse
    cycle-matcher group's matcher === the imported `PRETOOLUSE_CYCLE_MATCHER`; (d) the foreign
    hook is preserved unchanged with no duplicates; (e) the registered uni event count matches
    the actual opt-in state (8 without `SubagentStop` opt-in, 9 with).
- `refire_negative-control_broken-install-path-FAILS-the-assertion`  ← **R-01 negative control (MANDATORY)**
  - Arrange: point the re-fire at a deliberately broken path (non-existent
    `index.js` / a corrupted copy that throws non-zero).
  - Assert: the re-fire assertion FAILS (non-zero / unexpected output) — proving the
    exit-0-empty-stdout assertion is non-vacuous (it can detect a bad install). Encoded as a
    test that EXPECTS the broken invocation to be detected (e.g. `assert.notStrictEqual(rc,0)`
    on the broken path), so a regression to a vacuous check would surface here.

**Coverage Requirement (R-01):** at least one real re-fire of the installed entrypoint asserting
runtime behavior, AND a negative control that fails when the install is broken. A settings-file
string assertion alone is insufficient.

## R-03 — Scratch-hash distinct from live; daemon-absent genuinely exercised (CRITICAL)

- `scratch-hash_distinct-from-live-repo-hash`  ← R-03 core
  - Act: compute the scratch root's project-root hash via the shipped `computeProjectHash`.
  - Assert: it DIFFERS from this repo's live hash (`computeProjectHash(realpath(repoRoot))`),
    so the daemon-absent fail-open path is genuinely exercised and the harness cannot perturb
    live runtime state.
- `scratch-hash_no-socket-before-refire`
  - Assert: no `~/.unimatrix/{scratchHash}/unimatrix.sock` exists before the re-fire (the
    daemon-absent precondition) AND `scratchHash` is not the live hash.
- `scratch-hash_computed-over-realpath_tmpdir-symlink-safe`  ← #4796 guard
  - Assert: the hash is computed over the `realpath`-resolved scratch root, mirroring
    `config.js`, so a symlinked `os.tmpdir()` cannot collapse the scratch root onto the live
    repo's realpath/hash. (Directly informed by #4796 macOS tmpdir-symlink state-dir split.)

**Coverage Requirement (R-03):** scratch hash asserted DISTINCT from live; no scratch socket
before re-fire; realpath handling matches `config.js`.

## R-04 — Isolation = code-freeze, proven with a behavior-changing edit (CRITICAL — negative control)

- `isolation_in-repo-edit-in-throwaway-copy_installed-bytes-and-behavior-unchanged`  ← R-04 core
  - Arrange: capture content hash of installed `lib/hook-client/index.js` after install.
  - Act: make a **behavior-changing** edit (e.g. inject a `process.stderr.write("LEAK-MARKER")`)
    to the in-repo `lib/hook-client/` source in a **throwaway copy of the tree** (or
    git-stash-and-restore) — NEVER the live working tree (R-13).
  - Assert: (a) installed-path content hash UNCHANGED; (b) re-fired installed-path behavior
    unchanged (the `LEAK-MARKER` is ABSENT from stderr). This is the behavior-changing proof,
    not a no-op "bytes unchanged" check.
- `isolation_installed-entrypoint_is-not-symlink`
  - Assert: `fs.lstatSync(installedIndexJs).isSymbolicLink() === false` — the structural
    anti-`npm link` (C-6) guarantee.
- `isolation_negative-control_symlink-or-overlay-leak-WOULD-be-detected`  ← **R-04 negative control (MANDATORY)**
  - Arrange: construct a deliberately-leaky install in a temp dir — either a symlink from the
    install path back into the working tree, OR an overlay where the edited source bytes reach
    the installed path.
  - Assert: the isolation assertions (non-symlink AND/OR marker-absent) DETECT the leak (the
    assertions FAIL on the leaky fixture) — proving the isolation proof is non-vacuous and could
    catch a future `npm link` regression.
- `isolation_explicitly-not-state-dir-separation`
  - Assert (documented negative assertion): the test does NOT require separate `{hash}` state
    dirs — isolation is byte/behavior freeze of the installed `lib/`, NOT state separation
    (#4923, SR-07). Encoded as a comment-backed assertion / note that shared `{hash}` state is
    expected and acceptable.

**Coverage Requirement (R-04):** isolation proven via behavior-changing in-repo edit + installed
byte/behavior invariance + non-symlink assertion + a negative control that detects a leak. A
no-op bytes-unchanged check alone is insufficient.

## R-07 — Daemon-absent re-fire exits 0 (C-7) (High)

- `refire_socketless-scratch-hash_exit-0` (also satisfies R-01 positive) — exit 0 / empty stdout.
- `refire_malformed-empty-stdin_exit-0`
  - Assert: re-fire with a malformed/empty stdin payload exits 0 (fail-open on bad input, not
    just absent daemon).
- `refire_emitted-command-equals-buildHookClientCommand-form`
  - Assert: the command form re-fired is exactly what `buildHookClientCommand` produces (bare
    path, no stray whitespace) — so the shipped fail-open path is the one actually invoked.

## R-08 — Zero live-settings mutation; tmpdir guard (High)

- `live-settings_pre-post-suite-hash-unchanged`  ← R-08 core
  - Arrange: capture sha256 of `/workspaces/unimatrix/.claude/settings.json` in a top-level
    `before` (suite start).
  - Assert (in a top-level `after`): byte-identical hash after the full suite (NFR-4 — zero live
    writes), regardless of pass/fail of individual tests.
- `live-settings_tmpdir-guard-rejects-live-path`
  - Assert: the `--settings` guard rejects a path equal to the live settings path AND asserts
    the path passed to the script is under `os.tmpdir()`. Negative test: a non-tmpdir / live
    path is rejected.
- `live-settings_read-only-shape-copy`
  - Assert: the harness opens live settings ONLY for reading (to copy its shape into the scratch
    fixture); it never opens it for write.

## R-09 / R-10 — Matcher delta + event-set + foreign/duplicates (Medium)

(Driven through the real `promote` here as the executor; assertions mirror dogfood-switchover.)

- `promote_PreToolUse-equals-imported-PRETOOLUSE_CYCLE_MATCHER` — asserted against the IMPORTED
  constant, not a literal (R-09).
- `promote_event-count-matches-actual-optin-state` — 8 without `SubagentStop` opt-in; 9 with
  opt-in (R-10), asserted against the actual scratch opt-in state.
- `promote_foreign-hook-survives_no-duplicates` — foreign preserved; uni commands updated/pruned
  to a single installed-entrypoint group per event, no duplicate Unimatrix entries (R-10).

## Stale-uni-hook PRUNE — CLEAN post-state (rework: clean-switch; R-09 context)

> The switchover is extended with a prune so the dogfood soak runs entirely on the installed
> entrypoint. `mergeSettings` alone keys every op on `EVENT_MATCHERS[event]` and never touches a
> stale `"*"` PreToolUse Rust uni hook (#4930); the prune removes any uni-owned hook group whose
> command form does NOT match the post-promote target form, leaving foreign groups untouched.
> These are MANDATORY non-vacuous assertions — the prune must be PROVEN real, not assumed.

- `promote_prunes-stale-star-rust-uni-hook_count-zero`  ← prune core (positive)
  - Arrange: live-shaped seed with the real `"*"` PreToolUse Rust uni hook + foreign hook.
  - Act: real `promote`.
  - Assert: after promote, enumerating ALL hook groups, the number of uni-owned hooks (per shipped
    `isUnimatrixHook`) whose command is the stale Rust `"*"` form == 0; the only surviving uni
    PreToolUse group is under `PRETOOLUSE_CYCLE_MATCHER` with the installed-entrypoint command; the
    foreign hook is byte-unchanged.
- `promote_prune-NEGATIVE-CONTROL_removing-prune-leaves-stale-hook-FAILS`  ← **prune negative control (MANDATORY)**
  - Purpose: prove the prune assertion is non-vacuous — that it FAILS if the prune is removed
    (i.e. the prune is real, not a tautology against a seed that never had a stale hook).
  - Arrange: the SAME live-shaped seed carrying the stale `"*"` Rust uni hook, but exercise the
    no-prune path — either by invoking the switchover with the prune disabled (a test-only
    `--no-prune` / env shim if the implementation exposes one) OR, if no such shim exists, by
    constructing the post-state mergeSettings ALONE would produce (call the installed
    `mergeSettings` directly on the seed, with NO prune step) and feeding it to the SAME prune
    assertion helper used by the positive test.
  - Assert: against that no-prune post-state, the prune assertion (`count of stale "*" Rust uni
    hooks == 0`) FAILS (the stale `"*"` Rust uni hook is still present). This proves the positive
    `promote_prunes-stale-star-rust-uni-hook_count-zero` assertion can actually detect an unpruned
    leftover and is not vacuously green. The positive and negative controls MUST share the same
    assertion helper so a regression to a no-op check surfaces here.

## R-13 — Working tree provably clean after isolation test (Medium)

- `isolation_teardown_working-tree-clean-even-on-failure`
  - Assert: after the isolation test (including on an assertion-failure path), there is zero diff
    in `packages/unimatrix/lib/hook-client/`. The in-repo edit is performed in a throwaway copy
    (preferred) or restored in a `finally`/`after` hook — restoration in teardown, never inline.

## R-15 — Before-hook install only into test-scoped temp dir (Medium)

- `setup_install-target-under-tmpdir`
  - Assert: the harness `before`-hook `--target` resolves under `os.tmpdir()` (or a clearly
    test-scoped path), never the real `~/.unimatrix/dogfood-client/`.
- `setup_real-dogfood-install-untouched`
  - Assert: if `~/.unimatrix/dogfood-client/` pre-exists, its content hash is unchanged across
    the suite (the suite never disturbs a human-staged install / soak).

## Rollback (AC-04 effect half — R-06)

- `rollback_live-shaped-seed_promote-then-rollback_restores-exact-rust-form-clean`  ← R-06 + clean-switch
  - Arrange: live-shaped seed (real `"*"` Rust uni hook + foreign hook); run `promote` then
    `rollback`.
  - Assert: (a) EVERY uni-owned hook (per shipped `isUnimatrixHook`, enumerated over ALL groups)
    is exactly `LD_LIBRARY_PATH=<repo>/target/release <repo>/target/release/unimatrix hook <EVENT>`
    over the correct event set; (b) NO stale node-client uni hook survives — the count of
    uni-owned hooks still in the `node <installed>/lib/hook-client/index.js <EVENT>` form == 0
    (the promote-side node-client group is PRUNED on rollback, mirror of the promote-side prune of
    the Rust group); (c) the foreign hook is preserved unchanged, no duplicates; (d) emitted by
    the shipped `normalizeCommandSource` legacy arm, NOT a nan-016 bespoke revert string.
- `rollback_idempotent_twice-equals-once` — running rollback twice yields byte-identical settings
  (idempotent re-point + idempotent prune; a stale node-client group is gone after the first
  rollback and stays gone).
- `rollback_prune-NEGATIVE-CONTROL_removing-prune-leaves-stale-node-hook-FAILS` (optional but
  recommended) — the rollback-side analogue: against the no-prune post-rollback state, the "no
  stale node-client uni hook survives" assertion FAILS, proving the rollback prune is real.

## Coverage Requirement (Critical risks — all mandatory)

- R-01: real `execFileSync` re-fire + negative control that fails on broken install. (The R-01
  re-fire + its broken-install negative control are UNCHANGED by the clean-switch rework — the
  vacuous-test guard still holds.)
- R-03: scratch hash distinct from live; no scratch socket; realpath-mirrors-config.js.
- R-04: behavior-changing edit + byte/behavior invariance + non-symlink + leak-detecting
  negative control.
- Plus R-07 (exit-0 daemon-absent + malformed stdin), R-08 (pre/post live hash + tmpdir guard),
  R-13 (clean tree on failure paths), R-15 (tmpdir-only install).
- **Clean-switch prune (rework):** AC-02 asserts the CLEAN post-promote state — every uni-owned
  hook points at the installed entrypoint, stale `"*"` Rust uni hook count == 0, foreign
  preserved; PLUS a prune negative control that FAILS if the prune is removed. Rollback asserts
  the CLEAN post-rollback state — every uni hook is the exact Rust form, no stale node-client uni
  hook survives, foreign preserved, idempotent.
