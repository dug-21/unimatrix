# Test Plan: C6 -- handler_integration

Module: `crates/unimatrix-server/src/mcp/tools.rs` (context_retrospective handler)

## Integration Tests

These tests exercise the full handler pipeline including new steps. They require Store setup, observation seeding, and async runtime.

### Full pipeline with new fields

#### test_retrospective_produces_session_summaries
- **Setup**: Seed observation records for 3 distinct sessions attributed to topic "test-feature". Run context_retrospective.
- **Assert**: Report contains session_summaries with 3 entries. Each has non-empty tool_distribution.
- **AC**: AC-01

#### test_retrospective_produces_knowledge_reuse
- **Setup**: Seed entry in session "s1", query_log returning that entry in session "s2", injection_log for same entry in session "s3". Run retrospective.
- **Assert**: Report contains knowledge_reuse with tier1_reuse_count >= 1.
- **AC**: AC-06

#### test_retrospective_produces_rework_count
- **Setup**: Seed 3 sessions: outcomes "result:pass", "result:rework", "result:failed". Run retrospective.
- **Assert**: rework_session_count = 2
- **AC**: AC-09

#### test_retrospective_produces_reload_pct
- **Setup**: Session 1 with Read calls to files A, B. Session 2 with Read calls to files B, C. Run retrospective.
- **Assert**: context_reload_pct is approximately 0.5 (1 reload out of 2 files in session 2).
- **AC**: AC-10

#### test_retrospective_produces_attribution
- **Setup**: discover_sessions returns 5 sessions. Only 3 have direct feature_cycle match.
- **Assert**: attribution has attributed_session_count=3, total_session_count=5.
- **Risks**: R-03
- **AC**: FR-05.6

### Rework outcome detection

#### test_rework_count_case_insensitive
- **Setup**: Sessions with outcomes: "Result:Rework", "RESULT:FAILED", "result:pass"
- **Assert**: rework_session_count = 2 (case-insensitive per human-resolved variance)
- **Risks**: R-08
- **AC**: AC-09

#### test_rework_count_null_outcome_excluded
- **Setup**: Sessions with outcomes: None, "", "result:rework"
- **Assert**: rework_session_count = 1 (None and empty excluded)
- **Risks**: R-08

#### test_rework_count_substring_match
- **Setup**: Session with outcome "type:delivery,result:rework,gate:3c"
- **Assert**: Counted as rework (substring match)
- **Risks**: R-08

### Empty topic handling

#### test_retrospective_empty_topic_cached
- **Setup**: No observation records. Cached MetricVector exists.
- **Assert**: Returns cached report with is_cached=true. All new fields are None.
- **Risks**: R-10
- **AC**: AC-14

#### test_retrospective_empty_topic_no_cache
- **Setup**: No observation records. No cached MetricVector.
- **Assert**: Returns error (existing behavior, ERROR_NO_OBSERVATION_DATA).
- **Risks**: R-10
- **AC**: AC-14

### Graceful degradation

#### test_retrospective_knowledge_reuse_failure_graceful
- **Setup**: Seed observations. Make query_log/injection_log queries fail (e.g., table missing scenario or corrupted data).
- **Assert**: Report still contains existing fields (hotspots, metrics, narratives). knowledge_reuse is None. Warning logged.
- **Risks**: R-14

#### test_retrospective_session_summary_failure_graceful
- **Setup**: Force session summary computation to encounter edge case.
- **Assert**: Existing pipeline output preserved. session_summaries may be None.
- **Risks**: R-14

#### test_retrospective_counter_update_failure_graceful
- **Setup**: Seed observations. topic_deliveries upsert fails.
- **Assert**: Report still returned successfully. Counter failure logged as warning.
- **Risks**: R-14

### Counter update integration

#### test_retrospective_updates_topic_deliveries
- **Setup**: Seed observations for topic. Run retrospective.
- **Assert**: topic_deliveries record has total_sessions, total_tool_calls, total_duration_secs matching computed values.
- **Risks**: R-05
- **AC**: AC-12

#### test_retrospective_counter_idempotent_on_rerun
- **Setup**: Run retrospective. Record counter values. Run retrospective again with same data.
- **Assert**: Counter values identical after second run.
- **Risks**: R-05
- **AC**: AC-12

### Existing pipeline regression

#### test_retrospective_existing_fields_unchanged
- **Setup**: Standard observation data. Run retrospective.
- **Assert**: hotspots, metrics, baseline_comparison, entries_analysis, narratives, recommendations are populated as before. No regression.
- **AC**: AC-15
