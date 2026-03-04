# Test Plan: status-service

## Risk Coverage: R-03, R-11

## Tests

### T-SS-01: StatusService::compute_report with empty database
- **Type**: Unit test
- **Method**: Create StatusService with empty store. Call compute_report(None, None, false). Verify all counts are 0, distributions are empty.
- **Risk**: R-03

### T-SS-02: StatusService::compute_report with known test data
- **Type**: Snapshot/integration test
- **Method**: Populate store with known entries. Call compute_report. Verify StatusReport fields match expected values.
- **Risk**: R-03

### T-SS-03: StatusService::compute_report with topic filter
- **Type**: Unit test
- **Method**: Populate store with entries across topics. Call compute_report with topic_filter. Verify filtered distribution.
- **Risk**: R-03

### T-SS-04: StatusService::compute_report with category filter
- **Type**: Unit test
- **Method**: Same as T-SS-03 but with category_filter.
- **Risk**: R-03

### T-SS-05: context_status handler delegates to StatusService
- **Type**: Code review / grep
- **Command**: `wc -l` on context_status handler function body
- **Expected**: < 30 lines. Contains `self.services.status`.
- **Risk**: AC-17

### T-SS-06: StatusService uses correct table constants
- **Type**: Code review / grep
- **Command**: `grep -c 'ENTRIES\|COUNTERS\|CATEGORY_INDEX\|TOPIC_INDEX' src/services/status.rs`
- **Expected**: Uses imported constants from unimatrix_store, not hardcoded.
- **Risk**: R-11

### T-SS-07: StatusService in ServiceLayer
- **Type**: Grep verification
- **Command**: `grep 'status:' src/services/mod.rs`
- **Expected**: ServiceLayer struct contains `status: StatusService` field.
- **Risk**: AC-16

### T-SS-08: run_maintenance performs all maintenance operations
- **Type**: Integration test
- **Method**: Call run_maintenance with active_entries. Verify confidence refresh, observation cleanup, session GC are attempted.
- **Risk**: R-03

### T-SS-09: Existing context_status integration tests pass
- **Type**: Existing test suite
- **Command**: `cargo test -p unimatrix-server context_status`
- **Expected**: All existing tests pass unchanged.
- **Risk**: R-03
