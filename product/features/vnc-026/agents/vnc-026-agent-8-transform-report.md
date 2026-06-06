# Agent Report — vnc-026-agent-8-transform

## Task
Implement `packages/unimatrix/lib/hook-client/transform.js` (host-envelope stdout, ADR-002) plus unit suite per `test-plan/transform.md`.

## Files Created
- `packages/unimatrix/lib/hook-client/transform.js` — `renderEnvelope(reqSource, text) -> Buffer|null` (pure) + `writeSyncOutput(reqSource, sendResult)` (single stdout write site). SubagentStart envelope is the literal template with `JSON.stringify` on the inner text scalar only; plain path is `body + '\n'` iff non-empty; 204/empty/non-text/failure → zero stdout. Plain CommonJS, `"use strict"`, no deps, 70 lines.
- `packages/unimatrix/test/hook-client/transform.test.js` — 23 tests, `node:test`.

## Test Results
- transform suite: **23 pass / 0 fail**.
- Coverage: AC-04 envelope byte parity (structural + independent-escaper oracle), R-03 adversarial escaping (dense control chars 0x00–0x1F, quotes/backslashes, non-BMP emoji raw pass-through, U+2028/U+2029 raw pass-through, mixed CRLF, surrogate-pair-adjacent), AC-03 plain path (verbatim + one newline, println! parity), R-15 defensive drops (application/json Pong, absent/empty/wrong Content-Type, `text/plain; charset=utf-8` accepted, case-insensitive), C-05 (all five failure classes → zero stdout), ADR-002 grep-gate (exactly one `JSON.stringify` call site with `text` argument, `hookSpecificOutput` exactly once, one stdout write site, no console.log), plus a source-ASCII guard.
- Golden iteration: parity corpus not yet generated at implementation time; the golden-driven test emits a diagnostic and pins against an independent RFC-8259 escaper. Layer 1 suite (parity-corpus component) owns byte-authority golden comparison per ADR-001/ADR-002.
- Full package suite: 6 pre-existing failures in merge-settings/init tests (LD_LIBRARY_PATH command-format expectations) — outside this component's scope, untouched by this change.

## Commit
`ffd8e806` — `impl(transform): host-envelope stdout via ADR-002 literal templates (#679)`

## Issues / Blockers
None for this component. Note for the tester: merge-settings/init suites fail on this branch independent of hook-client work.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-002 (#4752), remote /observe sync-injection pattern (#4703), hook-side stdout dispatch (#3324); confirmed spec, no contradictions. context_search for stdout serialization patterns — no overriding prior art.
- Stored: entry #4769 "Build adversarial test strings via fromCharCode/fromCodePoint, never bare escape literals in JS source" via /uni-store-pattern (file-authoring tools converted bare \uXXXX escapes to raw control bytes, corrupting the test source; relevant to all remaining vnc-026 adversarial suites).
