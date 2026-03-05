# Test Plan: event-persistence

## Risk Coverage

- R-01: ImplantEvent payload field extraction (High)
- R-04: spawn_blocking write failure (Medium)
- R-06: Timestamp conversion overflow (Low)
- R-07: Batch insert partial failure (Medium)

## Test Scenarios

### T-EP-01: PreToolUse event persisted with correct field mapping
**Type**: Unit
**Risk**: R-01
**AC**: AC-02

Setup: Create ImplantEvent with event_type="PreToolUse", payload with tool_name="Read", tool_input={"file_path":"/tmp/test"}
Action: Call extract_observation_fields()
Assert:
- hook = "PreToolUse"
- tool = Some("Read")
- input = Some(JSON string of tool_input)
- response_size = None
- response_snippet = None
- ts_millis = timestamp * 1000

### T-EP-02: PostToolUse event persisted with all fields
**Type**: Unit
**Risk**: R-01
**AC**: AC-02

Setup: ImplantEvent with event_type="PostToolUse", payload with tool_name="Edit", response_size=1024, response_snippet="some output"
Action: Call extract_observation_fields()
Assert:
- hook = "PostToolUse"
- tool = Some("Edit")
- response_size = Some(1024)
- response_snippet = Some("some output")

### T-EP-03: SubagentStart event normalization
**Type**: Unit
**Risk**: R-01
**AC**: AC-02

Setup: ImplantEvent with event_type="SubagentStart", payload with agent_type="uni-pseudocode", prompt_snippet="Design components"
Action: Call extract_observation_fields()
Assert:
- hook = "SubagentStart"
- tool = Some("uni-pseudocode")
- input = Some("Design components") -- plain string, not JSON object

### T-EP-04: SubagentStop event with no fields
**Type**: Unit
**Risk**: R-01
**AC**: AC-02

Setup: ImplantEvent with event_type="SubagentStop"
Action: Call extract_observation_fields()
Assert:
- hook = "SubagentStop"
- tool = None
- input = None
- response_size = None
- response_snippet = None

### T-EP-05: Missing optional payload fields stored as NULL
**Type**: Integration
**Risk**: R-01
**AC**: AC-02

Setup: ImplantEvent with event_type="PreToolUse", payload missing tool_name
Action: Insert into observations via handler
Assert:
- Row exists in observations
- tool column is NULL

### T-EP-06: Timestamp conversion (normal case)
**Type**: Unit
**Risk**: R-06
**AC**: AC-02

Input: timestamp = 1700000000 (2024 epoch seconds)
Assert: ts_millis = 1700000000000

### T-EP-07: Timestamp year 3000 no overflow
**Type**: Unit
**Risk**: R-06

Input: timestamp = 32503680000 (year 3000)
Assert: ts_millis = 32503680000000 (within i64 range, max = 9.2e18)

### T-EP-08: Batch insert all events
**Type**: Integration
**Risk**: R-07
**AC**: AC-03

Setup: Create 5 valid ImplantEvents
Action: Process as RecordEvents batch
Assert:
- All 5 rows in observations table
- All fields correct

## Implementation Notes

- Unit tests for field extraction go in listener.rs mod tests
- Integration tests that need Store go in listener.rs mod tests (existing pattern: tests use Store::open with tempfile)
- Batch atomicity test: verify all-or-nothing by checking row count
