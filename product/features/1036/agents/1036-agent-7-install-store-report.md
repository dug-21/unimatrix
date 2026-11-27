# 1036-agent-7-install-store — C12 install store

Files: packages/unimatrix/lib/install-store.js, packages/unimatrix/test/install-store.test.js (not committed, per instructions).

- Every store write goes through safe-write.writeFileSync with enforceMode/newMode: shim, shim-entry.js, trust.js at 0700 (0600 on win32); install.json and the root marker at 0600 (D-025).
- Legs `{harness:"unimatrix", surface:"store", action, path}`; package-copy leg is created/unchanged via isCompleteCopy. `actions` is derived from the legs.
- copyPackage: an exclusive `.<ver>.tmp-<hex>` dir (non-recursive mkdir, 5 name tries), `.complete` written with wx and fsynced, fault `before-pkg-rename`, then one atomic dir rename. An invalid dest is moved aside, then deleted.
- writeRootMarker returns {file, action} (Q7). No external callers.
- refuseSymlinkedTrustJson runs before the trust step, with the canonical message. TrustError "symlink not allowed" is rewrapped with the §3.7 message. plannedTrustAction covers win32 dry-run.
- Prune keys on `.tmp-` plus age; the pid exemption is removed. Dry-run creates no dirs (FR-62).
- Private atomicWrite/tmpName/renameWithRetry/sleepMs removed. atomic-write.js is untouched.

Tests: install-store 21/21 (6 new). trust, trust-selfcontained, safe-write, init*, wire: 188/188 pass. check-zero-deps OK.

Issue: on win32, recordTrustIdentity returns undefined on failure, so the leg falls back to "updated".

## Knowledge Stewardship
- Queried: context_briefing. #5908 (copyPackage rmrf race lesson) applied; ADR #5962.
- Stored: nothing novel to store. The patterns are already covered by #5908 and ADR-003.
