# Risk-Based Test Strategy: vnc-013
## Canonical Event Normalization for Multi-LLM Hook Providers

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `mcp_context.tool_name` promotion fails silently — Gemini `BeforeTool` falls through to generic `RecordEvent` instead of `cycle_start`/`cycle_stop` | High | High | Critical |
| R-02 | `ImplantEvent.provider` is `None` at one or more `build_request()` construction sites — silent `"claude-code"` mislabel at Site A | High | Med | High |
| R-03 | Codex events mislabeled as `"claude-code"` when `--provider codex-cli` is omitted from reference config or operator installs without it | High | High | High |
| R-04 | Approach A fallback to `"claude-code"` silently regresses for `"Stop"`, `"SessionStart"`, `"cycle_start"`, `"cycle_stop"` if registry lookup contract changes | High | Low | High |
| R-05 | Rework detection enters for Gemini `AfterTool` (normalized `"PostToolUse"`) due to missing or incorrect provider gate | High | Med | High |
| R-06 | `test_parse_rows_unknown_event_type_passthrough` contract breaks if Approach A fallback is implemented without restoring `"claude-code"` for unknown types | Med | Med | Medium |
| R-07 | `extract_observation_fields()` `"post_tool_use_rework_candidate"` guard absent or misplaced — internal string escapes to `hook` column | Med | Low | Medium |
| R-08 | Gemini `SessionEnd` normalized to `"Stop"` but `SessionClose` dispatch arm not reached — session deregistration silently skipped | Med | Med | Medium |
| R-09 | Blast radius miss: one of the 6 files changed without AC coverage (AC-05 for wire.rs, AC-07 for observation.rs, AC-16 for listener.rs, AC-07b for background.rs) | Med | Med | Medium |
| R-10 | `extract_event_topic_signal()` degrades silently for Gemini `BeforeTool` events if `tool_input` is not at top-level in Gemini payload | Med | Low | Medium |
| R-11 | Gemini `AfterTool` `response_size`/`response_snippet` null due to field name mismatch — no test catches the silent null | Low | Med | Low |
| R-12 | `.gemini/settings.json` matcher regex `mcp_unimatrix_.*` invalid in Gemini CLI v0.31+ — no hooks fire, no error surfaced | Low | Low | Low |
| R-13 | Backward deserialization regression: existing Claude Code hook JSON fails to deserialize after `HookInput` gains `provider`/`mcp_context` fields | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: `mcp_context.tool_name` Promotion — Silent Fallthrough (SR-08)

**Severity**: High | **Likelihood**: High | **Impact**: Gemini `context_cycle` calls never write `cycle_start`/`cycle_stop` to `cycle_events`. `context_cycle_review` finds nothing for Gemini-sourced sessions. The failure is invisible — exit code 0, no error log, only missing data.

**Root cause**: `build_cycle_event_or_fallthrough()` reads `input.extra["tool_name"]` at its first step. For Gemini payloads, this key is absent — the tool name is at `input.mcp_context["tool_name"]`. Without the promotion adapter, the function finds no tool name, falls through to `generic_record_event`, and produces `HookRequest::RecordEvent { event_type: "PreToolUse" }` instead of `"cycle_start"`.

**Historical evidence**: Entry #4298 (hook-normalization-boundary pattern) explicitly flags this promotion step as the single highest-risk integration point and mandates a dedicated test before other Gemini BeforeTool ACs are attempted.

**Test Scenarios**:
1. Unit test `test_gemini_mcp_context_tool_name_promotion`: construct a synthetic Gemini `BeforeTool` payload with `mcp_context: { "tool_name": "context_cycle", "server_name": "unimatrix" }` and `tool_input: { "type": "start", "feature": "test-feat" }`. Deserialize into `HookInput`. Assert `input.mcp_context` is `Some`. Call the promotion adapter. Call `build_cycle_event_or_fallthrough()`. Assert the returned `HookRequest` is `RecordEvent { event_type: "cycle_start" }`.
2. Unit test `test_gemini_before_tool_non_cycle_no_promotion_needed`: `mcp_context.tool_name = "context_search"`. Assert result is `RecordEvent { event_type: "PreToolUse" }` (fallthrough, not cycle_start).
3. Unit test `test_mcp_context_missing_tool_name_degrades_gracefully`: `mcp_context` present but `tool_name` key absent. Assert result is `RecordEvent { event_type: "PreToolUse" }`, no panic.
4. Integration test (AC-02, AC-09): inject a synthetic Gemini `BeforeTool`+`context_cycle(type="start")` event through the full hook-to-listener path. Assert `cycle_events` row exists with `event_type = "cycle_start"` and `source_domain = "gemini-cli"`. Run `context_cycle_review` on the feature and assert it returns results.

**Coverage Requirement**: AC-14 (unit: deserialization + promotion) and AC-02 (integration: cycle_events write) must both pass. AC-14 is a gate prerequisite — implement and green-bar it before any other Gemini `BeforeTool` AC is attempted.

---

### R-02: `ImplantEvent.provider` is `None` at Construction Sites (SR-08 secondary)

**Severity**: High | **Likelihood**: Med | **Impact**: Site A `listener.rs` falls back to `"claude-code"` for Gemini or Codex events. No error. Source attribution is silently wrong in the written `ObservationRecord`.

**Root cause**: `build_request()` and `build_cycle_event_or_fallthrough()` each construct `ImplantEvent` directly. If any construction site is missed when threading `provider`, it is `None`. ADR-002 specifies a `debug_assert!(event.provider.is_some())` canary in `listener.rs` but this only catches misses at runtime (debug builds only).

**Test Scenarios**:
1. Unit test `test_implant_event_provider_set_for_all_variants`: for each `HookRequest` variant produced by `build_request()` with each canonical event name, assert that the embedded `ImplantEvent.provider` is `Some`. Cover: `SessionRegister`, `SessionClose`, `RecordEvent` (PreToolUse, PostToolUse, PostToolUseFailure, SubagentStart, SubagentStop), `ContextSearch` (UserPromptSubmit, SubagentStart), `CompactPayload`.
2. Unit test `test_cycle_event_provider_propagated`: call `build_cycle_event_or_fallthrough()` with provider `"gemini-cli"`. Assert the returned `ImplantEvent.provider == Some("gemini-cli")`.
3. Unit test `test_provider_none_falls_back_at_listener`: deserialize `ImplantEvent { provider: None }` and simulate Site A derivation. Assert `source_domain == "claude-code"` (fallback). This documents the known degraded case.

**Coverage Requirement**: AC-05. Every `ImplantEvent` construction path must be exercised with provider asserted non-None.

---

### R-03: Codex `--provider codex-cli` Omission — Silent Mislabel (SR-01)

**Severity**: High | **Likelihood**: High | **Impact**: Codex events attributed as `"claude-code"` on the write path. Detection rules scoped to `source_domain` will misfire or miss. Behavioral rework is not blocked (Codex doesn't fire hooks today due to #16732) but the reference config is the only guard against future mislabeling.

**Root cause**: Codex shares all event names with Claude Code. `normalize_event_name("PreToolUse", None)` returns `("PreToolUse", "claude-code")` — correct for Claude Code, wrong for Codex. The `--provider codex-cli` flag is the only discriminator. ADR-006 mandates the flag in the reference config but cannot enforce it at runtime.

**Test Scenarios**:
1. Unit test `test_normalize_codex_with_provider_hint`: assert `normalize_event_name("PreToolUse", Some("codex-cli"))` returns `("PreToolUse", "codex-cli")`. Repeat for `PostToolUse`, `SessionStart`, `Stop`.
2. Unit test `test_normalize_shared_name_without_hint_defaults_to_claude_code` (AC-18): assert `normalize_event_name("PreToolUse", None)` returns `("PreToolUse", "claude-code")`. Documents the silent-mislabel risk explicitly.
3. Unit test `test_codex_provider_flag_threads_to_implant_event` (AC-17): simulate `run()` receiving `provider = Some("codex-cli")` and event `"PreToolUse"`. Assert the resulting `ImplantEvent.provider == Some("codex-cli")`.
4. Config review test (AC-19): assert `.codex/hooks.json` contains `--provider codex-cli` on every hook invocation line. Assert caveat text about Codex bug #16732 is present.

**Coverage Requirement**: AC-17, AC-18, AC-19. The config content check (AC-19) is mandatory — without it, the blast radius of a missing flag is undetected.

---

### R-04: Approach A Fallback Contract Regression — Sites B and C (SR-03)

**Severity**: High | **Likelihood**: Low | **Impact**: `source_domain` changes from `"claude-code"` to `"unknown"` for `"Stop"`, `"SessionStart"`, `"cycle_start"`, `"cycle_stop"`, `"UserPromptSubmit"`, `"PreCompact"`, `"PostToolUseFailure"` on the DB-read paths. Downstream consumers checking `source_domain == "claude-code"` on session or cycle events would silently receive wrong results.

**Root cause**: The builtin claude-code pack only lists 4 event types. `resolve_source_domain()` returns `"unknown"` for everything else. Approach A restores `"claude-code"` via `DEFAULT_HOOK_SOURCE_DOMAIN` fallback. If the fallback is accidentally removed, dropped, or the condition is inverted, all non-listed event types return `"unknown"`.

**Test Scenarios**:
1. Unit test `test_parse_rows_unknown_event_type_passthrough` (existing — must be preserved with updated comment): assert `source_domain == "claude-code"` for `"UnknownEventType"`. This is the primary regression sentinel for Approach A fallback correctness.
2. Unit test `test_approach_a_fallback_for_stop_event`: call `parse_observation_rows()` (or the equivalent background.rs path) with `event_type = "Stop"`. Assert `source_domain == "claude-code"`. ("Stop" is not in the builtin pack's 4-event list.)
3. Unit test `test_approach_a_fallback_for_cycle_events`: same for `"cycle_start"` and `"cycle_stop"`. These are Category 2 events frequently used in `context_cycle_review` — wrong `source_domain` on the read path would corrupt retrospective attribution.
4. Unit test `test_parse_rows_hook_path_always_claude_code` (existing — must remain unchanged): assert `source_domain == "claude-code"` for `"PreToolUse"` (registry resolves it directly).

**Coverage Requirement**: AC-07(b) and AC-07(c). Both Sites B and C must have explicit tests for both listed and non-listed event types to verify the fallback branch is exercised.

---

### R-05: Rework Detection Entered for Gemini `AfterTool`

**Severity**: High | **Likelihood**: Med | **Impact**: `is_rework_eligible_tool()` is called for Gemini `AfterTool` events. Today this is accidentally safe (Unimatrix MCP tool names don't match the `Bash`/`Edit`/`Write`/`MultiEdit` allowlist). If Gemini adds a built-in tool named `Bash` or `Edit`, or if the matcher regex changes, rework tracking fires incorrectly for Gemini tool calls. ADR-005 resolves OQ-1 explicitly with a provider gate.

**Test Scenarios**:
1. Unit test `test_gemini_after_tool_skips_rework_path` (AC-04, AC-12): call `build_request()` with canonical event `"PostToolUse"` and `provider = "gemini-cli"`. Assert the returned `HookRequest` is `RecordEvent { event_type: "PostToolUse", provider: "gemini-cli" }`. Assert `is_rework_eligible_tool()` is never called (spy/mock, or restructure so the guard returns before the call site).
2. Unit test `test_codex_post_tool_use_skips_rework_path`: same with `provider = "codex-cli"`. Rework detection is Claude Code-only.
3. Unit test `test_claude_code_post_tool_use_enters_rework_path`: assert Claude Code `"PostToolUse"` with `provider = "claude-code"` DOES reach `is_rework_eligible_tool()`. Verifies the gate does not over-block.
4. Code review assertion: gate expression in `"PostToolUse"` arm references the `provider` variable, not a tool-name predicate. Comment cites ADR-005.

**Coverage Requirement**: AC-04, AC-12. The test for Gemini skip (scenario 1) must verify the gate mechanism, not just the output, to protect against future tool-name overlap.

---

### R-06: `test_parse_rows_unknown_event_type_passthrough` Contract Break

**Severity**: Med | **Likelihood**: Med | **Impact**: Test fails post-implementation if Approach A fallback is misimplemented. Indicates a real behavioral regression where unknown event types return `"unknown"` instead of `"claude-code"` — breaks consumers.

**Test Scenarios**:
1. Review existing test: confirm it calls `parse_observation_rows()` with `event_type = "UnknownEventType"` and asserts `source_domain == "claude-code"`.
2. Update test comment only — remove "always claude-code" framing; replace with: "registry returns `unknown` for unregistered types; Approach A fallback to `DEFAULT_HOOK_SOURCE_DOMAIN` restores `claude-code` to preserve hook-path invariant (FR-06.4)."
3. Add a second assertion in the same test: confirm `DEFAULT_HOOK_SOURCE_DOMAIN == "claude-code"` so the constant value is visible in the test record.

**Coverage Requirement**: AC-07(c), AC-08. The test must not be deleted or skipped — it is the Approach A regression canary.

---

### R-07: `"post_tool_use_rework_candidate"` Escapes to `hook` Column (SR-04)

**Severity**: Med | **Likelihood**: Low | **Impact**: Internal routing label stored in `observations.hook`. Downstream code treating `hook` as a canonical event name would see an unknown string. Detection rules, `extract_observation_fields()` match arms, and `context_cycle_review` would encounter an unlisted event type.

**Test Scenarios**:
1. Unit test `test_rework_candidate_guard_fires_in_debug`: in debug mode, attempt to insert an observation row with `event_type = "post_tool_use_rework_candidate"` via `extract_observation_fields()`. Assert the `debug_assert!` fires (or the exhaustive guard triggers). This test must run in `#[cfg(debug_assertions)]`.
2. Code review: confirm the guard in `extract_observation_fields()` is placed before the match arm, not inside it. Confirm `PostToolUseFailure` arm is unchanged — no guard applied to it.
3. Unit test `test_post_tool_use_failure_arm_unchanged`: verify `PostToolUseFailure` events still produce `ObservationRecord.event_type == "PostToolUseFailure"` (not `"PostToolUse"`). Explicitly validates that AC-16's guard is scoped correctly per ADR-003 col-027 (entry #3475).

**Coverage Requirement**: AC-16. The `#[cfg(debug_assertions)]` guard test is the only mechanism to make the contract visible — code review alone is insufficient.

---

### R-08: Gemini `SessionEnd` → `"Stop"` Dispatch Path Not Reached

**Severity**: Med | **Likelihood**: Med | **Impact**: Session deregistration silently skipped for Gemini sessions. `SessionClose` is never written; session state may be stale in the listener.

**Root cause**: After normalization, `"SessionEnd"` arrives as `"Stop"`. The `"Stop"` arm in `build_request()` routes to `HookRequest::SessionClose`. If the normalization step happens in `run()` before `build_request()` receives the canonical name, this arm is reached correctly. If the Gemini dispatch arms in `build_request()` are implemented with pre-normalization names as a defense-in-depth guard, both paths must route to `SessionClose`.

**Test Scenarios**:
1. Unit test `test_normalize_session_end_to_stop`: `normalize_event_name("SessionEnd", None)` returns `("Stop", "gemini-cli")` (AC-01).
2. Unit test `test_gemini_session_end_produces_session_close`: call `build_request()` with canonical `"Stop"` and `provider = "gemini-cli"`. Assert `HookRequest::SessionClose`.
3. Unit test `test_gemini_session_end_defense_in_depth_arm`: if `"SessionEnd"` reaches `build_request()` unnormalized (defense-in-depth arm), assert it still produces `HookRequest::SessionClose`.

**Coverage Requirement**: AC-01 covers normalization. AC-05 and AC-15 cover dispatch. An explicit scenario for `SessionEnd` → `SessionClose` is needed to close the gap.

---

### R-09: Blast Radius Miss — File Without AC Coverage (SR-07)

**Severity**: Med | **Likelihood**: Med | **Impact**: A changed file has no test exercising its change. Regression introduced without detection. Historical evidence: entry #3492 (blast-radius bugfix gate failure) and entry #2906 (col-023 HookType refactor rework) both show that multi-file blast-radius changes accumulate rework when a site is omitted from coverage.

**Blast radius coverage table** (from ARCHITECTURE.md C-11):

| File | Change | Validating ACs | Risk if missed |
|------|--------|----------------|----------------|
| `wire.rs` | `provider` + `mcp_context` on `HookInput`/`ImplantEvent` | AC-05, AC-14 | Silent None fields; fallback mislabels |
| `hook.rs` | `normalize_event_name()`, Gemini arms, rework gate, promotion | AC-01–05, AC-11, AC-12, AC-14, AC-15, AC-17, AC-18 | Core normalization broken |
| `listener.rs` | Site A `source_domain`, AC-16 guard | AC-06, AC-07(a), AC-16 | Live write path wrong; rework candidate escapes |
| `background.rs` | Site B `source_domain` Approach A | AC-07(b) | DB read path regresses to "unknown" for non-listed events |
| `services/observation.rs` | Site C `source_domain` Approach A, `_registry` prefix removed | AC-07(c), AC-08 | Same as Site B; test comment stale |
| `main.rs` | `--provider` CLI arg on `Hook` variant | AC-15, AC-17, AC-18 | Flag silently ignored; Codex always mislabeled |

**Test Scenarios**:
1. After implementation, run `grep -n '"claude-code"'` in each of the three source_domain sites and assert no literal assignment remains — AC-07 code review check.
2. Confirm CI runs `cargo test --workspace` (AC-08): no existing tests regress, confirms additive nature.
3. Per-file checklist review during gate: each row in the blast-radius table above must have at least one green test before gate sign-off.

**Coverage Requirement**: AC-08 (regression gate), plus per-file AC coverage as mapped above.

---

### R-10: `extract_event_topic_signal()` Silent Degradation for Gemini (SR-09)

**Severity**: Med | **Likelihood**: Low | **Impact**: Topic signal stored as generic stringified JSON instead of the extracted `tool_input` for Gemini `BeforeTool` events. Affects knowledge reuse quality; not a correctness failure.

**Test Scenarios**:
1. Unit test `test_gemini_before_tool_topic_signal_extraction` (AC-11): construct synthetic Gemini `BeforeTool` payload with `tool_input: { "query": "test query" }` at top-level position. After normalization to `"PreToolUse"`, call `extract_event_topic_signal()`. Assert the returned signal is non-empty and contains `"test query"` (not a generic stringification of the whole payload).
2. If `tool_input` is NOT at top-level in real Gemini payloads, the promotion adapter (FR-04.3) must also promote `tool_input`. The spec writer must confirm from ASS-049 FINDINGS-HOOKS.md before the implementer proceeds.

**Coverage Requirement**: AC-11. This test must be written even if it initially passes trivially — it documents the `tool_input` position assumption (SR-09).

---

### R-11: Gemini `AfterTool` `response_size`/`response_snippet` Null

**Severity**: Low | **Likelihood**: Med | **Impact**: Null fields in `ObservationRecord` for all Gemini `PostToolUse` observations. Retrospective and review quality degraded for Gemini sessions. Not a data loss scenario.

**Test Scenarios**:
1. Unit test `test_gemini_after_tool_response_fields_degrade_gracefully`: construct `AfterTool` payload without `tool_response` field (or with a different field name). Assert `build_request()` produces `RecordEvent` without panic. `response_size` and `response_snippet` are null.
2. If implementer confirms Gemini's response field name during implementation, add a positive assertion test using the confirmed name.

**Coverage Requirement**: OQ-C resolution documented; degraded-mode behavior is explicitly tested.

---

### R-12: Gemini Matcher Regex Invalid

**Severity**: Low | **Likelihood**: Low | **Impact**: No `BeforeTool`/`AfterTool` hooks fire for Unimatrix tools under Gemini CLI. System silently works without hook events.

**Test Scenarios**:
1. Config review (AC-10): assert `.gemini/settings.json` is present, valid JSON, and `matcher` field is `"mcp_unimatrix_.*"`. Format validation against Gemini CLI v0.31+ docs.
2. Confirm the pattern covers all 12 tool names (`context_search`, `context_lookup`, `context_get`, `context_store`, `context_correct`, `context_deprecate`, `context_status`, `context_briefing`, `context_quarantine`, `context_enroll`, `context_retrospective`, `context_cycle`).

**Coverage Requirement**: AC-10.

---

### R-13: Backward Deserialization Regression on `HookInput`

**Severity**: Low | **Likelihood**: Low | **Impact**: Existing Claude Code hook invocations fail to deserialize because `HookInput` gained required (not optional) fields.

**Test Scenarios**:
1. Unit test `test_hook_input_deserializes_without_new_fields` (AC-08, NFR-05): deserialize a minimal Claude Code `BeforeTool`-equivalent JSON (`{ "event_type": "PreToolUse", "tool_name": "Bash" }`) into `HookInput`. Assert `provider == None`, `mcp_context == None`. No deserialization error.
2. Unit test `test_implant_event_deserializes_without_provider`: deserialize an `ImplantEvent` JSON that omits `provider`. Assert `provider == None`.

**Coverage Requirement**: AC-08 (all existing tests pass). The `#[serde(default)]` annotation is the mechanism; tests verify the annotation is present and effective.

---

## Integration Risks

**Cross-crate `provider` threading**: `ImplantEvent` is defined in `unimatrix-engine` and consumed in `unimatrix-server`. The new `provider` field must be serialized and deserialized across the UDS wire frame. The `skip_serializing_if = "Option::is_none"` on `ImplantEvent.provider` means Claude Code events that omit `--provider` produce wire frames without the field — the listener must handle missing field as `None` without error. This is a deserialization boundary risk between crates, not just within a function.

**`build_cycle_event_or_fallthrough()` call-site coupling**: The function reads `input.extra["tool_name"]` directly. The promotion adapter in the `"PreToolUse"` arm writes to `extra_clone["tool_name"]`. If the function is called with the original (uncloned) `input` rather than the clone containing the promotion, the tool name is still absent. The implementation must use the cloned/mutated input, not the original.

**`DomainPackRegistry` zero-change invariant**: AC-13 and C-11 both state `domain/mod.rs` requires no changes. If any implementer adds Gemini event names to the builtin pack's `event_types` list, normalization breaks (provider-specific strings would reach the registry for unrecognized events). The registry must only ever see canonical names.

---

## Edge Cases

**Unknown provider string in `--provider` flag**: `unimatrix hook PreToolUse --provider unknown-llm`. `normalize_event_name` returns `("PreToolUse", "unknown-llm")` via the provider-hint path. `ImplantEvent.provider = Some("unknown-llm")`. Site A writes `source_domain = "unknown-llm"`. No crash. This is correct behavior — the fallback is not invoked when a hint is given.

**`mcp_context` present but not an object**: Gemini payload contains `"mcp_context": "string-value"`. The `as_object()` call returns `None`; promotion step is skipped; `tool_name` is absent; `build_cycle_event_or_fallthrough()` produces `RecordEvent { event_type: "PreToolUse" }`. Must not panic (NFR-04).

**Empty `mcp_context.tool_name`**: `tool_name = ""`. The `contains("context_cycle")` check in `build_cycle_event_or_fallthrough()` returns false; falls through to `generic_record_event`. No panic.

**`normalize_event_name` with `provider_hint = Some("")`**: Empty string hint. The function should treat this as the hint value and return `("PreToolUse", "")`. The downstream `source_domain` would be `""`. This is a degenerate case — the CLI should not pass an empty string for `--provider`, but the normalization function should not panic on it.

**Gemini `SessionStart` (shared name)**: No normalization needed — name is identical to canonical. Provider must still be set. Without `--provider gemini-cli`, inference defaults to `"claude-code"`. This is a known semantic imprecision, not a bug — Gemini session start events are attributed as `"claude-code"` on the write path unless `--provider gemini-cli` is in the config. The reference config must include it.

**Codex bug #16732 — null hook path**: Codex events do not arrive. The code paths are compiled and unit-tested with synthetic events. No integration test can cover the live path. The risk is that synthetic tests diverge from what Codex actually sends when #16732 is fixed. Synthetic tests must be structurally faithful to what `codex-rs/core/src/hook_runtime.rs` actually produces (confirmed by ASS-049).

---

## Security Risks

**Untrusted input via `mcp_context.tool_name`**: The `tool_name` value from Gemini's payload is used in the `contains("context_cycle")` match. An attacker who can control Gemini hook payloads could inject `"malicious_name_contains_context_cycle"` as the tool name and cause `build_cycle_event_or_fallthrough()` to attempt cycle event construction. The function reads `tool_input["type"]` next — an injected payload could produce spurious `cycle_start` or `cycle_stop` entries in `cycle_events`. **Blast radius**: cycle state corruption for the targeted feature. No code execution risk; no path traversal. The `contains("context_cycle") || contains("unimatrix")` guard is the current boundary — it is permissive. A stricter equality check (`tool_name == "context_cycle"`) would be safer and is sufficient since bare names are confirmed.

**`--provider` flag injection**: The `provider` value flows from CLI argument into `ImplantEvent.provider` and then into `source_domain`. There is no allowlist validation. An operator could supply `--provider '; DROP TABLE observations; --'`. The value is stored as a `String` field in `ObservationRecord.source_domain` and never executed as SQL (it is a field, not a SQL parameter). No SQL injection risk. No sanitization required, but the CLI help text should note expected values.

**`HookInput` deserialization from untrusted stdin**: The hook binary reads stdin from the LLM client process. A compromised or malicious client could send a large `mcp_context` JSON blob. `serde_json::Value` deserialization with no size limit could cause memory exhaustion on a pathological input. This is a pre-existing risk not introduced by this feature — `extra: Value` with `#[serde(flatten)]` has the same exposure. No new risk introduced.

---

## Failure Modes

**Normalization failure (unknown event name)**: `normalize_event_name` returns `(event, "unknown")`. `build_request()` wildcard arm produces `RecordEvent { event_type: event, provider: "unknown" }`. Exit code 0. Observation is stored with the raw unrecognized event name in `hook` column. This is the documented graceful degradation path (NFR-04).

**Server not running when hook fires**: `run()` attempts UDS connection, fails, logs via `eprintln!`, exits 0. No event stored. This is the existing behavior — normalization does not change it.

**`mcp_context` promotion produces a clone that is not passed to the function**: If the implementer writes to `input.extra["tool_name"]` directly but `build_cycle_event_or_fallthrough()` receives a different `input` reference, the promotion has no effect. The function silently produces `RecordEvent { event_type: "PreToolUse" }`. Test scenario R-01/scenario-1 catches this if it verifies the `cycle_start` output end-to-end.

**`_registry` prefix not removed in `services/observation.rs`**: The compiler does not warn on the `_` prefix — it suppresses the "unused variable" warning. If the implementer changes the fallback logic but forgets to remove the underscore, the registry is never called. Approach A is never executed. `source_domain` is derived from the constant alone (or the old hardcode if not replaced). AC-07(c) unit test catches this.

**`debug_assert!` fires in production (release build)**: `debug_assert!` is compiled out in release builds. If `"post_tool_use_rework_candidate"` somehow escapes normalization in production, the guard is silent. The existing `"PostToolUse" | "post_tool_use_rework_candidate"` match arm in `extract_observation_fields()` normalizes it correctly before any DB write — the `debug_assert!` is an earlier, belt-and-suspenders check. The underlying normalization in the match arm is the real enforcement.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: Codex silent mislabel without `--provider` | R-03 | ADR-006: mandatory `--provider codex-cli` in reference config; `normalize_event_name` documented fallback; AC-17/AC-18/AC-19 tests |
| SR-02: Gemini `AfterTool` response field divergence | R-11 | Graceful degradation accepted (OQ-6 resolved); null-test in R-11 scenario 1 |
| SR-03: DB-read-path `resolve_source_domain` returns "unknown" | R-04 | ADR-004: Approach A (registry-with-fallback) required; explicit tests for "Stop", "cycle_start"; sentinel test preserved |
| SR-04: `post_tool_use_rework_candidate` guard scope creep | R-07 | ADR-003 col-027 (entry #3475) cited in spec; AC-16 scoped to rework candidate only; `PostToolUseFailure` arm explicitly tested as unchanged |
| SR-05: Codex code paths not live-testable | R-03 (partial) | Accepted limitation; synthetic tests structurally faithful to ASS-049 findings; caveat in reference config |
| SR-06: `context_cycle_review` finds Gemini `cycle_start` | R-01 | AC-09 mandatory integration test (not stretch); end-to-end hook-to-listener-to-review path covered |
| SR-07: Blast radius across 6 files | R-09 | ARCHITECTURE.md C-11 blast-radius table with per-file AC mapping; coverage checklist enforced at gate |
| SR-08: Gemini `mcp_context.tool_name` promotion silent fallthrough | R-01 | ADR-003: named field on `HookInput`; dedicated unit test (AC-14) as gate prerequisite; integration test (AC-02) |
| SR-09: `extract_event_topic_signal()` degrades for Gemini | R-10 | AC-11 unit test with synthetic Gemini payload; OQ-B requires spec writer confirmation of `tool_input` position |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 4 scenarios (unit: promotion, non-cycle, missing key; integration: cycle_events write + review) |
| High | 4 (R-02, R-03, R-04, R-05) | 3–4 scenarios each; 14 scenarios total |
| Medium | 5 (R-06, R-07, R-08, R-09, R-10) | 2–3 scenarios each; 12 scenarios total |
| Low | 3 (R-11, R-12, R-13) | 1–2 scenarios each; 5 scenarios total |

**Total minimum scenarios**: ~35, spread across unit (28) and integration (7) tests.

**Gate prerequisite**: R-01 scenarios 1–3 (AC-14 unit tests) must be green before any other Gemini `BeforeTool` AC is attempted. This is the highest-risk integration point and serves as an early-warning signal for the entire Gemini dispatch path.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection hook normalization blast radius"` — Entry #3492 (blast-radius bugfix gate failure from missed non-code files) and #1203 (gate validation must check all files in one pass) inform R-09 blast-radius coverage requirement.
- Queried: `/uni-knowledge-search` for `"risk pattern hook event normalization provider inference"` — Entry #4298 (hook-normalization-boundary pattern, vnc-013 tagged) directly informs R-01 priority and AC-14 gate-prerequisite framing. Entry #3471 (specialized event-type handler before generic dispatch) informs R-05 rework gate design.
- Queried: `/uni-knowledge-search` for `"source_domain hardcode observation wire protocol ImplantEvent rework detection"` — Entry #763 (server-side observation intercept pattern) and #4306 (ADR-002 vnc-013 provider field) confirm R-02 construction-site coverage requirement.
- Stored: nothing novel to store — R-01 mcp_context promotion pattern is already captured in entry #4298; blast-radius lesson is already captured in #3492. No new cross-feature pattern identified.
