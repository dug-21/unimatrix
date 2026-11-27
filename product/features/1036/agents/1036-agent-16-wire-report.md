# 1036-agent-16-wire — C6 orchestrator

Files (not committed): packages/unimatrix/lib/wire.js (226→189 lines), packages/unimatrix/lib/codex-install.js (1-line fix), packages/unimatrix/test/wire.test.js, packages/unimatrix/test/init-integration.test.js, packages/unimatrix/test/helpers/wire-fixture.js.

## Done
- wire.js: resolveConfigDirs once → preflight → installStore (store.legs) → buildWirePlan/runWirePlan → resolved: lines (FR-50), same-env note (FR-51), leg lines, D-013 notice (FR-52), warnings into actions + stderr (FR-53). Deleted gitIgnored, GITIGNORE_WARNING, dispatch*, removeProjectFileMcp, storeLegs, hardcoded ~/.claude.json (no-transport leg uses dirs.claude.claudeJson). No writeClaudeMcp adaptation remains.
- Codex resolved: lines/note only when Codex is processed (CX leg not `skipped-not-detected`); opencode only when W5 planned.
- Tests: wire.test.js +49 (AC-1a, AC-1b×3, AC-2a×3, AC-3×3, AC-4×3, AC-6, AC-5, AC-7×4, R-11×2, R-12×3, FR-52, no-transport, AC-9 parity harness, S-2×20, S-6×2); 2 stale manifest tests updated, gitignore-warning test replaced by AC-8 exclude test. init-integration +10 spawned (AC-1c real claude, AC-2b real opencode ×3 incl. precedence, AC-8 ×2, AC-5, AC-9, FR-53 exit 1, FR-53 invalid override exit 0).
- Helpers: makeGitProject, gitEnv, ownedHooks, seedLegacy, seedSymlinked, writeJsonAt.

## Results
- wire + wire-plan + init + init-integration: 148 pass / 0 fail / 0 skip (claude and opencode on PATH; real-binary tests ran).
- npm test: 3 failures, none from this change: remote footprint 473881 > 440000 (lib growth across waves incl. untracked wire-plan.js/status.js), resolve-binary ×2 (local node_modules/@dug-21/unimatrix-linux-x64 present). check-test-count PASS; zero-deps PASS.

## Issues
- Fixed in Wave 2 module codex-install.js:361: invalid CODEX_HOME warning was appended to every Codex leg reason even when wire passes dirs, breaking S-2 (manifest == no-override). Now gated on `!o.dirs`.
- AC-7 has no fault seam through wire(); tests inject via node:test `mock.method` on fs.openSync/renameSync (trust.js uses `fs.` member calls).
- claude-json reports W2 leg `created` (entry-level) when the file pre-existed; AC-7 test accepts created|updated.
- removeOwnedHooks leaves `"hooks": {}` after removing all owned hooks; test asserts that existing behaviour.
- Not written: AC-4 RT-07 (Codex project/user removal) via wire — covered only by existing codex-install tests; AC-3 project-file D-026 outside-root spawned set, AC-5 `--bundle`, AC-8 worktree/submodule/no-git/tracked rows, AC-9 over every fixture — left for C17 / git-exclude plan owners (scope/time).
- init has no `--harness`; AC-2b relies on FR-13 prior-opt-in twin.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- #5776 wire-orchestrator traps, #5911 temp-home rule, #5959 ADR-004, #5960 ADR-002.
- Stored: nothing novel to store -- fs mock.method seam is noted here; existing #5911 covers env isolation.
