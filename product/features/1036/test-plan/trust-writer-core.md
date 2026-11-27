# Test Plan — C2 writer core (`lib/trust.js`) — extend `test/trust.test.js`, `trust-selfcontained.test.js`

Risks: R-06, R-23, R-01, R-24.

## Migration first (FR-33)
- Existing trust.js rename/retry/mode tests are re-pointed at `hardenedWriteSync`/`renameWithRetry` and pass BEFORE private writers are removed. Test count per file non-decreasing.

## Unit
- `hardenedWriteSync(f, data, {newMode:0o600})` new file → mode 0600 under umask 000 and 022.
- `enforceMode:0o600` on existing 0644 file with equal bytes → chmod only, result `updated`, no rename (spy `renameSync` count 0), mtime unchanged.
- `renameWithRetry(a,b,"win32")` with injected `fs.renameSync` failing EPERM ×3 → succeeds on 4th; failing 10× → throws; on `"linux"` no retry on first EPERM.
- Symlinked `trust.json` (live, dangling) via `trust set`/install path → `TrustError`/exit 1 with exact FR-20a text; `lstat` still symlink, target bytes unchanged.
- Hook-time `trust.json` symlink → write skipped, no throw.
- Trust check / `trust set` behavior unchanged (existing tests green).

## Self-containment (R-23)
- `trust-selfcontained.test.js`: only built-in requires.
- After `init`, installed `~/.unimatrix/bin/trust.js` byte-identical to `lib/trust.js`.
- Hook-path latency re-measured against nan-024 ADR-005 budget (record number in 3c report).
