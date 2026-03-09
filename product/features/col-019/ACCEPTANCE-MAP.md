# col-019: Acceptance Map

## Acceptance Criteria to Implementation Mapping

| AC | Description | Implementation Step | Test(s) | Risk |
|----|-------------|-------------------|---------|------|
| AC-01 | Non-rework PostToolUse response capture | Step 1 (extract_response_fields), Step 2 (match arm) | T-04 | R-02 |
| AC-02 | Rework PostToolUse observation persistence | Step 3 (payload), Step 4 (dual-write) | T-02, T-10 | R-01 |
| AC-03 | MultiEdit batch observation persistence | Step 2 (match arm handles rework_candidate), Step 3 (payload) | Existing RecordEvents path + T-10 | R-01 |
| AC-04 | Missing tool_response -> NULL | Step 1 (extract_response_fields None path) | T-05, T-06 | R-02 |
| AC-05 | Large response truncation at 500 chars | Step 1 (chars().take(500)) | T-09 | R-05 |
| AC-06 | Rework detection preserved | Step 3 (additive payload fields), Step 4 (rework first) | T-01 (8 existing), T-03 | R-01 |
| AC-07 | Hook type normalization | Step 2 (hook normalization after match) | T-09 (rework candidate) | -- |
| AC-08 | Existing tests pass | Step 7 (full test suite) | T-01 (all existing) | R-01 |

## Test to Risk Mapping

| Test ID | Test Description | Risks Covered | AC Covered |
|---------|-----------------|---------------|------------|
| T-01 | Existing 8 rework tests (unchanged) | R-01 | AC-06, AC-08 |
| T-02 | Edit rework + observation dual-write | R-01 | AC-02 |
| T-03 | Bash failure rework + observation | R-01 | AC-06 |
| T-04 | Normal tool_response -> size/snippet | R-02 | AC-01 |
| T-05 | Absent tool_response -> None/None | R-02 | AC-04 |
| T-06 | Null tool_response -> None/None | R-02 | AC-04 |
| T-07 | Empty object tool_response | R-02 | -- |
| T-08 | String tool_response | R-02 | -- |
| T-09 | Large response truncation | R-02, R-05 | AC-05 |
| T-10 | Rework payload includes tool_response | R-04 | AC-02, AC-03 |
| T-11 | Multi-byte UTF-8 truncation | R-05 | AC-05 |

## Coverage Summary

- **All 8 ACs** have at least one test
- **All 5 risks** have at least one test
- **R-01 (HIGH)** has 3 dedicated tests + 8 existing regression guards
- **R-02 (MEDIUM)** has 6 dedicated tests covering all edge cases
- **No gaps identified**

## Implementation Order

1. Step 1: extract_response_fields() helper -- enables all response field computation
2. Step 2: Update extract_observation_fields() match arm -- fixes non-rework path immediately
3. Step 6a: Unit tests for Steps 1-2 -- verify before proceeding
4. Step 3: Enhance rework candidate payload -- enables rework observation data
5. Step 4: Add observation persistence in rework handler -- completes dual-write
6. Step 6b: Unit tests for Steps 3-4
7. Step 7: Full test suite -- final verification
