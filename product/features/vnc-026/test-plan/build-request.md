# Test Plan: build-request.js (parity port)

Oracle: `hook.rs:440-951` (`build_request`, `extract_topic_signal`, `validate_cycle_params`,
`is_bash_failure`, `extract_file_path`). Risks: R-01 (Critical), R-02, R-19, AC-01, AC-14.
Suite: `test/hook-client/build-request.test.js` (targeted units) + the Layer 1 parity runner
`test/hook-client/parity-layer1.test.js` (authoritative — every case below has a corpus golden;
NO hand-written expected request values, #2984).

## Layer 1 Parity Cases (executed against goldens; structural equality after normalizing `timestamp`→0, `ppid-\d+`→`ppid-X`)

### Event coverage (R-01 scenario 1 / ADR-001 inventory)
- All 13 canonical events; Gemini aliases; unknown-event passthrough (raw name in generic observation); empty stdin; malformed stdin JSON; missing `session_id` (→ `ppid-${process.ppid}` fallback, normalized in comparison); missing `cwd`.

### UserPromptSubmit (MIN_QUERY_WORDS = 5 gate)
- empty prompt; whitespace-only; exactly 4 words (→ RecordEvent); exactly 5 words (→ ContextSearch); long multi-word.

### PostToolUse rework extraction
- Bash `exit_code` 0 / non-zero / missing / non-integer; `interrupted` true/false; Edit (`tool_input.path`); Write (`tool_input.file_path`); MultiEdit normal (→ RecordEvents fan-out) / empty `edits` / missing `edits` / non-array `edits`; non-rework tool; non-claude-code provider.

### PostToolUseFailure (explicit arm)
- nominal; empty `extra`; null `extra`; missing `tool_name`; null error.

### PreToolUse context_cycle interception
- bare `context_cycle`; `mcp__unimatrix__context_cycle`; near-miss names (`context_cycles`, `mcp__other__context_cycle`) must NOT intercept; invalid cycle params → fall through to generic observation; `mcp_context` promotion; goal > MAX_GOAL_BYTES=1024 truncated at a multi-byte boundary (byte-safe).

### SubagentStart
- `prompt_snippet` present / empty / whitespace; fallback query via transcript tail (cases owned by transcript.md, wired through here: result is ContextSearch with `source: "SubagentStart"`, role from `agent_type`); RecordEvent fallback when no query derivable.

### Topic-signal extraction
- payloads with/without topic signal; assert field equality with golden.

## Targeted Unit Tests (fast-fail locality on top of parity)

- `test_build_request_pure` — no fs/network/process side effects (spies); deterministic given fixed `process.ppid`.
- `test_unknown_stdin_fields_preserved` — **ass-071 carry-in**: stdin with extra unknown fields (`{"future_field": {...}, "subagent_id": "x"}`) → fields survive into the request payload exactly as Rust's `extra` flatten (`wire.rs:71-72`) preserves them: not dropped, not reordered relative to the golden. Corpus case `unknown-stdin-fields` is mandatory.
- `test_ppid_collision_documented` — R-19: two inputs missing `session_id`, same `process.ppid` → identical `ppid-{N}` session id (parity with Rust fallback; shared-offset consequence documented, sanitization tested in state.md).

## Contract Round-Trip (AC-14 / FR-25)

- Extend the `contract.test.mjs` pattern: every distinct HookRequest variant the builder can emit
  (Ping, SessionRegister, SessionClose, RecordEvent, RecordEvents, ContextSearch, CompactPayload)
  round-trips against `crates/unimatrix-engine/bindings/fixtures/*.json`, **including**
  `transcript_delta_payload.json` for the delta frame (built in delta.md, validated here).
- Assert raw `session_id` on the wire — no `http-` prefix ever (integration-risk row).

## Coverage Requirement (R-01/R-02 gate)

Every case above appears in the corpus manifest with a named fixture; the manifest audit
(parity-corpus.md) proves no `build_request` arm lacks a case. Adversarial-content cases
(control chars, emoji, lone-surrogate-adjacent, embedded quotes/backslashes) included — they
feed transform.md's byte goldens too.
