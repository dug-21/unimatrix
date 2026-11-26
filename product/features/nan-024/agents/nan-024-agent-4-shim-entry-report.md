# nan-024-agent-4-shim-entry Report

## Report
- Created `packages/unimatrix/lib/shim-entry.js`: inlined project predicate, effectiveEnv, installedDecision; sibling `./trust.js` loaded (with pre-require lstat/owner/mode check on POSIX) only after the project predicate is non-null; then in-process `require(<packageRoot>/bin/unimatrix.js).run(args, {preflight})`. Exports `isUnimatrixProject, effectiveEnv, installedDecision, run`; `require.main` guard.
- Created `packages/unimatrix/test/shim-entry.test.js` (14 tests, spawned against an assembled fake `~/.unimatrix` with stub launcher). Top-level `test/*.test.js` is auto-discovered by run-hook-client.js; no registration edit needed.
- Skipped parity rows in `test/project-parity.test.js` now run and pass.
- Tests: `node --test test/shim-entry.test.js test/project-parity.test.js` -> 64 pass, 0 fail, 0 skipped.
- Not committed; no integration tests run; no deps added.
- Gaps: no `--print-project` subprocess mode (parity imports the module directly); require-trace test for "zero package modules" is covered behaviorally (broken package/trust, silent exit 0), not via module trace; Windows path untested locally.

Unimatrix entries consulted: #5900, #5901, #5896, #5897, #5898, #5894, #1194

## Knowledge Stewardship
- Queried: context_briefing -- ADR-002/004 bind order and exit codes; no pattern for in-process shim found.
- Stored: nothing novel to store -- no surprises beyond pseudocode.
