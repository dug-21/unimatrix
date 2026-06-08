# vnc-027 Agent 5 — win32 UDS test guard

## Task
Remediate the Windows CI matrix failure on PR #701 (feature/vnc-027). The new UDS
test suites bind/connect real Unix-domain sockets; Windows returns `listen EACCES`
for a filesystem-path Unix socket. UDS is Unix-only by design (ARCHITECTURE.md:
"UDS is Unix-only — document, don't shim"). Add platform-skip guards to the
socket-dependent test suites only. No production/lib changes; no weakened assertions.

## 1. Files modified
- `packages/unimatrix/test/hook-client/transport-uds.test.js`
- `packages/unimatrix/test/hook-client/parity-layer2-uds.test.js`

## 2. Suites: skip on win32 vs still run
SKIP on win32 (bind/connect a real UDS socket):
- `transport-uds.test.js` — uses `startUdsStubServer` / `startUdsBlackholeServer`
  (`net` `server.listen(socketPath)`) via `test/helpers/stub-server.js`. This is
  the confirmed-failing suite (17 fails, `listen EACCES`). Guarded at the top-level
  `describe("transport-uds", { skip: IS_WINDOWS }, ...)`.
- `parity-layer2-uds.test.js` — live daemon via `startRealServer` +
  `udsPost`/`udsConnectRaw` (`test/helpers/real-server.js`). Guarded the top-level
  `before` hook (`if (IS_WINDOWS) return;` so it never starts the daemon) AND each
  of the 8 top-level `describe(...)` blocks with `{ skip: IS_WINDOWS }`.

STILL RUN on win32 (pure offline byte-compare against committed fixtures — NO
socket; provide Windows coverage):
- `parity-uds-framing.test.js` — `encodeFrame`/`mapHookResponse` vs
  `fixtures/parity/uds-framing` (file comment: "offline byte-compare"). No
  `listen`/`createServer`/helper imports. Left untouched.
- `parity-uds-sync-stdout.test.js` — `mapHookResponse`/`writeSyncOutput` vs
  `fixtures/parity`. No socket. Left untouched.

## 3. config.test.js separator check
No change needed and not broken. `socketPath` assertions use `path.join(...)`
(separator-agnostic) throughout. The one POSIX-path-shape assertion already returns
early on win32 (`if (process.platform === "win32") return; // posix path shape`),
and `test_root_walk_windows_separators` is `{ skip: process.platform !== "win32" }`.

## 4. Tests (Linux, full hook-client suite)
`node test/run-hook-client.js`:
- tests 536, pass 535, fail 0, skipped 1, todo 0
- The single skip is the pre-existing win32-only `test_root_walk_windows_separators`.
- Guards are inert on Linux (`IS_WINDOWS === false`), so every UDS suite still runs
  and passes — confirms no regression and no weakened assertions. On win32 the
  guarded suites will register as skipped (node:test `skip` option), not failures.

## 5. Size-gate
`node test/check-hook-client-size.js` → OK: stripped 68907/100000, raw
112773/160000. Only `lib/` files count; my edits are `test/`-only, so the gate is
unaffected and passes.

## Confirmations
- No production/lib code changed — `git diff --stat` shows only the two `test/`
  files (14 insertions, 9 deletions).
- Guards match an existing repo idiom: `const IS_WINDOWS = process.platform === "win32";`
  + `{ skip: IS_WINDOWS }` per `it`/`describe`, as used in
  `test/hook-client/state.test.js` (`const IS_WINDOWS = process.platform === "win32";`,
  `it(..., { skip: IS_WINDOWS }, ...)`), `test/hook-client/queue.test.js`, and the
  inline form in `test/hook-client/index.test.js`
  (`{ skip: process.platform === "win32" }, // symlinks need privileges on Windows`).
  No new idiom invented.
- Pushed commit: `17fba5a2` on `feature/vnc-027`
  (`4c35ddd6..17fba5a2`), message "fix: skip UDS-only tests on win32 (#680)".

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced related hook-client/UDS
  entries (#4828 live UDS layer-2 session keying, #4823 config.js UDS selection
  traps, #4824 transport-uds half-close contract, #4780 hook-client size-gate
  lesson). None altered the approach.
- Stored: nothing novel to store — the win32-skip pattern is already an established
  repo idiom (state.test.js / queue.test.js / index.test.js) and the Unix-only
  constraint is already documented in ARCHITECTURE.md. This is a mechanical CI
  test-guard fix with no new failure mode or reusable pattern.
