# Gate 3b Report: vnc-039

> Gate: 3b (Code Review)
> Date: 2026-06-18
> Result: PASS
> Branch: feature/vnc-039 @ d822bcf7 (design 45e2d23e, pseudocode 069a8861, impl edf200f7 + d822bcf7)
> Scope: JS-only edge-client (no Rust). Validation = JS suites + check-zero-deps.js + check-hook-client-size.js (CI is JS-client-only; cargo not applicable).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | C1–C5 match credstore/mcp-bridge/bin-unimatrix/init-remote/config-resolve pseudocode; five bridge units decomposed per ADR-001. |
| 2. Architecture compliance | PASS | ADR-001..005 honored: pinned-flush trust contract, mcp-bridge subcommand JS routing, projectHash key + colocated remote.json, reconciled canonical schema, bundle-only boundary. |
| 3. Interface implementation | PASS | Canonical remote.json schema, `node <bridge> <projectHash>` argv, token-free `.mcp.json`, cert-pin reuse — all as designed. |
| 4. Test case alignment | PASS | Every component test plan scenario present; critical risk gates exercised by name (see below). |
| 5. Code quality | PASS (1 WARN) | All suites green; zero anti-stubs; size + zero-dep gates pass. WARN: init.js 684 lines (pre-existing, JS governed by byte-budget not 500-line Rust rule). |
| 6. Security | PASS | No hardcoded secrets; token never on wire pre-pin / in logs / in `.mcp.json`; projectHash key has no path-traversal surface; SSE/JSON bounded at 1 MiB. |
| 7. Knowledge stewardship | PASS | Both impl agents (credstore, mcp-bridge) have `## Knowledge Stewardship` with Queried + Stored entries (#5121, #5124). |

**Critical risk gates (verified BY NAME, not by green suite):**

| Risk | Status | Evidence |
|------|--------|----------|
| R-01/R-02 live wrong-pin | PASS | `mcp-bridge-tls.test.js` runs a REAL `https.createServer` self-signed leaf (7 tests, 0 skipped): wrong-pin asserts capturing server saw ZERO requests (`requests.length === before`) + non-zero exit + expected-vs-presented stderr; per-socket re-pin counts `secureConnects === socketsOpened`; negative-control proves the assertion is non-vacuous (no-op pin DOES leak); mid-session cert-swap rejects socket #2 with no body flushed. |
| R-06 file-mode observe over LOCAL pinned HTTPS (AC-08d) | PASS | `config.test.js` AC-08d suite stands up a real local pinned `https.createServer`: good-pin delivers (204, lands on `/v1/slug/observe`), wrong-pin fails connect-class with zero Authorization on the wire, no UDS fall-through. Behavioral, not field-presence — avoids the vnc-034 dead-pin false-green. |
| R-17 session/attribution stability | PASS | `mcp-bridge.test.js` asserts `Mcp-Session-Id` byte-identical across initialize→tools/list→tools/call, never client-minted on follow-up, stable constant `clientInfo.name`. |

## Detailed Findings

### 1. Pseudocode fidelity — PASS
- **C1 credstore.js** (190 LoC): `pathFor`/`read`/`write` + `STORE_SCHEMA_VERSION=1` exactly per credstore.md. ENOENT/no-home → null; malformed/unknown-version → token-free throw; idempotent merge-write at 0600 with chmod re-assert; merge-preserves unknown fields via `Object.assign`.
- **C2 mcp-bridge** decomposed into the five ADR-001 units (`stdio-frame` 50, `http-session` 158, `sse-parse` 61, `dispatch` 80, `lifecycle` 69) plus entry `mcp-bridge.js` 105. Pinned-flush ordering, capture/replay, fail-loud, verbatim POST all match mcp-bridge.md. `agent:false` (no pool) per R-02. `req.end(body)` lives only in the secureConnect-success branch.
- **C3 bin/unimatrix.js**: `mcp-bridge` early-return JS branch inserted after `init`, before the Rust `execFileSync` fallthrough; lazy `require`; argv-shaped delegation. Matches bin-unimatrix.md.
- **C4 init.js**: `writeMcpBridgeEntry` (token-free, idempotent, merge-preserving, dry-run, malformed-throws), `cleanStaleRemoteSubtree`, `credstore.write` on both paths, `LEGACY_MCP_UNSUPPORTED_MESSAGE`, `gitignoreWarning` + "Skipped .mcp.json" line removed. Matches init-remote.md.
- **C5 config.js**: file-mode branch repointed to `credstore.read`; reads `observe_url`+`fingerprint` (not `url`); `okHttp` gains trailing `pinnedFp` arg; precedence env→store→UDS preserved. Matches config-resolve.md.

### 2. Architecture compliance — PASS
ADR-001 (pinned-flush, per-socket re-pin, fail-loud) implemented in `http-session.js`. ADR-002 (subcommand JS routing) in `bin/unimatrix.js`. ADR-003 (projectHash key, colocated `~/.unimatrix/<hash>/remote.json`) in `credstore.pathFor` + `initRemote` step 1b. ADR-004 (single canonical schema, reconcile-not-port) in credstore + config repoint. ADR-005 (bundle-only boundary, Scope B universal relocation with `fingerprint:null` on legacy) in `initRemote`.

### 3. Interface implementation — PASS
- Canonical `remote.json` schema written/read identically by both consumers; `STORE_SCHEMA_VERSION` shared constant.
- `.mcp.json` entry: `{command:"node", args:[bridgePath, projectHash], env:{}}` — no token/mcp_url/fp (AC-09).
- Bridge argv `node <bridge> <projectHash>`; `argv[2]` → `credstore.read`.
- `applyCertPin`/`verifyPeerFingerprint` reused verbatim from cert-pin.js (no re-implementation of TLS trust).

### 4. Test case alignment — PASS
Suites run individually (per known parallel-timing flakiness note; integration is Stage 3c): credstore 33, mcp-bridge 38, mcp-bridge-tls 7, mcp-bridge-sse 2, bin-mcp-bridge 7, config 67 (1 pre-existing Windows-path skip), init-remote 54, index 54, remote-client 26 — all green, 0 failures. R-04 SSE coverage: single/multi-line data:, multi-event, priming tolerated, chunk-split-invariant fuzz (every offset), CRLF/LF, 1 MiB guard. R-16 stdio-frame byte-split-invariance. AC-05 verbatim URL, AC-09/AC-10/R-09/R-11/R-12/R-15 in init-remote.

### 5. Code quality — PASS (1 WARN)
- `check-zero-deps.js`: PASS (no runtime deps; 25 hook-client modules require only built-ins/relative).
- `check-hook-client-size.js`: PASS — stripped 98,440/100,000 (PRIMARY budget UNCHANGED, passes); raw 169,317/180,000 (BACKSTOP raised 160k→180k with documented HUMAN approval recorded on #775 per the gate's CAP-CHANGE RULE — not a self-raise). Install cap 250k→290k similarly human-approved (#775 comment 4741554518).
- Anti-stub scan: zero `TODO`/`FIXME`/`unimplemented`/`todo!`/placeholder in vnc-039 source.
- **WARN — init.js 684 lines** exceeds the 500-line guideline. This is a PRE-EXISTING modified file (617 lines before vnc-039; +67 net for C4). JS client governance is the byte-budget gate (which passes), not the Rust 500-line convention. Non-blocking; flagged for a future split of init.js if it grows further.

### 6. Security — PASS
- **R-01 token-never-on-wire-pre-pin** proven live (capturing server, zero requests on mismatch). `Authorization: Bearer` flushed only inside the secureConnect-match branch.
- No token in `printSummary`, stdout/stderr, `.mcp.json`, or pin-mismatch error (NFR-06/R-09 tested; `token` absent from error in tls test).
- No path traversal: store path = `homedir()` + literal `.unimatrix` + 16-hex SHA-256 projectHash (fixed grammar, no user string).
- Deserialization bounded: dispatch/sse-parse capped at 1 MiB (`readBounded`/`SseParser.collect` destroy on excess); malformed JSON → JSON-RPC error, no panic.
- No hardcoded secrets.

### 7. Knowledge stewardship — PASS
- `vnc-039-agent-3-credstore-report.md`: Queried (context_search + #5117/#5118), Stored #5121 (credstore asymmetric posture pattern).
- `vnc-039-agent-4-mcp-bridge-report.md`: Queried (#4965/#5105/#5115/#4708), Stored #5124 (pinned-flush bridge test/impl traps).

## Live-wire deferrals (correctly NOT failed at this gate)
G1 (MCP-Protocol-Version echo), session-id mint-direction handshake, G2 (SSE priming/teardown), G3 (clientInfo.name acceptance) are stub/local-validated now and sequenced to Stage 3c as the DELIVERY CHECKPOINT (live cloud validation, #774 merged). SSE IS implemented and built (not stubbed): server forces `text/event-stream` via `StreamableHttpServerConfig::default()`; the bridge sends `Accept: application/json, text/event-stream`; `sse-parse` is a real unit with golden-corpus tests. Confirmed implemented, not deferred-as-stub.

## Rework Required
None.

## Scope Concerns
None.
