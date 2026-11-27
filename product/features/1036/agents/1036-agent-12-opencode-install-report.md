# 1036-agent-12-opencode-install — C10 report

## Files
- packages/unimatrix/lib/opencode-install.js
- packages/unimatrix/test/opencode-install.test.js

## Changes
- Writes go through safe-write `updateFileSync` (pure compute mutators: computeUserMcp, computeLegacyRemoval, computeProjectRemoval). atomicWrite and the catch-all "skipped" are removed, so SafeWriteError propagates.
- `writeOpencodeMcp(o)` requires `o.file` (the resolved path). `o.legacyFile` holding an owned entry counts as prior opt-in (D-015).
- New `removeOwnedOpencodeUserMcpLeg(file, dryRun)` (R10). Malformed file → skipped-malformed with a warning; foreign entry → no leg, `.warnings`.
- `removeOwnedOpencodeProjectMcp` removes R11 mcp.unimatrix and the G3 legacy plugin[] entry in one mutate (`containWithin: root`). Malformed file throws (D-029). Returns `removedPlugin`.
- New `removeOwnedOpencodeProjectLegs` (R11+R12 legs). New `parseJsonRaw`. `readJsonSafe`, `isWithinProject` and `detectIndent` are kept because codex-install, claude-json and merge-settings import them.

## Tests
`node --test test/opencode-install.test.js`: 59 pass, 0 fail. codex-install: 52/0. merge-settings has 79 failures with or without this change (another agent's work in progress).

## Issues for the integrator (C6 wire.js)
- wire.js `dispatchOpencode` still calls writeOpencodeMcp without `file`/`legacyFile`. It must pass `resolveConfigDirs(...).opencode.configJson` and `legacy.opencodeJson`, and delete its try/catch.
- Gate R10 on W5 leaving the destination wired (`created`/`updated`/`unchanged`, otherwise `skipped-dest-unwired`). Skip R10 entirely when `sameRealFile`. The tests model this gate in an `install()` helper.
- Project legs use surface `"mcp"` as the pseudocode specifies. The W5 and R10 legs keep `"retrieval"` (opencodeLeg). Confirm against ARCHITECTURE §6a.
- Size gate: not applicable (outside lib/hook-client).

Unimatrix entries consulted: #5947, #5743, #5960, #5773, #5763

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- nan-024 ADR-006, 1036 ADR-002 resolver, opencode entry pattern
- Stored: nothing novel to store -- this applies the existing safe-write compute/re-derive-from-`before` pattern
