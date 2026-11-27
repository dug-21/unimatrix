# 1036-agent-19-tests-b — C17 tests part B (test-only)

## Files edited (not committed)
- packages/unimatrix/test/init-integration.test.js
- packages/unimatrix/test/status.test.js
- packages/unimatrix/test/git-exclude.test.js

No lib/ changes by this agent. (lib/init.js and wire.test.js show as modified in the tree; those are other agents' edits.)

## Results
- git-exclude.test.js + status.test.js: 36 pass / 0 fail (before: 17 + 0 new). status.test.js 17 pass; git-exclude.test.js 19 pass.
- init-integration.test.js: 26 pass / 0 fail / 0 skip. Real `claude` and `opencode` on PATH, so the AC-1c and AC-2b real-binary tests ran.
- Baseline for the three files before this work: 50 pass.

## Added
init-integration.test.js (inside "init 1036 spawned acceptance"):
- test_ac5_run_twice_overrides_and_twins_all_unchanged: no override, CLAUDE_CONFIG_DIR, and OPENCODE+XDG; owned twins seeded (legacy claude hooks, opencode twin, default ~/.claude.json entry). Run 2: all legs unchanged, no D-013 notice, no byte/mtime/mode change over dir, home, override dirs.
- test_ac5_bundle_run_twice_all_unchanged: async, real `init --bundle` against the e2e-harness TLS stub, with and without CLAUDE_CONFIG_DIR. Skips with a reason if no cert material.
- test_ac5_mode_drift_repaired_then_converges_unchanged: install.json 0644 → `updated` and 0600 on run 2 → run 3 fully unchanged.
- test_ac8_fr42_fixtures_expected_action_and_clean_diff: 15 spawned FR-42 fixtures (gitignored, core.excludesFile, existing exclude, tracked, `!` override, PATH without git, linked worktree, submodule with --project-dir, subdir with `a*b[1]!`, missing info/, symlinked exclude, GIT_DIR in env). Dry-run and real action, `git diff` and `git diff --cached` clean where applicable.
- test_ac9_parity_dry_run_matches_real_over_every_fixture: over every fixture above plus AC-1 and AC-2 twin fixtures and the mode-drift fixture, and an outside-root link (exit 1 both modes, target untouched). Checks: dry-run leaves tree, modes and mtimes unchanged; manifest parity (only allowed diff is appended-not-effective vs updated); planned paths equal the real changed-file set both ways.

status.test.js (new "S-1 spawned" block, real bin/unimatrix.js, scrubbedEnv, tmp HOME):
- missing local-scope entry: text MISSING line, json status missing, exit 0.
- all five claudeMcp.status values through --json.
- twin-only owned hook under CLAUDE_CONFIG_DIR: stale line and staleLegacy.
- unparseable opencode.json named, exit 0.
- spawned init `resolved:` lines equal status.collect().configDirs for claude mcp, claude hooks, opencode mcp across three override sets.

git-exclude.test.js:
- already listed in existing exclude: unchanged, bytes and mtime equal, git diff clean.
- linked worktree and submodule: entry in git-reported file, git diff clean.

## Lib observations (no lib bugs found; behaviour to confirm)
1. Linked worktree and submodule: init with no --project-dir resolves the project root to the main checkout / superproject (hook-client walkToProjectRoot and detectProjectRoot, detect_project_root parity). Config lands in the main tree. Looks intentional; the FR-42 rows were written with --project-dir, which the tests use.
2. `--dry-run` for the `!` override reports `updated` where the real run reports `appended-not-effective`. This is the one difference AC-9 allows; the parity test normalizes it.
3. The outside-root refusal message says "outside project root" and names link and target. It does not carry the `outside-project` code token. Tests assert the message text.

## Incident: real home written by my fixture (fixed; needs owner attention)
- The first parity run had a fixture with no `home`, so init ran against the real $HOME (/home/vscode). Effects:
  - ~/.claude.json: 6 `projects` keys for /var/tmp/unimatrix-* temp dirs added. I removed exactly those keys (re-serialized with JSON, indent 2, atomic rename). Other keys untouched.
  - ~/.unimatrix: 6 per-project root-marker dirs for temp projects created. Removed.
  - ~/.unimatrix/install.json, bin/{unimatrix,shim-entry.js,trust.js}, pkg/0.11.3 were rewritten by a real init run from this checkout (version 0.11.3, shim script). The previous contents are not recoverable from here. A real `init` from this checkout would write the same shape, but confirm the real store is what you expect.
- Fix: every fixture now supplies a tmp home, and the `ri` helper calls assertTmpHome. No further real-home writes after the fix (checked: zero temp-project keys in ~/.claude.json).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned #5778 (node --test on Node 24), #4774 (spawned hook-client stub), #5911-era temp-home guidance via the wire report; no new entry needed for this task.
- Stored: nothing novel to store -- the missing-home fallthrough is a case of the existing temp-home rule (#5911); flagging it here for the owner.
