# Agent Report: vnc-026-agent-26-worktree-parity-investigation

Read-only investigation of the F3-blocking worktree-parity concern against feature/vnc-026 HEAD (`c873850a`, incl. realpath fix `05ed76e5`). No code modified.

## Verdict

- **Claim (a) — per-worktree state dir: REGRESSION CONFIRMED.**
- **Claim (b) — total silent event loss in remote mode from worktrees: REGRESSION CONFIRMED** (consequence of (a); env vars are the only escape hatch).

Both were empirically reproduced against a real `git worktree add` scenario; exact diffs below.

## 1. The Oracle (Rust)

`crates/unimatrix-engine/src/project.rs`:

- `detect_project_root` walk path: `.git` **file** triggers `resolve_git_file` (project.rs:58-59).
- `resolve_git_file` (project.rs:76-114) parses the `gitdir:` line, resolves relative paths against the worktree dir (project.rs:89-93), canonicalizes, then walks UP from the gitdir target to the `.git` **directory** ancestor and returns its parent — the **main repo root** (project.rs:98-110). Fallback on unresolvable: worktree dir itself (project.rs:112-113). Manual walk + `fs`; no git2/gitoxide. Every return path canonicalizes.
- Override path (`Some(cwd)` — how the hook calls it): canonicalize then `resolve_worktree_root` (project.rs:44-48, 118-125) — chases the `.git` file if cwd itself contains one.
- Hook caller: `crates/unimatrix-server/src/uds/hook.rs:176` — `detect_project_root(Some(&cwd)).unwrap_or(cwd)`; the resulting hash selects the daemon socket (hook.rs:179-183). So today, **worktree agents' events land on the main repo's daemon/state**.
- Rust test coverage: project.rs:323-430 — `test_detect_root_worktree_git_file`, `test_worktree_same_hash_as_main_repo`, `test_worktree_relative_gitdir`, `test_worktree_ensure_data_dir_matches_main`.

## 2. The Client (JS)

`packages/unimatrix/lib/hook-client/config.js`:

- `walkToProjectRoot` (config.js:52-73): stops at the first dir containing `.git` (dir **or file** — config.js:61) and returns that dir via realpath. **Never reads the `.git` file; never chases gitdir.**
- The divergence is documented in-code, config.js:48-50:
  > "Divergence from project.rs: Rust resolves `.git` worktree FILES to the real gitdir; this walk stops at the containing directory. The hash is client-only (state-dir identity), so worktree users get a per-worktree state dir."
- The rationale is **incomplete**: the same `projectRoot` also anchors config-file resolution — `config.js:201` reads `{projectRoot}/.claude/settings.local.json`, and config.js:8-10 itself states the same string "feeds the config lookup and the state-dir hash." The divergence note reasons only about state-dir identity and ignores the config consumer.
- Miss path: `resolve` returns `{ok:false, reason:"missing"}` (config.js:206-207, 223); `index.js:325-332` then breadcrumbs + returns — exit 0, no stdout, **no network**. Total silent loss.

`lib/init.js:21-36` (`detectProjectRoot`, the claimed port source) also does not chase gitdir — so `unimatrix init --remote` run from a worktree would even write `settings.local.json` into the worktree root, fragmenting config too.

## 3. Design Provenance

- `product/features/vnc-026/pseudocode/config.md:80-83` — **deliberately accepted**, same incomplete rationale:
  > "Documented divergence: Rust resolves `.git` worktree FILES to the real gitdir; this walk stops at the directory containing `.git`. The hash is consumed only by THIS client (state-dir identity), so internal consistency is what matters. Worktree users get a per-worktree state dir — accepted."

  The premise "consumed only by THIS client" is true for the *hash* but the *root* feeds config lookup — the acceptance never evaluated claim (b).
- `architecture/ADR-006-config-resolution-precedence.md` and `specification/SPECIFICATION.md`: **zero mentions** of worktree/gitdir (grep across specification/ and architecture/ matches only pseudocode/config.md). ADR-006 (Unimatrix entry #4756) says "resolve cwd as the Rust hook does … detectProjectRoot port from init.js" — internally contradictory, since init.js does not match the Rust hook for worktrees. **Missed at ADR/spec level, then accepted at pseudocode level with a rationale that only covered half the consumers.**
- Carried into delivery knowingly: `agents/vnc-026-agent-5-config-report.md:12` — "Documented divergence (worktree gitdir not chased) carried from pseudocode."

## 4. Empirical Reproduction

Real repo: `git init main` + `.claude/settings.local.json` (url+token) in main root only + `git worktree add ../wt`.

| Input cwd | Rust `detect_project_root` | JS `walkToProjectRoot` |
|---|---|---|
| `/tmp/.../main` | `/tmp/.../main`, hash `d011178c62cf88ae` | `/tmp/.../main`, hash `d011178c62cf88ae` |
| `/tmp/.../wt` | `/tmp/.../main`, hash `d011178c62cf88ae` | **`/tmp/.../wt`, hash `6a3e310c9621e2b5`** |

Hash parity holds on the main root; the **roots themselves diverge for worktrees**.

Claim (b) verified: `config.resolve("/tmp/.../wt")` with env vars unset → `{ok:false, reason:"missing", root:"/tmp/.../wt"}` while `resolve(main)` → `{ok:true, source:"file"}`. Per index.js:325-332 the worktree spawn breadcrumbs and exits 0 — every event from every worktree agent is silently dropped unless `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN` are exported.

(Rust side run via a scratch crate path-depending on unimatrix-engine; JS via direct require of config.js at branch HEAD.)

## 5. Test Coverage on the Branch

One committed test touches worktrees — and it **locks in the regression**: `packages/unimatrix/test/hook-client/config.test.js:269-273` (`test_git_file_worktree_accepted`) asserts `walkToProjectRoot(root) === root` for a dir whose `.git` is a gitdir file. No test asserts main-root resolution, shared hash across worktrees, or config resolution from a worktree.

## Fix Locus (when delivery proceeds)

- **Primary**: `config.js:walkToProjectRoot` — when `path.join(current, ".git")` is a **file**, port `project.rs::resolve_git_file` (project.rs:76-114): read it, parse the `gitdir:` line, resolve relative against `current`, realpath, walk up to the `.git` directory ancestor, return its parent (realpathed). Non-throwing contract: any read/parse/realpath failure → fall back to current behavior (return `current`), mirroring project.rs:112-113. (Note one oracle nuance: Rust *errors* on a `.git` file with no `gitdir:` line, and hook.rs:176 `unwrap_or(cwd)` then falls back to raw cwd; JS fail-open should fall back to the containing dir — benign divergence, worth a comment.)
- **Secondary (recommended, same defect class)**: `init.js:detectProjectRoot` (init.js:21-36) — `init --remote` from a worktree currently writes config into the worktree root.
- **Doc sync**: delete/replace the divergence comments at config.js:48-50 and pseudocode/config.md:80-83.

### Blast radius
Small. `walkToProjectRoot` has exactly one production caller — `config.resolve` (config.js:185); everything downstream (hash → stateDir → settings path) inherits the corrected root, which is the point. The new branch executes **only** when `.git` is a file; normal-repo behavior is byte-identical. Cost: one extra file read + short ancestor walk, worktree-only, well inside the ~12 ms budget. No transport/state/envelope code touched.

### Invalidated tests/goldens
- `config.test.js:269-273` `test_git_file_worktree_accepted` must be **rewritten** (assert main-root resolution + a fallback case for an unparseable `.git` file).
- Hash-parity goldens (`test_hash_parity_with_rust`, config.test.js:275-286) are **unaffected** — they exercise the hash algorithm on fixed normalized strings; the fix changes the input path, not the hash.
- index.test.js helpers (lines 54-60, 598) use temp dirs without worktree `.git` files — unaffected.
- Parity corpus / replay fixtures: no worktree scenarios exist (grep), so nothing breaks; a worktree corpus row is the missing test for the Rust↔JS parity harness.

### Missing test
A worktree-parity test: real `gitdir:`-file layout (absolute and relative variants, per project.rs tests 323-390), asserting (1) `walkToProjectRoot(worktree) === realpath(mainRoot)`, (2) identical `computeProjectHash` for main and worktree, (3) `resolve(worktreeCwd)` finds `{mainRoot}/.claude/settings.local.json`.

## Confidence

**High.** Oracle behavior read from source and confirmed by its own unit tests; client behavior confirmed by source, its own in-code divergence note, and a live reproduction against a real `git worktree`; design provenance traced through pseudocode → ADR → agent reports.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-006 (#4756), which states root resolution must align config identity with state identity; this framed the (b) analysis.
- Stored: lesson via /uni-store-lesson on incomplete divergence-acceptance rationale (see session log).
