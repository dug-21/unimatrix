# C14 — Credential store write (`lib/hook-client/credstore.js`)

## Purpose
`write()` uses the hardened writer with `enforceMode: 0o600` (D-020, D-025). Lazy require keeps the hook hot path free of the writer. Reads unchanged.

```
function write(projectHash, cred, opts) -> string[]
  ...existing path/merge logic unchanged...
  if dryRun: return ["[dry-run] Would write credential store " + p + " (mode 0600)"]
  fs.mkdirSync(dirname(p), {recursive:true, mode:0o700})        // keep existing mkdir (add mode only if current code lacks it: do not change)
  sw = require("../safe-write.js")                                // lazy, inside write()
  r = sw.writeFileSync(p, JSON.stringify(merged, null, 2) + "\n", {enforceMode:0o600, newMode:0o600})
  // delete fs.writeFileSync + chmodSync block
  return [(r.action === "unchanged" ? "Credential store unchanged " : "Wrote credential store ") + p + " (mode 0600)"]
```
Action text change for `unchanged` is optional; if any test asserts the exact "Wrote ..." string on a re-run, keep "Wrote" for all actions. Prefer keeping the string identical (minimise churn) — implementer checks `test/init-remote.test.js`.

Check: `lib/hook-client/` size budget (`test/check-hook-client-size.js`) — the lazy require adds ~1 line; safe-write is outside hook-client so not counted. Confirm the size check does not follow requires.

## Error handling
`SafeWriteError` propagates to `initRemote` → exit 1.

## Key tests
- Existing credstore mode tests green (FR-33).
- Pre-chmod `remote.json` 0644 → 0600 after write; content-equal + loose mode → `updated` via chmod.
- Symlinked `remote.json` → followed (user/global, AC-3), link kept.
- Hook-time `read()` path does not load safe-write (assert `require.cache` lacks it after a read-only hook run).
