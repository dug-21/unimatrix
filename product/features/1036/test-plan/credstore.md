# Test Plan — C14 credstore (`lib/hook-client/credstore.js`) — extend hook-client credstore tests

Risks: R-01, R-06, R-23.

- Existing credstore mode tests green against writer (FR-33).
- `write()` with existing `remote.json` at 0644 → 0600 (`enforceMode`), content correct.
- Equal content, drifted mode → chmod only.
- Lazy require: after a hook event through `run-hook` path, `require.cache` has no `safe-write.js` key; after `write()`, it does.
- Hook-client size check (`check-hook-client-size.js`) still passes.
