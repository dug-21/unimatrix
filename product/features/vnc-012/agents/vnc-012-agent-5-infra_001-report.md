# Agent Report: vnc-012-agent-5-infra_001

**Feature**: vnc-012
**Component**: product/test/infra-001/suites/test_tools.py
**Agent**: vnc-012-agent-5-infra_001

## Summary

Added IT-01 and IT-02 Python smoke tests to the infra-001 integration test suite under a new section header `# === vnc-012: String-encoded integer coercion (IT-01, IT-02) ================`.

## Files Modified

- `/workspaces/unimatrix/.claude/worktrees/vnc-012/product/test/infra-001/suites/test_tools.py`

## Test Functions Added

1. `test_get_with_string_id` (IT-01) — `@pytest.mark.smoke`
   - Stores an entry with `format="json"`, extracts integer id, converts to string via `str(entry_id)`
   - Calls `context_get` via `server.call_tool` with `{"id": string_id, "agent_id": "human"}`
   - Asserts success, non-empty content, and content matches stored text

2. `test_deprecate_with_string_id` (IT-02) — `@pytest.mark.smoke`
   - Stores an entry with `format="json"`, extracts integer id, converts to string
   - Calls `context_deprecate` via `server.call_tool` with `{"id": string_id, "agent_id": "human", "reason": "IT-02 coercion test"}`
   - Asserts success

## Design Decisions

- Used `server.call_tool` directly (not the typed `server.context_get`/`server.context_deprecate` wrappers) to ensure the id value is serialized as a JSON String over the wire. This is critical: the typed wrappers accept `int` by annotation and would not prevent accidental int serialization.
- Both tests use the `server` fixture (function scope) per the test plan — no state leakage between tests.
- `assert_tool_success` is called before `get_result_text` so the error message from the serde failure (`"invalid type: string"`) is surfaced immediately on failure.

## Issues

None.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- skipped (no blocking need; pseudocode and test plan were fully specified)
- Stored: nothing novel to store -- the pattern of using `call_tool` instead of typed wrappers to control JSON serialization type is already implied by the test plan spec; no novel gotcha discovered beyond what the pseudocode documents
