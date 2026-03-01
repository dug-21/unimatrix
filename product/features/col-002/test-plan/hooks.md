# Test Plan: hooks

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-07 (Hook scripts fail silently) | Valid input, invalid input, missing session_id, dir creation |
| R-12 (Observation directory permissions) | Missing parent directory |

## Shell Integration Tests

These tests pipe synthetic JSON to each hook script and verify JSONL output.

### PreToolUse Hook (AC-01, AC-02, AC-04)

1. **test_pre_tool_valid_input** -- Pipe valid PreToolUse JSON -> JSONL file created at OBS_DIR/{session_id}.jsonl with correct fields
2. **test_pre_tool_record_schema** -- Parse output JSONL line as JSON, assert ts, hook, session_id, tool, input present (AC-04)

### PostToolUse Hook (AC-01, AC-05)

3. **test_post_tool_valid_input** -- Pipe valid PostToolUse JSON -> JSONL with response_size and snippet
4. **test_post_tool_snippet_truncation** -- Pipe JSON with 10KB response -> response_snippet <= 500 chars (AC-05)

### SubagentStart Hook

5. **test_subagent_start_valid** -- Pipe SubagentStart JSON -> JSONL with agent_type and prompt_snippet

### SubagentStop Hook

6. **test_subagent_stop_valid** -- Pipe SubagentStop JSON -> JSONL with agent_type (empty string expected)

### Error Handling (AC-03, R-07)

7. **test_invalid_json_exit_zero** -- Pipe garbage to each script -> exit code 0, no JSONL written (AC-03)
8. **test_missing_session_id_exit_zero** -- Pipe JSON without session_id -> exit 0 (R-07 scenario 3)
9. **test_creates_observation_dir** -- Run with nonexistent OBS_DIR -> dir created (R-12)

### File Routing (AC-02)

10. **test_session_id_routing** -- Two calls with different session_ids -> two separate .jsonl files (AC-02)
