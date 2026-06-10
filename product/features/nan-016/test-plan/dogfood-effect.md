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
  the current Rust-hook shape (`"*"` PreToolUse) plus a **foreign hook** to prove preservation.
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
- `promote_scratch_commands-point-at-real-installed-entrypoint`
  - Assert: every Unimatrix command in the parsed scratch settings ==
    `node <installed>/lib/hook-client/index.js <EVENT>`, and `<installed>` is the real
    test-scoped install dir (not a placeholder/literal).
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
  opt-in (R-10).
- `promote_foreign-hook-survives_no-duplicates` — foreign preserved; Rust commands updated in
  place, no duplicate Unimatrix entries (R-10).

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

- `rollback_promote-then-rollback_restores-exact-rust-form` — see dogfood-switchover.md R-06;
  executed here on the scratch settings. Assert exact Rust command over correct events,
  idempotent, foreign preserved, shipped legacy arm.

## Coverage Requirement (Critical risks — all mandatory)

- R-01: real `execFileSync` re-fire + negative control that fails on broken install.
- R-03: scratch hash distinct from live; no scratch socket; realpath-mirrors-config.js.
- R-04: behavior-changing edit + byte/behavior invariance + non-symlink + leak-detecting
  negative control.
- Plus R-07 (exit-0 daemon-absent + malformed stdin), R-08 (pre/post live hash + tmpdir guard),
  R-13 (clean tree on failure paths), R-15 (tmpdir-only install).
