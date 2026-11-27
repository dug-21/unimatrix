# Security Review: 1036-security-reviewer

## Risk Level: low

## Summary
The hardened writer (`lib/trust.js` core, `lib/safe-write.js` API) is an improvement over the deleted `atomic-write.js`. It uses random `wx` temp files, renames onto the resolved target, re-checks the hash and containment before rename, fails loud on dangling links, re-tightens credential and trust modes, and refuses a symlinked trust.json. I probed project-file containment with five symlink escapes and every one was refused with nothing written outside the project. No blocking findings. Two non-blocking findings concern git invocation; the first predates this PR.

## Findings

### F1: On Windows, bare `git` resolves from the project directory before PATH
- **Severity**: medium (Windows only; predates this PR)
- **Location**: `packages/unimatrix/lib/git-exclude.js:19` (`execFileSync("git", args, { cwd: root, ... })`)
- **Description**: On Windows, libuv's `search_path` checks the child's `cwd` before PATH when given a bare executable name. Running `npx unimatrix init` in an untrusted repo that ships `git.exe` or `git.com` at its root would therefore run that file. main has the same exposure through `wire.js gitIgnored`, so this PR does not introduce it, but it now makes 5–6 git calls through that path. Every other Windows spawn in the package already uses an absolute `%SystemRoot%\System32\*.exe` path (`trust.js:305/314`, `install-store.js:87`). git is the only exception.
- **Recommendation**: Follow-up issue. Resolve git to an absolute path from PATH entries only, never cwd; `codex-install.js onPath` is a starting point. Pass that path to `execFileSync`, or skip with `skipped-no-git` if git is not found.
- **Blocking**: no

### F2: The exclude-file write is not contained and follows symlinks
- **Severity**: low
- **Location**: `packages/unimatrix/lib/git-exclude.js:75-82`
- **Description**: The path comes from `git rev-parse --git-path info/exclude` and is written without `containWithin`. Verified: with `.git/info/exclude` symlinked to an outside file, init appended `/.claude/settings.local.json` to that file. Exploiting this needs an attacker-supplied `.git` directory (an extracted archive or shared volume); `git clone` cannot produce one. The appended content is a fixed, benign line. An attacker-controlled `.git/config` could also set `core.fsmonitor`, which some git subcommands execute. This is pre-existing as well: main already ran `check-ignore`.
- **Recommendation**: Optionally pass `containWithin: realpath(git rev-parse --git-common-dir)` for the exclude write. Optionally add `-c core.fsmonitor=false` to the git argv.
- **Blocking**: no

### F3: The package-tmp prune now ignores which process owns a tmp dir
- **Severity**: low (availability)
- **Location**: `packages/unimatrix/lib/install-store.js:199-204`, `TMP_AGE_MS = 60000`
- **Description**: Prune deletes any `.tmp-` dir older than 60 s. Before, it exempted the current pid's dir. If two inits run concurrently and one copy takes longer than 60 s, the other init can delete its staging dir mid-copy. That init then fails loudly (exit 1), with no corruption: `.complete` is written last, and install.json is swapped after the rename.
- **Recommendation**: Consider a longer age, such as 10 min. Accepting it as is also works.
- **Blocking**: no

### F4: Windows init reports a failed trust.json write as `updated`
- **Severity**: low (reporting only)
- **Location**: `packages/unimatrix/lib/install-store.js` (`a = trust.recordTrustIdentity(set, o.home) || "updated"`)
- **Description**: `recordTrustIdentity` swallows errors and returns `undefined`, so a failed write shows as `updated`. The next hook event runs a full re-check, so this does not weaken security.
- **Recommendation**: Map `undefined` to a warning leg.
- **Blocking**: no

## OWASP checks
- **Injection**: git uses `execFileSync` with an argv array and no shell. `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_COMMON_DIR` and `GIT_PREFIX` are scrubbed, and there is a timeout. The exclude entry is glob-escaped. A newline in the prefix could add extra ignore lines, which is harmless. Hook and MCP command construction did not change; `checkShimPathSafe` is retained.
- **Path traversal / symlinks**: `checkContained` uses the realpath of the nearest existing ancestor and runs before mkdir and again just before rename. Probes (refused, nothing written outside): `.claude` linked outside, `.claude` dangling to outside, `settings.local.json` linked outside, `settings.local.json` dangling, and a relative `../` dir link. User/global files follow links by design (AC-3). trust.json links are refused at install and skipped at hook time.
- **TOCTOU**: A small window remains between the final check and `rename`. It needs a concurrent attacker running as the same user, which is outside the threat model. The D-013 lost update against a running Claude is an accepted risk, backed by status checks.
- **Deserialization / prototype pollution**: Only `JSON.parse` is used, which creates `__proto__` as an ordinary own property. The code does no deep-merge of parsed data into live objects; writes go to fixed keys (`projects[root]`, `mcp.unimatrix`, constant hook events). Parse failures abort without touching the file. The `protectNumbers` sentinel guards against collisions.
- **Env var validation**: Config-dir variables must be non-empty absolute paths with no NUL; relative values are ignored with a warning. Env is read only, never set. Test helpers scrub all four variables and assert that overrides point under tmpdir.
- **File modes**: remote.json, install.json, trust.json and `<hash>/root` are forced to 0600 on every write, including unchanged content; `bin/*` to 0700. New files get 0600 by default; existing files keep their mode. Temp files are opened with the final mode before any data is written, so no wider-permission window exists. Skills now install at 0600 instead of copying the source mode. That is harmless, because git tracks only the exec bit.
- **Dependencies**: package.json is unchanged and has no `dependencies`. The new code uses Node built-ins only, and trust.js stays self-contained.
- **Secrets**: None in the diff. Error messages carry paths, never file contents or tokens. `status --json` filters out `_` keys.

## Blast Radius Assessment
Worst case is a writer bug that corrupts or truncates `~/.claude.json`, which holds all of Claude Code's per-project state, or that writes credentials with loose modes. Mitigations:
- Rename is atomic.
- Content is verified against a fixpoint after the write.
- Parse failures abort with the file untouched.
- Removal steps are gated behind a successful write.
- Credential modes are re-enforced on every write.

A residual risk is a concurrent Claude Code write being lost (D-013, accepted and reported). If containment broke, a malicious repo could steer writes into user files; the probes above show it holds.

## Regression Risk
- Behaviour changes: `credstore.write` now throws on chmod or fchmod failure where it used to be best-effort. Filesystems without chmod support could fail init. This is intended under D-025.
- Codex and OpenCode wrappers now propagate `SafeWriteError` instead of swallowing it.
- Removing `isWithinProject` is covered by `containWithin`.
- Writer tests: 106/106 pass across safe-write, git-exclude, trust, claude-json and install-store.

## PR Comments
- Posted 1 review comment on PR #1056
- Blocking findings: no

Unimatrix entries consulted: #5962, #5955, #5963, #5960 (ADR index #5937-#5963 reviewed), #5958, #5763, #199

## Knowledge Stewardship
- Not stored: attempted lesson "Windows: bare command name in child_process with cwd=project root runs a repo-planted exe" (topic security, lesson-learned); context_store refused — `uni-security-reviewer` lacks Write capability. Scrum master should store it or enroll the agent.
