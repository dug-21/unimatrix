# nan-024-agent-3-trust Report

## Report
- Created `packages/unimatrix/lib/trust.js` (231 lines, Node builtins only, no package requires), `test/trust.test.js` (244), `test/trust-selfcontained.test.js` (64).
- Exports: TrustError, checkTrust, trustSet, fullCheckWindows, recordTrustIdentity, statIdentity, normalizeKey, rightsGrantWrite.
- Tests: 27 pass / 0 fail. Windows branches run on Linux via platform arg + fake spawn. Gate-3a WARN test added (dir ino/mode mismatch -> exactly one full check; dir mtime change alone does not).
- Runner: top-level `test/*.test.js` are auto-selected by `--set package`; no runner edit needed.
- Not covered here (need install-store/shim-entry, other components): installed-sibling byte-equality, atomic refresh/fault injection, shim-entry ordering.
- Not covered: real icacls on Windows (CI cell).
- Note: `check-hook-client-size.js` currently FAILS (stripped 114927/113639, raw 202903/201399) from other agents' hook-client changes (not lib/trust.js; trust.js is outside lib/hook-client).
- Spec deviation: trustSet always lists required members (u, bin, shim, shim-entry.js, trust.js, install.json) even if absent so a missing one is refused; statIdentity also returns `mode`; uses lstat (symlink refused on win32 too).

## Knowledge Stewardship
- Queried: context_briefing, context_get #5900 (ADR-002), context_search pattern -- no trust-specific JS patterns.
- Stored: nothing novel to store -- no gotchas beyond the plan.

Unimatrix entries consulted: #5900, #4779, #261
