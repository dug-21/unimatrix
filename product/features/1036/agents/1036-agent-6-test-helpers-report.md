# 1036-agent-6-test-helpers report (C17 Wave 1, home-guard only)

## Files
- packages/unimatrix/test/helpers/home-guard.js: added `assertUnderTmp(p)`, `scrubbedEnv(extra)`, `OVERRIDE_VARS`. `assertTmpHome` unchanged.
- packages/unimatrix/test/home-guard.test.js: 2 new cases (scrub strips four vars; overrides re-added only under tmp, else throw).

## Results
- `node --test` on home-guard.test.js + importers (init-preflight, pkg-binary-copy, mcp-stdio-spawn, codex-detect): 21 pass, 0 fail.
- Not committed. No package.json/lockfile change.

## Notes
- `scrubbedEnv` asserts every override value (not only absolute ones), so a relative override also throws. Stricter than pseudocode; relative paths resolve against cwd and could escape tmp.
- wire-fixture.js and wireOpts changes deferred to a later wave, per brief.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced #5911 (wire-init: inject temp home/env, assert with assertTmpHome), #5782, #251 (tests leaking dirs under hardcoded paths). Consistent with the approach taken.
- Stored: nothing novel to store -- implementation followed the existing NFR-05 pattern.
