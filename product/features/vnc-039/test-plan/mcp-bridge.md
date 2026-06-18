# Test Plan — C2 `mcp-bridge.js` (stdio↔Streamable-HTTP bridge)

> Scope **A** · `[stub/local] + live` · NEW file `lib/hook-client/mcp-bridge.js`.
> The single highest-risk component. Risks: **R-01, R-02 (Critical trust-boundary), R-04*, R-05 (Critical), R-03, R-09, R-16, R-17 (High)**. ACs: **AC-03, AC-04, AC-04b, AC-05, AC-06, AC-09, AC-12**.
> New files: `test/hook-client/mcp-bridge.test.js` (lifecycle/session/identity/framing/no-leak), `test/hook-client/mcp-bridge-tls.test.js` (LIVE trust boundary — clone of `cert-pin-tls.test.js`), `test/hook-client/mcp-bridge-sse.test.js` (**probe-gated**), `test/helpers/mcp-stub-server.js` (provenance-pinned Streamable-HTTP MCP stub).
> **Binding rule (lesson #4970 / pattern #4965):** trust-boundary tests exercise a REAL `https.createServer` handshake; shape assertions are rejected at gate.

## Units under test (the five translation areas — ARCHITECTURE §3.1)
`stdio-frame` (~80) · `http-session` (~120) · `sse-parse` (~90, contingent) · `dispatch` (~80) · `lifecycle` (~80). Each is separately testable with its own fixtures (SR-01). The bridge is driven as a child process (stdin/stdout, `node <bridge> <projectHash>`) AND its internal units are unit-tested directly where exported.

## Harness: the MCP Streamable-HTTP stub (`test/helpers/mcp-stub-server.js`)
Extends `helpers/stub-server.js` over `https.createServer` with a self-signed leaf (generated via `openssl` like `cert-pin-tls.test.js`; skip if unavailable). Contract **pinned to a captured rmcp `initialize` response** (`test/fixtures/mcp/rmcp-initialize-capture.json`, provenance comment — R-03):
- On `initialize` → 200, sets `Mcp-Session-Id` response header (server-minted), `Content-Type: application/json`.
- On any follow-up → requires the `Mcp-Session-Id` request header; **4xx if absent** (session-not-found), echoes the value seen.
- Logs every request: URL, method, `Authorization`, `Mcp-Session-Id`, parsed `clientInfo.name`, body.
- Exposes `server.pinnedFp = "sha256:"+hex(sha256(leaf DER))` via the production `computeFingerprint` (#5098 convention).
- `application/json` always; `text/event-stream` framing only enabled when a test opts in (SSE branch, probe-gated).

---

## R-01 — Silent token leak: bearer flushed only AFTER pin matches (Critical, AC-04)
**LIVE handshake, `mcp-bridge-tls.test.js`. The #4970 recipe — not shape assertions.**
- `test_bridge_goodPin_connectsAndRoundTrips` — real self-signed `https.createServer`; bridge configured with its REAL `fp`; drive a full lifecycle (`initialize`→`tools/list`→`tools/call`) through stdin; assert stdout carries valid JSON-RPC results AND the capturing server received the authenticated request (token DID reach it on good pin — proves the path is live, not dead).
- `test_bridge_wrongPin_destroysSocket_zeroAuthorization` — same server, bridge configured with a **different** `fp`; assert: (a) the connection is rejected/destroyed; (b) **the capturing server received NO `Authorization` header and NO request body** (token never crossed); (c) a loud diagnosable expected-vs-presented error to **stderr** and a **non-zero exit**.
- `test_bridge_wrongPin_hammer_neverLeaksToken` — drive many requests against the wrong-pin server (flush-race exposure, mirrors `cert-pin-tls.test.js` (b')); assert `observedAuth` stays empty and the token never appears in anything the server saw.
- `test_bridge_negativeControl_wouldLeakIfPinNoOp` — **negative control**: with `applyCertPin`/`verifyPeerFingerprint` stubbed to a no-op, the wrong-pin server WOULD receive the token (assertion is non-vacuous — guards the test itself).
- **Gate rule:** a test asserting only `rejectUnauthorized===false` or `typeof verifyPeerFingerprint==="function"` does NOT satisfy R-01 and is rejected (AC-04 verified by name).

## R-02 — Per-socket re-pin on the persistent connection (Critical, AC-04b)
**The divergence the single-shot observe path never exercised.**
- `test_bridge_everySocket_repinsBeforeFirstBodyByte` — drive `initialize` then `tools/call` against the live good-pin server; instrument the cert-pin seam (count `secureConnect`→`verifyPeerFingerprint` runs vs sockets opened); assert each opened TLS socket re-pinned **before** its first body byte.
- `test_bridge_noConnectionPoolAgent` — static + behavioral: assert the bridge does not construct an `https.Agent` with `keepAlive`/pooling that could dispatch on an unverified socket; force a NEW socket mid-session and confirm it re-pins.
- `test_bridge_midSessionCertSwap_socket2Rejected_noTokenFlushed` — server presents the correct leaf on socket #1, a WRONG leaf on socket #2; assert socket #2 is rejected and **no token-bearing body is flushed on it** (capturing server sees the request only from socket #1).
- `test_bridge_keepAliveReuse_onlyOnPinnedSocket` — if keep-alive reuse exists, assert it only reuses an already-pinned socket (no body on an un-verified reused socket).

## R-05 — `Mcp-Session-Id` capture & replay correctness (Critical, AC-03) — unit `http-session`
- `test_httpSession_capturesSessionIdFromInitializeResponse` — stub returns `Mcp-Session-Id` on `initialize`; assert it is retained.
- `test_httpSession_replaysSessionIdOnToolsList` and `..._onToolsCall` — assert each post-initialize request carries the captured `Mcp-Session-Id` request header **verbatim**.
- `test_httpSession_absentSessionHeaderOnInitialize_definedBehavior` — server returns no session header → assert the spec'd behavior (proceed sessionless vs fail-loud), **pinned to observed rmcp** (reconciled from the live handshake checklist item #1).
- `test_httpSession_transportSessionId_notConflatedWithToolParamSessionId` — assert the transport `Mcp-Session-Id` is distinct from any tool-param `session_id` (entry #4708).
- `test_httpSession_teardown_sendsDeleteWithSessionHeader` — on stdin EOF, assert a best-effort `DELETE` with the session header.

## R-17 — Session/attribution identity stability (High, AC-12) — unit `http-session`/`lifecycle`
> Distinct from R-05: the captured value must also be **stable**, and `clientInfo.name` must not drift.
- `test_identity_sessionIdByteIdenticalAcrossAllRequests` — drive `initialize`→`tools/list`→`tools/call` in one process; assert the **same** `Mcp-Session-Id` on every post-initialize request (not regenerated, not blank-then-refilled).
- `test_identity_clientInfoNameStableConstant` — assert `initialize` `clientInfo.name` is a fixed bridge identifier, byte-identical across requests, **not** random/timestamped/per-spawn.
- `test_identity_twoSpawnsSameProject_sameClientInfoName` — two successive bridge spawns for the same project advertise the same `clientInfo.name` (deterministic).
- `test_identity_neverMintsOwnSessionIdOnFollowup` — when the server returned a session id on `initialize`, the bridge never mints its own on a follow-up (replays server-minted verbatim — ties to R-05 + live checklist item #1).
- `test_identity_distinctProjects_distinctIdentity` — two distinct bridge sessions for two distinct projects do NOT collide on session id or `clientInfo.name` (no shared/global mutable identity → no attribution bleed).

## R-04* — SSE parse correctness (Critical, CONTINGENT on the SSE-skip probe, AC-06) — unit `sse-parse`
> **`mcp-bridge-sse.test.js` is written ONLY if the live SSE-skip probe (OVERVIEW §4.4) forces `text/event-stream`.** If the live probe is JSON-only, this section is dropped, the `sse-parse` unit is not built, and `test_dispatch_jsonOnly_noSseParserUsed` asserts the parser is absent/unused. If SSE is required, the golden-corpus fixtures below apply.
- `test_sseParse_singleDataLine_oneObject` — one `data:` line → one JSON-RPC object.
- `test_sseParse_multiLineData_reassembledPayload` — RFC-style `\n`-concatenated multi-line `data:` → one payload.
- `test_sseParse_multipleEvents_NObjectsInOrder` — events separated by blank lines → N JSON-RPC objects, in order.
- `test_sseParse_eventAndIdLines_handledPerSpec` — `event:`/`id:` ignored-or-honored per spec; `Last-Event-ID` carried.
- `test_sseParse_chunkSplitInvariant_fuzz` — **byte-split fuzz**: feed the same SSE stream split at EVERY offset (incl. mid-`data:` and mid-record-boundary); assert identical parsed output regardless of split. (The Critical-correctness assertion.)
- `test_sseParse_crlfAndBareLf_bothParse`.
- `test_sseParse_1MiBBodyGuardEnforced` — reuse the `transport-http.js` 1 MiB constant on the SSE path.

## R-16 — stdio framing under chunked input (Med, AC-03) — unit `stdio-frame`
- `test_stdioFrame_oneMessageSplitAcrossChunks_oneParsed` — a JSON-RPC line split across multiple stdin chunks → one parsed message.
- `test_stdioFrame_multipleMessagesOneChunk_NParsedInOrder`.
- `test_stdioFrame_chunkBoundaryOnNewline_noEmptyOrDropped`.
- `test_stdioFrame_writeIsNewlineFramed` — stdout writes are newline-delimited (one JSON-RPC message per line).
- `test_stdioFrame_byteSplitInvariantOnRead` — same multi-message byte stream split at every offset → identical parsed sequence.

## R-03 — JSON-first lifecycle through the bridge against the provenance-pinned stub (High, AC-03/05/06)
- `test_bridge_fullLifecycle_initializeToolsListToolsCall_jsonResults` — drive `initialize`→`tools/list`→`tools/call` (`context_search`, `context_get`) through stdin against the stub; assert valid JSON-RPC results on stdout; `tools/list` returns the `context_*` surface.
- `test_bridge_postsToMcpUrlVerbatim` (AC-05) — assert the stub's logged request URL **equals `mcp_url` exactly** for every proxied request (no path composed, no slug derived, nothing appended — dumb-client invariant).
- `test_bridge_dispatchOnContentType` — `application/json` → single JSON-RPC object to stdout; (SSE branch only if probe forces it).
- `test_bridge_idCorrelation` — concurrent/interleaved requests correlate responses to the right JSON-RPC `id`.
- `test_bridge_streamableHttp4xx5xx_surfacedAsJsonRpcError` — a 4xx/5xx from the endpoint surfaces as a JSON-RPC error on stdout (per-request), not a crash.
- **Provenance:** the stub fixture carries a comment citing the captured rmcp response; the live checklist (OVERVIEW §4.4) reconciles divergence.

## R-09 — No token to any loggable surface (High, AC-09)
- `test_bridge_noTokenInStdout_stderr_onHappyPath` — capture stdout+stderr across a full run; assert the token string appears in neither.
- `test_bridge_noTokenInPinMismatchError` — the expected-vs-presented mismatch message (FR-12) contains no token.
- `test_bridge_noTokenInThrownStoreOrBundleErrors` — thrown store/bundle errors carry token-free messages.
- `test_bridge_tokenReadFromStoreAtSpawn_notFromArgvOrEnv` — assert the token is resolved via `credstore.read(projectHash)` at spawn, never from `process.argv` or `.mcp.json` (argv is `[<bridge>, <projectHash>]` only).

## Store read-error posture for the bridge (R-13)
- `test_bridge_storeEnoent_exitsNonZero_noCredentialForProject` — ENOENT → loud "no credential for project", non-zero exit (NOT fail-open).
- `test_bridge_storeMalformed_exitsNonZeroLoud` — malformed/unknown `schema_version` → non-zero loud exit; **never fails open unpinned**.
- `test_bridge_incompleteEntry_missingFingerprint_definedPosture` — a bundle credential missing `fingerprint` → defined posture (not an unpinned silent run).

## Live cloud validation (OVERVIEW §4.4 — Scope-A delivery gate)
AC-03/04/05/06/12 re-run LIVE against the real `/v1/{slug}` endpoint; session-id handshake item #1 FIRST. Coverage report records each as `validated-live` or `pending-live-run` — never `validated-live` on stub evidence alone. **Fresh-context security review of the trust boundary even on green gates** (#4970).
