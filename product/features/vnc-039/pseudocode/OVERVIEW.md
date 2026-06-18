# vnc-039 Pseudocode — OVERVIEW

Per-component pseudocode for the pure-JS stdio→HTTPS MCP bridge (Scope A) + out-of-tree
credential relocation (Scope B). Source of truth: ARCHITECTURE.md (C1–C5), ADR-001..005,
SPECIFICATION.md (FR-01..27 / AC-01..12), RISK-TEST-STRATEGY.md (R-01..17).

All JS lives under `packages/unimatrix/`. Paths in this doc are relative to that package root
(e.g. `lib/hook-client/credstore.js` = `packages/unimatrix/lib/hook-client/credstore.js`).

## Components

| File | Component | New/Mod | Scope | Pseudocode |
|------|-----------|---------|-------|-----------|
| `lib/hook-client/credstore.js` | C1 store owner | NEW | B | credstore.md |
| `lib/hook-client/mcp-bridge.js` | C2 bridge | NEW | A | mcp-bridge.md |
| `bin/unimatrix.js` | C3 entrypoint | MOD | A | bin-unimatrix.md |
| `lib/init.js` `initRemote()` | C4 init writer | MOD | A+B | init-remote.md |
| `lib/hook-client/config.js` `resolve()`/`okHttp` | C5 hook resolve | MOD | B | config-resolve.md |

## Dependency / sequencing

```
C1 credstore.js  (NEW, B — sole store owner: pathFor/read/write, schema, 0600)
   ├──> C4 init-remote   write(projectHash, cred)   [B half]  ── lands FIRST
   ├──> C5 config-resolve read(projectHash)          [B]      ── lands FIRST
   └──> C2 mcp-bridge     read(projectHash)           [A]     ── lands SECOND (depends on C1+C3)
C3 bin/unimatrix.js  (MOD, A — routes `mcp-bridge` subcommand to C2)  ── A
C4 init-remote .mcp.json bridge-entry write  [A half]                 ── A
```

**Build C1 first.** C5 and the B-half of C4 ship without a reachable cloud (Scope B lands first,
ADR-005). C2/C3 and the A-half of C4 follow. C2 and C5 both consume `credstore.read`; they MUST
read the one schema C1 owns — no hand-rolled store access in either consumer (ADR-004).

## Data flow

```
init --bundle <v:2>
  └ decodeBundle -> {mcp_url, observe_url, token, fp}
      ├ C1 credstore.write(projectHash, {mcp_url,observe_url,token,fingerprint:fp,timeouts?})
      │     -> ~/.unimatrix/<projectHash>/remote.json (0600)          [Scope B]
      └ C4 writes .mcp.json unimatrix stdio entry: node <bridge> <projectHash>  [Scope A]

runtime (hook event)  -> C5 resolve() -> credstore.read(projectHash)
      -> okHttp(observe_url, token, timeouts, "file", pinnedFp=fingerprint)
      -> transport.post (PINNED HTTPS to observe_url)                  [Scope B fix]

runtime (Claude Code spawns bridge) -> C2 mcp-bridge.js <projectHash>
      -> credstore.read(projectHash) -> {mcp_url, token, fingerprint}
      -> pinned HTTPS POST mcp_url <-> stdio JSON-RPC                  [Scope A]
```

Field ownership (one schema, no per-consumer dialect — ADR-004 §4.3):
`mcp_url` → bridge only · `observe_url`+`timeouts` → hook client only · `token`+`fingerprint` → both.

## Shared type — canonical `remote.json` schema (the contract both reads obey)

Path: `~/.unimatrix/<projectHash>/remote.json`, mode `0600`, colocated with `unimatrix.sock` +
`hook-client/` (ADR-003). One file per project (no global slug→entry map).

```jsonc
{
  "schema_version": 1,                       // unknown version => TERMINAL read error (never silent)
  "mcp_url":     "https://host/v1/<slug>",   // bridge reads; POSTed verbatim
  "observe_url": "https://host/v1/<slug>/observe", // hook client reads; post target
  "token":       "<64 lowercase hex>",       // both; "Authorization: Bearer <token>"
  "fingerprint": "sha256:<64 hex>" | null,   // both; the pin. null = legacy/unpinned path
  "timeouts":    { "connect_ms":750, "sync_ms":2000, "fnf_ms":3000 }  // OPTIONAL; absent => DEFAULT_TIMEOUTS
}
```

`STORE_SCHEMA_VERSION = 1` is a single named constant in C1; reader and writer share it (R-13,
schema-bump cascade pattern #4153/#4373).

## Shared type — `.mcp.json` bridge entry (token-free — AC-09, FR-17)

```jsonc
{
  "mcpServers": {
    "unimatrix": {
      "command": "node",
      "args": ["<abs resolved path to mcp-bridge.js>", "<projectHash>"],
      "env": {}
    }
  }
}
```
No token, no `mcp_url`, no `fp`. The only arg is `projectHash` (the store key). The bridge resolves
the credential from the store at spawn time.

## Bridge argv contract

`node <bridge> <projectHash>` (ARCHITECTURE §8). `process.argv[2]` is the `projectHash` the bridge
passes to `credstore.read`. C4 writes this entry; C2 consumes `argv[2]`; C3 routes the human-facing
`unimatrix mcp-bridge <projectHash>` subcommand to the same module.

## Reused integration surface (verified against source — do NOT re-invent)

| Symbol | Source (verified) | Used by |
|--------|-------------------|---------|
| `computeProjectHash(projectRoot) -> 16 hex` | `lib/hook-client/config.js:123` | C1,C4,C5 — the store key oracle |
| `detectProjectRoot(startDir)` | `lib/init.js:25` | C4 (write-side root) |
| `walkToProjectRoot(startDir)` | `lib/hook-client/config.js:44` | C5 (read-side root) |
| `applyCertPin(options,isTls,pinnedFp)` | `lib/hook-client/cert-pin.js:131` | C2 |
| `verifyPeerFingerprint(socket,pinnedFp) -> Error\|null` | `lib/hook-client/cert-pin.js:67` | C2 |
| `computeFingerprint(derBuffer) -> "sha256:"+hex` | `lib/hook-client/cert-pin.js:26` | C2 (parity) |
| pinned-flush pattern | `lib/hook-client/transport-http.js:150-176` | C2 (reference) |
| `DEFAULT_TIMEOUTS`, `BODY_LIMIT_BYTES (1 MiB)` | `transport-http.js:19,23` | C2 |
| `decodeBundle(raw) -> {v:2,mcp_url,observe_url,token,fp}` | `lib/hook-client/bundle.js` | C4 |
| `okHttp(url,token,timeouts,source,root,hash,stateDir)` | `config.js:203` (GAINS `pinnedFp`) | C5 |
| `mergeTimeouts(t)` | `config.js:135` | C5 (timeouts→DEFAULT_TIMEOUTS) |

## VERIFIED WIRE VALUES (rmcp 1.7.0 — server source, not just the brief)

Verified against `crates/unimatrix-server/src/http/router.rs` and the vendored
`rmcp-1.7.0/src/transport/streamable_http_server/{tower.rs, session.rs, common/http_header.rs}`.
The MCP transport (session-id, content negotiation, session minting) is owned by rmcp, NOT by
Unimatrix code — so these MUST be re-confirmed LIVE in delivery (the DELIVERY CHECKPOINT,
ARCHITECTURE §3.3 / SPECIFICATION live-validation sequencing). The values below are what the
pinned rmcp 1.7.0 source does today and what the stub contract must be derived from.

1. **`Mcp-Session-Id` header name/casing** — constant `HEADER_SESSION_ID = "Mcp-Session-Id"`
   (`common/http_header.rs:1`). HTTP headers are case-insensitive; replay the captured name/value
   verbatim. (Node lowercases response header keys: read `res.headers["mcp-session-id"]`.)
2. **Server-minted, NOT client-minted** — on a POST with NO session header in `stateful_mode`,
   rmcp calls `session_manager.create_session()` and returns the id in the `Mcp-Session-Id`
   RESPONSE header (`tower.rs:1077-1081, 1157-1162`). The bridge sends `initialize` with NO session
   header, CAPTURES the response header, and REPLAYS it verbatim on every later request (FR-02/03).
3. **Content negotiation — CRITICAL.** `handle_post` REQUIRES the request `Accept` header to
   contain BOTH `application/json` AND `text/event-stream`, else `406 Not Acceptable`
   (`tower.rs:966-978`). The bridge MUST send `Accept: application/json, text/event-stream` on
   EVERY POST. A JSON-only `Accept` (the naive SSE-skip probe form) gets a 406 — see the SSE-skip
   probe note below.
4. **Response framing — SSE by default in this server's config.** Unimatrix builds the service with
   `StreamableHttpServerConfig::default()` overriding only `allowed_origins`
   (`router.rs:326-336`). Defaults are `stateful_mode: true`, `json_response: false`
   (`tower.rs:106-114`). In that config rmcp returns `text/event-stream` (an SSE stream) for
   `initialize`, `tools/list`, AND `tools/call` — the JSON-direct branch (`tower.rs:1187`) is
   reachable ONLY when `stateful_mode == false && json_response == true`. **Implication:** against
   the current server, the SSE-skip probe will FAIL and `sse-parse` is REQUIRED. Treat
   `sse-parse` as built unless the LIVE probe proves otherwise; `dispatch` keeps the JSON branch
   regardless (responses are single SSE events carrying one JSON-RPC payload).
5. **`Content-Type` request header** — must start with `application/json` else `415`
   (`tower.rs:981-996`). The bridge always POSTs `Content-Type: application/json`.
6. **`MCP-Protocol-Version` header** — `validate_protocol_version_header` runs on non-initialize
   requests in stateful mode (`tower.rs:1033-1034`). The bridge SHOULD echo the protocol version it
   negotiated at `initialize` on subsequent requests. CONFIRM the exact required value live (rmcp
   spec 2025-06-18); pin from the captured `initialize` result.
7. **`clientInfo.name` attribution** — the server keys audit attribution on `clientInfo.name` +
   the transport session id (vnc-014/#4708). The bridge sends a STABLE, fixed `clientInfo.name`
   in `initialize` (a constant bridge identifier; e.g. `"unimatrix-mcp-bridge"`), never per-spawn
   random/timestamped (FR-03a, AC-12). CONFIRM live that the value is accepted and that attribution
   is stable.
8. **Session teardown** — best-effort `DELETE` with the `Mcp-Session-Id` header on stdin EOF
   (ARCHITECTURE §3.3). CONFIRM the DELETE path live; failure is non-fatal.

> **SSE-skip probe (FIRST delivery experiment, runs LIVE — R-04, ADR-001 §3.1).** Because rmcp
> rejects a JSON-only `Accept` with 406 (finding 3), the probe is NOT "send `Accept: application/json`
> only and see if you get JSON." The probe is: send the spec-required `Accept: application/json,
> text/event-stream` and observe whether the server EVER responds with `Content-Type:
> application/json` across the full lifecycle (it does only in stateless+json_response mode). Source
> analysis above says it will respond `text/event-stream` in the current config, so the expected
> outcome is "SSE required, build `sse-parse`." The live probe is the definitive gate; if a future
> server config flips to JSON-direct, `sse-parse` drops. Document the probe's live result in the
> delivery report and reconcile it into the stub contract (R-03).

## Error-boundary postures (ARCHITECTURE §9 — by consumer)

| Origin | C2 bridge (persistent, FAIL-LOUD) | C5 hook client (single-shot, FAIL-OPEN) |
|--------|-----------------------------------|------------------------------------------|
| store ENOENT (read→null) | exit non-zero, "no credential for project <hash>" | UDS fall-through (resolveUds) |
| store malformed / unknown schema_version | exit non-zero, diagnosable | terminal `malformed` |
| cert-pin mismatch | socket destroyed BEFORE body; stderr expected-vs-presented; exit non-zero | connect-class failure → breadcrumb → exit 0 |
| store write fail (C4) | n/a (init) throws → init exit 1 | n/a |

## Open questions / gaps (flagged, not guessed)

- **G1 (wire, delivery-resolved):** rmcp owns session-id + framing; values above are source-verified
  for rmcp 1.7.0 but the live handshake is the gate (DELIVERY CHECKPOINT). The exact
  `MCP-Protocol-Version` value the bridge must echo is not pinned here — confirm from the live
  `initialize` result.
- **G2:** SSE single-event vs multi-event in `initialize`/`tools/call` responses — rmcp wraps the
  single JSON-RPC response in one SSE `data:` event (`tower.rs:1140-1141`), plus an optional
  priming event when `sse_retry` is set (default `Some(3s)` → priming IS emitted). `sse-parse` must
  tolerate a leading priming/empty event and a keep-alive comment line. Confirm live.
- **G3:** `clientInfo.name` exact accepted value / attribution behavior — confirm live (AC-12).
