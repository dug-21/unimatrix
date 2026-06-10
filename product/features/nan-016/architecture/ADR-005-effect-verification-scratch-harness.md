## ADR-005: Verify By Effect via Scratch Project Root + Re-Fired Hook; Never Touch Live Settings

### Context
nan-016 delivers the switchover but must NOT execute the live flip, and must NOT start the
F6 (#682) soak clock. Yet AC-02/AC-03 must be *real* proofs, not tautologies (SR-04). A test
that string-diffs the script is vacuous; a test that transiently flips and reverts this
repo's live `.claude/settings.json` could be read as "executed" and perturb a future soak
(SR-06). Isolation (AC-03) is frequently mis-modeled as state-dir separation when it is
actually code-freezing of the installed `lib/` bytes (SR-07).

### Decision
Verify by effect in `test/dogfood-effect.test.js` (`node --test`), entirely against
**scratch** fixtures and the **real installed path** — never this repo's live settings:

- **Scratch project root:** a tmp dir with a real `.git/` directory (so `walkToProjectRoot`
  hashes it to its own isolated `~/.unimatrix/{scratchHash}/`) and a scratch
  `.claude/settings.json` seeded with the current Rust-hook shape (`"*"` PreToolUse) plus a
  foreign hook.
- **AC-02:** run the real `promote`, parse the scratch settings, assert commands point at
  the installed `index.js`, the **PreToolUse matcher equals `PRETOOLUSE_CYCLE_MATCHER`**
  (the SR-05 delta), the foreign hook survives, no duplicates; then **re-fire the hook**
  (`node <installed>/lib/hook-client/index.js SessionStart`, cwd = scratch root) and assert
  exit 0 / empty stdout — proving the emitted command runs the installed client and
  fail-opens with no daemon (SR-08).
- **AC-03 (code-freeze, SR-07):** assert the installed `index.js` is a regular file, **not a
  symlink** (`lstatSync().isSymbolicLink() === false` — structural C-6 guarantee), capture
  its hash, perform an in-repo source edit **in a throwaway copy of the tree** (never the
  working tree), and assert the installed bytes/behavior are unchanged. Framed explicitly as
  byte/behavior freeze, NOT state separation; #4923 cited inline.
- **Guards (SR-06):** a tmpdir assertion on the `--settings` path; the live
  `.claude/settings.json` is read-only (only its shape is copied into the fixture).

### Consequences
Easier: AC-02/AC-03 become assertable, repeatable, non-vacuous; the soak boundary is
provably respected (no live write); CI can run the proof without a real flip.
Harder: the harness depends on a prior install (run `dogfood-install.sh` into a test-scoped
temp `--target` in a `before` hook); the AC-03 edit-in-throwaway-copy adds fixture machinery
to avoid working-tree mutation (OQ-1 in ARCHITECTURE.md).

Related: ADR-003 (the mechanism under test), ADR-004 (the fail-open assertion). Respects
C-6/C-7/C-8 and the soak-clock boundary. Cites #4923.
