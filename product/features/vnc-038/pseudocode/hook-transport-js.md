# Component 4 — Hook Transport (JS)

**File:** `packages/unimatrix/lib/hook-client/transport-http.js`
**ADR:** ADR-001 (#5080) · **AC:** AC-08 · **Risk:** R-01, R-12

## Purpose

Post observe telemetry to the bundle's `observe_url` VERBATIM. Delete the `/observe` append (C-3, `:84`) — the sole remaining client path-composition site. Every runtime hook event and the init Ping post to the server-composed observe URL byte-for-byte.

## The Closed-Set Deletion (C-3)

```
// BEFORE (transport-http.js:84) — the third and last compose site:
//   const pathName = u.pathname.replace(/\/+$/, "") + "/observe";
//   ... request uses { hostname: u.hostname, port: u.port, path: pathName }
// AFTER (ADR-001): config.url IS the finished observe URL. Post to it verbatim — append NOTHING.
```

## Modified Functions

### `post(config, frame, opts)` (MODIFY — verbatim observe URL)

```
function post(config, frame, opts):
    options = opts || {}
    // config.url is now the server-composed OBSERVE URL (observe_url), stored verbatim by init.js.
    try: u = new URL(config.url)
    catch: return fail("connect", 0)
    if u.protocol not in {http:, https:}: return fail("connect", 0)
    isTls = (u.protocol === "https:"); mod = isTls ? https : http

    body = options.bodyBuf || Buffer.from(JSON.stringify(frame), "utf8")
    if body.length > BODY_LIMIT_BYTES: return fail("http_4xx", 0)   // C-02 guard, no network write

    // DELETED: pathName = u.pathname + "/observe".
    // The request path is u.pathname (+ search if any) of the VERBATIM observe URL — no suffix.
    requestPath = u.pathname + u.search          // verbatim; do NOT mutate trailing slashes

    headers = { Content-Type: application/json, Content-Length: body.length,
                Authorization: "Bearer " + config.token,
                Accept: options.sync ? "text/plain" : "application/json" }
    // remaining request construction (cert pin on secureConnect, timeouts, single-settle) UNCHANGED
    request to { hostname: u.hostname, port: u.port, path: requestPath, method: POST, headers }
```

> The cert-pin / timeout / single-resolve machinery below `:84` is UNCHANGED. Only the path source changes: from `u.pathname + "/observe"` to `u.pathname` of the verbatim observe URL.

### `pingForInit(observeUrl, token, timeouts, pinnedFp)` (MODIFY — first arg is the observe URL)

```
function pingForInit(observeUrl, token, timeouts, pinnedFp):
    host = safeHost(observeUrl)
    res  = await post({ url: observeUrl, token, timeouts: timeouts||DEFAULT_TIMEOUTS, pinnedFp: pinnedFp||null },
                      { type: "Ping" }, { sync: true })
    // Ping/Pong validation UNCHANGED. The Ping now reaches /v1/{slug}/observe (AC-07).
    ... (unchanged: ok-check, JSON parse, Pong type-check, version extract)
```

> `config.url` is passed straight through to `post`, which posts it verbatim. Both the init Ping (AC-07) and every runtime hook (AC-08) therefore use the SAME server-composed `observe_url` — neither re-derives the route (R-12 sc.3).

## Data Flow

- IN (runtime): hook event `frame` + `config = { url: observe_url, token, pinnedFp, timeouts }` (read from `settings.local.json`).
- IN (init): `pingForInit(observe_url, token, ...)`.
- OUT: HTTP POST to the verbatim `observe_url` → server `/v1/{slug}/observe` → 200.

## Error Handling

- Unparseable/non-http(s) `config.url` → `fail("connect", 0)` (unchanged).
- Oversized body → `fail("http_4xx", 0)`, no network write (unchanged C-02 guard).
- All settle paths resolve exactly once; never rejects (unchanged).

## Key Test Scenarios (hints)

1. Closed-set invariant (R-01 sc.1): grep/AST assertion that `transport-http.js` contains NO `+ "/observe"` and no other path suffixing — composition set empty.
2. Verbatim post (R-01 sc.2): capture the outgoing request URL; assert string equality with `config.url` (the bundle's `observe_url`) — no trailing-slash mutation, no suffix.
3. AC-08: a runtime hook event posts to `observe_url`; the per-slug `/v1/{slug}/observe` returns 200 and resolves the bundle's project store.
4. R-12: assert init Ping AND runtime hook both target the SAME `observe_url` value (neither re-derives).
5. Cert pin still enforced on the verbatim URL (non-regression: body not flushed until fingerprint verified).
