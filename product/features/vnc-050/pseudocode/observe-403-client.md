# C6c Observe 403 Client Handling — `lib/hook-client/observe-latch.js` (new, < 100 lines), `index.js`, `queue.js` replay call site

ADR-004 (#5826), FR-24a, AC-18, R-17. Depends on C10 (`denyReason` in SendResult, `config.js` resolve).

## Purpose
When observe returns `403 {"error":"token lacks SessionWrite"}`: one stderr warning, stop further observe POSTs, never queue the frame, hook exit code and stdout unchanged. Other failures (including slug 403, 401, 5xx, connect) keep existing behavior.

## Design Basis
ADR-004 as amended by HD 10: persistent per-credential marker latch (hook = one process per event). In-process flag avoids re-reading the marker within one process.

## `observe-latch.js`
```
const MARKER = "observe-denied.json"
let denied = false                       // in-process latch (ADR-004)
let warned = false

/** Non-secret credential id: full SHA-256 hex of the credential (HD 10 cond. 2). Token never written. */
function credId(token) { return crypto.createHash("sha256").update(token).digest("hex") }

function isDenied(config) {
  if (denied) return true
  try {
    const m = JSON.parse(fs.readFileSync(path.join(config.stateDir, MARKER), "utf8"))
    if (m && m.cred === credId(config.token)) { denied = true; return true }
  } catch (_e) {}                        // missing/corrupt/unreadable marker = not denied (fail open toward retry)
  return false
}

function isSessionWriteDenial(res) { return res && res.status === 403 && res.denyReason === "token lacks SessionWrite" }

/** Called ONLY on 403 "token lacks SessionWrite". Warn once; persist marker 0600, no expiry (best effort, atomic). */
function latch(config, stderrWrite) {
  denied = true
  if (!warned) {
    warned = true
    stderrWrite("unimatrix: observe disabled for this credential (403, read-only token); hook events will not be recorded\n")
  }
  try { state.atomicWrite(path.join(config.stateDir, MARKER), JSON.stringify({ cred: credId(config.token), at: nowSecs() }), { mode: 0o600 }) } catch (_e) {}   // write failure: no change to exit/output
}
module.exports = { isDenied, isSessionWriteDenial, latch, MARKER }
```
No expiry (HD 10 cond. 3); `at` is informational only. A new credential has a different `credId` → fresh attempt. A corrupt marker is overwritten on the next denial. A read-write credential never matches a read token's marker. `state.atomicWrite` is the existing helper (export it if not already exported; add a `mode` option if absent, matching the credential file's 0600).

Only processes that themselves receive the 403 warn; later processes see the marker and stay silent — total one warning per credential per stateDir (AC-18). If the marker cannot be written, each process that hits 403 warns once and retries (fail open toward retry).

## `index.js` integration (HTTP mode only; UDS untouched)
In `main()` after `config.ok` and before `selectTransport`:
```
if (config.mode === "http" && observeLatch.isDenied(config)) return     // exit 0, no network, no queue, no stdout
```
`runFireAndForget`:
```
await queue.replay(config, guardedPost)          // guardedPost below; stop-at-first-failure semantics unchanged
const tasks = [guardedPost(config, request, { sync:false })] ...
carrying = settledSendResult(results[0])
if (observeLatch.isSessionWriteDenial(carrying)) {
  observeLatch.latch(config, s => process.stderr.write(s))
  // existing queued frames are left in place; the 24 h age prune removes them
  // do NOT enqueue, do NOT emit the generic "send failed, event queued" line
} else if (!carrying.ok) { ...existing enqueue + stderrLine... }
```
Delta send: if its result `isSessionWriteDenial`, latch the same way and skip the delta stderr line.
`runSync`: if `isSessionWriteDenial(res)` → latch, skip the existing `stderrLine(..."sync request failed")`, keep stdout empty (no injection), exit 0.

```
function guardedPost(config, frame, opts) {
  if (observeLatch.isDenied(config)) return Promise.resolve({ ok:false, status:403, contentType:null, body:null, failureClass:"auth", denyReason:"token lacks SessionWrite", latched:true })
  return transport.post(config, frame, opts).then(res => { if (observeLatch.isSessionWriteDenial(res)) observeLatch.latch(config, w); return res })
}
```
`queue.replay` stops at the first failure, so after a latch no further POST leaves the process.
`state.recordSendOutcomes` still receives results (breadcrumb shows `auth`); this is not a stderr line.

MCP bridge: not affected (bridge never posts observe).

## Error Handling
All latch I/O wrapped in try/catch; never throws into `main` (C-05 exit-0 contract).

## Key Test Scenarios
- Spawn real hook process with a read bundle against a real server, N = 3 events (FNF) + 1 sync (SubagentStart): exactly one warning line total on stderr; exit 0 each; stdout identical to no-remote run; capturing proxy sees exactly one observe POST (R-17.2).
- Queue: after the denial, `queueDepth` unchanged (frame not enqueued).
- Read-write bundle: observe flows, no warning, no marker (R-17.4).
- 500, connection refused, 401, slug 403 (`token not permitted for project`) → existing behavior, no latch (R-17.3).
- Credential change → marker ignored, one fresh attempt.
- Corrupt marker → exactly one retry, one 403, marker rewritten.
- Marker file mode 0600, contains no token.
- Unwritable stateDir → hook exit code and output unchanged; no crash.
- Warning contains no token, header value, or URL path.
