## ADR-001: Copy-Install via `npm pack` + Extract (Clean-Replace), Never `npm link`

### Context
The dogfood re-release must freeze a runnable copy of the in-repo TS hook client at an
external path (C-6: copy install, never `npm link` — a symlink to the working tree
reintroduces the source-mutation leak isolation exists to prevent). Two copy-install
mechanisms were candidates: `npm pack` + extract, and `npm install --prefix`. They produce
different on-disk trees and side-effects (SR-01), and a stale prior install can silently
shadow a new build (SR-02). The client run from this repo's cwd shares
`~/.unimatrix/{hash}/` state with the running Rust daemon by design (#4923); the install
mechanism must not try to "fix" that.

### Decision
Use **`npm pack` + extract with clean-replace** in `scripts/dogfood-install.sh`:

1. `npm pack` in `packages/unimatrix` → tarball honoring the `files` array
   (`bin/ lib/ skills/ postinstall.js protocols/`). Verified: the tarball **excludes the
   platform binary** (it is an `optionalDependency`, downloaded by postinstall, not bundled).
   The hook client needs only `node` + `lib/hook-client/**`; it never execs the binary.
2. **Clean-replace** `~/.unimatrix/dogfood-client/`: extract `package/` to a sibling temp
   dir, then atomic `mv` over the target (removing any prior install first). Replace, never
   overlay (SR-02).
3. **postinstall is copied but never executed** — extraction is a file copy, so no host
   side-effect runs (SR-01). `postinstall.js` only downloads the ONNX model for the binary,
   which the client does not use.
4. After extract, assert the full `lib/hook-client/*.js` set + `lib/merge-settings.js` +
   `lib/hook-client/config.js` exist and that `node <target>/lib/hook-client/index.js
   SessionStart </dev/null` exits 0. Missing/broken → loud non-zero (tooling may be loud;
   hooks may not).

Rejected: `npm install --prefix` (runs lifecycle scripts / resolves optionalDependencies
into the tree, pulling the binary and a postinstall model download — heavier, slower,
host-touching, and not what the client needs). Rejected: `npm link` (forbidden by C-6).

### Consequences
Easier: deterministic, minimal frozen tree (exactly the published `files` set, no binary);
no postinstall side-effects; clean-replace makes re-runs byte-stable (C-9 dependency-free
build), so re-running is a reliable F6 soak-reset point (AC-01). Completeness is asserted,
not assumed.
Harder: the script must stage extraction and verify assets rather than trusting `npm` to
place a working tree; `npm pack` version drift in the tarball name must be globbed, not
hardcoded.

Related: ADR-002 (install location), ADR-003 (switchover consumes the installed tree's
`mergeSettings`). Cites #4923 (shared-state-by-hash).
