## ADR-001: Pure-JS stdio→Streamable-HTTP bridge with a fail-loud pinned-flush trust contract

### Context

The personal-cloud contract (#4946) owes the `context_*` MCP surface over HTTPS, but Claude Code's native `http`/`streamable-http` MCP transports run CA-chain verification first and reject the cloud's self-signed leaf (`DEPTH_ZERO_SELF_SIGNED_CERT`) before any fingerprint pin can apply (entry #5105, the F1 wall), and they offer no seam to supply the bundle `fp`. The house pattern for daemon-backed MCP (entry #1897, local UDS daemon) is a thin stdio bridge that owns its own backend connection so Claude Code sees an ordinary stdio server and never does TLS itself.

The remote analogue cannot be a byte forwarder like the local UDS bridge (#2582): the cloud speaks **Streamable-HTTP** (HTTP request/response, `Mcp-Session-Id` header, `application/json` **and** `text/event-stream` framing — entry #4708), while stdio is newline-delimited JSON-RPC. The bridge must terminate and re-frame. ass-080 (#777) decided BUILD/DIY over adopting `@modelcontextprotocol/sdk` (a 91-package/25 MB server-laden tree that gives zero leverage on the hard part — TLS pinning — which must be hand-written on `node:https` either way), budgeting ~450 LoC across five areas with TLS reused at ~0 net LoC.

Two risks dominate. **SR-01:** the ~260-LoC SSE-parse + session-replay surface is hand-rolled and subtly-wrong JSON-RPC round-trips are plausible. **SR-02:** cert-pin reuse is trust-boundary code — vnc-034's F1 (#4970/#4965) shipped DEAD pin code through three green gates; a bridge that flushes the bearer before the pin matches is a silent token-leak regression. The bridge is also a **persistent** connection with **fail-loud** posture, unlike the single-shot fail-open observe path — so the "TLS reused at ~0 LoC" assumption (A3) needs an explicit lifecycle contract.

### Decision

Build `lib/hook-client/mcp-bridge.js`: a pure-Node-stdlib (`https`, `crypto`, `net`/stdio) **single-session** stdio process, spawned per Claude Code session, that translates stdio JSON-RPC ⇄ Streamable-HTTP against `mcp_url`. No runtime dependency (AC-02, by decision per ass-080).

**Five translation areas**, each a separately-testable unit with its own fixtures (SR-01 mitigation):
1. `stdio-frame` (~80) — newline-delimited JSON-RPC read/write.
2. `http-session` (~120) — HTTP POST + `Mcp-Session-Id` capture (from `initialize` response headers) and replay (on every subsequent request). The transport-level session UUID, distinct from any tool-param session_id (entry #4708).
3. `sse-parse` (~90) — `text/event-stream` line parser (`event:`/`data:`/`id:`, blank-line record boundary, `Last-Event-ID`). **CONTINGENT — see the SSE-skip probe below.**
4. `dispatch` (~80) — route by response `Content-Type` (`application/json` single object vs. SSE stream), correlate by JSON-RPC `id`, write to stdout.
5. `lifecycle` (~80) — `initialize`→session→`tools/list`/`tools/call`; reuse `transport-http.js` timeout + 1 MiB body-guard constants; `DELETE`-session teardown on stdin EOF.

**SSE-skip delivery probe (FIRST delivery task, per OQ-1 JSON-first direction).** `sse-parse` is the hardest correctness surface (it dominates SR-01). Before building it, delivery runs a probe: send `Accept: application/json` **only** (no `text/event-stream`) and exercise the **full lifecycle** — `initialize → tools/list → tools/call` — against the rmcp endpoint. If the endpoint answers every step with `application/json` (never forcing a `text/event-stream` response), then **the `sse-parse` unit is DROPPED**: the bridge requests JSON-only and `dispatch` needs no SSE branch. This removes ~90 LoC of the hardest hand-rolled code. The probe is a hard gate, not a guess. If the probe **fails** (the endpoint emits SSE for any lifecycle step even under JSON-only `Accept`), `sse-parse` is built as specified above — it is **designed as a contingency, not removed from the design**. Either way the bridge keeps the dual-framing `dispatch` shape; only the SSE branch's existence is contingent on the probe outcome.

**Trust contract (the load-bearing invariant — SR-02):** the bearer token reaches the wire **only after** the leaf fingerprint matches `fp`. Reuse `cert-pin.js` + the `transport-http.js:150-176` pinned-flush pattern verbatim — never re-implement TLS trust:
- `applyCertPin(opts, true, pinnedFp)` → `rejectUnauthorized:false`, `ca:undefined`.
- On `req.on('socket')` → `s.once('secureConnect')` → `verifyPeerFingerprint(s, pinnedFp)`.
- Mismatch → `req.destroy(err)`, **no body flush**, fail **loud**: diagnosable expected-vs-presented message to stderr, non-zero exit.
- Match only → `req.end(body)` flushes the `Authorization: Bearer` body.

**Two divergences from the observe path, made explicit (A3):**
- **Persistent connection.** Every new TLS socket the bridge opens re-runs `verifyPeerFingerprint` on `secureConnect` before its first body byte; keep-alive reuse is permitted only on an already-pinned socket. The bridge MUST NOT use a connection-pool agent that could flush a request on an unverified socket.
- **Fail-loud, not fail-open.** A broken pin surfaces to the user (stderr + non-zero exit) — a dead `context_*` surface with a diagnosable cause, never a silent degrade.

**Testable acceptance (SR-02, routed to fresh-context security review even if gates are green):** a live self-signed-handshake test asserting (a) good-pin connects and round-trips a full MCP lifecycle, (b) wrong-pin is rejected AND the token never reaches the wire. The documented hybrid flip-bar (ass-080 OQ-1) is a concrete pre-agreed delivery checkpoint: flip to the SDK-transport-behind-custom-fetch fallback **only if** the ~260-LoC SSE/session surface proves materially harder than estimated — at the cost of the full dep tree plus a still-hand-written custom-fetch+Response adapter, so the bar is high.

**Session/attribution stability contract (load-bearing on `http-session` + `lifecycle`):** the server keys audit attribution on the transport `Mcp-Session-Id` header and the `initialize` `clientInfo.name` (vnc-014 / #4708). Both units MUST therefore present a **stable** identity:
- `http-session` captures the server-issued `Mcp-Session-Id` once on `initialize` and replays the **same** value on every subsequent request for the life of the process — it never regenerates, rotates, or drops it mid-session.
- `lifecycle` sends a **stable** `clientInfo.name` in `initialize` (a fixed bridge identifier, not a per-spawn random/timestamped value).
Unstable identity (a rotating session id or a varying `clientInfo.name`) causes **cross-session attribution bleed** at the server — distinct sessions mis-merge or mis-split in the audit trail — which directly undercuts the 1-client:1-project integrity basis (vnc-034 A1). This is a correctness contract on the units, not a nicety: a test asserts session-id and `clientInfo.name` are byte-stable across `initialize → tools/list → tools/call` within one process.

The bridge POSTs `mcp_url` **verbatim** — composes no path, derives no slug (dumb-client invariant, ADR-002 vnc-038 #5081).

### Consequences

**Easier:** Claude Code never does TLS (the self-signed wall is subsumed); the `context_*` surface is restored at parity with local; zero-dep posture preserved; the unit decomposition makes the high-risk SSE/session code independently testable; TLS trust is reused, not re-authored.

**Harder:** the hand-rolled SSE+session correctness is the residual delivery risk (SR-01) — though the SSE-skip probe can retire ~90 LoC of it (the `sse-parse` unit) if the rmcp endpoint honors JSON-only `Accept` across the full lifecycle, leaving ~120 LoC of session-replay as the irreducible hand-rolled surface; the persistent-connection lifecycle adds a per-socket re-pin obligation the single-shot observe path never had (A3) — the "free TLS" assumption holds only if that obligation is honored; the bridge must present a stable session id + `clientInfo.name` or leak audit attribution across sessions (the attribution contract above); the trust boundary demands a live-handshake test + fresh-context review, not a shape assertion (the exact failure vnc-034 shipped). Live end-to-end validation — including the SSE-skip probe, which needs the real rmcp endpoint to be conclusive — is gated on #774; stub validation only until then (SR-04).

Related: ADR-002 (this feature, entrypoint), ADR-004 (this feature, store schema supplying `mcp_url`/`fingerprint`), ADR-002 vnc-038 #5081 (dumb-client, `mcp_url`), entry #5105 (stdio-bridge-not-native-http), entry #1897 (UDS house pattern), entry #4708 (session-id semantics), ass-080 #777 (BUILD/DIY), #4970 (vnc-034 F1 dead-pin false-green — the SR-02 precedent).
