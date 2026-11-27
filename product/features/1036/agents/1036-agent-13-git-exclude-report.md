# 1036-agent-13-git-exclude — C5 report

Files: packages/unimatrix/lib/git-exclude.js (new, 100 lines), packages/unimatrix/test/git-exclude.test.js (new).
Tests: `node --test test/git-exclude.test.js` 17/17 pass (real git: fresh, update incl. CRLF/no-newline, .gitignore/core.excludesFile, tracked, `!` override, non-repo, no git, hang timeout, linked worktree, submodule, escaped subdir prefix, missing info/, symlinked exclude, scrubbed GIT_DIR, dry-run read-only).
Size gate: pass (hook-client untouched). Deps: none added. Not committed (per instruction).

Notes:
- Export `ensureSettingsLocalIgnored(root, {dryRun, run})` -> WireLeg {harness:"claude-code", surface:"git-exclude", ...}.
- Deviation-free from pseudocode except: CRLF exclude files get CRLF appended; `info/` mkdir'd before append on real runs (safe-write does not create parents); ls-files spawn error -> skipped-git-error.
- Env leaks other than the 5 scrubbed GIT_* vars (e.g. user global core.excludesFile) are honored by design; tests pin core.excludesFile per repo.
- Hang test costs ~10s (fixed GIT_TIMEOUT_MS).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- ADR-006 #5963, worktree .git-file lesson #588, #4797.
- Stored: nothing novel to store -- behavior covered by ADR-006.

Unimatrix entries consulted: #5963, #588, #4797
