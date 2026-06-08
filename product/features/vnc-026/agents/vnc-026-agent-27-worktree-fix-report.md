# Agent Report: vnc-026-agent-27-worktree-fix

Human-directed FIX on feature/vnc-026 (PR #696, #679) for the two regressions confirmed by agent-26: (a) per-worktree project hash, (b) silent event loss in remote mode from worktrees (config anchored to the wrong root).

## What Changed

- **lib/hook-client/config.js** — ported `project.rs::resolve_git_file` as `resolveGitFile(gitFile, worktreeDir)` (exported): parse the `gitdir:` line, resolve relative targets against the containing dir, realpath, walk UP to the `.git` DIRECTORY ancestor, return its parent (realpath'd) — the main repo root. ANY failure (unreadable file, no gitdir line, dangling target, no `.git`-dir ancestor) → non-throwing fallback to realpath of the containing dir (project.rs:112-113). `walkToProjectRoot` now stats `.git`: DIRECTORY → root (realpath'd, preserving 05ed76e5), FILE → `resolveGitFile`. Oracle nuance kept as a comment: Rust *errors* on a missing gitdir line (hook.rs falls back to raw cwd); fail-open JS falls back to the containing dir — benign divergence. Stale divergence comment replaced with the parity statement.
- **lib/init.js** — `detectProjectRoot` applies the same port (requires `resolveGitFile` from config.js): `init --remote` from a worktree now writes settings/hooks into the MAIN root. Throwing no-`.git` contract unchanged; normal-repo behavior byte-identical.
- **test/hook-client/config.test.js** — REWROTE `test_git_file_worktree_accepted` (it asserted the regression). New `makeWorktree` fixture (mirrors project.rs unit-test layouts) + 8 tests: absolute gitdir → main root; relative gitdir; shared hash main↔worktree; worktree subdirectory cwd; **claim (b) end-to-end** (settings.local.json only in main root, `resolve(worktreeCwd)` finds it, `source:"file"`, `projectRoot === main`); fallbacks for no-gitdir-line, dangling gitdir, no-`.git`-dir-ancestor. Mirrors all four oracle scenarios (project.rs:323-430).
- **test/init-remote.test.js** — `detectProjectRoot` worktree parity: worktree `.git` file → main root; malformed `.git` file → containing-dir fallback.
- **pseudocode/config.md** — accepted-divergence paragraph replaced, marked "(corrected post-delivery: ported resolve_git_file — see agent-26/27 reports)".

## Size Gate (C-04 / AC-12)

`node test/check-hook-client-size.js` → **OK, 99997 / 100000 bytes** (3 B headroom). The port cost ~1.4 KB; compensated by behavior-preserving comment-prose trims within config.js only (per constraint). lib/init.js trimmed to exactly 500 lines (comment prose only) after the port pushed it to 507.

> NOTE for future hook-client agents: the payload budget is effectively EXHAUSTED (3 bytes). Any lib/hook-client addition requires trimming elsewhere first.

## Test Results

- `npm run test:hook-client` → **430 tests, 429 pass, 0 fail, 1 skipped** (win32-only) — includes the 8 new worktree tests.
- `node --test test/init-remote.test.js` → **37/37 pass** (2 new).
- Full package `npm test` → 564 tests, 562 pass, **1 fail = pre-existing** `test_creates_mcp_json_on_clean_project` (LD_LIBRARY_PATH env assertion; verified failing on branch HEAD via `git stash`; unrelated to this fix).
- Rust oracle untouched and green: `cargo test -p unimatrix-engine --lib -- project` → 19/19 incl. all four worktree tests.

## Empirical Parity Table (re-run, real `git worktree add`)

| Input cwd | JS projectRoot | hash | config | init detectProjectRoot |
|---|---|---|---|---|
| `/tmp/wtcheck/main` | `/tmp/wtcheck/main` | `9942a159dada268d` | found (file) | `/tmp/wtcheck/main` |
| `/tmp/wtcheck/wt` | **`/tmp/wtcheck/main`** | **`9942a159dada268d`** | **found (file)** | **`/tmp/wtcheck/main`** |

Rust column: `sha256("/tmp/wtcheck/main")[..16] = 9942a159dada268d` (independent computation) and the oracle's own worktree unit tests passing — matches the agent-26 investigation's Rust behavior. Both regressions resolved: shared hash, config found from worktree cwd. (A standalone Rust scratch-crate run was attempted but unimatrix-engine cannot link outside the workspace — missing ORT dylib; oracle verified via its unit tests instead, as in the four project.rs scenarios.)

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced #588 (.git dir-vs-file detection lesson), #4785 (the driving lesson: enumerate ALL consumers of a diverged value), #4784 (macOS tmpdir symlink aliasing — applied: all test fixtures realpath'd).
- Stored: entry #4786 "hook-client 100 KB payload budget is exhausted — trim comment prose before adding any lib/hook-client code" via /uni-store-pattern (topic `unimatrix-package`).
