# Test Plan: retrospective-migration

## Risk Coverage

- R-09: ObservationStats schema change (Medium)
- R-03: SQL path produces valid report (High)

## Test Scenarios

### T-RM-01: context_status observation stats from SQL
**Type**: Integration
**Risk**: R-09
**AC**: AC-11

Setup: Insert observations into database
Action: Call StatusService::compute_report()
Assert:
- report.observation_record_count > 0
- report.observation_session_count > 0
- report.observation_oldest_record_days >= 0

### T-RM-02: Retention cleanup deletes old observations
**Type**: Integration
**Risk**: R-09
**AC**: AC-12

Setup: Insert observations with ts_millis from 90 days ago and from today
Action: Run retention cleanup (DELETE WHERE ts_millis < 60 days cutoff)
Assert:
- 90-day-old observations deleted
- Today's observations remain

### T-RM-03: Full retrospective pipeline produces report
**Type**: Integration
**Risk**: R-03
**AC**: AC-04, AC-10

Setup:
- Insert session with feature_cycle
- Insert multiple observations covering PreToolUse, PostToolUse events
Action: Run the retrospective pipeline (load_feature_observations -> detect_hotspots -> compute_metrics -> build_report)
Assert:
- RetrospectiveReport returned
- session_count > 0
- total_records matches inserted count
- metrics computed (no panic)

## Implementation Notes

- T-RM-01 is complex (StatusService has many dependencies). Consider testing the observation stats portion in isolation via SqlObservationSource.
- T-RM-03 can be tested without the full MCP handler by calling the pipeline functions directly
- Retention test needs precise timestamp control
