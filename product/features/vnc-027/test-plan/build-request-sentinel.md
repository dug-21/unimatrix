# Test Plan — build-request-sentinel (`lib/hook-client/build-request-tools.js`)

Component 6 / ADR-004 §1 / FR-27 / **AC-08** / Risk R-11 (High).
`buildCycleEventOrFallthrough` returns `null` sentinel on every non-cycle PreToolUse path (stderr lines retained);
fallthrough RecordEvent observation removed for PreToolUse ONLY. `node --test` on `build-request.test.js`.

## Sentinel matrix — R-11 s2 (every non-cycle path → null, no frame, no queue entry)

- `test_non_cycle_tool_name_returns_null` — an ordinary tool name (e.g. `Bash`) → `null`; no RecordEvent fallthrough.
- `test_missing_tool_input_returns_null_retains_stderr` — missing `tool_input` → `null`, existing stderr diagnostic line still emitted.
- `test_failed_validate_cycle_params_returns_null_retains_stderr` — `validateCycleParams` failure → `null` + retained stderr.
- `test_valid_cycle_start_returns_frame_parity` — valid `context_cycle` start → cycle frame byte-identical to the Rust hook's (cycle frames stay in the byte-parity corpus). Same for `phase-end` and `stop`.

## F-02 exact-equality security gate — R-11 s3 (defense in depth)

- `test_exact_equality_only_context_cycle_intercepted` — only exact `context_cycle` and `mcp__unimatrix__context_cycle` produce a frame.
- `test_evil_substring_bypass_sends_nothing` — `evil_context_cycle_bypass` (a regex-substring match that the narrowed install matcher would let SPAWN the hook) → `null` sentinel, sends nothing. The exact-equality gate holds even when the matcher admits the spawn (the two-layer defense).
- `test_near_miss_suffixed_not_intercepted` — `context_cycle_extra` / suffixed names → `null` (reuse `cycle-near-miss`, `cycle-near-miss-suffixed` corpus cases).

## Scope guard — only PreToolUse changes (FR-27)

- `test_posttooluse_fallthrough_untouched` — PostToolUse / PostToolUseFailure fallthrough RecordEvent observation is UNCHANGED (only PreToolUse gets the sentinel) — R-11 s6 regression guard.
- `test_other_event_builders_unchanged` — SessionStart/UserPromptSubmit/SubagentStart/PreCompact/Stop builders produce identical frames to F3 (corpus parity).

## Edge cases
- `tool_input` present but malformed (non-object) → `null` + stderr, not a throw.
- Cycle frame goldens remain fully parity-tested (Layer 1 corpus) — the reduction is event-set divergence, not frame-format divergence (FR-21).
