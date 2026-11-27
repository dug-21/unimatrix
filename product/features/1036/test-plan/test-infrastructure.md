# Test Plan — C17 test infrastructure (`test/helpers/*`)

Risk: R-09. Cumulative: extend `home-guard.js`, `wire-fixture.js`; no new scaffolding.

## home-guard.js (`test/home-guard.test.js`)
- `scrubbedEnv()` lacks all four vars even when `process.env` sets them; `scrubbedEnv({CLAUDE_CONFIG_DIR:tmpD})` includes it; non-tmp value → throws.
- `assertTmpHome` rejects an override outside tmp.

## wire-fixture.js
- `seedSymlinked(file,target)` (relative/absolute), `seedLegacy(home,dirs)`, `seedAlternates(home,env)`, `makeProject({git, worktree, submodule})` with real git; `wireOpts` uses `scrubbedEnv`.
- Shared helpers: `treeHash(dirs)` incl. modes and mtimes; `ownedCount(files)` scanner (AC-6); `parity(fixture)` (AC-9); `runOpencodeDebugConfig` asserting `--standalone` and timeout; `which(bin)` skip helper with #1053 message.

## Sentinel run
- Full `package` set with all four vars → non-tmp sentinel dir; sentinel unchanged afterwards.

## Gates
- `check-test-count.js` passes (counts non-decreasing); new suites registered by `package` set (top-level, non-recursive).
