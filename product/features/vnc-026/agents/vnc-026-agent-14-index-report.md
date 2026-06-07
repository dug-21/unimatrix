# Agent Report — vnc-026-agent-14-index

Component: `lib/hook-client/index.js` (entry / dispatch)

## Files
- Created: `packages/unimatrix/lib/hook-client/index.js`
- Created: `packages/unimatrix/test/hook-client/index.test.js`
- Committed: a41691c4 on `feature/vnc-026`

## Result
- Pipeline ports hook.rs::run() step-for-step with the OVERVIEW deviations
  (HTTP transport, ADR-003 queue, client deltas, no client PreCompact prepend).
- Index suite: 36/36 pass.
- Full hook-client suite: 307 pass, 0 fail, 1 pre-existing intentional skip.
- Package suites (`test/*.test.js`): 115 pass, 1 fail =
  `test_creates_mcp_json_on_clean_project` (known stale, out of scope per brief).
- Integration tests: not run / not modified (per instruction).
- No Rust touched; cargo workspace unaffected.

## Implementation notes / deviations from pseudocode
- Pseudocode referenced `state.queueDepthFast` and `state.queueDepthFast` for
  the breadcrumb depth; that export does not exist. Used `queue.queueDepth
  (config.stateDir)` (the actual export). Behavior identical (count of `*.json`
  in queue dir), non-throwing.
- `delta.maybeSendDelta` returns `{attempted, send?}` (a DeltaOutcome), not a
  bare SendResult. index extracts `.send` only when `attempted === true` before
  feeding the breadcrumb / composing the delta stderr line — matches state.js
  `recordSendOutcomes` aggregation (carrying class wins; null entries excluded).
- Removed the literal `'/dev/stdin'` from an index.js comment so the closed
  gate-note-1 grep-gate (string must not appear in `lib/hook-client/`) passes;
  mechanism is `fs.readFileSync(0)` as mandated by FR-01.

## Verification highlights
- AC-08 / R-13: sync spawn creates no queue dir and persists no offsets; exactly
  one POST. SubagentStart->ContextSearch (sync) performs the single RQ-6 tail
  read and still one POST.
- AC-09: `Promise.allSettled` with rejected-promise -> synthetic connect failure
  for the carrying result and non-attempt for the delta outcome (independence).
- C-05: exit-0 / zero-stdout matrix across malformed stdin, missing config,
  partial env, ECONNREFUSED, 401, 500, unwritable state dir, directory
  transcript_path.
- FNF ordering: pre-seeded queue frame replays BEFORE the carrying event;
  carrying + delta both issued (2 POSTs).

## ass-071 freebie
- SubagentStop raw-stdin dump: NOT captured. Requires a real Claude Code session
  payload; only synthetic stdin is available in this environment, and a fabricated
  payload would mislead ass-071/crt-052. Declined deliberately (advisory, no
  assertion).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced ADR-004
  (#4759 deltas never queued), ADR-007 (#4757 concurrent separate POST), ADR-003
  (#4753 state/queue mini-spec); applied all three in the FNF orchestration.
- Stored: entry #4774 "node:test spawn-level hook tests must use async spawn,
  never spawnSync, when the stub server runs in-process" via
  context_store (pattern, topic unimatrix-hook-client). Also captures the
  state-dir-mismatch and spawnSync input-encoding gotchas.
