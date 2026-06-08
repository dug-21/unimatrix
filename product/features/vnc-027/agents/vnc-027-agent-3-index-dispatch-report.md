# Agent Report — vnc-027-agent-3-index-dispatch

Component 6 (index-dispatch): `lib/hook-client/index.js`. Wave-4 wiring of transport
selection, null sentinel short-circuit, FR-16 offset-delete rekey, and pruneOffsets.

## Files modified
- `/workspaces/unimatrix/packages/unimatrix/lib/hook-client/index.js`
- `/workspaces/unimatrix/packages/unimatrix/test/hook-client/index.test.js`

## What changed (per pseudocode/index-dispatch.md, ADR-002/004/006)
- Replaced the hard `require("./transport-http")` with both transports +
  `selectTransport(config)` → `config.mode === "uds" ? transportUds : transportHttp`.
  Selected ONCE per spawn in `main()` after the null guard; threaded into `runSync`,
  `runFireAndForget` (carrying + delta), and `queue.replay(config, transport.post)`.
- `null` build-request sentinel (non-cycle PreToolUse) → immediate `return` in `main()`
  BEFORE transport selection and the SubagentStart promotion (which reads request.type):
  no network, no queue, no stdout, exit 0.
- Removed the `request.type === "SessionClose"` offset delete. `deleteOffset` now fires
  only when the carrying send succeeds AND `canonicalEvent === "TaskCompleted"` — keyed on
  the canonical event NAME from `normalizeEventName`, never frame type or request.type.
  Stop (canonical "Stop") builds the SAME SessionClose frame and does NOT delete.
- `state.pruneOffsets(config.stateDir)` wired at the top of `runFireAndForget` alongside
  `queue.prune` — FNF path ONLY (NFR-4: sync trio gains no extra file I/O).
- `main()` passes `canonical` (not `effectiveEvent`, not `request.type`) into
  `runFireAndForget`.
- Dispatch fns gained a `transport` parameter defaulted to `transportHttp` so existing
  positional in-process tests (which monkeypatch the http module's `.post`) stay green.
  Exported `selectTransport` for unit testing.

## Tests (node --test)
- `test/hook-client/index.test.js`: 54 pass / 0 fail (added 12 new vnc-027 cases:
  transport selection x3, null-sentinel x1 spawn, FR-16 rekey/discrimination x4,
  pruneOffsets FNF-only/fail-open x3, replay-post-injection x1, plus an end-to-end
  spawn proving Stop preserves / TaskCompleted deletes the offset).
- Full hook-client suite: 528 pass / 7 fail / 1 skipped.
  - The 7 failures are all in `parity-layer1.test.js` (golden corpus for non-cycle
    PreToolUse + cycle near-miss events). They are PRE-EXISTING and NOT caused by this
    component: confirmed by stashing both my files and re-running parity-layer1 → same
    7 fails. They are the consequence of the build-request-sentinel null change
    (component 7) and are owned by the parity-corpus-uds golden update (component 10),
    out of my scope.

## Size gate
`node test/check-hook-client-size.js` → OK.
- index.js: stripped 9308, raw 15557.
- Totals: stripped 68907/100000, raw 112773/160000. Within both budgets.

## Confirmations
- Transport selected from `config.mode` once per spawn (uds→transport-uds, http→transport-http): YES.
- `null` request → immediate return before transport selection: YES.
- `deleteOffset` keyed on canonical "TaskCompleted"; Stop does NOT delete: YES (unit + spawn tested).
- `pruneOffsets` on FNF path only (not sync): YES (test_pruneoffsets_fnf_path_only).
- Size gate passes (byte counts above): YES.

## Issues / blockers
- None in scope. Note for the delivery leader: the 7 parity-layer1 golden failures must be
  resolved by the parity-corpus-uds / build-request-sentinel components before Gate 3c.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search(category=pattern) -- surfaced
  ADR-002 (#4803), ADR-006 keying rationale, vnc-030 ADR-002/007 seam contracts, and
  pattern #4809 (verify event registration before keying behavior to it). Applied: keyed
  delete on canonical name not frame type; retained unreachable TaskCompleted branch with
  assertable-negative test.
- Stored: entry #4825 "Thread a per-spawn selected dependency via a default-bound parameter
  to keep monkeypatch tests green" via context_store (pattern). The canonical-name-vs-frame-type
  trap itself was already captured by ADR-006 + pattern #4809, so only the test-preservation
  injection pattern was novel.
