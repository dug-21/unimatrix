# Agent Report — vnc-039-agent-4-mcp-bridge (C2)

Role: uni-js-dev. Component C2 `lib/hook-client/mcp-bridge.js` (NEW, Scope A) — pure-stdlib stdio↔Streamable-HTTP MCP bridge, 5 sub-modules + entry.

## Files created
- `lib/hook-client/mcp-bridge.js` — entry: argv parse, store read, field validation, wiring, run()
- `lib/hook-client/mcp-bridge/stdio-frame.js` — newline-delimited JSON-RPC framing (R-16)
- `lib/hook-client/mcp-bridge/http-session.js` — pinned POST + Mcp-Session-Id capture/replay + per-socket re-pin (R-01/R-02/R-05/R-17)
- `lib/hook-client/mcp-bridge/sse-parse.js` — text/event-stream parser (R-04)
- `lib/hook-client/mcp-bridge/dispatch.js` — Content-Type routing, id-correlate, 1 MiB bound (R-04)
- `lib/hook-client/mcp-bridge/lifecycle.js` — initialize→tools/list/call proxy, stable clientInfo.name (R-17)
- `test/hook-client/mcp-bridge.test.js` — 38 unit/behavioral tests
- `test/hook-client/mcp-bridge-tls.test.js` — 7 LIVE trust-boundary tests (R-01/R-02)
- `test/hook-client/mcp-bridge-sse.test.js` — 2 SSE wire tests (R-04, probe-gated → built)
- `test/helpers/mcp-stub-server.js` — provenance-pinned Streamable-HTTP MCP stub (live https self-signed)
- `test/fixtures/mcp/rmcp-initialize-capture.json` — rmcp initialize capture fixture (R-03 provenance pin)

## Tests
- C2 suite: **80 pass / 0 fail / 0 skipped** (mcp-bridge 38 + tls 7 + sse 2 + credstore 33 co-run).
- R-01/R-02 covered BY NAME: good-pin round-trips full lifecycle; wrong-pin → capturing server received ZERO Authorization + non-zero loud exit naming expected-vs-presented fp; hammer (15x) never leaks; **negative control** proves the assertion non-vacuous (no-op pin DOES leak); per-socket re-pin (secureConnects === socketsOpened); mid-session cert swap rejects socket #2 with no body flushed; no-pool (agent:false, rejectUnauthorized:false).
- R-17 covered BY NAME: session id byte-identical across all requests; never minted client-side on follow-up; clientInfo.name a stable constant (not timestamped/random); distinct projects no shared mutable identity.
- R-05/R-04/R-16: capture/replay verbatim, verbatim URL (AC-05), Accept both + JSON Content-Type; SSE single/multi-line/multi-event/chunk-split-fuzz/CRLF/1 MiB guard; stdio split/multi/boundary/byte-split-invariant.
- R-13/R-09: store ENOENT/malformed/missing-fingerprint all exit non-zero LOUD (never unpinned); token read from store at spawn (not argv); token absent from all stderr/error paths.

## Size gate — **BLOCKER (raw backstop)**
`node test/check-hook-client-size.js`: **FAIL**. stripped=98,440/100,000 (**PASS**, 1,560 headroom); raw=169,325/160,000 (**FAIL, +9,325 over**).

This is a genuine architecture-level cap conflict, NOT trimmable comment bloat:
- Pre-existing tree (incl. committed C1 credstore) = 152,301 raw, leaving only **7,699 raw headroom**.
- The bridge's 6 files = ~17,274 raw / ~13,326 fully comment-stripped. **Even with every comment removed the bridge floors at ~13.3 KB raw — ~5.6 KB over the backstop.**
- The architecture budgeted the bridge at ~450 LoC (ass-080 #777); at ~40 B/line that is ~18 KB raw, which structurally exceeds the 7.7 KB raw headroom the backstop allows.

I did NOT self-raise the gate (forbidden — human decision recorded on the feature issue). I trimmed my own comments to the load-bearing trust-contract anchors; the overage is structural code volume, not my comments. **This needs a human decision:** either (a) raise the raw BACKSTOP cap (the PRIMARY/stripped limit still passes, so shipped-logic is within budget — the backstop is the only failing limit), or (b) reclaim raw headroom by trimming JSDoc/comment volume in existing hook-client modules (out of my C2 scope; the existing tree carries ~71 KB of comments+whitespace). The stripped budget passing suggests the backstop, set when the tree was smaller, is the binding constraint rather than logic growth.

## Other gates
- Zero-dep audit: **PASS** (no runtime deps; all 25 hook-client modules require only built-ins/relative).
- `package.json` / lockfile: **unchanged** (verified via read-only git status).
- Module system: CJS throughout, matching the hook-client.

## Flags (adjacent / not fixed)
- **Pre-existing spawn-test failures (NOT mine):** `test/hook-client/index.test.js` fails 11/54 and `cycles.test.js` ~1 spawn test **with my bridge files moved aside** — environmental (the spawned child hook-client cannot reach its stub under the sandbox; assertions are "expected 1 POST, got 0"). My files are not referenced by the spawn/index path. Flagging for the tester/Stage 3c; not caused by C2.

## Live-wire items still needing Stage 3c confirmation (DELIVERY CHECKPOINT)
The stub is pinned to source-verified rmcp 1.7.0 shapes but these are rmcp-owned and unpinnable from server source alone — confirm LIVE against `/v1/{slug}`, session-id handshake FIRST:
- G1: exact `MCP-Protocol-Version` value to echo (bridge echoes the captured initialize `result.protocolVersion`).
- Session-id handshake: server-minted vs client-minted, and verbatim replay (implemented as capture-from-response-header + replay; confirm minting direction live).
- G2: SSE priming/keep-alive event shape + the DELETE teardown path (best-effort, tolerated).
- G3: `clientInfo.name = "unimatrix-mcp-bridge"` accepted + attribution stable.
- R-04 SSE-skip probe: source says SSE required (json_response:false default); `sse-parse` is BUILT. Live probe is the definitive gate — if a future config flips to JSON-direct it can drop, but `dispatch` keeps its JSON branch regardless.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (cert-pin secureConnect pinned-flush; vnc-039 decisions) + context_briefing — surfaced #4965 (verify-on-secureConnect + gate the token write), #5105 (stdio-bridge-not-native-http), #5115 ADR-001, #4708 (session-id semantics). Applied: reused cert-pin.js verbatim, pinned-flush ordering, server-minted session-id capture/replay.
- Stored: entry #5124 "Pinned-flush bridge: settle the request Promise on mismatch and spy at the requester seam (vnc-039 C2)" via context_store — three test/impl traps invisible in source (fail-loud await-hang on injected exit; import-bound pin helper un-spyable by reassignment; mid-session cert swap needs setSecureContext not SNICallback on an IP host).
