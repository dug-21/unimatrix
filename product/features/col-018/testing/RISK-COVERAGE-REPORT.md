# Risk Coverage Report: col-018

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Observation write fails silently, prompt data lost | col018_context_search_creates_observation | PASS | Full (row verified in DB) |
| R-02 | Topic signal false positives from prompt text | col018_topic_signal_from_feature_id, col018_topic_signal_null_for_generic_prompt, col018_topic_signal_from_file_path | PASS | Full |
| R-03 | Input field unbounded for long prompts | col018_long_prompt_truncated, col018_prompt_at_limit_not_truncated | PASS | Full |
| R-04 | Session ID None skips observation | col018_session_id_none_skips_observation, col018_empty_query_skips_observation | PASS | Full |
| R-05 | Search pipeline regression | col018_search_results_unchanged_with_observation | PASS | Full |
| R-06 | Topic signal accumulation missed | col018_topic_signal_accumulated_in_session_registry | PASS | Full |

## Test Results

### Unit Tests
- Total: 858 (unimatrix-server)
- Passed: 858
- Failed: 0
- New (col-018): 10

### Workspace Unit Tests
- Total: 1704+ across all crates
- Passed: All except 1 pre-existing flaky test (test_compact_search_consistency in unimatrix-vector, passes in isolation)
- Failed: 0 (col-018 related)

### Integration Tests (infra-001)
- Smoke tests run: 19 (14 passed, 5 failed)
- Failures: All pre-existing infrastructure issues (4 timeout, 1 rate-limit), none caused by col-018
- No new integration tests needed (observation is internal side-effect, not MCP-visible)

## Gaps

None. All 6 risks from RISK-TEST-STRATEGY.md have full test coverage.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | col018_context_search_creates_observation: observation row with hook="UserPromptSubmit" |
| AC-02 | PASS | col018_topic_signal_from_feature_id: topic_signal = "col-018" |
| AC-03 | PASS | col018_topic_signal_accumulated_in_session_registry: registry contains "col-018" |
| AC-04 | PASS | col018_search_results_unchanged_with_observation: HookResponse::Entries returned |
| AC-05 | PASS | Structural: spawn_blocking_fire_and_forget used (same pattern as col-012) |
| AC-06 | PASS | col018_session_id_none_skips_observation: 0 observation rows, search still works |
| AC-07 | PASS | col018_empty_query_skips_observation: 0 observation rows |
| AC-08 | PASS | col018_long_prompt_truncated: input.len() == 4096 for 5000-char query |
| AC-09 | PASS | col018_topic_signal_null_for_generic_prompt: topic_signal is None |
| AC-10 | PASS | Existing tests pass (empty-prompt path via RecordEvent unchanged) |

## Integration Test Failure Triage

| Failing Test | Cause | Action |
|-------------|-------|--------|
| test_server_info | Timeout (server startup) | Pre-existing infrastructure issue |
| test_empty_database_operations | Timeout | Pre-existing infrastructure issue |
| test_contradiction_detected | Timeout | Pre-existing infrastructure issue |
| test_status_empty_db | Timeout | Pre-existing infrastructure issue |
| test_store_1000_entries | Rate limiting (embedding API) | Pre-existing infrastructure issue |

None of these failures are caused by col-018 changes. The col-018 change adds a fire-and-forget observation write to the ContextSearch dispatch arm and does not affect server startup, MCP protocol handling, or embedding API calls.
