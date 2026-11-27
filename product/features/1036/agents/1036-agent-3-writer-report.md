# 1036-agent-3-writer — C2 (trust.js core) + C3 (safe-write.js)

## Files
- packages/unimatrix/lib/trust.js (modified: SafeWriteError, hardenedWriteSync, renameWithRetry, tempPath, resolveTarget, checkContained, readState, sha256, realPathForm, trustRecordFor; recordTrustIdentity rewritten on the core, returns created|updated|unchanged, skips symlinked trust.json)
- packages/unimatrix/lib/safe-write.js (new, ~100 code lines: updateFileSync, writeFileSync, re-exports)
- packages/unimatrix/test/safe-write.test.js (new, 22 tests)
- packages/unimatrix/test/trust.test.js (extended, +5 tests)

## Tests
- trust, trust-selfcontained, install-store, safe-write: 74 pass / 0 fail.
- check-zero-deps OK; hook-client size gate OK (trust.js/safe-write.js are not in lib/hook-client).
- Full suite: 5 failures (remote install footprint, wire-plan intent gate/fail-safe/idempotence/warnings) also fail on baseline without my changes — not from C2/C3.

## Decisions / notes
- writeFileSync with `expectHash`: CAS mismatch throws `conflict` immediately (retrying against a fixed hash cannot succeed).
- checkContained uses realPathForm for the root (tolerates absent root) and case-folds on win32 and darwin.
- Old private writers (atomic-write.js, install-store atomicWrite/tmpName) untouched; Wave 4 removes them.
- Hook-time trust.json temp name changed from `.tmp.<pid>...` to `.trust.json.tmp-<hex16>`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- ADR-003 #5962, ADR-005 #5955 (writer core in trust.js, safe-write API).
- Stored: nothing novel to store -- one trap noted here: stubbing fs.openSync for flag "r" also intercepts fs.readFileSync (Node opens via openSync internally); filter on the path.

Unimatrix entries consulted: #5955, #5962
