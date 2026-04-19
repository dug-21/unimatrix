# Agent Report: vnc-013-agent-5-source-domain

**Component:** source-domain-derivation
**Sites:** A (listener.rs), B (background.rs), C (services/observation.rs)
**Date:** 2026-04-17

---

## Files Modified

- `crates/unimatrix-server/src/services/observation.rs`
- `crates/unimatrix-server/src/background.rs`
- `crates/unimatrix-server/src/uds/listener.rs`

---

## Implementation Summary

### Site C — `services/observation.rs`

- Added `pub(crate) const DEFAULT_HOOK_SOURCE_DOMAIN: &str = "claude-code";` before `parse_observation_rows()`
- Removed `_` prefix from `_registry` parameter → `registry` (now used)
- Replaced `"claude-code".to_string()` hardcode with Approach A pattern: `registry.resolve_source_domain(&event_type)` with fallback to `DEFAULT_HOOK_SOURCE_DOMAIN`
- Updated `test_parse_rows_unknown_event_type_passthrough` comment (Approach A contract, FR-06.4, ADR-004) and added `assert_eq!(DEFAULT_HOOK_SOURCE_DOMAIN, "claude-code")` regression sentinel

### Site B — `background.rs`

- Imported `DEFAULT_HOOK_SOURCE_DOMAIN` from `crate::services::observation`
- Added `DomainPackRegistry::with_builtin_claude_code()` inside `fetch_observation_batch()` (no signature change — registry created locally, same approach as `background_tick()`)
- Replaced `"claude-code".to_string()` hardcode with Approach A pattern

### Site A — `listener.rs`

- Imported `DEFAULT_HOOK_SOURCE_DOMAIN` from `crate::services::observation`
- Replaced `source_domain: "claude-code".to_string()` in `content_based_attribution_fallback()` with `DEFAULT_HOOK_SOURCE_DOMAIN.to_string()`
  - Note: the spawn prompt specified `event.provider.clone().unwrap_or_else(...)` but `event.provider` is not in scope at this location — it is a DB read path where `ImplantEvent` is not available (confirmed via pseudocode and source reading). `DEFAULT_HOOK_SOURCE_DOMAIN.to_string()` is the correct Approach A simplified form per pseudocode Section "Site A Pseudocode (content_based_attribution_fallback)"
- Added `debug_assert!` guard in `extract_observation_fields()` before the match block (AC-16, ADR-005)

---

## Test Results

**2908 passed, 0 failed**

### New Tests Added (Site C — observation.rs)

- `test_approach_a_fallback_for_stop_event` — Stop not in builtin pack, fallback to claude-code
- `test_approach_a_fallback_for_session_start` — SessionStart not in builtin pack
- `test_approach_a_fallback_for_cycle_events` — cycle_start/stop/phase_end all fall back correctly

### Existing Test Updates (listener.rs)

- `extract_observation_fields_rework_candidate_normalized` — gated `#[cfg(not(debug_assertions))]`
- `extract_observation_fields_rework_candidate_with_tool_response` — gated
- `extract_observation_fields_rework_candidate_preserves_topic_signal` — gated
- `test_dispatch_rework_candidate_valid_session_id_succeeds` — gated

Added: `test_rework_candidate_guard_fires_in_debug` — `#[cfg(debug_assertions)]` `#[should_panic]` test verifying the assert fires (AC-16 explicit coverage)

---

## Issues / Deviations

**Site A variable name deviation (spawn prompt vs. source):** The spawn prompt specified `source_domain: event.provider.clone().unwrap_or_else(|| "claude-code".to_string())` at line ~1894, but `event.provider` does not exist in scope at that location — line 1894 is inside `content_based_attribution_fallback()`, a DB read path with only `row` in scope. Used `DEFAULT_HOOK_SOURCE_DOMAIN.to_string()` per pseudocode resolution. The live write path's `ImplantEvent.provider` is used by another component (normalization agent / hook.rs).

**debug_assert gating:** The col-019 tests that call `extract_observation_fields()` directly with `"post_tool_use_rework_candidate"` must be gated to `#[cfg(not(debug_assertions))]` because the new `debug_assert!` fires before the match arm that normalizes the string. This is expected behavior per AC-16 — those tests are testing the fallback match arm, not the assert.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-004 (#4308) confirming Approach A; entry #4304 confirming `resolve_source_domain()` returns "unknown" for unlisted types.
- Stored: entry #4314 "debug_assert in shared functions panics in debug test runs when direct-call unit tests bypass the guarded boundary" via /uni-store-pattern
