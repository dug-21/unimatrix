# Agent Report: col-022-agent-6-hook-handler

## Component
hook-handler (C2) -- PreToolUse interception for context_cycle

## Files Modified
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/hook.rs`

## Changes Summary

### `build_request()` -- new match arm
Added `"PreToolUse"` match arm before the `_ =>` fallthrough, delegating to `build_cycle_event_or_fallthrough()`.

### `build_cycle_event_or_fallthrough()` -- new function (~70 lines)
Implements the full pseudocode:
1. Extracts `tool_name` from `input.extra`, checks for `"context_cycle"` substring
2. R-09 mitigation: verifies `"unimatrix"` in prefix (or bare `"context_cycle"`)
3. Extracts `type`, `topic`, `keywords` from `tool_input` JSON
4. Validates via shared `validate_cycle_params()` (ADR-004)
5. Constructs `RecordEvent` with `event_type: "cycle_start"/"cycle_stop"`, `feature_cycle` in payload, keywords as JSON string, `topic_signal` set to topic
6. All failure paths fall through to `generic_record_event` (FR-03.7)

## Tests
- **15 new tests**, all passing
- **108 total** hook tests pass (0 failures)
- 1 pre-existing failure in `server::tests::test_migration_v7_to_v8_backfill` (schema version mismatch from schema-migration agent bump to v12 -- not caused by this component)

### Test Coverage per Test Plan
| Test Plan Item | Test Name | Status |
|---|---|---|
| Tool name with prefix (R-09) | test_build_request_pretooluse_context_cycle_with_prefix | PASS |
| Tool name without prefix | test_build_request_pretooluse_context_cycle_without_prefix | PASS |
| Wrong server prefix (R-09) | test_build_request_pretooluse_wrong_server_prefix | PASS |
| Substring false positive | test_build_request_pretooluse_context_cycle_substring_no_match | PASS |
| cycle_start event_type | test_build_request_cycle_start_event_type | PASS |
| cycle_start with keywords | test_build_request_cycle_start_with_keywords | PASS |
| cycle_stop event_type | test_build_request_cycle_stop_event_type | PASS |
| Invalid type fallthrough (R-02) | test_build_request_cycle_invalid_type_falls_through | PASS |
| Missing topic fallthrough | test_build_request_cycle_missing_topic_falls_through | PASS |
| Malformed tool_input fallthrough | test_build_request_cycle_malformed_tool_input_falls_through | PASS |
| Missing tool_input key | test_build_request_cycle_missing_tool_input_key_falls_through | PASS |
| Topic too long fallthrough | test_build_request_cycle_topic_too_long_falls_through | PASS |
| Session ID propagation | test_build_request_cycle_preserves_session_id | PASS |
| No session ID fallback | test_build_request_cycle_no_session_id | PASS |
| Other tool not matched | test_build_request_pretooluse_other_tool_not_cycle | PASS |
| Non-string keywords filtered | test_build_request_cycle_keywords_with_non_string_items | PASS |
| No keywords field | test_build_request_cycle_no_keywords_field | PASS |
| Extra fields ignored | test_build_request_cycle_extra_fields_in_tool_input | PASS |
| Constants match | test_build_request_cycle_event_type_constants_match | PASS |
| Fire-and-forget | test_build_request_cycle_is_fire_and_forget | PASS |

## Issues
- **hook.rs file size**: Non-test code is ~673 lines, exceeding the 500-line guideline. This was already ~560 lines before col-022. Splitting would require a larger refactor of existing PostToolUse handler code. The new cycle handler follows the established pattern and is self-contained.

## Dependencies
- Shared validation types (`validate_cycle_params`, `CycleType`, `CYCLE_START_EVENT`, `CYCLE_STOP_EVENT`) from shared-validation agent -- already implemented in `validation.rs`.

## Knowledge Stewardship
- Queried: /query-patterns not available (MCP server not running in this context) -- proceeded based on existing code patterns in hook.rs
- Stored: nothing novel to store -- implementation followed established build_request() match arm pattern with no runtime surprises. The hook's synchronous, no-tokio constraint was already documented and the shared validation function is pure computation.
