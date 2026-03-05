# Test Plan: sql-implementation

## Risk Coverage

- R-03: SQL-to-ObservationRecord mapping fidelity (High)
- R-05: NULL feature_cycle in SESSIONS (High)
- R-10: Input field type mismatch in detection rules (High)

## Test Scenarios

### T-SI-01: Round-trip: all fields populated
**Type**: Integration
**Risk**: R-03
**AC**: AC-04

Setup:
- Insert a session with feature_cycle = "col-012"
- Insert observation row with all fields (session_id, ts_millis, hook="PostToolUse", tool="Read", input='{"file_path":"/tmp/test"}', response_size=1024, response_snippet="output")
Action: Call load_feature_observations("col-012")
Assert:
- Returns 1 record
- record.ts = ts_millis (both millis)
- record.hook = HookType::PostToolUse
- record.tool = Some("Read")
- record.input = Some(Value::Object with file_path key)
- record.response_size = Some(1024)
- record.response_snippet = Some("output")

### T-SI-02: NULL optional fields -> None
**Type**: Integration
**Risk**: R-03

Setup: Insert observation with tool=NULL, input=NULL, response_size=NULL, response_snippet=NULL
Action: load_feature_observations()
Assert: tool=None, input=None, response_size=None, response_snippet=None

### T-SI-03: SubagentStart input is Value::String
**Type**: Integration
**Risk**: R-03, R-10
**AC**: AC-07

Setup: Insert observation with hook="SubagentStart", input="Design components"
Action: load_feature_observations()
Assert: record.input = Some(Value::String("Design components"))

### T-SI-04: Tool input JSON deserialized to Value::Object
**Type**: Integration
**Risk**: R-10
**AC**: AC-07

Setup: Insert observation with hook="PreToolUse", input='{"command":"ls -la"}'
Action: load_feature_observations()
Assert: record.input.unwrap().get("command") = Some(Value::String("ls -la"))

### T-SI-05: NULL feature_cycle sessions excluded
**Type**: Integration
**Risk**: R-05
**AC**: AC-05, AC-06

Setup:
- Insert session A with feature_cycle = "col-012"
- Insert session B with feature_cycle = NULL
- Insert observations for both sessions
Action: load_feature_observations("col-012")
Assert: Only session A's observations returned

### T-SI-06: Empty result for non-existent feature
**Type**: Unit
**Risk**: R-05

Setup: No sessions with feature_cycle = "nonexistent"
Action: load_feature_observations("nonexistent")
Assert: Returns Ok(empty vec)

### T-SI-07: observation_stats aggregate values
**Type**: Integration
**Risk**: R-09
**AC**: AC-11

Setup: Insert 10 observations across 3 sessions
Action: observation_stats()
Assert:
- record_count = 10
- session_count = 3
- oldest_record_age_days is reasonable

## Implementation Notes

- Tests go in `crates/unimatrix-server/src/services/observation.rs` mod tests
- Use Store::open with tempdir
- Manually insert sessions and observations via SQL for setup
- These tests cover the highest-priority risks (R-03, R-05, R-10)
