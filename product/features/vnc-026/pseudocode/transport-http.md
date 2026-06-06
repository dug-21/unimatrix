# transport-http.js — HTTP POST to {url}/observe

## Purpose
The only network module. `post(config, frame, opts) -> Promise<SendResult>` using Node
built-in `http`/`https`. Bearer auth, content negotiation, ADR-005 timeouts, response
classification for the breadcrumb. NEVER throws and NEVER rejects — always resolves a
SendResult (callers' `Promise.allSettled` independence then degenerates to values; the
synthetic-rejection arm in index.md is belt-and-braces).

## Functions

### post(config, frame, opts) -> Promise<SendResult>
`opts = { sync: boolean, bodyBuf?: Buffer }` — `bodyBuf` lets delta.js pass its already
size-checked serialization; otherwise `JSON.stringify(frame)`.
```
async function post(config, frame, opts):
  try: u = new URL(config.url) catch: return fail("connect", 0)
  if u.protocol not in {"http:", "https:"}: return fail("connect", 0)
  mod = (u.protocol === "https:") ? require("https") : require("http")

  body = opts.bodyBuf ?? Buffer.from(JSON.stringify(frame), "utf8")
  if body.length > 1_048_576: return fail("http_4xx", 0)   // client-side 1 MiB guard (C-02);
                                                           // delta.js pre-checks, this is the backstop
  pathName = u.pathname.replace(/\/+$/, "") + "/observe"   // path-prefix + trailing-slash support
  headers = { "Content-Type": "application/json",
              "Content-Length": body.length,
              "Authorization": "Bearer " + config.token,
              "Accept": opts.sync ? "text/plain" : "application/json" }
  totalMs = opts.sync ? config.timeouts.syncMs : config.timeouts.fnfMs

  return new Promise(resolve => {           // resolve exactly once (guard flag)
    done = once(resolve)
    req = mod.request({ protocol:u.protocol, hostname:u.hostname, port:u.port || undefined,
                        path: pathName + u.search, method:"POST", headers })
    // connect deadline (750 ms): armed now, cleared on socket connect
    connectTimer = setTimeout(() => { req.destroy(); done(fail("connect", 0)) },
                              config.timeouts.connectMs)
    req.on("socket", s => s.once(u.protocol==="https:" ? "secureConnect" : "connect",
                                 () => clearTimeout(connectTimer)))
    // total deadline
    totalTimer = setTimeout(() => { req.destroy(); done(fail("timeout", 0)) }, totalMs)
    req.on("error", err => done(fail(classifyErrno(err), 0)))     // also fires after destroy — once-guarded
    req.on("response", res => {
      chunks = []; cap = 1_048_576
      res.on("data", c => { if (sum(chunks) < cap) chunks.push(c) else res.destroy() })
      res.on("end", () => done(classifyResponse(res.statusCode,
                                res.headers["content-type"] ?? null, Buffer.concat(chunks))))
      res.on("error", () => done(fail("connect", 0)))
    })
    req.end(body)
    // cleanup: done() clears both timers (timers must not hold the event loop open:
    // use .unref() on both so a completed spawn exits promptly)
  })

function classifyResponse(status, contentType, bodyBuf):
  if status >= 200 and status < 300:
    return { ok:true, status, contentType, body:bodyBuf, failureClass:null }
  cls = (status === 401 or status === 403) ? "auth"
      : (status >= 400 and status < 500)   ? "http_4xx"
      : (status >= 500)                    ? "http_5xx" : "connect"
  return { ok:false, status, contentType, body:null, failureClass:cls }

function classifyErrno(err):
  code = err && err.code
  if code in {"ETIMEDOUT"}: return "timeout"
  return "connect"      // ECONNREFUSED, ENOTFOUND, ECONNRESET, EAI_AGAIN, EPIPE, TLS errors…
function fail(cls, status): return { ok:false, status, contentType:null, body:null, failureClass:cls }
```

### pingForInit(url, token, timeoutsOpt) -> Promise<{ok, message}>
Used by init-remote (FR-19/R-18). Strict Pong validation — the ONE loud path.
```
async function pingForInit(url, token, timeouts = DEFAULTS):
  res = await post({url, token, timeouts}, {type:"Ping"}, {sync:true})
  if not res.ok:
    return { ok:false, message: actionable(res.failureClass, res.status) }
    // auth → "token rejected (HTTP 401/403) — check --token"
    // connect/timeout → "cannot reach {host} — check --remote URL"; 4xx/5xx → status text
  try: obj = JSON.parse(res.body.toString("utf8")) catch:
    return { ok:false, message:"server returned a non-JSON Ping response" }
  if obj?.type !== "Pong": return { ok:false, message:"unexpected response type: " + obj?.type }
  return { ok:true, message:"Pong from " + host + " (server " + (obj.server_version ?? "?") + ")" }
```

## Error Handling
- Resolves SendResult on every path; the `once` guard prevents double-settle from
  `destroy()`-triggered error events after a timeout classification.
- No retries anywhere (queue/offset re-drive are the retry mechanisms).
- Never logs the token, full URL, or body content (R-16): the module emits NO stderr —
  classification strings only, callers compose messages.
- Timers `.unref()`ed so the per-event process never lingers on a dead deadline.

## Key Test Scenarios
1. Header matrix per request type: Bearer present, Content-Type, Accept text/plain on
   sync vs application/json on FNF (the #4703 canary).
2. Failure matrix → classes: ECONNREFUSED→connect, ENOTFOUND→connect, stalled
   connect→connect at 750 ms, stalled response→timeout at sync 2000 / FNF 3000 ms,
   401/403→auth, 404/413→http_4xx, 500→http_5xx.
3. 2xx incl. 204 → ok with empty body; oversized response body capped without hang.
4. URL forms: trailing slash, path prefix `/base` → `/base/observe`, port, IPv6 literal,
   http vs https; invalid URL → connect class, no throw.
5. >1 MiB request body → rejected client-side, no network write.
6. Strict Pong: 200 JSON non-Pong → init failure; 200 with wrong token → auth message
   (proves Bearer is exercised, R-18).
7. Sync expiry timing test: spawn returns ≈ deadline, exit 0, no stdout (no hang).
