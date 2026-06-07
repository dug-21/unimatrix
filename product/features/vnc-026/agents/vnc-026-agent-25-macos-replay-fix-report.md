# Agent Report: vnc-026-agent-25-macos-replay-fix

**Task**: Fix macOS CI failure in `test_replay_precedes_carrying_event` (index.test.js:564) — 1 POST observed instead of 2.

## Root Cause

**Both layers, lib-side primary (oracle parity).** The diagnosis hypothesis was verified, not assumed:

1. **Mechanism (confirmed by Linux reproduction)**: `os.tmpdir()` on macOS is a symlink alias (`/var/folders/...` → `/private/var/folders/...`). The spawned child's `process.cwd()` is the kernel-physical path (getcwd resolves symlinks), so the child hashed `/private/var/.../tmpRoot` while the parent test helper `childStateDir()` hashed `path.resolve(tmpRoot)` = the logical `/var/...` string. Different hash → pre-seeded queue landed in a state dir the child never read → no replay POST.

2. **Oracle evidence (lib-side divergence)**: `crates/unimatrix-engine/src/project.rs::detect_project_root` canonicalizes (`.canonicalize()` = realpath) on **every** return path — lines 46 (override), 56 (`.git` found), 68 (fallback cwd). `config.js::walkToProjectRoot` used `path.resolve` only; the sole documented divergence covered worktree gitdir files, not symlinks. Parity governs → lib fix. The divergence also has a real runtime consequence: `stdin.cwd` (potentially a logical alias) vs `process.cwd()` (physical) would split the same project into two state dirs, losing queued frames — the production form of this exact CI failure.

## Fix

- **lib/hook-client/config.js**: `walkToProjectRoot` now canonicalizes its result via `realpathOrSelf` (fs.realpathSync with non-throwing fallback — JS fail-open contract vs Rust's io::Result propagation). Doc comments updated.
- **test/hook-client/index.test.js**:
  - `childStateDir(root?)` now derives through the real lib functions (`walkToProjectRoot` + `computeProjectHash`) instead of re-implementing with `path.resolve` — helper can no longer drift from child behavior.
  - New regression test `test_replay_through_symlinked_project_root` (skip win32) drives the full replay scenario through an explicit symlink to the project root — reproduces the macOS mechanism on any POSIX OS. Verified it **fails against the old lib** (1 fail) and passes with the fix.

## Latent-trap audit (other tests)

- Other spawn-level tests in index.test.js use `findQueueDir`/`findOffsetsDir`/`hookClientDirs`, which scan all hash dirs under HOME — immune.
- `test/hook-client/config.test.js` fixtures already `fs.realpathSync` their mkdtemp roots (compensating for the lib) — unaffected by the lib change (inputs already canonical); all pass.
- **Out-of-ownership flag**: `test/hook-client/benchmark-spawn.js:182` has the same trap (`computeProjectHash(path.resolve(root))` to locate the child's health.json). On macOS its `breadcrumb_written` field would report `false`. Benchmark artifact only, not a CI gate; not touched per file ownership. Recommend a follow-up one-liner: derive via `config.walkToProjectRoot(root)`.

## Verification

- `npm run test:hook-client`: **422 pass / 0 fail / 1 skip / 0 todo** (421 + 1 new regression test).
- Symlinked-tmpdir reproduction: `TMPDIR=/tmp/uni-link-tmp node --test test/hook-client/index.test.js` — failed pre-fix (1 !== 2 at line 580, identical to macOS CI), **41/41 pass post-fix**.
- Regression-test sanity: stashed lib fix → new test fails; restored → passes.
- `node test/check-hook-client-size.js`: **OK** (99,758 / 100,000 bytes — 242 bytes headroom; note for future lib edits).

## Files Modified

- `/workspaces/unimatrix/packages/unimatrix/lib/hook-client/config.js`
- `/workspaces/unimatrix/packages/unimatrix/test/hook-client/index.test.js`

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced #4774 (spawn+stub childStateDir pattern, the one this fix refines) and #4766 (hash after path.resolve — now shown insufficient without canonicalize). Briefing was run late (after investigation, before storing) — noted as process slip.
- Stored: entry #4784 "macOS tmpdir symlink vs child process.cwd(): canonicalize before hashing path-derived identity" via context_store (pattern, topic unimatrix-hook-client; Supports edges → #4774, #4766).
