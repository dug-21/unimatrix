# 1036-agent-9-claude-json — C7 report

## Files
- packages/unimatrix/lib/claude-json.js (rewritten: pure mutates via updateFileSync)
- packages/unimatrix/test/claude-json.test.js (new, 10 tests)
- packages/unimatrix/lib/wire.js (minimal adaptation, line 103: `home: c.home` -> `file: path.join(c.home, ".claude.json")`; Wave 3 rewires to C1-resolved file)

## Exports
writeClaudeMcp, claudeMcpLegs, computeClaudeMcp, removeOwnedMcpJson, removeOwnedMcpJsonLeg, computeMcpJsonRemoval, readClaudeMcpState, resolveProjectKey, ClaudeJsonError. Removed: atomicWriteText, statOrNull/sameStat CAS, o.home. removeOwnedClaudeLegacy absent.

## Tests
- test/claude-json.test.js: 10/10 pass.
- Fidelity: non-round-tripping number tokens (9007199254740993, 1.0) preserved via escaped string sentinels (Node 18 has no reviver `source`).
- Baseline already red from concurrent Wave 2 edits (`o.fault is not a function`, 131 fails in merge-settings/init/codex suites). My delta: +5 fails, all in test/merge-settings.test.js "nan-024 per-machine Claude MCP" block + one 0600 test calling `writeClaudeMcp({home,...})`. Out of my scope; owner must switch to `file: path.join(home, ".claude.json")` (or move to claude-json.test.js).

## Issues
- lib/merge-settings.js:8 and lib/codex-install.js:23 import `atomicWriteText` from claude-json.js — now undefined. Owners of those files must switch to safe-write.
- Size gate (hook-client): pass, stripped 114996/115012 (tight; not affected by this component).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- no claude-json-specific entries; nan-023 ADR-002 (surgical writer) noted.
- Stored: nothing novel to store -- number-sentinel technique is component-local; pending gate review.
