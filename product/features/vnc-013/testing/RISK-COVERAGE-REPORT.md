# Risk Coverage Report: vnc-013
## Canonical Event Normalization for Multi-LLM Hook Providers

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `mcp_context.tool_name` promotion fails silently — Gemini BeforeTool falls through to generic RecordEvent | `test_gemini_mcp_context_tool_name_promotion`, `test_gemini_before_tool_non_cycle_fallthrough`, `test_mcp_context_missing_tool_name_degrades_gracefully`, `test_cycle_event_provider_propagated` | PASS | Full |
| R-02 | `ImplantEvent.provider` is None at construction sites — silent "claude-code" mislabel | `test_implant_event_provider_set_for_record_event_variants`, `test_cycle_event_provider_propagated`, `test_implant_event_provider_none_not_serialized` | PASS | Full |
| R-03 | Codex `--provider codex-cli` omission silent mislabel | `test_run_codex_provider_hint`, `test_codex_post_tool_use_skips_rework_path`, AC-19 config check | PASS | Full |
| R-04 | Approach A fallback contract regression for Sites B and C | `test_approach_a_fallback_for_stop_event`, `test_approach_a_fallback_for_session_start`, `test_approach_a_fallback_for_cycle_events`, `test_parse_rows_hook_path_always_claude_code` | PASS | Full |
| R-05 | Rework detection entered for Gemini AfterTool | `test_gemini_after_tool_skips_rework_path`, `test_codex_post_tool_use_skips_rework_path`, `test_claude_code_post_tool_use_enters_rework_path` | PASS | Full |
| R-06 | `test_parse_rows_unknown_event_type_passthrough` contract break | `test_parse_rows_unknown_event_type_passthrough` (comment updated, DEFAULT_HOOK_SOURCE_DOMAIN assertion added) | PASS | Full |
| R-07 | `"post_tool_use_rework_candidate"` escapes to hook column | `test_rework_candidate_guard_fires_in_debug` (#[cfg(debug_assertions)], should_panic), `test_post_tool_use_failure_arm_unchanged` | PASS | Full |
| R-08 | Gemini SessionEnd → "Stop" dispatch path not reached | `test_gemini_session_end_produces_session_close`, `test_normalize_event_name_gemini_unique_names` | PASS | Full |
| R-09 | Blast radius miss — file without AC coverage | Per-file checklist verified (see below) | PASS | Full |
| R-10 | `extract_event_topic_signal()` silent degradation for Gemini | `test_gemini_before_tool_topic_signal_extraction` | PASS | Full |
| R-11 | Gemini AfterTool response fields null | `test_gemini_after_tool_response_fields_degrade_gracefully` | PASS | Full |
| R-12 | Gemini matcher regex invalid | AC-10 config validation (file check + regex coverage test) | PASS | Full |
| R-13 | Backward deserialization regression on HookInput | `test_hook_input_deserializes_without_new_fields`, `test_implant_event_deserializes_without_provider` | PASS | Full |

---

## Blast Radius Coverage Checklist (R-09)

| File | Validating ACs | Verified |
|------|----------------|---------|
| `crates/unimatrix-engine/src/wire.rs` | AC-05, AC-14 | PASS — `test_hook_input_deserializes_gemini_payload_with_mcp_context`, `test_implant_event_provider_present_serializes`, `test_implant_event_provider_none_not_serialized` |
| `crates/unimatrix-server/src/uds/hook.rs` | AC-01–AC-05, AC-11, AC-12, AC-14, AC-15, AC-17, AC-18 | PASS — 19 new tests cover all ACs; normalize_event_name exhaustive table tests; rework gate tests |
| `crates/unimatrix-server/src/uds/listener.rs` | AC-06, AC-07(a), AC-16 | PASS — `test_rework_candidate_guard_fires_in_debug` (AC-16); Site A uses `DEFAULT_HOOK_SOURCE_DOMAIN` fallback (AC-07a); AC-06 covered via unit path |
| `crates/unimatrix-server/src/background.rs` | AC-07(b) | PASS — Approach A pattern at line 1338–1345; `test_approach_a_fallback_for_stop_event` etc. cover the fallback contract |
| `crates/unimatrix-server/src/services/observation.rs` | AC-07(c), AC-08 | PASS — `test_approach_a_fallback_*` tests; `_registry` prefix removed; `DEFAULT_HOOK_SOURCE_DOMAIN` constant defined here |
| `crates/unimatrix-server/src/main.rs` | AC-15, AC-17, AC-18 | PASS — `--provider` flag accepted; AC-15 verified via CLI (exit 0 for BeforeTool, AfterTool, SessionEnd); AC-17/AC-18 via normalize tests |
| `.gemini/settings.json` | AC-10 | PASS — File exists, valid JSON, matcher `mcp_unimatrix_.*`, all 4 events present |
| `.codex/hooks.json` | AC-19 | PASS — File exists, valid JSON, all 4 events with `--provider codex-cli`, bug #16732 caveat present |

---

## Test Results

### Unit Tests

- Total (workspace): 4,725
- Passed: 4,725
- Failed: 0
- Ignored: 28 (all pre-existing — ONNX runtime tests requiring native library)

#### vnc-013 New Tests (wire.rs)

| Test | AC | Result |
|------|----|--------|
| `test_hook_input_deserializes_without_new_fields` | AC-08, NFR-05 | PASS |
| `test_hook_input_deserializes_with_provider_field` | AC-17 | PASS |
| `test_hook_input_deserializes_gemini_payload_with_mcp_context` | AC-14 | PASS |
| `test_hook_input_mcp_context_non_object_deserializes` | NFR-04 | PASS |
| `test_hook_input_provider_none_when_absent` | AC-08 | PASS |
| `test_hook_input_clone_includes_new_fields` | AC-05 | PASS |
| `test_implant_event_deserializes_without_provider` | AC-08, R-13 | PASS |
| `test_implant_event_provider_present_serializes` | AC-05 | PASS |
| `test_implant_event_provider_none_not_serialized` | NFR-05 | PASS |
| `test_mcp_context_not_duplicated_in_extra` | AC-14 | PASS |

#### vnc-013 New Tests (hook.rs)

| Test | AC | Result |
|------|----|--------|
| `test_normalize_event_name_gemini_unique_names` | AC-01 | PASS |
| `test_normalize_event_name_claude_code_passthrough` | AC-01, AC-18 | PASS |
| `test_normalize_event_name_unknown_fallback` | AC-01 | PASS |
| `test_normalize_event_name_category2_passthrough` | AC-01 | PASS |
| `test_gemini_mcp_context_tool_name_promotion` | AC-14 (gate) | PASS |
| `test_gemini_before_tool_non_cycle_fallthrough` | AC-14 (gate) | PASS |
| `test_mcp_context_missing_tool_name_degrades_gracefully` | AC-14 (gate) | PASS |
| `test_mcp_context_non_object_degrades_gracefully` | NFR-04 | PASS |
| `test_gemini_after_tool_skips_rework_path` | AC-04, AC-12 | PASS |
| `test_codex_post_tool_use_skips_rework_path` | AC-04, R-05 | PASS |
| `test_claude_code_post_tool_use_enters_rework_path` | AC-12 | PASS |
| `test_implant_event_provider_set_for_record_event_variants` | AC-05 | PASS |
| `test_cycle_event_provider_propagated` | AC-05, R-02 | PASS |
| `test_gemini_session_end_produces_session_close` | AC-01, R-08 | PASS |
| `test_run_codex_provider_hint` | AC-17 | PASS |
| `test_run_session_start_provider_hint_precedence` | AC-20 | PASS |
| `test_gemini_before_tool_topic_signal_extraction` | AC-11 | PASS |
| `test_gemini_after_tool_response_fields_degrade_gracefully` | R-11 | PASS |
| `test_post_tool_use_failure_arm_unchanged` | AC-16 | PASS |
| `test_build_request_debug_assert_fires_for_before_tool` | AC-01 | PASS |
| `test_build_request_debug_assert_fires_for_after_tool` | AC-01 | PASS |
| `test_build_request_debug_assert_fires_for_session_end` | AC-01 | PASS |

#### vnc-013 New Tests (listener.rs)

| Test | AC | Result |
|------|----|--------|
| `test_rework_candidate_guard_fires_in_debug` | AC-16 | PASS |

#### vnc-013 New Tests (services/observation.rs)

| Test | AC | Result |
|------|----|--------|
| `test_approach_a_fallback_for_stop_event` | AC-07(b), AC-07(c), R-04 | PASS |
| `test_approach_a_fallback_for_session_start` | R-04 | PASS |
| `test_approach_a_fallback_for_cycle_events` | R-04 | PASS |

### Integration Tests (infra-001)

- Smoke gate (`-m smoke`): **23 passed, 0 failed** — GATE PASSED
- `suites/test_tools.py` + `suites/test_lifecycle.py`: **166 passed, 7 xfailed, 2 xpassed, 0 failed**

#### Pre-existing xfail/xpass status (not caused by vnc-013)

| Test | Status | Notes |
|------|--------|-------|
| `test_auto_quarantine_after_consecutive_bad_ticks` | XFAIL | Pre-existing: requires UNIMATRIX_TICK_INTERVAL_SECONDS env var; unit tests in background.rs cover trigger logic |
| `test_dead_knowledge_entries_deprecated_by_tick` | XFAIL | Pre-existing: 15-min tick interval, unit tests cover trigger logic |
| `test_context_status_supports_edge_count_increases_after_tick` | XFAIL | Pre-existing: requires CI tick interval config |
| `test_s1_edges_visible_in_status_after_tick` | XFAIL | Pre-existing: test timeout; validates S1 edge count increase |
| `test_inferred_edge_count_unchanged_by_s1_s2_s8` | XFAIL | Pre-existing: validates inferred_edge_count for bugfix-491 |
| `test_search_multihop_injects_terminal_active` | XPASS | Pre-existing: search injection implemented; xfail marker can be removed in follow-up |
| `test_inferred_edge_count_unchanged_by_cosine_supports` | XPASS | Pre-existing: Path C bugfix-491 implemented; xfail marker can be removed in follow-up |

All xfails and xpasses are pre-existing and unrelated to vnc-013. No new integration test failures introduced by this feature.

### AC-07 Grep Check (Code Review Gate)

Verified: No direct `source_domain: "claude-code".to_string()` assignment remains at any of the three target production sites.

- **Site A** (`listener.rs`): Uses `DEFAULT_HOOK_SOURCE_DOMAIN.to_string()` in attribution fallback. The live-write path's `ObservationRow` struct does not include a `source_domain` field — the column is derived at read time via `DomainPackRegistry`. Site A write path correctly threads `ImplantEvent.provider` through `extract_observation_fields()`.
- **Site B** (`background.rs:1338-1345`): Approach A pattern: `registry.resolve_source_domain()` with `DEFAULT_HOOK_SOURCE_DOMAIN` fallback.
- **Site C** (`services/observation.rs:594-600`): Approach A pattern; `_registry` prefix removed; `DEFAULT_HOOK_SOURCE_DOMAIN` constant defined in this file.

### AC-13: DomainPackRegistry Zero-Change Invariant

`git diff HEAD -- crates/unimatrix-server/src/domain/mod.rs` — no diff (file not modified). The builtin claude-code pack in `crates/unimatrix-observe/src/domain/mod.rs` contains only canonical event names: `PreToolUse`, `PostToolUse`, `SubagentStart`, `SubagentStop`. Gemini event names (`BeforeTool`, `AfterTool`, `SessionEnd`) are absent. AC-13 PASS.

### AC-15, AC-17, AC-18: CLI Exit Code Verification

```
unimatrix hook BeforeTool --provider gemini-cli    → exit 0  (AC-15)
unimatrix hook AfterTool --provider gemini-cli     → exit 0  (AC-15)
unimatrix hook SessionEnd --provider gemini-cli    → exit 0  (AC-15)
unimatrix hook PreToolUse --provider codex-cli     → exit 0  (AC-17)
unimatrix hook PreToolUse                          → exit 0  (AC-18)
```

---

## Gaps

None. All 13 risks from RISK-TEST-STRATEGY.md have test coverage.

**AC-02 / AC-09 integration test assessment**: The OVERVIEW.md correctly identified that AC-02 (cycle_events write via Gemini path) and AC-09 (context_cycle_review finds Gemini-sourced cycles) require a subprocess/stdin hook injection fixture not available in the infra-001 harness. Per the OVERVIEW.md guidance, these ACs are covered at the unit level:

- AC-02: `test_gemini_mcp_context_tool_name_promotion` verifies that `build_request()` produces `RecordEvent { event_type: "cycle_start" }` for the Gemini path. The cycle_events write path in `listener.rs::handle_cycle_event()` is unchanged from the pre-vnc-013 path and is exercised by existing `test_dispatch_cycle_start_*` tests.
- AC-09: The `context_cycle_review` tool uses canonical event names in its SQL queries. Since normalization ensures Gemini `BeforeTool` arrives as canonical `"PreToolUse"` (and `context_cycle` interception produces `"cycle_start"`), the existing lifecycle suite tests (`test_cycle_review_knowledge_reuse_cross_feature_split`, `test_phase_tag_store_cycle_review_flow`) that exercise `context_cycle_review` cover the relevant code paths. End-to-end Gemini-specific integration test (full subprocess injection) would require new harness infrastructure — filed as a known enhancement gap, not a blocking deficiency.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_normalize_event_name_gemini_unique_names`: BeforeTool→(PreToolUse, gemini-cli), AfterTool→(PostToolUse, gemini-cli), SessionEnd→(Stop, gemini-cli). Claude Code passthrough confirmed. Unknown fallback confirmed. |
| AC-02 | PASS (unit) | `test_gemini_mcp_context_tool_name_promotion`: build_request() produces RecordEvent{cycle_start} for Gemini BeforeTool+context_cycle. Full subprocess integration test deferred (harness gap). |
| AC-03 | PASS | `test_gemini_before_tool_non_cycle_fallthrough`: mcp_context.tool_name="context_search" produces RecordEvent{PreToolUse} |
| AC-04 | PASS | `test_gemini_after_tool_skips_rework_path`: PostToolUse with provider=gemini-cli produces RecordEvent{PostToolUse}, not rework_candidate |
| AC-05 | PASS | `test_implant_event_provider_set_for_record_event_variants`: all RecordEvent-producing paths thread provider into ImplantEvent |
| AC-06 | PASS (unit path) | `dispatch_record_event_returns_ack` confirms RecordEvent dispatch; source_domain derivation at read path uses `event.provider.clone().unwrap_or_else()` pattern in attribution fallback |
| AC-07 | PASS | Grep check: no literal `source_domain: "claude-code"` assignment at Sites A/B/C. Site A: DEFAULT_HOOK_SOURCE_DOMAIN fallback. Sites B/C: Approach A pattern. |
| AC-08 | PASS | `cargo test --workspace`: 4,725 passed, 0 failed. All pre-vnc-013 tests unaffected. |
| AC-09 | PASS (unit) | `test_cycle_review_knowledge_reuse_cross_feature_split` and `test_phase_tag_store_cycle_review_flow` cover context_cycle_review with canonical event names. Gemini normalization produces canonical names. Full subprocess test deferred. |
| AC-10 | PASS | `.gemini/settings.json` exists, valid JSON, matcher `mcp_unimatrix_.*`, events: BeforeTool, AfterTool, SessionStart, SessionEnd. All 12 tools covered by pattern. |
| AC-11 | PASS | `test_gemini_before_tool_topic_signal_extraction`: extract_event_topic_signal("PreToolUse", ...) returns non-empty signal for Gemini payload with tool_input at top-level |
| AC-12 | PASS | `test_gemini_after_tool_skips_rework_path`: provider gate `provider.as_deref() != Some("claude-code")` fires before is_rework_eligible_tool() for Gemini events |
| AC-13 | PASS | `git diff HEAD -- crates/unimatrix-server/src/domain/mod.rs`: no changes. DomainPackRegistry builtin pack contains only canonical event names. |
| AC-14 | PASS | Gate prerequisite: `test_gemini_mcp_context_tool_name_promotion`, `test_gemini_before_tool_non_cycle_fallthrough`, `test_mcp_context_missing_tool_name_degrades_gracefully` — all 3 gate tests green. |
| AC-15 | PASS | CLI verification: `unimatrix hook BeforeTool --provider gemini-cli` → exit 0; AfterTool → exit 0; SessionEnd → exit 0. |
| AC-16 | PASS | `test_rework_candidate_guard_fires_in_debug` (#[cfg(debug_assertions)], #[should_panic]) confirms debug_assert! fires in listener.rs extract_observation_fields(). `test_post_tool_use_failure_arm_unchanged` confirms PostToolUseFailure unaffected. |
| AC-17 | PASS | `test_run_codex_provider_hint`: build_request("PreToolUse", input_with_provider="codex-cli") → ImplantEvent.provider=Some("codex-cli"). CLI: exit 0. |
| AC-18 | PASS | `test_normalize_event_name_claude_code_passthrough`: normalize_event_name("PreToolUse", None) = ("PreToolUse", "claude-code"). CLI: exit 0 with no --provider flag. |
| AC-19 | PASS | `.codex/hooks.json` exists, valid JSON, events: PreToolUse/PostToolUse/SessionStart/Stop, 5 occurrences of `--provider codex-cli` (≥4), bug #16732 caveat present. |
| AC-20 | PASS | `test_run_session_start_provider_hint_precedence`: provider hint "codex-cli" overrides inference for SessionStart. |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — Entry #4311 (silent-fallthrough test prerequisite pattern), #4312 (normalization validation lesson), #3386 (edge case test omission risk) surfaced. Entry #4311 confirms the gate-prerequisite designation for AC-14 tests was well-placed.
- Stored: nothing novel to store — the gate-prerequisite pattern for silent-exit-0 normalization failures is already captured in entry #4311. The Approach A registry-with-fallback pattern is already captured in vnc-013 ADR entries. No new cross-feature pattern emerged from this test execution.
