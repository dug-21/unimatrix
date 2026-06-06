# Agent Report — vnc-026-agent-6-normalize

## Task
Implement `packages/unimatrix/lib/hook-client/normalize.js` (mapToCanonical /
normalizeEventName, Gemini aliases, `__unknown__` sentinel) — exact port of
`hook.rs:50-105` — plus unit tests per `test-plan/normalize.md`.

## Files Created
- `packages/unimatrix/lib/hook-client/normalize.js` (124 lines)
- `packages/unimatrix/test/hook-client/normalize.test.js` (156 lines)

Committed: `cdcc80e1 impl(normalize): event canonicalization port of hook.rs:50-105 with Gemini aliases + __unknown__ sentinel (#679)`

## Implementation Notes
- Exact-match `switch` mirrors the Rust `match` arm-for-arm: 11 canonical names
  (identity), 3 Gemini aliases (BeforeTool/AfterTool/SessionEnd → PreToolUse/
  PostToolUse/Stop with `gemini-cli`), default → `["__unknown__","unknown"]`.
  Case-sensitive, no trimming — verified against the oracle source directly.
- `normalizeEventName` returns a 2-element array `[canonical, provider]` (tuple
  parity); `mapToCanonical` exported for the future hint path per pseudocode.
- Exported `UNKNOWN_EVENT` / `UNKNOWN_PROVIDER` constants for index.js /
  build-request.js sentinel checks.
- Test-plan discrepancy resolved golden-driven: the plan says "13 canonical
  event names" but the oracle (`hook.rs:57-71`) and pseudocode both define 11.
  Tests assert the oracle's 11. Whitespace/case probes assert the sentinel
  (Rust does no lowercasing/trimming).

## Test Results
- Component suite `test/hook-client/normalize.test.js`: **8/8 pass** (node:test).
- Full package suite: 89 tests, 83 pass, 6 fail — the 6 failures
  (`mergeSettings`/`writeMcpJson`: LD_LIBRARY_PATH-prefixed command expectations)
  are **pre-existing on the clean tree** (verified via stash round-trip:
  81 tests / 6 fail without my changes). Not introduced by this component;
  belongs to the init-remote/merge-settings workstream.

## Issues / Blockers
None. Noted for the delivery leader: the 6 pre-existing merge-settings/init
failures above predate this work and need ownership by the init-remote agent.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced entry 4298
  (hook-normalization pattern) and vnc-013 ADR-001/ADR-002 (canonical names,
  provider field); confirmed the table and provider-inference semantics, no
  contradictions with the pseudocode.
- Stored: nothing novel to store — this was a mechanical 1:1 port of a closed
  56-line Rust match to a JS switch; the normalization pattern itself is
  already captured in entry 4298.
