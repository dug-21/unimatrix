# Component: transport-uds (`lib/hook-client/transport-uds.js`, NEW)

ADR-002, ADR-003, ADR-001 §2. FR-5..FR-11, AC-01, AC-03, AC-05.
Risks R-01, R-06, R-15, R-18. Merge step 3.
Oracle for the contract: `transport-http.js:5-7`. Byte authority for framing:
`wire.rs:16,349,372`.

## Purpose

A second transport behind the existing narrow seam. Exposes the same
`post(config, frame, opts) -> Promise<SendResult>` as transport-http.js, framing
byte-identical to wire.rs, with the ADR-003 socket lifecycle (flush-before-FIN for
FNF, half-close + accumulate read for sync) and the ADR-002 HookResponse→SendResult
mapping. Never rejects, no stdout/stderr, no retry, no new npm deps (Node `net`).

## Constants

```
const net = require("net");
MAX_PAYLOAD_SIZE = 1048576           // wire.rs:16 — byte-identical cap
FRAME_HEADER_SIZE = 4                // 4-byte BE u32 length prefix
TIMEOUT_MS = 40                      // ADR-002 §6 fixed parity (connect/sync/fnf all 40)
SYNC_ACCEPT_TYPES = { ContextSearch: true, CompactPayload: true }  // ADR-001 §2
```

## SendResult helpers (mirror transport-http.js fail()/classify shape)

```
FUNCTION fail(cls, status): RETURN { ok:false, status, contentType:null, body:null, failureClass:cls }
FUNCTION okResult(status, contentType, body): RETURN { ok:true, status, contentType, body, failureClass:null }
```

## Frame encoding — `encodeFrame(frame, opts) -> Buffer | null` (FR-6, AC-01, R-18)

```
FUNCTION encodeFrame(frame, opts):
  payloadObj = frame
  // ADR-001 §2 / ADR-002: inject accept ONLY for sync injection-bearing frames, at
  // serialization time. Never mutate the caller's frame (queue stays transport-agnostic).
  IF opts.sync AND SYNC_ACCEPT_TYPES[frame.type]:
      payloadObj = Object.assign({}, frame, { accept: "text/plain" })
  TRY:
      json = Buffer.from(JSON.stringify(payloadObj), "utf8")
  CATCH: RETURN null                              // unserializable → caller maps to http_4xx
  IF json.length > MAX_PAYLOAD_SIZE: RETURN null  // C-02-equivalent client-side reject, no write
  header = Buffer.alloc(4); header.writeUInt32BE(json.length, 0)
  RETURN Buffer.concat([header, json])
```

Note: `accept` is added to the SERIALIZED bytes only, never to the object the queue
stored (R-07 s5 — queued frames never carry `accept`).

## Response framing read — accumulate to declared length (FR-7, ADR-003 §3, R-06)

State for the sync read loop: `chunks=[]`, `received=0`, `declaredLen=null`.

```
ON 'data'(chunk):
  push chunk; received += chunk.length
  IF declaredLen === null AND received >= 4:
      buf = concat(chunks)
      declaredLen = buf.readUInt32BE(0)
      IF declaredLen === 0 OR declaredLen > MAX_PAYLOAD_SIZE:   // R-18 s2: reject BEFORE reading body
          done(fail("connect", 0)); destroy(); return          // protocol violation → connect class
  IF declaredLen !== null AND received >= 4 + declaredLen:
      body = concat(chunks).subarray(4, 4 + declaredLen)
      destroy()                                                  // got the one frame
      done(mapHookResponse(parseJson(body)))
```

`parseJson(body)` is wrapped: parse failure → `done(fail("connect", 0))` (malformed
response = protocol violation, mirrors transport-http malformed handling).

## HookResponse → SendResult — `mapHookResponse(obj)` (ADR-002 §2, normative)

```
FUNCTION mapHookResponse(obj):
  IF obj is null/not-object: RETURN fail("connect", 0)
  SWITCH obj.type:
    CASE "Text":            RETURN okResult(200, "text/plain", Buffer.from(obj.body || "", "utf8"))
    CASE "Ack":             RETURN okResult(204, null, null)        // sync empty injection → 204-equiv
    CASE "Pong":            RETURN okResult(200, "application/json", Buffer.from(JSON.stringify(obj), "utf8"))
    CASE "Error":
       cls = (obj.code >= 500) ? "http_5xx" : "http_4xx"
       RETURN fail(cls, obj.code)
    DEFAULT:                RETURN fail("connect", 0)              // unexpected variant → protocol violation
```

FNF success (no read) is mapped in the FNF path, not here: flushed → `okResult(0,
null, null)` (status 0, ADR-002 §2 — non-HTTP interpretation, breadcrumb consumers
must not assume HTTP).

## `post(config, frame, opts)` — top level (FR-5, never rejects)

```
FUNCTION post(config, frame, opts):
  options = opts || {}
  frameBuf = encodeFrame(frame, options)
  IF frameBuf === null:
      RETURN Promise.resolve(fail("http_4xx", 0))   // ADR-002 §2: client-side reject (>1MiB/unserializable)
  RETURN new Promise((resolve) => {
     settled = false; deadline = null
     done = (result) => {                            // settle-once (transport-http.js:98-104 pattern)
        IF settled: return
        settled = true; clearTimeout(deadline); resolve(result)
     }
     socket = null
     TRY: socket = net.connect(config.socketPath)
     CATCH: done(fail("connect", 0)); return         // synchronous connect throw (wrapped)

     deadline = setTimeout(() => { TRY socket.destroy(); done(fail("timeout", 0)) }, TIMEOUT_MS)
     IF deadline.unref: deadline.unref()             // never holds the event loop open

     connected = false
     socket.on("connect", () => { connected = true; <arm FNF or sync per options.sync> })
     socket.on("error", (err) => done(fail(classifyErrno(err), 0)))  // ENOENT/ECONNREFUSED/EACCES → connect
     // (per ADR-003 §1: error before 'connect' → connect failure; classifyErrno maps ETIMEDOUT→timeout else connect)

     IF options.sync: armSyncPath(socket) ELSE armFnfPath(socket)
  })
```

`classifyErrno(err)` mirrors transport-http.js:43-48 (`ETIMEDOUT`→"timeout", else
"connect"). ENOENT (dir absent), ECONNREFUSED (stale socket), EACCES (peer-cred) all
→ "connect" → enqueue path (edge cases enumerated in RISK-TEST-STRATEGY).

## FNF path — flush before FIN (FR-8, ADR-003 §2, R-01)

```
FUNCTION armFnfPath(socket):
  ON 'connect':
     socket.end(frameBuf)                  // write THEN FIN; Node flushes queued data to kernel before FIN
  ON 'finish':                             // all data handed to the OS — Rust write_all-equivalent guarantee
     done(okResult(0, null, null))         // FNF success → status 0 (ADR-002 §2)
     TRY socket.destroy()                  // cleanup AFTER settle only
  // NO read. NO data handler. destroy() is NEVER called before 'finish' (R-01 s3),
  // except on deadline expiry (→ timeout) handled by the shared deadline timer.
  // Server EPIPE writing its Ack to our FIN'd socket is its problem, DEBUG-classed (#3448).
```

`'error'` after `end()` (e.g. EPIPE locally) is once-guarded by `done`; the deadline
classification wins if it fired first.

## Sync path — half-close + accumulate (FR-7, ADR-003 §3, R-06)

```
FUNCTION armSyncPath(socket):
  ON 'connect':
     socket.end(frameBuf)                  // half-close: client→server FIN after flush; socket stays readable
  install the 'data' read loop above (accumulate to declaredLen, then destroy + done)
  ON 'end':                                // server closed before a complete frame
     IF NOT settled: done(fail("connect", 0))   // truncated/short response → connect class, no stdout (R-06 s2)
  // deadline expiry mid-read → shared timer destroys + done(timeout) (R-06 s4)
```

## State machine (both paths)

```
[connecting] --connect--> [FNF: ending] --finish--> [settled okResult(0)]
[connecting] --connect--> [sync: ending] --data...--> [reading] --full frame--> [settled mapHookResponse]
ANY          --error-->   [settled fail(connect|timeout)]
ANY          --deadline-> [destroy + settled fail(timeout)]
[reading]    --end before full--> [settled fail(connect)]
[reading]    --bad declaredLen--> [destroy + settled fail(connect)]   (0 or >1MiB)
```

Settle-once invariant: exactly one `done(...)` per `post()`. Every FNF resolution
ends in exactly one of {okResult(0) delivered-complete, fail→enqueued} — no third
state (R-01 coverage).

## Exit/loop discipline (ADR-003 §5, R-06 s5)

- NO `process.exit()` anywhere in this module (grep-gate, #4768 pattern).
- ALL timers `unref()`d.
- The pending promise holds the event loop open until settle; index.js `await`s it,
  so the loop cannot drain before the socket settles (sync stdout flush ordering,
  R-06 s6).

## Module exports

```
module.exports = { post, MAX_PAYLOAD_SIZE, TIMEOUT_MS }
```

(`encodeFrame`, `mapHookResponse` exported too if unit tests need them directly —
parity-corpus framing fixtures compare `encodeFrame` output against wire.rs goldens.)

## Error handling summary (NFR-3 fail-open)

| Condition | Resolution |
|-----------|-----------|
| connect throws / ENOENT / ECONNREFUSED / EACCES | `fail("connect", 0)` |
| deadline (40 ms) | `destroy()` + `fail("timeout", 0)` |
| payload > 1 MiB or unserializable | `fail("http_4xx", 0)`, never written |
| response declared len 0 / >1 MiB / malformed JSON / `'end'` early | `fail("connect", 0)` |
| FNF flushed | `okResult(0, null, null)` |
| `Text`/`Ack`/`Pong`/`Error` | per ADR-002 §2 table |

Never throws to caller; never writes stdout/stderr.

## Key test scenarios (hints for tester)

1. Framing byte-identical to wire.rs: `encodeFrame` output vs committed Rust-generated
   fixtures; write rejects >1 MiB; exactly 1,048,576-byte payload accepted — AC-01,
   R-18 s1/s3.
2. Read rejects declared length 0, 1 MiB+1, 0xFFFFFFFF BEFORE allocating body —
   R-18 s2, R-06 s3.
3. Chunked delivery: 1-byte chunks and split-header chunks accumulate to full frame —
   R-06 s1.
4. `'end'` before complete frame → `connect`, no stdout, exit 0 — R-06 s2.
5. Deadline mid-read → `destroy()` + `timeout`, no partial stdout — R-06 s4.
6. FNF: `destroy()` never called before `'finish'` (instrument socket, assert order);
   live-listener 1 MiB FNF frame recorded complete; flush-timeout → `ok:false`
   (enqueued) — R-01 s1/s3/s4.
7. SendResult mapping: every ADR-002 §2 row unit-tested (transform/state/queue key
   off it) — integration risk table.
8. Settle-once: timer cleared on every path; all timers `unref()`d; grep proves no
   `process.exit()` in the module — R-06 s5.
9. Latency p95 < 20 ms over the live local listener (F3 AC-13 protocol); FNF and sync
   measured separately — AC-05, R-15.
