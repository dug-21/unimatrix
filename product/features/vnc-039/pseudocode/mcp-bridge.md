# C2 — `lib/hook-client/mcp-bridge.js` (NEW, Scope A)

**Purpose.** A pure-Node-stdlib, single-session stdio↔Streamable-HTTP bridge. Reads newline-delimited
JSON-RPC on stdin, holds a fingerprint-PINNED HTTPS connection to `mcp_url`, captures+replays the
server-minted `Mcp-Session-Id`, dispatches `application/json` and (contingent) `text/event-stream`
responses, runs the MCP lifecycle, and writes JSON-RPC responses to stdout. FAIL-LOUD on pin
mismatch. Sources: ADR-001, ADR-002 (argv), ADR-004 (store read). Risks: R-01,R-02,R-04,R-05,R-16,R-17.

**Zero runtime deps (AC-02).** stdlib only: `https`, `http`(unused for prod path), `crypto`, `os`,
`process` stdin/stdout/stderr. Reuses `cert-pin.js` + `credstore.js`. NO `@modelcontextprotocol/sdk`.

**500-line modularity.** Decompose into the five ADR-001 units as separate files under
`lib/hook-client/mcp-bridge/` re-exported by `mcp-bridge.js`, so no file exceeds 500 lines and each
unit is independently testable (SR-01):
```
mcp-bridge.js          -- entry: argv parse, store read, wiring, run()              (~60)
mcp-bridge/stdio-frame.js   -- newline-delimited JSON-RPC read/write                (~80, R-16)
mcp-bridge/http-session.js  -- pinned POST + Mcp-Session-Id capture/replay          (~120, R-05,R-17,R-01,R-02)
mcp-bridge/sse-parse.js     -- text/event-stream parser (CONTINGENT — R-04)         (~90)
mcp-bridge/dispatch.js      -- route by Content-Type, id-correlate, write stdout    (~80, R-04)
mcp-bridge/lifecycle.js     -- initialize -> tools/list/tools/call; clientInfo.name (~80, R-17)
```

---

## VERIFIED wire constants (rmcp 1.7.0 — see OVERVIEW "VERIFIED WIRE VALUES"; CONFIRM LIVE)

```
SESSION_HEADER       = "Mcp-Session-Id"     // common/http_header.rs:1; Node lowercases on read
ACCEPT_VALUE         = "application/json, text/event-stream"   // BOTH required else 406 (tower.rs:966-978)
CONTENT_TYPE_REQUEST = "application/json"    // else 415 (tower.rs:981-996)
CLIENT_INFO_NAME     = "unimatrix-mcp-bridge"  // STABLE, fixed (FR-03a/AC-12); CONFIRM accepted live
BODY_LIMIT_BYTES     = 1048576               // reuse transport-http.js:23 (1 MiB guard)
```
> IMPLEMENTER: every one of these is a wire value owned by rmcp, NOT by unimatrix-server. Confirm
> against the LIVE `/v1/{slug}` endpoint at the start of delivery (DELIVERY CHECKPOINT) — the
> session-id handshake FIRST. The current server config (`StreamableHttpServerConfig::default()`
> + only `allowed_origins`) is stateful_mode=true / json_response=false, so responses are SSE:
> `sse-parse` is REQUIRED unless the live probe shows JSON-direct.

---

## Entry: `mcp-bridge.js`

```
function main(argv = process.argv):
    projectHash = argv[2]
    if not non-empty string projectHash:
        stderr.write("usage: unimatrix mcp-bridge <projectHash>\n"); exit(2)

    // Step 1: load credential from the out-of-tree store (FR-21, ADR-004)
    try:
        cred = credstore.read(projectHash)
    catch e:
        stderr.write("mcp-bridge: " + e.message + "\n"); exit(1)   // malformed/unknown version -> LOUD (R-13)
    if cred === null:
        stderr.write("mcp-bridge: no credential for project " + projectHash + " (run init --bundle)\n")
        exit(1)                                                     // ENOENT -> LOUD (bridge is not fail-open, R-13)

    // Step 2: validate the fields the BRIDGE owns (mcp_url, token, fingerprint)
    if not https mcp_url / not token:
        stderr.write("mcp-bridge: credential missing mcp_url or token\n"); exit(1)
    // fingerprint MUST be present+non-null for the bridge (cloud MCP is bundle-only; legacy is never
    // wired as a bridge — AC-10). If fingerprint is null here, the store entry is legacy:
    if cred.fingerprint is null:
        stderr.write("mcp-bridge: this credential has no pinned fingerprint (cloud MCP requires a v:2 bundle)\n")
        exit(1)                                                     // never run unpinned (NFR-06, ADR-005)

    session = HttpSession.create({ mcpUrl: cred.mcp_url, token: cred.token, pinnedFp: cred.fingerprint })
    run(session)
```

```
function run(session):
    framer = StdioFramer(process.stdin, process.stdout)
    lifecycle = Lifecycle(session)
    framer.onMessage(async (jsonRpcMsg) =>
        responseMsg = await lifecycle.handle(jsonRpcMsg)   // may be null for notifications
        if responseMsg !== null: framer.write(responseMsg)
    )
    process.stdin.on("end", async () =>
        await session.teardown()        // best-effort DELETE with Mcp-Session-Id (ARCHITECTURE §3.3)
        process.exit(0)
    )
    // fail-loud paths inside session.request() exit non-zero directly (pin mismatch, R-01)
```

---

## Unit: `stdio-frame.js` (R-16)

Newline-delimited JSON-RPC; one message per line; buffer partial lines across chunks.

```
class StdioFramer(stdin, stdout):
    buffer = ""
    onMessage(cb): this.cb = cb
    // read side
    stdin.setEncoding("utf8")
    stdin.on("data", chunk =>
        buffer += chunk
        loop while buffer contains "\n":
            idx = buffer.indexOf("\n")
            line = buffer.slice(0, idx)        // exclude the newline
            buffer = buffer.slice(idx + 1)
            if line.trim() is empty: continue   // tolerate blank lines
            try: msg = JSON.parse(line)
            catch: write a JSON-RPC parse-error to stdout (id:null, code:-32700); continue
            this.cb(msg)
    )
    // write side
    write(obj): stdout.write(JSON.stringify(obj) + "\n")     // newline-framed (R-16 write invariant)
```

Invariants (R-16): byte-split-invariant on read (a message split across N chunks parses once);
N messages in one chunk parse as N in order; a chunk boundary exactly on `\n` yields no empty/dropped
message (the `line.trim() empty` guard handles trailing `\n`).

---

## Unit: `http-session.js` (R-05, R-17, R-01, R-02 — HIGH risk)

Owns the pinned POST, session-id capture/replay, per-socket re-pin, fail-loud-on-mismatch. This is
the bridge's hardest seam (DELIVERY CHECKPOINT).

```
class HttpSession.create({ mcpUrl, token, pinnedFp }):
    this.url = new URL(mcpUrl)              // POSTed VERBATIM — compose nothing (AC-05/FR-06, dumb-client)
    this.token = token
    this.pinnedFp = pinnedFp
    this.sessionId = null                   // captured from initialize response (server-minted)
    this.protocolVersion = null             // captured from initialize result; echoed on later reqs (G1)
```

### request(jsonRpcBody, { isInitialize }) -> Promise<{ status, contentType, stream|body }>

```
function request(bodyObj, opts):
    body = Buffer.from(JSON.stringify(bodyObj), "utf8")
    if body.length > BODY_LIMIT_BYTES: fail loud / reject (client-side guard)

    headers = {
        "Content-Type": CONTENT_TYPE_REQUEST,        // application/json (else 415)
        "Content-Length": body.length,
        "Accept": ACCEPT_VALUE,                       // BOTH json+event-stream (else 406)
        "Authorization": "Bearer " + this.token,      // flushed ONLY after pin (below)
    }
    if this.sessionId !== null:
        headers[SESSION_HEADER] = this.sessionId      // replay verbatim on every post-initialize req (FR-03)
    if this.protocolVersion !== null and not opts.isInitialize:
        headers["MCP-Protocol-Version"] = this.protocolVersion   // CONFIRM exact value live (G1)

    // PINNED-FLUSH (reuse transport-http.js:150-176 pattern verbatim — ADR-001 §3.2)
    reqOptions = applyCertPin({
        protocol: "https:", hostname: stripIPv6Brackets(url.hostname), port: url.port || undefined,
        path: url.pathname + url.search,              // VERBATIM; no append (AC-05)
        method: "POST", headers,
        agent: false,                                 // NO pool: each request a fresh socket; never
                                                      // flush on an unverified pooled socket (R-02)
    }, /*isTls*/ true, this.pinnedFp)

    return new Promise((resolve, reject) =>
        req = https.request(reqOptions)
        // PER-SOCKET RE-PIN: verify the leaf the instant THIS socket's handshake completes,
        // BEFORE any body byte. Every new socket re-runs verifyPeerFingerprint (R-02).
        req.on("socket", s =>
            s.once("secureConnect", () =>
                err = verifyPeerFingerprint(s, this.pinnedFp)     // cert-pin.js:67 (reused)
                if err:
                    req.destroy(err)                              // socket killed BEFORE body flush (R-01)
                    // FAIL-LOUD: diagnosable expected-vs-presented (err.message is token-free), exit non-zero
                    stderr.write("mcp-bridge: " + err.message + "\n")
                    process.exit(1)                               // persistent bridge fail-closed (ADR-001)
                else:
                    req.end(body)                                 // pin OK -> NOW flush token-bearing body
            )
        )
        req.on("response", res =>
            // CAPTURE Mcp-Session-Id from response headers (server-minted) on initialize (FR-02)
            if opts.isInitialize and res.headers[lower(SESSION_HEADER)] is set:
                this.sessionId = res.headers[lower(SESSION_HEADER)]   // STABLE for process life (R-17)
            contentType = res.headers["content-type"] || ""
            // Return the raw response to dispatch; bound the buffered bytes at BODY_LIMIT_BYTES
            resolve({ status: res.statusCode, contentType, res })
        )
        req.on("error", err => reject(err))    // connect/timeout class -> surfaced as JSON-RPC error by caller
        // NOTE: req.end(body) is NOT called here — only inside secureConnect after pin match.
    )
```

Critical invariants:
- **R-01 (token never before pin):** `req.end(body)` lives ONLY in the `secureConnect` success
  branch. On mismatch `req.destroy(err)` runs first → the body (with `Authorization: Bearer`) is
  never written. A capturing wrong-pin server MUST observe zero `Authorization` and zero body.
- **R-02 (per-socket re-pin):** `agent:false` forces a fresh socket per request; each runs
  `verifyPeerFingerprint` on its own `secureConnect`. NO `https.Agent({keepAlive})` that could
  dispatch on an unverified socket. (Keep-alive reuse is permitted ONLY on an already-pinned socket;
  the simplest correct implementation is no pooling — fresh socket per request — matching the
  observe path's `agent:false`.)
- **R-17 (stable session id):** `this.sessionId` is set exactly once (on initialize) and replayed
  verbatim; never regenerated, never minted client-side on a follow-up. The bridge never invents a
  session id (server-minted only — finding 2).
- **NFR-06:** no token in any stderr/exit message, including the mismatch path.

### teardown()

```
function teardown():
    if this.sessionId === null: return
    best-effort: send a DELETE to this.url with headers { [SESSION_HEADER]: this.sessionId } over a
    pinned socket (same secureConnect re-pin); swallow all errors (non-fatal). CONFIRM DELETE shape live (G2).
```

---

## Unit: `dispatch.js` (R-04)

Route by response `Content-Type`. Always handle `application/json`; handle `text/event-stream` via
`sse-parse` (the SSE branch is the contingent unit). Correlate by JSON-RPC `id`. Bound at 1 MiB.

```
async function dispatchResponse({ status, contentType, res }) -> jsonRpcMessage[]:
    if contentType starts with "application/json":
        body = await readBounded(res, BODY_LIMIT_BYTES)      // 4xx/5xx body too
        if status >= 400:
            return [ jsonRpcError(idFromBodyOrNull(body), httpStatusToJsonRpc(status), body) ]
        return [ JSON.parse(body) ]                          // single JSON-RPC object
    else if contentType starts with "text/event-stream":
        return await SseParser.collect(res, BODY_LIMIT_BYTES) // -> array of JSON-RPC objects (R-04)
    else:
        // unexpected content type -> JSON-RPC error on stdout, do not crash
        drain+discard res
        return [ jsonRpcError(null, -32603, "unexpected content-type: " + contentType) ]
```

`readBounded(res, limit)`: accumulate `res` data chunks; if total exceeds `limit`, `res.destroy()`
and return what was buffered (no hang) — same posture as `transport-http.js:192-200`.

---

## Unit: `sse-parse.js` (R-04 — CONTINGENT; built unless live probe shows JSON-direct)

Parse `text/event-stream`: `event:` / `data:` / `id:` lines, blank-line record boundary, multi-line
`data:` concatenation (`\n`-join), tolerate a leading priming/empty event and keep-alive comment
(`:` ) lines. rmcp wraps the JSON-RPC response in ONE SSE `data:` event (tower.rs:1140-1141) and may
emit a priming event when `sse_retry` is set (server default → priming IS emitted, G2).

```
class SseParser:
    static async collect(res, limit) -> jsonRpcMessage[]:
        messages = []
        // accumulate text, split into records on blank line, byte-split-invariant (R-04 scenario 5)
        buffer = ""; total = 0
        for await chunk of res:
            total += chunk.length; if total > limit: res.destroy(); break
            buffer += chunk.toString("utf8")
            // normalize CRLF -> LF (R-04 scenario 6); split complete records (terminated by blank line)
            while buffer contains a record terminator ("\n\n" after CRLF-normalize):
                record = take up to terminator; remainder stays in buffer
                ev = parseRecord(record)
                if ev.dataPayload is non-empty:
                    try: messages.push(JSON.parse(ev.dataPayload))
                    catch: /* skip priming/non-JSON events (e.g. retry-only) */
        // flush any trailing complete record without terminator at stream end
        return messages

    static parseRecord(record) -> { event?, id?, dataPayload }:
        dataLines = []
        for each line in record.split("\n"):
            if line starts with ":" -> ignore (comment / keep-alive)
            else if line starts with "data:" -> dataLines.push(line.slice(5).replace(/^ /,""))
            else if line starts with "event:" -> event = trimmed value
            else if line starts with "id:" -> id = trimmed value   // -> Last-Event-ID if resuming
            // ignore other fields
        return { event, id, dataPayload: dataLines.join("\n") }     // RFC concatenation
```

R-04 coverage targets: single-line, multi-line, multi-event-in-stream, chunk-split-at-every-offset
invariance, CRLF vs LF, 1 MiB guard. Each is a `sse-parse` unit test with a golden corpus.

---

## Unit: `lifecycle.js` (R-17)

Drives `initialize` -> capture session/protocol -> proxy `tools/list`/`tools/call`. The bridge is a
transparent proxy: it forwards the client's JSON-RPC method/params to `mcp_url` and returns the
result. The ONLY synthesized field is a stable `clientInfo.name` on `initialize` IF the client did
not already set one (the bridge advertises the cloud identity; CONFIRM live whether to override or
pass through — G3).

```
class Lifecycle(session):
    async handle(msg) -> jsonRpcMessage | null:
        isInit = (msg.method === "initialize")
        if isInit:
            // ensure STABLE clientInfo.name attribution (FR-03a/AC-12). Pass the client's request
            // through but pin a stable clientInfo.name (server keys attribution on it):
            msg.params.clientInfo = msg.params.clientInfo || {}
            msg.params.clientInfo.name = CLIENT_INFO_NAME      // fixed; never per-spawn random (R-17)
        try:
            { status, contentType, res } = await session.request(msg, { isInitialize: isInit })
        catch err:
            // connect/timeout/pin-class already exited loud for pin; transport errors -> JSON-RPC error
            return jsonRpcError(msg.id ?? null, -32603, classify(err))
        out = await dispatchResponse({ status, contentType, res })
        if isInit:
            this.protocolVersion = extractProtocolVersion(out)   // for MCP-Protocol-Version echo (G1)
            session.protocolVersion = this.protocolVersion
        // a request expects exactly one correlated response; notifications (no id) expect none
        if msg.id === undefined or msg.id === null: return null   // notification -> no stdout write
        return correlateById(out, msg.id) ?? out[0] ?? jsonRpcError(msg.id, -32603, "empty response")
```

Stability contract (R-17, AC-12): `CLIENT_INFO_NAME` is a module constant; `session.sessionId` is
captured once and replayed verbatim. A test asserts both are byte-identical across
`initialize → tools/list → tools/call` within one process, and distinct sessions/projects do not
share a global mutable identity.

---

## State machine (process lifetime)

```
START -> argv parse -> credstore.read
   ENOENT/malformed -> exit non-zero (LOUD)
   ok -> CREATED (no session yet)
CREATED --(first stdin msg = initialize)--> request(initialize)
   secureConnect: pin mismatch -> destroy + exit 1 (LOUD, token never flushed)
   pin ok -> capture Mcp-Session-Id + protocolVersion -> READY
READY --(tools/list, tools/call, …)--> request(replay session-id) -> dispatch -> stdout
   (each request re-pins its own socket; mismatch -> exit 1)
READY --(stdin EOF)--> teardown() best-effort DELETE -> exit 0
```

## Error handling (ARCHITECTURE §9 — bridge is FAIL-LOUD)

| Origin | Behavior |
|--------|----------|
| store ENOENT | exit non-zero "no credential for project" |
| store malformed / unknown schema_version | exit non-zero diagnosable |
| credential fingerprint null (legacy) | exit non-zero "cloud MCP requires a v:2 bundle" |
| cert-pin mismatch (any socket) | destroy socket before body; stderr expected-vs-presented; exit non-zero |
| HTTP 4xx/5xx | surface as JSON-RPC error on stdout (per-request, not fatal) |
| malformed/oversized response | 1 MiB guard; JSON-RPC error on stdout; no hang |
| stdin parse error | JSON-RPC parse-error (-32700) on stdout; continue |

## Key test scenarios (hints; full plan in test-plan/mcp-bridge.md)

- **R-01 live good-pin/wrong-pin** (the #4970 recipe): real `https.createServer` self-signed leaf;
  good-pin lifecycle round-trips; wrong-pin → capturing server received NO `Authorization`/body,
  loud expected-vs-presented stderr, non-zero exit; negative-control (no-op pin → token WOULD leak).
- **R-02 per-socket re-pin**: each opened socket runs `verifyPeerFingerprint` before its first byte;
  no `https.Agent` pool; mid-session cert swap on socket #2 → rejected, no body flushed.
- **R-05 capture/replay**: `Mcp-Session-Id` captured from initialize response headers; replayed on
  `tools/list`/`tools/call`; absent-on-initialize → defined posture (pin to observed rmcp); distinct
  from tool-param session_id; teardown DELETE.
- **R-17 stable identity**: session-id + `clientInfo.name` byte-identical across requests in one
  process; distinct across two project sessions; never per-request minted.
- **R-04 sse-parse**: single/multi-line/multi-event/chunk-split-invariant/CRLF/1MiB (only if probe
  shows SSE — which the source analysis says it does).
- **R-16 stdio-frame**: split-message, multi-message-per-chunk, boundary-on-newline.
- **AC-05 verbatim**: request URL === `mcp_url` exactly for every proxied request.
- **AC-02 zero-dep**: no `@modelcontextprotocol/sdk`/`mcp-remote` import; `package.json` deps empty.
- **Live (#774 merged)**: full lifecycle against real `/v1/{slug}`; session-id handshake FIRST.
