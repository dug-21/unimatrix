# Agent Report: vnc-026-agent-10-build-request

## Task
Implement `lib/hook-client/build-request.js` — full `build_request` parity port
(hook.rs:440-951): ppid/cwd fallback, MIN_QUERY_WORDS=5, PostToolUse rework
extraction, MultiEdit fan-out, PostToolUseFailure arm, context_cycle
interception (MAX_GOAL_BYTES=1024), topic-signal extraction, SubagentStart
prompt_snippet path.

## Files Modified
- `packages/unimatrix/lib/hook-client/build-request.js` (148 lines) — dispatch
- `packages/unimatrix/lib/hook-client/build-request-tools.js` (465 lines) — arm
  builders + rework helpers + topic-signal selection + shared implantEvent/nowSecs
- `packages/unimatrix/lib/hook-client/cycle-validation.js` (127 lines) —
  validate_cycle_params port (validation.rs:411-509)
- `packages/unimatrix/lib/hook-client/topic-signal.js` (183 lines) —
  attribution.rs:15-92 chain
- `packages/unimatrix/test/hook-client/build-request.test.js` (78 tests)

All four lib files are under the 500-line gate. The pseudocode anticipated a
single split (`build-request-tools.js`); a second split (`cycle-validation.js`)
was required because tools.js alone landed at 561 lines.

## Tests
- Component suite: 78 pass / 0 fail (incl. 6 corpus golden spot-checks:
  ptu-bash-exit-zero, ptu-multiedit-fanout, cycle-mcp-context-promotion,
  event-session-start, event-ping, event-precompact).
- Full hook-client suite (`node --test "test/hook-client/*.test.js"`):
  272 pass / 0 fail / 1 skipped (skip pre-existing, not mine).
- Did NOT run or modify integration tests (per instructions).
- The 6 pre-existing merge-settings/init LD_LIBRARY_PATH failures are outside
  hook-client and were not exercised.

## Parity Decisions / Notes
- `extra` flatten parity: `payloadFromExtra` returns `input.extra` as-is —
  `null` on parse failure, `{}` when parsed with no unknown keys. Verified
  unknown-field preservation + insertion order (ass-071 carry-in,
  `test_unknown_stdin_fields_preserved`).
- `is_bash_failure` honors `as_i64`/`as_bool` parity: `Number.isInteger` rejects
  1.5/"2"/true for exit_code; only JSON `true` counts for interrupted.
- Cycle validation: topic uses BYTE length (`Buffer.byteLength`, validation.rs
  `str::len()`); phase/outcome use CODE-POINT count (`Array.from().length`,
  Rust `chars().count()`). This dual-measure split is the easy-to-miss trap.
- Goal truncation reuses `truncateUtf8` from transcript.js (byte-boundary safe);
  emoji-overflow test confirms no split surrogate / replacement char.
- mcp_context promotion clones input (Object.assign + shallow extra copy);
  `test_cycle_mcp_context_promotion_does_not_mutate_input` asserts caller input
  unmutated (R-01).
- Security gate F-02: exact-equality tool-name match; near-miss names
  (`context_cycles`, `mcp__other__context_cycle`, `evil_context_cycle_bypass`)
  fall through to generic, asserted.
- Optional-field encoding matches bindings fixtures: ContextSearch omits
  `source` key (None), CompactPayload omits `transcript_excerpt`, ImplantEvent
  omits `topic_signal`/`provider` when null.
- `implantEvent`/`nowSecs` exported from build-request.js (and tools.js) per
  OVERVIEW.md helper contract for delta.js to consume.

## Issues / Blockers
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search -- NOT AVAILABLE.
  The Unimatrix MCP tools could not be resolved in this session (deferred
  ToolSearch for context_search / context_store / context_briefing returned no
  matches). Per the non-blocking guidance, proceeded using the read-only Rust
  oracles (hook.rs, attribution.rs, validation.rs), the committed parity corpus
  goldens, and ts-rs bindings fixtures as the authority.
- Stored: UNABLE TO STORE -- context_store tool unavailable this session.
  Pattern worth capturing for a future steward (unimatrix-edge / hook-client):
  "Cycle-param validation port mixes measurement units -- topic limit is BYTE
  length (Rust str::len) while phase/outcome limits are CODE-POINT count (Rust
  chars().count()). Porting both as String.length silently diverges on
  multibyte input. Use Buffer.byteLength for topic, Array.from().length for
  phase/outcome."
