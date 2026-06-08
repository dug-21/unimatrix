# Agent Report — vnc-027-agent-3-transport-uds

Component: `lib/hook-client/transport-uds.js` (NEW). Merge step 3. JS/Node component.

## Files Created/Modified
- `packages/unimatrix/lib/hook-client/transport-uds.js` (NEW)
- `packages/unimatrix/test/hook-client/transport-uds.test.js` (NEW — 33 unit tests)
- `packages/unimatrix/test/helpers/stub-server.js` (MOD additive — UDS stub + blackhole listeners, `frameResponse`, `absentSocketPath`)

## Tests
- transport-uds suite: 33 pass / 0 fail (`node --test`).
- transport-http oracle suite: 28 pass / 0 fail (unaffected).
- Full hook-client suite: 515 pass / 7 fail — the 7 failures are pre-existing parity-layer1 request-golden mismatches (verified: they fail identically with my changes stashed). They belong to the not-yet-landed hook-set-reduction work (build-request-sentinel / merge-settings, merge step 4), NOT this component.
- Gates: size-gate OK (stripped 68,399/100,000; raw 110,832/160,000). zero-deps OK (14 modules, `net` core only).

## Confirmations (per task)
1. `post()` never rejects — every path resolves a SendResult; `test_never_rejects` fuzzes connect/timeout/oversize/unserializable via `Promise.allSettled`, all fulfilled.
2. Framing caps enforced BEFORE allocation — write: `encodeFrame` returns null on >1 MiB → `fail("http_4xx")`, no connection. read: declared length 0 / >1 MiB / 0xFFFFFFFF rejected on the 4-byte header before reading the body (`fail("connect")`). Both directions accept exactly 1,048,576 B.
3. Size gate passes: stripped 68,399 B / raw 110,832 B (new file added ~5 KB stripped; ample headroom remains).
4. ADR conformance: ADR-002 SendResult mapping table unit-tested per row; ADR-003 lifecycle (FNF `socket.end`→resolve on `finish`, `destroy()` never before `finish`; sync half-close + accumulate; settle-once; 40 ms `unref()`'d deadline; no `process.exit`); ADR-001 §2 `accept:"text/plain"` injected at serialization for sync ContextSearch/CompactPayload only, never mutating the caller frame, never on FNF/other types.
5. Fail-open: every fs/net call wrapped; no stdout/stderr/console (grep-gated); no secrets logged. UDS Unix-only documented in the module header (no Windows shim).

## Scope notes
- Integration/live-listener assertions (AC-03 round-trip, AC-04 truncation, AC-05 p95) intentionally deferred to parity-corpus-uds (Stage 3c) — not run here.
- `module.exports` includes `encodeFrame`/`mapHookResponse` for the parity-corpus framing fixtures.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) — surfaced ADR-002/003/001 (#4803/#4804/#4802, already in-hand), #4768 (node:test grep-gate + stub traps, applied), #2616 (4-byte BE framing test patterns, applied).
- Stored: entry #4824 "UDS stub listeners for half-close clients must set allowHalfOpen:true (node net)" via context_store (pattern) — the half-close auto-FIN trap plus chunk-budget, FNF-receipt-race, and grep-gate gotchas hit while building the suite.
