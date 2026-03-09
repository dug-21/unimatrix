# col-019: Acceptance Map

## Acceptance Criteria to Implementation Mapping

| AC | Description | Implementation Step | Test(s) | Risk |
|----|-------------|-------------------|---------|------|
| AC-01 | Non-rework PostToolUse response capture | Step 1 (extract_response_fields), Step 2 (match arm) | T-04 | R-02 |
| AC-02 | Rework PostToolUse observation persistence + topic signal | Step 3 (payload), Step 4 (dual-write) | T-02, T-10, T-12 | R-01, R-03 |
| AC-03 | MultiEdit batch observation persistence | Step 2 (match arm handles rework_candidate), Step 3b (payload) | Existing RecordEvents path + T-12 | R-01 |
| AC-04 | Missing tool_response -> NULL | Step 1 (extract_response_fields None path) | T-05, T-06 | R-02 |
| AC-05 | Large response truncation at 500 chars | Step 1 (chars().take(500)) | T-09, T-13 | R-05 |
| AC-06 | Rework detection preserved | Step 3 (additive payload fields), Step 4 (rework first) | T-01 (existing), T-03 | R-01 |
| AC-07 | Hook type normalization | Step 2 (hook normalization after match) | T-09 (rework candidate) | -- |
| AC-08 | Existing tests pass | Step 6 (full test suite) | T-01 (all existing) | R-01 |
| AC-09 | Topic signal preserved in observations | Step 4 (placement after topic accumulation) | T-10 | R-03 |

## Test to Risk Mapping

| Test ID | Test Description | Risks Covered | AC Covered |
|---------|-----------------|---------------|------------|
| T-01 | Existing rework tests (unchanged) | R-01 | AC-06, AC-08 |
| T-02 | Edit rework + observation dual-write | R-01 | AC-02 |
| T-03 | Bash failure rework + observation | R-01 | AC-06 |
| T-04 | Normal tool_response -> size/snippet | R-02 | AC-01 |
| T-05 | Absent tool_response -> None/None | R-02 | AC-04 |
| T-06 | Null tool_response -> None/None | R-02 | AC-04 |
| T-07 | Empty object tool_response | R-02 | -- |
| T-08 | String tool_response | R-02 | -- |
| T-09 | Large response truncation + hook normalization | R-02, R-05 | AC-05, AC-07 |
| T-10 | Rework candidate observation preserves topic_signal | R-03 | AC-02, AC-09 |
| T-11 | Topic signal accumulation for rework events | R-03 | -- |
| T-12 | Rework payload includes tool_input and tool_response | R-04 | AC-02, AC-03 |
| T-13 | Multi-byte UTF-8 truncation | R-05 | AC-05 |

## Coverage Summary

- **All 9 ACs** have at least one test
- **All 6 risks** have at least one test
- **R-01 (HIGH)** has 3 dedicated tests + existing regression guards
- **R-02 (MEDIUM)** has 6 dedicated tests covering all edge cases
- **R-03 (MEDIUM)** has 2 dedicated tests covering topic signal preservation
- **No gaps identified**

## Implementation Order

1. Step 1: extract_response_fields() helper -- enables all response field computation
2. Step 2: Update extract_observation_fields() match arm + hook normalization -- fixes non-rework path and enables rework path
3. Step 5a: Unit tests for Steps 1-2 -- verify extraction before proceeding
4. Step 3: Enhance rework candidate payload in hook.rs -- enables rework observation data
5. Step 5b: Unit tests for Step 3 -- verify payload structure
6. Step 4: Add observation persistence in rework handler -- completes dual-write
7. Step 5c: Integration tests for Step 4 -- verify dual-write + topic signal preservation
8. Step 6: Full test suite -- final verification
