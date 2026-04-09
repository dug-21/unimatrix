# Agent Report: 550-agent-1-fix

## Task
Fix GH #550 — linux-x64 release build disabled.

## Files Modified

1. `packages/unimatrix/test/resolve-binary.test.js`
   - Updated `test_platform_map_contains_linux_x64` deepStrictEqual assertion from a one-entry object to the full two-entry object including `linux-arm64`.

2. `.github/workflows/release.yml`
   - Replaced the commented-out x64 stub (lines 18-21) with a full `build-linux-x64` job using `ubuntu-22.04` (glibc 2.35, matching arm64 pinning).
   - Changed `package-npm.needs` from `[build-linux-arm64]` to `[build-linux-x64, build-linux-arm64]`.
   - Added `actions/download-artifact` step for `unimatrix-linux-x64` into `packages/unimatrix-linux-x64/bin/`.
   - Merged the single-line `chmod` into a two-line `Set permissions` step covering both architectures.
   - Added `Publish @dug-21/unimatrix-linux-x64` step before the root package publish step.

## Test Results

- `node packages/unimatrix/test/resolve-binary.test.js`
- 9 tests, 9 pass, 0 fail

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — entry #4273 confirmed ubuntu-22.04 pinning rationale (glibc 2.35 compatibility for both arm64 and x64 builds). No other entries directly applicable.
- Stored: nothing novel to store — the fix is a direct restoration of a previously-working pattern. The glibc pinning rationale is already documented in entry #4273.
