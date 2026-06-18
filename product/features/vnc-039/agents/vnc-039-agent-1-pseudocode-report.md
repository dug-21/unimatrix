# Agent Report — vnc-039-agent-1-pseudocode

## Deliverables
Per-component pseudocode for all five architecture components, plus OVERVIEW.

- product/features/vnc-039/pseudocode/OVERVIEW.md
- product/features/vnc-039/pseudocode/credstore.md      (C1, NEW, Scope B)
- product/features/vnc-039/pseudocode/mcp-bridge.md     (C2, NEW, Scope A)
- product/features/vnc-039/pseudocode/bin-unimatrix.md  (C3, MOD, Scope A)
- product/features/vnc-039/pseudocode/init-remote.md    (C4, MOD, Scope A+B)
- product/features/vnc-039/pseudocode/config-resolve.md (C5, MOD, Scope B)

## Components covered
C1 credstore, C2 mcp-bridge (decomposed into 5 sub-modules to stay under 500 LoC/file),
C3 bin entrypoint, C4 initRemote, C5 hook-client resolve. All five from ARCHITECTURE §2.

## Wire-value verification (nan-011 lesson — verified against server source, not the brief)
Verified against `crates/unimatrix-server/src/http/router.rs` and vendored `rmcp-1.7.0`
(`transport/streamable_http_server/{tower.rs, session.rs, common/http_header.rs}`):
- `Mcp-Session-Id` header constant `HEADER_SESSION_ID = "Mcp-Session-Id"` (http_header.rs:1).
- Session id is **server-minted** via `create_session()` and returned in the response header
  (tower.rs:1077-1162); bridge captures+replays verbatim.
- **rmcp requires `Accept` to contain BOTH `application/json` AND `text/event-stream` or returns
  406** (tower.rs:966-978). The bridge must always send both — the naive JSON-only probe form would
  406. Documented in OVERVIEW + mcp-bridge.
- **Unimatrix builds the service with `StreamableHttpServerConfig::default()` overriding only
  `allowed_origins`** (router.rs:326-336) → `stateful_mode:true`, `json_response:false`
  (tower.rs:106-114) → responses are `text/event-stream` (SSE) for the whole lifecycle. **Implication
  flagged: the SSE-skip probe will FAIL against the current server and `sse-parse` is REQUIRED**
  unless the live probe proves otherwise. The live probe remains the definitive gate (server config
  could change). This is the single most consequential wire finding.
- Request `Content-Type` must start with `application/json` or 415 (tower.rs:981-996).
- `MCP-Protocol-Version` validated on non-init requests (tower.rs:1033-1034) — bridge must echo it;
  exact value left to live confirmation.
The pseudocode explicitly instructs the implementer to re-confirm every rmcp-owned wire value LIVE
(DELIVERY CHECKPOINT, session-id handshake first), since rmcp — not unimatrix-server — owns them.

Verified reuse contracts by reading the real source: cert-pin.js (applyCertPin/verifyPeerFingerprint
/computeFingerprint), transport-http.js (pinned-flush 150-176, BODY_LIMIT 1 MiB, DEFAULT_TIMEOUTS),
config.js (computeProjectHash:123, walkToProjectRoot:44, okHttp:203, mergeTimeouts:135,
socketPathFor null-posture), init.js (writeMcpJson idempotency, writeRemoteSettingsLocal 0600 +
chmod, gitignoreWarning, initRemote flow), bundle.js (decodeBundle → {v:2,mcp_url,observe_url,token,
fp}), bin/unimatrix.js (init early-return pattern). package.json deps confirmed empty (AC-02 baseline).

## Open questions / gaps (flagged, not guessed)
- G1: exact `MCP-Protocol-Version` value the bridge echoes — confirm from live `initialize` result.
- G2: SSE single vs multi event + priming/keep-alive lines (rmcp sse_retry default Some(3s) → priming
  emitted) — `sse-parse` tolerates leading priming/comment; confirm live.
- G3: whether the bridge overrides or passes through client `clientInfo.name` — confirm live (AC-12).
- G-C4-1/2: `writeRemoteSettingsLocal` and `gitignoreWarning` become unused by initRemote — delete if
  unreferenced (non-blocking delivery cleanup).
All wire-owned-by-rmcp items are delivery-resolved via the mandated live handshake, not blockers.

## Knowledge Stewardship
- Queried: Unimatrix MCP tools (context_search/context_get/context_briefing) were NOT available in
  this session (ToolSearch returned no matches for the deferred MCP tools). Per agent instructions
  this is non-blocking; proceeded from the five in-repo ADR files (ADR-001..005) and source-of-truth
  documents, which fully covered the design decisions.
- Deviations from established patterns: none. Pseudocode reuses cert-pin/transport pinned-flush, the
  writeMcpJson idempotency contract, and the config.js single-derivation projectHash key verbatim;
  introduces no new conventions.
