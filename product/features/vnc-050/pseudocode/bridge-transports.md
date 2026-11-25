# C10 Bridge / Transports — `mcp-bridge.js`, `mcp-bridge/http-session.js`, `transport-http.js`, `config.js` (+ `init.js` ping call)

ADR-006 (#5829), ADR-007 (#5817), FR-19..30. Depends on C8, C9. Each file stays ≤ 400 lines. Existing pin test files: zero diff (AC-05, R-11).

## Shared TLS Guard (new, in `cert-pin.js` is FORBIDDEN — keep cert-pin untouched; put in `lib/hook-client/tls-guard.js`, < 40 lines)
```
/** Throws if an unpinned TLS request carries any trust override (SR-01, R-01.4). */
function assertCaOptions(options) {
  // explicit true: ignores NODE_TLS_REJECT_UNAUTHORIZED=0 by design (ADR-006, Decision 11)
  if (options.rejectUnauthorized !== true) throw new Error("internal: unpinned request must set rejectUnauthorized: true")
  if ("checkServerIdentity" in options) throw new Error("internal: unpinned request must not override hostname verification")
  return options
}
/** Unpinned TLS: verified iff Node marked the socket authorized. */
function caVerifyError(socket) { return socket.authorized === true ? null : new Error("TLS certificate verification failed") }
function applyExtraHeaders(target, extraHeaders) {
  // copy caller headers FIRST; caller then sets reserved headers, which win (ADR-007 2nd line of defense)
  for (const [k, v] of extraHeaders || []) {
    if (RESERVED_HEADERS.has(k.toLowerCase())) continue     // parser already rejected; defense in depth
    target[k] = v
  }
  return target
}
module.exports = { assertCaOptions, caVerifyError, applyExtraHeaders }
```

## `mcp-bridge/http-session.js`
Constructor: add `extraHeaders` (default `[]`), store `this.extraHeaders`. `pinnedFp` may be `null` (CA branch).

`_headers(body, opts)`:
```
const headers = applyExtraHeaders({}, this.extraHeaders)
headers["Content-Type"] = ...; headers["Content-Length"] = ...; headers.Accept = ...; headers.Authorization = "Bearer " + this.token
(session id, protocol version as today)
return headers
```
`teardown()` DELETE headers: `applyExtraHeaders({}, this.extraHeaders)` then set `Mcp-Session-Id`, `Authorization` (as today).

`_options(method, headers)`:
```
const base = { protocol, hostname, port, path, method, headers, agent: false, timeout: this.idleMs }   // as today
if (this.pinnedFp) return applyCertPin(base, true, this.pinnedFp)      // UNCHANGED pinned branch
return assertCaOptions({ ...base, rejectUnauthorized: true })          // CA branch: explicit true, no ca, no checkServerIdentity
```
`_pinThenFlush(req, onPin, onMismatch, onConnect)` — one added branch; pinned path statements unchanged:
```
s.once("secureConnect", () => {
  if (onConnect) onConnect()
  let err
  if (this.pinnedFp) { try { err = verifyPeerFingerprint(s, this.pinnedFp) } catch { err = new Error("pinned certificate verification failed") } }   // unchanged
  else { err = caVerifyError(s) }
  if (err) { req.destroy(err); onMismatch(err); return }
  onPin()
})
```
`request()` `req.on("error")` — CA verification failures before secureConnect (Node emits `error` with codes like `UNABLE_TO_VERIFY_LEAF_SIGNATURE`, `DEPTH_ZERO_SELF_SIGNED_CERT`, `ERR_TLS_CERT_ALTNAME_INVALID`, `SELF_SIGNED_CERT_IN_CHAIN`, `CERT_HAS_EXPIRED`):
```
req.on("error", (err) => {
  if (!flushed && this.pinnedFp && err.code !== "ETIMEDOUT") return      // unchanged
  if (!flushed && !this.pinnedFp && isTlsVerifyCode(err.code)) {
    try { this.errOut("mcp-bridge: TLS certificate verification failed (" + err.code + ")\n") } catch {}
    this.exit(1)
  }
  settle(reject, err)
})
```
`isTlsVerifyCode(code)`: `typeof code === "string" && (code.startsWith("ERR_TLS_") || /CERT|SELF_SIGNED|UNABLE_TO_VERIFY/.test(code))`. Message carries code only — no token, no URL path.

SSE GET: the bridge issues no standalone SSE GET today (POST responses may stream `text/event-stream`). Every request the bridge makes goes through `_headers`/`teardown`, so headers cover all request types it sends. Gap flagged: ADR-007/FR-28 name "SSE GET"; if a GET stream is added later it must build headers via `_headers`.

## `mcp-bridge.js` `buildSession(projectHash, deps)`
```
const env = (deps && deps.env) || process.env
let r
try { r = resolveRemote(env, { projectHash }) }
catch (e) {
  if (e instanceof ResolveError) { errOut("mcp-bridge: " + e.message + "\n"); exit(1); return null }
  throw e
}
if (r.mcp === null) { errOut("mcp-bridge: no MCP credential (set UNIMATRIX_BUNDLE, UNIMATRIX_REMOTE_MCP_URL + UNIMATRIX_REMOTE_TOKEN, or run init --bundle)\n"); exit(1); return null }
if (!/^https:/.test(r.mcp.url) || !isNonEmptyString(r.mcp.token)) { errOut("mcp-bridge: credential missing https mcp_url or token\n"); exit(1); return null }
// REMOVED: refusal of null fingerprint (vnc-039 ADR-005 corrected by ADR-006) — null = CA trust
return HttpSession.create({ mcpUrl: r.mcp.url, token: r.mcp.token, pinnedFp: r.mcp.pinnedFp, extraHeaders: r.headers, exit, errOut })
```
`main(argv, deps)`: `projectHash` becomes optional when `UNIMATRIX_BUNDLE` or `UNIMATRIX_REMOTE_MCP_URL` is non-empty (FR-21: bundle alone, no init). Otherwise keep the existing usage error, exit 2. Gap flagged: architecture does not state the argv contract for headless launch; this is the minimal change satisfying FR-21.

## `transport-http.js` `post(config, frame, opts)`
`config` gains `extraHeaders` (array, default `[]`).
```
const headers = applyExtraHeaders({}, config.extraHeaders)
headers["Content-Type"] = ...; headers["Content-Length"] = ...; headers.Authorization = ...; headers.Accept = ...   // reserved set after
const pinned = isTls && !!config.pinnedFp                                   // unchanged
const caTls  = isTls && !config.pinnedFp
reqOptions = applyCertPin({...as today...}, isTls, config.pinnedFp)        // unchanged call
if (caTls) assertCaOptions(reqOptions)
req.on("socket", s => {
  if (pinned) { ...UNCHANGED... }
  else if (caTls) {
    s.once("secureConnect", () => {
      clearTimeout(connectTimer)
      const err = caVerifyError(s)
      if (err) { req.destroy(err); done({ok:false, status:0, contentType:null, body:null, failureClass:"connect", message: err.message}); return }
      if (!settled) req.end(body)                    // defer token-bearing body until verified (R-01)
    })
  } else { s.once("connect", () => clearTimeout(connectTimer)) }      // plain http unchanged
})
...
if (!pinned && !caTls) req.end(body)                  // plain http only flushes immediately
```
Behavior note: unpinned HTTPS previously flushed immediately (relying on Node buffering); deferral is strictly safer and invisible on success.

`classifyResponse`: add 403 deny detail without changing other fields:
```
if (status === 403) {
  let deny = null
  try { const o = JSON.parse(bodyBuf.toString("utf8")); if (o && typeof o.error === "string") deny = o.error } catch {}
  return { ok:false, status, contentType, body:null, failureClass:"auth", denyReason: deny }
}
```
(`denyReason` is the server's fixed error string; it never contains a token.)

`pingForInit(url, token, timeouts, pinnedFp, extraHeaders)`: pass `extraHeaders: extraHeaders || []` into `post` config. `init.js` caller: `parseExtraHeaders(process.env.UNIMATRIX_REMOTE_HEADERS)`; `ResolveError` → print message, exit 1 (init is the loud path).

## `config.js` `resolve(cwd)`
```
projectRoot/projectHash/stateDir as today
let r
try { r = resolveRemote(process.env, { projectHash }) }
catch (e) {
  if (!(e instanceof ResolveError)) return { ok:false, reason:"malformed", projectRoot, projectHash, stateDir }
  reason = e.code === "partial_pair" ? "partial_env" : e.code === "store_malformed" ? "malformed" : "misconfig"
  return { ok:false, reason, detail: e.message, projectRoot, projectHash, stateDir }     // never throws (module contract)
}
if (r.observe === null) return resolveUds(projectRoot, projectHash, stateDir)            // unchanged fall-through
source = { bundle: "bundle", env: "env", store: "file" }[r.observe.tier]   // keep existing "env"/"file" labels; add "bundle"
return okHttp(r.observe.url, r.observe.token, mergeTimeouts(r.observe.timeouts), source, projectRoot, projectHash, stateDir, r.observe.pinnedFp, r.headers)
```
`okHttp(..., pinnedFp, extraHeaders)` adds `extraHeaders: extraHeaders || []` to the config object. Env-path observe now honors `UNIMATRIX_REMOTE_FINGERPRINT` (pinned) and stays CA when absent (unchanged).
`index.js`: `describeReason("misconfig")` returns `config.detail` (token-free by construction); `classForReason("misconfig")` → `"auth"`. Hook keeps its exit-0 contract (C-05). Gap flagged: FR-20/FR-23 say "exit non-zero"; for the hook process this conflicts with the C-05 exit-0 guarantee and FR-24a "never fail the hook". Pseudocode: bridge exits 1; hook exits 0 with one stderr line naming the variable. Needs confirmation.

## Key Test Scenarios
- R-01: byte-capturing raw TLS server, untrusted cert and hostname mismatch (CA via NODE_EXTRA_CA_CERTS): zero app bytes for bridge initialize, observe POST, pingForInit; bridge exit 1, no token in stderr. `NODE_TLS_REJECT_UNAUTHORIZED=0` + untrusted cert on CA path → refused, zero app bytes. Guard throws when `rejectUnauthorized !== true` reaches unpinned path.
- AC-02: env MCP pair + local CA → initialize + tools/call succeed, no remote.json.
- AC-05/R-11: existing pin test files unchanged and green; bundle fp, env fp, stored fp each select pinned branch vs real self-signed server; mismatched fp refused with zero bytes.
- R-08: capturing proxy sees extra headers on initialize, POST, DELETE, observe POST, pingForInit; reserved names cannot be overridden even if parser bypassed (inject directly into `extraHeaders`).
- R-07: same env matrix through `mcp-bridge.js` and `config.js` → same outcome; `UNIMATRIX_BUNDLE` only → observe arrives at bundle `observe_url` with bundle trust.
- R-09: sentinel token + header values absent from all stdout/stderr across every error path.
- Stored `fingerprint:null` credential → bridge now attaches via CA (compat row: was error).
