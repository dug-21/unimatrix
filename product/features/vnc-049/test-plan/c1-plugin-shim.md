# Test Plan — C1 OpenCode Plugin Shim (TS, net-new)

`packages/unimatrix/opencode-plugin/` — maps 7 canonical events from OpenCode typed hooks + event
bus to Claude-shaped `HookInput`; carries `--provider opencode` + `--model`; derives subagent
alignment; shells to `unimatrix hook`. Net-new module, must ship at/under the 500 code-line cap.

Risks owned: **R-11, R-12, R-13, R-14, R-16** (primary), contributes to **R-01, R-06, R-07, R-15**.
ACs: AC-01 (7-event mapping + parity gap), AC-04 (subagent observe-channel), contributes AC-06.

Test surface: node/jest (or the edge client's existing test runner — extend it, do not fork). Drive
the plugin's `Hooks` handlers with OpenCode-shaped inputs; assert on the constructed `HookInput`
frame and the shell invocation (`unimatrix hook <EVENT> --provider opencode --model <...>`), spying
the `$` Bun-shell/exec boundary. Where a behavior is only observable on the stored record, the
authoritative assertion lives in c5 (crossing test) / infra-001 — this plan covers the shim's
frame-construction and fail-open contract.

## Unit expectations

### 7-event mapping (FR-01, AC-01, R-16)
- `test_plugin_maps_session_created_to_sessionstart` — `session.created` bus event → `HookInput`
  with `hook_event_name="SessionStart"`, `cwd` derived from `PluginInput.directory`, no bogus
  `transcript_path`.
- `test_plugin_maps_chat_message_to_userpromptsubmit` — `chat.message` → `UserPromptSubmit`,
  `prompt` from message/parts, `--model` carries the resolved `providerID/modelID`.
- `test_plugin_maps_tool_execute_before_to_pretooluse` — `tool.execute.before` → `PreToolUse`,
  `extra.tool_name`/`extra.tool_input` from args.
- `test_plugin_maps_tool_execute_after_to_posttooluse` — `tool.execute.after` → `PostToolUse`,
  failure semantics synthesized from output/metadata; failure → `extra.exit_code` non-zero.
- `test_plugin_maps_experimental_compacting_to_precompact` — present-path lands (R-12).
- `test_plugin_maps_session_idle_to_stop` — `session.idle` → `Stop`, degraded/bus-derived.
- `test_plugin_derives_subagentstart_from_child_session_created` — child `session.created` with
  non-null `parentID` + validated agent → `SubagentStart` frame (see R-06/R-07 below).

### Parity-gap honesty (FR-12, R-16, AC-01)
- `test_plugin_subagent_injection_not_attempted` — SubagentStart retrieval-**injection** is never
  attempted; observation frame is emitted, injection path is absent. Recorded as measured parity
  gap, NOT a false-pass and NOT a silent drop. Assert no injection call is made and the observe
  frame IS emitted.
- `test_plugin_degraded_legs_marked_not_full_fidelity` — SessionStart/Stop frames carry a
  degraded/bus-derived marker (or omit fields honestly) rather than fabricating full-fidelity
  values.

### Stop over-count guard (FR-02, R-11)
- `test_plugin_repeated_session_idle_single_stop` — repeated `session.idle` transitions produce
  exactly one Stop emission (over-count guard). Concrete assertion (exactly-one vs debounce window)
  pinned to the delivery-time PoC (OQ-3/R-11); the guard behavior — no inflation — is the invariant.
- `test_plugin_stop_computes_duration_outcome_plugin_side` — `duration`/`outcome` computed
  plugin-side for the bus-derived Stop.

### PreCompact experimental dependency (FR-12, R-12, C-6)
- `test_plugin_precompact_present_lands` — with `experimental.session.compacting` available, the
  leg fires.
- `test_plugin_precompact_absent_failsafe` — with the experimental API absent/renamed, the leg
  fails safe (flag/gate default), does NOT crash the session, and is documented as parity gap — NOT
  a cap failure. Assert no throw escapes to the session and the other legs still function.

### Fail-open transport (NFR-02, R-13)
- `test_plugin_emit_failure_does_not_block_session` — `unimatrix` binary absent → the handler
  resolves (does not throw/hang), OpenCode session continues. Spy the `$`/exec boundary to simulate
  a non-zero exit / ENOENT.
- `test_plugin_socket_down_failopen` — UDS socket unavailable → same fail-open; event dropped, no
  crash. Reuses the existing queue/drop-detector — the plugin adds no blocking wait.

### Bus-derived field synthesis (R-14)
- `test_plugin_cwd_derived_from_plugininput` — `cwd`/`worktree` derived correctly from
  `PluginInput.directory`/`.worktree`.
- `test_plugin_absent_transcript_path_handled` — SessionStart/Stop lack `transcript_path`; the
  frame omits it rather than storing an empty/bogus value.
- `test_plugin_malformed_bus_payload_degrades_honestly` — empty/malformed bus payload does not
  produce a frame claiming full fidelity; degrades or drops honestly, no corrupt fields.

### Subagent observe-channel provenance (FR-07, R-06, R-07, AC-04)
- `test_plugin_session_agent_rides_extra_agent_type` — the validated `session.agent` is written to
  `extra.agent_type` (observe channel), NEVER into MCP tool args.
- `test_plugin_conflicting_tool_args_agent_not_used` — a crafted event with a conflicting agent
  value in tool args does NOT override the validated `session.agent`. (Spoof-rejection; protects
  AC-04 alignment and the AC-07 seam.)
- `test_plugin_subagent_carries_parentid` — the SubagentStart frame carries `parentID` for the
  parent→cycle correlation performed downstream (alignment asserted on the stored record in c5).

### Untrusted-input validation at the shim boundary (R-15)
- `test_plugin_model_id_over_length_rejected` — a `model_id` violating `^[a-z0-9_-]{1,64}$` (too
  long / bad chars) is rejected or sanitized at the shim before shelling — never passed raw as
  `--model`. (Defense in depth; the authoritative validation is also enforced server-side, c4/c5.)

## Edge cases (from Risk Strategy)
- Frame with no backend model (cloud/absent) — `--model` omitted, frame still valid (pairs with
  R-10.2 back-compat in c4).
- `session.idle` firing repeatedly (R-11).
- PreCompact API absent/renamed (R-12).
- Underivable `cwd` (R-14).

## What this component does NOT prove here
- Stored-record `source_domain="opencode"` / provider / model_id distinctness — assembled-path,
  proven in c5 + infra-001 (driving from the plugin/CLI entry point). A shim unit test asserting
  "the `--model` flag was passed" is a proxy for AC-06 and does NOT discharge it (AC-06 is observed
  on the queried record, R-01).
