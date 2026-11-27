# 1036-agent-24-clean-env-fix report

Fix (tests only, lib unchanged): new `stagedCli()` in `test/helpers/wire-fixture.js` stages bin/lib/skills/protocols/package.json into a tmp package root with fixture `node_modules/@dug-21/unimatrix-linux-{x64,arm64}` (package.json + stub bin/unimatrix). Removed on process exit. The four spawned-init groups (`init-integration.test.js:486`, `wire.test.js:918,1062,1137`) now spawn that entry, so init's required binary-package copy resolves without the optional dependency. No assertions were changed.

Clean worktree (`npm install --omit=optional`, @dug-21 empty, worktree removed afterwards):
- package: 955 pass, 0 fail, 1 skip (was 29 fail)
- wire.test.js + init-integration.test.js: 125/125
- wiring-e2e (E2E_REQUIRE_LOCAL=1, target/release symlinked): 45/45, 0 cancelled

Main checkout (platform package present):
- wire + init-integration: 125/125
- package: 969 pass, 2 fail, 2 skip. The 2 failures are in `resolve-binary.test.js` (missing-package cases) and fail identically on the stashed tree. That file matches main and was not touched. Those tests fail whenever the real platform package is installed; they pass in the clean environment, which is what CI uses.
- size gate: OK (hook-client not touched)

Unimatrix unavailable: context_briefing failed with "pool timed out while waiting for an open connection".

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing: unavailable (pool timeout)
- Stored: nothing. Unimatrix was unavailable. The pattern to store later: spawned-CLI tests that depend on optional deps should run from a staged tmp package root with fixture deps.
