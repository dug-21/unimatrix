# Test Plan — `scripts/dogfood-install.sh`

> Build + copy-install (`npm pack` + extract, clean-replace) of `packages/unimatrix` to a
> fixed external path. The F6 soak-reset point. Primary risks: **R-02 (Critical)** non-atomic /
> overlay replace; **R-11** npm pack drift / postinstall side-effect; R-12 container durability.
> Covers AC-01. All tests run via the `node --test` harness driving the real script with
> `--target <test-scoped temp dir>` (never the real `~/.unimatrix/dogfood-client/`).

## Test Conventions for this Component

- Install ALWAYS to a test-scoped temp `--target` under `os.tmpdir()` (R-15). Never the fixed
  real dir.
- Drive the real script via `execFileSync("bash",[installScript,"--target",tmpTarget],…)`.
- Assert on filesystem effects and exit codes — never grep the `.sh` source.

## Unit / Behavior Test Expectations

### R-11 — Frozen tree completeness, binary absent, postinstall inert (High)

- `install_fresh-target_extracts-full-files-array`
  - Arrange: empty temp `--target`.
  - Act: run install, exit 0.
  - Assert: every `files`-array dir is present — `bin/`, `lib/` (incl. `lib/hook-client/`),
    `skills/`, `postinstall.js`, `protocols/`. Assert `lib/hook-client/index.js`,
    `lib/merge-settings.js`, `lib/hook-client/config.js`, and the full `lib/hook-client/*.js`
    set exist.
- `install_fresh-target_platform-binary-absent`
  - Assert: NO platform binary anywhere under `--target` (it is an `optionalDependency`,
    excluded from the `npm pack` tarball). Confirms client-only freeze.
- `install_fresh-target_postinstall-inert`
  - Assert: `postinstall.js` is present-but-inert — no ONNX model file appears under `--target`
    or `$HOME`; no host mutation marker. (Extraction is a file copy; postinstall never runs.)
- `install_smoke_entrypoint-runs-exit-0`
  - Act: `execFileSync("node",[join(target,"lib/hook-client/index.js"),"SessionStart"],
    {input:""})` (daemon-absent).
  - Assert: exit 0 (the script's own completeness smoke check; ties to FR-2 / SR-01).

### R-02 — Clean-replace is atomic and overlay-free (CRITICAL)

This risk REQUIRES a mutate-then-reinstall byte-compare and evidence of staged-temp+atomic-`mv`.

- `reinstall_mutated-and-stray-file_byte-identical-to-fresh-and-stray-gone`  ← **R-02 core**
  - Arrange: install fresh → record per-file content hashes of the tree. Install again into a
    second temp target → this is the "fresh reference" tree.
  - Act: into the first target, MUTATE an installed file (append bytes) AND add a stray file
    (`STRAY.txt` and a stray dir) under the install dir; then re-run install onto that target.
  - Assert: post-reinstall tree is **byte-identical** (per-file hash set) to the fresh reference
    tree (NFR-1); the mutated file is restored to original bytes; the stray file/dir are GONE
    (an overlay would keep them). This single test proves clean-replace, not overlay.
- `reinstall_idempotent_second-run-byte-stable`
  - Assert: two consecutive clean installs from the same source produce identical hash sets
    (C-9 dependency-free ⇒ byte-stable; SR-03).
- `install_staged-then-rename_no-partial-at-target` ← staged-temp+atomic-mv evidence
  - Assert (effect-level, since we cannot interrupt mid-run deterministically): the install
    stages to a *sibling* temp path and `mv`s over the target — verified by asserting the
    target's parent never contains an in-progress/partial extract dir after a successful run,
    and that a pre-populated target is fully replaced (below). Implementation note for 3b/3c:
    if the script exposes the staging dir path (e.g. via env or a `--keep-staging` debug flag)
    the test SHOULD assert the staging dir is a sibling of `--target` and is removed on success.
- `install_pre-populated-target_clean-replaces-exit-0`
  - Arrange: pre-create `--target` with unrelated junk files.
  - Assert: install exits 0; junk is gone; tree == fresh. Defined behavior on a populated
    target (R-02 edge case).

### R-12 — Container-rebuild durability (Medium)

- `install_default-target_is-fixed-dir-not-npm-global`
  - Assert: with no `--target`, the resolved target is `~/.unimatrix/dogfood-client/` (a fixed,
    node-version-independent absolute path), NOT an npm global / `node/vX.Y.Z` prefix.
    (Test exercises the resolution logic without writing the real dir — e.g. via `--dry-run`
    or `--print-target`, or asserts the default constant; do not actually install to the real
    dir in CI.)

### AC-01 (a) — Copy-install, not symlink (anti-`npm link`, C-6)

- `install_root_is-not-symlink-into-working-tree`
  - Assert: `fs.lstatSync(join(target,"lib/hook-client/index.js")).isSymbolicLink() === false`;
    the installed tree is a real file copy, not a symlink back into `packages/unimatrix`.
    (Also asserted from the harness side in dogfood-effect; duplicated here as the install-level
    structural guard.)

## Security / Safety Assertions (clean-replace `rm -rf` guard)

- `install_empty-or-unsafe-target_refuses-rm`
  - Arrange: invoke with `--target ""` and with `--target /` (or `$HOME`).
  - Assert: the script REFUSES (loud non-zero, no removal) — target must be validated non-empty
    and resolve under `~/.unimatrix/` or be an explicit absolute path before any `rm -rf`.
    Never degrades to `rm -rf` of a parent or `$HOME`. (Security Risk: clean-replace removal.)

## Edge Cases (from Risk Strategy)

- Pre-existing populated target (overlay vs clean-replace) — `install_pre-populated-target_*`.
- Target path containing whitespace — install succeeds and the tree is intact (path-quoting).
- Interrupted/partial extract — staged-mv guarantee (asserted at effect level above).

## Coverage Requirement (must all hold)

- R-02: re-install removes ALL prior bytes (stray-file-gone) AND restores mutated bytes AND is
  byte-identical to fresh (mutate-then-reinstall compare). Staged+atomic-mv evidenced.
- R-11: full `files`-array set present, binary absent, postinstall inert, mechanism is `npm pack`.
- R-12: default target is the fixed dir, independent of node version.
- AC-01: copy not symlink; complete tree; idempotent clean-replace.
