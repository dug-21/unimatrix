## ADR-002: transport-uds.js Adapts UDS Frames to the Existing SendResult Contract; Transport Selection in config.resolve

### Context

The F3 client routes everything through `transport.post(config, frame, opts) ->
Promise<SendResult>` — a narrow seam (index.js:235,253; queue.js replay takes
`post` as a parameter). Downstream consumers (`transform.writeSyncOutput`,
`state.recordSendOutcomes`, queue enqueue-on-failure) key off
`{ok, status, contentType, body, failureClass}`. A UDS transport that invents a
different result shape would fork the pipeline; one that mimics HTTP semantics
reuses it wholesale. Transport selection (OQ1) and replay-over-either-transport
(SR-10) need a single decision point.

### Decision

1. **Contract**: `transport-uds.js` exports `post(config, frame, opts) ->
   Promise<SendResult>` with the identical never-reject, no-stdout/stderr,
   no-retry semantics as `transport-http.js`.
2. **SendResult mapping** (normative):

   | UDS outcome | SendResult |
   |---|---|
   | `Text { body }` | `{ ok:true, status:200, contentType:"text/plain", body:Buffer(body), failureClass:null }` |
   | `Ack` (sync; empty injection) | `{ ok:true, status:204, contentType:null, body:null, failureClass:null }` |
   | FNF frame flushed (no read) | `{ ok:true, status:0, contentType:null, body:null, failureClass:null }` |
   | `Pong {..}` | `{ ok:true, status:200, contentType:"application/json", body:Buffer(frameJson), failureClass:null }` |
   | `Error { code, .. }` | `{ ok:false, status:code, contentType:null, body:null, failureClass: code>=500 ? "http_5xx" : "http_4xx" }` |
   | connect failure (ENOENT/ECONNREFUSED/EACCES) | `{ ok:false, status:0, ..., failureClass:"connect" }` |
   | deadline exceeded | `{ ok:false, status:0, ..., failureClass:"timeout" }` |
   | frame build > 1 MiB / unserializable | `{ ok:false, status:0, ..., failureClass:"http_4xx" }` (client-side reject, mirrors transport-http C-02 guard) |

   No new failureClass values: `state.js` breadcrumbs and stderr classes work
   unchanged. The `http_4xx/5xx` names are kept as generic "rejected" classes.
3. **Transport selection** (config.js): `resolve()` returns `mode: "http"` when
   remote config resolves (env pair or `settings.local.json unimatrix.remote` —
   unchanged precedence; HTTP wins even if a local socket is live, OQ1 confirmed);
   otherwise `mode: "uds"` with `socketPath = ~/.unimatrix/{projectHash}/
   unimatrix.sock` (ADR-007) — replacing the terminal `{ok:false, reason:"missing"}`
   path. `partial_env` and `malformed` remain terminal misconfig breadcrumbs (they
   signal intent to use remote). No local-override knob in F4a; F5 owns init UX.
4. **Selection point**: `index.js` picks the transport module once per spawn from
   `config.mode` and passes its `post` to `queue.replay` — queued frames replay
   over whichever transport the current spawn selected (SR-10; consequences in
   ARCHITECTURE.md: `http-` session-id prefix split is accepted).
5. **Connection model**: one fresh connection per `post()` (parity: Rust
   `fire_and_forget` disconnects per frame; the listener handles exactly one frame
   per connection — listener.rs:377-516). No connection reuse, no pooling.
6. **Timeouts**: UDS uses fixed parity constants — connect/sync/fnf 40 ms each,
   sourced from the Rust hook's `HOOK_TIMEOUT` (uds/hook.rs:27). No new config
   surface in F4a (`unimatrix.remote.timeouts` remains HTTP-only); AC-05's
   <20 ms p95 measurement uses the F3 AC-13 protocol.
7. **No bare probe connection**: the TS client does not replicate the Rust hook's
   empty connect-replay-disconnect probe; daemon presence is discovered by the
   first real `post()`. Accepted process-level divergence (parity bar,
   ARCHITECTURE.md).

### Consequences

Easier: transform.js, state.js, queue.js, delta.js are byte-untouched —
the size-budget cost of UDS is one new file plus small config/index diffs; sync
stdout parity follows from ADR-001 + the existing transform path; cross-transport
replay works by construction (frames carry no transport state).

Harder: SendResult semantics now have non-HTTP interpretations (status 0 on FNF
success; `http_4xx` as generic reject) — documented here, and breadcrumb consumers
must not assume HTTP; the 40 ms fixed timeout is not user-tunable until F5 decides
it should be; `pingForInit` remains HTTP-only (F5 owns local-mode init checks).

Cross-references: ADR-001 (Text/Ack frames), ADR-003 (socket lifecycle behind
`post`), ADR-007 (socketPath derivation), vnc-026 ADR-001 (parity corpus the UDS
layer extends).
