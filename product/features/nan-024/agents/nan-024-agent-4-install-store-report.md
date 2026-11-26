# nan-024-agent-4-install-store Report

## Report
- Created: packages/unimatrix/lib/install-store.js, packages/unimatrix/test/install-store.test.js (auto-discovered by the package set; no registration edit needed).
- Tests: 14 pass / 0 fail (`node --test test/install-store.test.js`). Windows ACL/trust.json paths implemented but not exercised here (POSIX host).
- Extra options beyond pseudocode: `arch`, `requireBinary` (Linux local: throws BinaryPackageMissingError naming the pkg; default skips silently, for cloud attach), `libDir`, `spawn`, `_fault` (test fault injection). wire-init must pass `requireBinary: true` for Linux local.
- Exports: installStore, checkShimPathSafe, readInstallJson, writeRootMarker, UnsafeShimPathError, NodeNotRecordableError, BinaryPackageMissingError, PRENODE_ENABLED.
- Size gate: `test/check-hook-client-size.js` currently FAILS (stripped 115009/113639, raw 203000/201399) from lib/hook-client edits by siblings (mcp-bridge.js, remote-resolve.js); install-store.js is outside lib/hook-client.
- Not done: Stage 3c size measurement script (out of scope); no commit (per instruction).

Unimatrix entries consulted: #4925, #4336, #1198, #5825 (briefing); ADR-001 #5896, ADR-002 #5900 via ADR-INDEX/DECISIONS (D-078, D-082, D-087, D-090).

## Knowledge Stewardship
- Queried: context_briefing -- nothing install-store specific beyond older nan-016 copy-install ADRs.
- Stored: nothing novel to store -- (concurrent-init rename race on `dest` handled by accepting a peer's complete copy; documented in code).
