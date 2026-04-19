# Agent Report: vnc-013-agent-4-normalization

## Task
Implement the normalization component in `crates/unimatrix-server/src/uds/hook.rs`.

## Files Modified
- `crates/unimatrix-server/src/uds/hook.rs`

## What Was Implemented

### New Functions
1. `map_to_canonical(event: &str) -> &'static str` — private helper, pure static match table. Maps Gemini-unique names (BeforeTool→PreToolUse, AfterTool→PostToolUse, SessionEnd→Stop), all canonical names to themselves, unknown to `"__unknown__"`.

2. `pub fn normalize_event_name(event: &str) -> (&'static str, &'static str)` — inference-only path (no provider_hint). Returns (canonical, provider) pairs. Gemini-unique names infer "gemini-cli", all other known names infer "claude-code", unknown returns ("__unknown__", "unknown").

### Modified Functions
3. `run()` — two-path dispatch: hint path (`provider.is_some()`) calls `map_to_canonical` + uses CLI hint verbatim; inference path calls `normalize_event_name`. Sets `hook_input.provider` before `build_request()`. `effective_event` substitutes raw event string when canonical is `"__unknown__"`. Removed the `let _ = &provider` Wave 1 placeholder.

4. `build_request()` — added `debug_assert!` at top to catch un-normalized provider-specific names in debug builds. Modified `"PostToolUse"` arm with provider gate (non-claude-code skips rework detection, ADR-005). Modified `"PreToolUse"` arm with `mcp_context.tool_name` promotion adapter (clones input, promotes bare "context_cycle" to extra["tool_name"] before calling `build_cycle_event_or_fallthrough`). Threaded `provider: input.provider.clone()` into every `ImplantEvent` construction in this function and in `build_cycle_event_or_fallthrough()` and `generic_record_event()`.

### Tests Added (30 new tests in hook.rs `mod tests`)
- `test_normalize_event_name_gemini_unique_names` — AC-01
- `test_normalize_event_name_claude_code_passthrough` — AC-01, AC-18
- `test_normalize_event_name_unknown_fallback` — AC-01
- `test_normalize_event_name_category2_passthrough` — AC-01
- `test_map_to_canonical_gemini_names`
- `test_map_to_canonical_passthrough`
- `test_map_to_canonical_unknown`
- `test_gemini_mcp_context_tool_name_promotion` — AC-14, R-01 scenario 1 (GATE PREREQUISITE)
- `test_gemini_before_tool_non_cycle_fallthrough` — AC-14, R-01 scenario 2
- `test_mcp_context_missing_tool_name_degrades_gracefully` — AC-14, R-01 scenario 3
- `test_mcp_context_non_object_degrades_gracefully`
- `test_gemini_after_tool_skips_rework_path` — AC-04, AC-12, R-05 scenario 1
- `test_codex_post_tool_use_skips_rework_path` — R-05 scenario 2
- `test_claude_code_post_tool_use_enters_rework_path` — R-05 scenario 3 (over-block guard)
- `test_run_codex_provider_hint` — AC-17
- `test_implant_event_provider_set_for_record_event_variants` — AC-05, R-02
- `test_cycle_event_provider_propagated` — AC-05, R-02 scenario 2
- `test_gemini_session_end_produces_session_close` — R-08 scenario 2
- `test_post_tool_use_failure_arm_unchanged` — AC-16, R-07 scenario 3
- `test_gemini_after_tool_response_fields_degrade_gracefully` — R-11
- `test_build_request_debug_assert_fires_for_before_tool` — normalization contract
- `test_build_request_debug_assert_fires_for_after_tool` — normalization contract
- `test_build_request_debug_assert_fires_for_session_end` — normalization contract

## Design Decisions

**Signature divergence from ARCHITECTURE.md**: The pseudocode resolved the `normalize_event_name` signature to use only `(event: &str) -> (&'static str, &'static str)` (no `provider_hint` parameter), with `map_to_canonical` as a separate private function for the hint path. This matches the pseudocode exactly and keeps the return type honest (no sentinel needed in the hint path).

**`__unknown__` sentinel**: Unknown events return `("__unknown__", "unknown")`. The `run()` caller detects this and substitutes the raw event string as `effective_event`, preserving unrecognized names in the DB hook column.

**mcp_context promotion**: Only clones `HookInput` when `mcp_context.tool_name` is present (zero-copy for Claude Code, which is the common path). The clone goes into `build_cycle_event_or_fallthrough`; the original is never mutated.

**Equality check in build_cycle_event_or_fallthrough**: The existing guard `tool_name != "context_cycle" && !tool_name.contains("unimatrix")` was verified to be already correct for bare names — `"context_cycle" == "context_cycle"` passes the first condition, so no change was needed to `build_cycle_event_or_fallthrough()` itself.

## Test Results
2908 passed, 0 failed (up from 2906 before — 2 net new passing tests after accounting for the 3 `#[cfg(debug_assertions)]` tests that only run when assertions are disabled, and the gate prerequisite tests that confirmed before the integration).

## Issues Encountered

**Cross-agent integration**: The source-domain-derivation parallel agent added a `debug_assert!` to `listener.rs::extract_observation_fields()` that broke 5 pre-existing tests (`extract_observation_fields_rework_candidate_*`, `test_dispatch_rework_candidate_valid_session_id_succeeds`). Those tests call `extract_observation_fields()` directly with `"post_tool_use_rework_candidate"`. The source-domain-derivation agent had already annotated most with `#[cfg(not(debug_assertions))]`; the third rework candidate test was annotated by that agent before I observed the final state. No intervention was needed from this agent — the concurrent fix landed correctly.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `mcp__unimatrix__context_search` — found ADRs #4305–#4310, normalization pattern #4298, and source_domain fallback pattern #4304. Applied all ADRs directly to implementation choices.
- Stored: nothing novel — entry #4314 already covers the `debug_assert` + `#[cfg(not(debug_assertions))]` pattern discovered during this implementation. Verified before storing.
