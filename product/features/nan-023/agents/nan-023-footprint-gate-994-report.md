# nan-023 Footprint Gate (#994) — Agent Report

Agent: nan-023-footprint-gate-994 (uni-js-dev)
Branch: `fix/nan-023-footprint-gate-994`
PR: https://github.com/dug-21/unimatrix/pull/997 (Closes #994)

## Task
Human-approved cap raise for the remote-install footprint gate: 290KB → 380KB, and re-activate the previously-skipped guard. Post-merge follow-up (nan-023 branch already merged); worked from current main.

## Files modified
- `packages/unimatrix/test/remote-client.test.js` (test-only; `dirBytes` measuring logic untouched)

## Changes
- `SIZE_LIMIT` 290000 → 380000
- Test renamed `test_remote_install_under_290kb` → `test_remote_install_under_380kb`
- Removed `{ skip: "Pre-existing: GH#994 ..." }` — guard is active again
- Updated comment block: 290→380KB raise, human-approved on #994, supersedes #775

## Test result
`node --test --test-concurrency=1 test/remote-client.test.js`: 26 pass, 0 fail, **0 skipped**.
Footprint guard is **active and green**.

Measured footprint: **359,908 bytes** (lib=291,488 + skills=68,420) vs 380,000 cap → **20,092 bytes headroom**.

## Issues / blockers
None.

## Knowledge Stewardship
- Queried: skipped context_briefing — mechanical single-constant test change with fully-specified instructions; no design/parity surface touched.
- Stored: nothing novel to store — this is a human-decided gate-cap raise (recorded on the feature issue #994 / PR #997), not a reusable code pattern.
