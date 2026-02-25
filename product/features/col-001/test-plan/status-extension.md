# Test Plan: status-extension (server crate)

## Risk Coverage

| Risk | Scenario | Test Name |
|------|----------|-----------|
| R-06 | Correct type counts | test_outcomes_by_type_correct |
| R-06 | Correct result counts | test_outcomes_by_result_correct |
| R-06 | Correct feature cycle counts | test_outcomes_by_feature_cycle_correct |
| R-06 | Non-outcomes excluded | test_non_outcomes_excluded_from_stats |
| R-06 | Empty database | test_empty_database_outcome_stats |
| R-11 | Reasonable scale performance | test_outcome_stats_at_scale (monitor) |

## Integration Tests

### test_outcomes_by_type_correct
- Store 3 outcomes with type:feature, 2 with type:bugfix
- Call context_status
- Assert: total_outcomes == 5
- Assert: outcomes_by_type contains ("feature", 3) and ("bugfix", 2)
- **Covers**: R-06, AC-14

### test_outcomes_by_result_correct
- Store outcomes with result:pass (3), result:fail (1), result:rework (1)
- Call context_status
- Assert: outcomes_by_result contains correct counts
- **Covers**: R-06, AC-14

### test_outcomes_by_feature_cycle_correct
- Store 3 outcomes for "col-001", 2 for "crt-004"
- Call context_status
- Assert: outcomes_by_feature_cycle contains ("col-001", 3) and ("crt-004", 2)
- Assert: sorted by count descending (col-001 first)
- **Covers**: R-06, AC-14

### test_non_outcomes_excluded_from_stats
- Store 5 convention entries and 2 outcome entries
- Call context_status
- Assert: total_outcomes == 2 (not 7)
- **Covers**: R-06

### test_empty_database_outcome_stats
- Open empty database
- Call context_status
- Assert: total_outcomes == 0
- Assert: outcomes_by_type, outcomes_by_result, outcomes_by_feature_cycle all empty
- Assert: no error
- **Covers**: R-06

### test_outcome_stats_at_scale
- Store 100 outcome entries with various types and results
- Call context_status
- Assert: completes without error
- Assert: total_outcomes == 100
- Note: This is a functional test, not a timing assertion
- **Covers**: R-11 (monitoring)
