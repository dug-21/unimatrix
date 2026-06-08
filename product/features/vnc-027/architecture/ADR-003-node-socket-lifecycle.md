## ADR-003: Node UDS Socket Lifecycle Contract — Flush Before FIN, Never Destroy Unflushed, No process.exit

### Context

SR-01: Rust `fire_and_forget` uses blocking `write_all` (data reaches the kernel
buffer before the call returns) then drops the stream — frames are never silently
lost. Node `socket.write()` is asynchronous with a user-space buffer, and
`socket.destroy()` discards unflushed data; a naive write-then-destroy FNF path
loses frames invisibly (fail-open masks the loss). SR-05: the sync path in a
short-lived async process risks partial frame reads and premature exit before
drain. Unimatrix #3448 documents the expected server-side noise (early-EOF,
broken-pipe) this protocol legitimately produces.

### Decision

Normative lifecycle for `transport-uds.js` (both paths share connect/teardown):

1. **Connect**: `net.connect(socketPath)`; arm a single overall deadline timer
   (40 ms, ADR-002), `unref()`d so it can never hold the event loop open.
   `error` before `connect` event → resolve `connect` failure.
2. **FNF**: on `connect`, call `socket.end(frameBuffer)` — write then FIN; Node
   guarantees queued data is flushed to the kernel before FIN. Resolve success on
   the `'finish'` event (all data handed to the OS — the same guarantee Rust
   `write_all` gives). **`destroy()` is never called before `'finish'`**; it is
   only called (a) on deadline expiry — resolve `timeout` failure — or (b) after
   settle, as cleanup. No response read ever (parity: `fire_and_forget`). The
   server's EPIPE writing its Ack to the FIN'd socket is expected and already
   DEBUG-classified (listener.rs:502-513, #3448).
3. **Sync**: on `connect`, `socket.end(frameBuffer)` (half-close: client→server FIN
   after flush; the Unix socket stays readable — the listener reads exactly one
   frame then writes, so the FIN is harmless and signals exactly-one-request).
   Read loop: accumulate `'data'` chunks into a buffer; once ≥4 bytes, parse the
   BE u32 length — reject 0 or >1 MiB (resolve `http_5xx`-class reject? No:
   protocol violation → `connect` class, mirroring transport-http's malformed-
   response handling); once `4+len` bytes arrive, `destroy()` the socket, parse
   JSON, map per ADR-002. `'end'` before a complete frame → failure (`connect`
   class). Deadline expiry mid-read → `destroy()` + `timeout`.
4. **Enqueue rule (parity with hook.rs)**: enqueue happens ONLY on connect-failure
   results, exactly as today (index.js enqueues when `!carrying.ok` — and a
   post-connect write/flush timeout also yields `ok:false`, which enqueues; this
   matches AC-04 and risks at most a duplicate frame, never a silent loss —
   duplicates are the existing at-least-once queue semantic).
5. **Exit sequencing**: index.js's existing rule stands — no `process.exit()`
   anywhere; `main()` awaits the transport promise; all transport timers are
   `unref()`d; settle-once guard (`done()` pattern from transport-http.js:98-104)
   clears timers on every path. The process exits when the event loop drains,
   which cannot happen before the socket has settled, because the pending promise
   chain holds the loop.
6. **Truncation detection (test contract)**: the parity suite must include a
   server-side assertion that FNF frames arrive complete — a live-listener test
   that sends a max-size (1 MiB) FNF frame and asserts the daemon recorded the
   event, plus a kill-mid-write case asserting either full delivery or a clean
   server-side frame error (never a silently truncated accepted event). The
   listener's `read_exact(len)` already rejects short frames, so corruption
   cannot be silently ingested; the test proves the client side flushes.

### Consequences

Easier: frame loss becomes structurally impossible without a visible failure
(flush-before-FIN + enqueue-on-failure); the sync read loop and settle-once
pattern are direct ports of the transport-http.js timer discipline; server needs
zero changes (EPIPE/early-EOF handling already exists).

Harder: `end()`-based FNF means the client waits one flush round-trip (~µs on UDS,
within the 40 ms budget) instead of fire-and-truly-forget; the timeout-after-write
window can produce duplicate frames via the queue (accepted: at-least-once is the
existing semantic); the sync half-close pattern assumes one-frame-per-connection —
any future multi-frame protocol revisits this ADR.

Cross-references: ADR-002 (SendResult classes), #3448 (expected error taxonomy),
SR-01/SR-05.
