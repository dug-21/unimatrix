# Agent Report — vnc-026-agent-20-rework-size

## Task
Stage 3b rework: bring `packages/unimatrix/lib/hook-client/` under the AC-12 / C-04
100 KB CI size gate (`test/check-hook-client-size.js`, limit 100,000 bytes decimal).
Behavior-preserving comment-prose trimming only.

## Result
- Size: 104,240 bytes -> 97,734 bytes (97.7 KB). PASS with 2.3 KB margin (≈ 97 KB aim).
- Suite: `npm run test:hook-client` = 421 tests / 419 pass / 0 fail / 1 skip / 1 todo
  (identical to pre-rework baseline). Layer-2 parity goldens 8/8 — byte-for-byte
  wire behavior unchanged. All files `node --check` clean.
- No file exceeds 500 lines (largest: build-request-tools.js 452).

## Approach
Condensed verbose JSDoc/header/inline comment prose across all 13 modules. Kept
all load-bearing parity anchors (hook.rs:NNN, transcript_block.rs, attribution.rs,
validation.rs line refs) and ADR references — Gate 3b traceability. No minify, no
identifier renames, no file merges, no logic restructure, no check-script change.

## Files Modified (all under packages/unimatrix/lib/hook-client/)
- build-request-tools.js
- build-request.js
- config.js
- cycle-validation.js
- delta.js
- index.js
- normalize.js
- queue.js
- state.js
- topic-signal.js
- transcript.js
- transform.js
- transport-http.js

Committed on feature/vnc-026: `rework: trim hook-client under 100 KB size gate (#679)`
(13 files, +311 / -474).

## Issues / Blockers
None. The task brief cited "428 pass" pre-rework; the test-runner summary reports at
the runner level as 419 pass / 0 fail (subtests counted differently). fail=0 is the
load-bearing invariant and is unchanged.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced 500-line-gate split patterns
  and vnc-025/vnc-026 ADRs (transcript tail-read, parity-oracle corpus); confirmed
  no existing lesson on the hook-client byte-size gate.
- Stored: entry #4780 "JS hook client lib/hook-client/ is gated by a 100 KB decimal
  CI byte-size check; trim comment prose only, never minify or inflate the limit"
  via /uni-store-lesson (rework/bugfix-class -> lesson over pattern).
