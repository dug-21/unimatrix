# 1036-agent-10-merge-settings — C8

Files: packages/unimatrix/lib/merge-settings.js, packages/unimatrix/test/merge-settings.test.js

- mergeSettings/removeOwnedHooks compute inside updateFileSync (computeMerged/computeRemoveOwned); containWithin passed through; atomicWriteText import removed.
- Semantic write decision (FR-16/D-022): compare JSON.stringify of parsed-before vs merged; non-canonical correct file -> text null -> unchanged.
- SubagentStop opt-in via subagentStopEnabledFromContent on the CAS bytes; subagentStopEnabled(file) kept.
- Added wireProjectHooks (W1) and removeOwnedHooksLeg (R2-R4). crossScopeFiles kept (test caller exists), implemented via removeOwnedHooks.
- File 478 lines (net-neutral). No lexical isWithinProject guard existed in this file.
- Tests: merge-settings.test.js 110/110 pass (writeClaudeMcp calls updated with file: arg after C7 landed).
- Gotcha: safe-write opts are merged via Object.assign, so `fault: undefined` overrides the default noop. Spread optional opts conditionally.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- #4942 / #4939 Step 3c identity-keep gotchas (kept intact), #1195 merge pattern.
- Stored: nothing novel to store -- the Object.assign-undefined trap is generic JS behavior, recorded in this report.
