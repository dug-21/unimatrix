# 1036-agent-8-credstore-init — C14 credstore, C13 init

## Files
- packages/unimatrix/lib/hook-client/credstore.js — write() lazy-requires ../safe-write.js, enforceMode/newMode 0o600; writeFileSync+chmod block removed; "Wrote credential store" string kept.
- packages/unimatrix/lib/init.js — installSkills writes via safeWrite.writeFileSync(containWithin: projectRoot), called in dry-run too (FR-22a); cleanStaleRemoteSubtree via updateFileSync with detectIndent, rethrows SafeWriteError.
- packages/unimatrix/test/hook-client/credstore.test.js — +3 (mode-drift chmod-only, symlink followed/link kept, lazy-require subprocess check).
- packages/unimatrix/test/init.test.js — +4 (skill symlink outside → outside-project dry-run+real, tab indent kept + idempotent, malformed → note, settings.local.json symlink outside → rethrow).

## Tests
node --test init, init-remote, init-integration, credstore, size-gate: 152 pass, 0 fail.

## Size gate
stripped 114996/115012 (16 B headroom), raw 202841/203058 (217 B headroom). Budget not edited.

## Notes
- No duplicate warnings print loop exists in init.js/printSummary; top-level handler prints `unimatrix init failed: <msg>`, exit 1.
- Not committed (per instruction).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- ADR-003 #5962, ADR-005 #5955, credstore homedir asymmetry #5121, size-gate #4780/#5372.
- Stored: nothing novel to store -- straightforward migration; lazy-require pattern already in brief.

Unimatrix entries consulted: #5962, #5955, #5121, #5372
