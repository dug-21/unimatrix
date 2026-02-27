# Test Plan: outcome-tags (server crate)

## Risk Coverage

| Risk | Scenario | Test Name |
|------|----------|-----------|
| R-02 | All recognized keys accepted | test_all_recognized_keys_accepted |
| R-02 | Open gate values accepted | test_gate_accepts_any_nonempty_string |
| R-02 | Open agent values accepted | test_agent_accepts_any_nonempty_string |
| R-02 | Wave accepts non-negative int | test_wave_accepts_valid_integers |
| R-02 | Mixed plain and structured tags | test_mixed_plain_and_structured_tags |
| R-02 | Plain tag with type present | test_plain_tag_with_type_passes |
| R-09 | Missing type error message | test_missing_type_error_message |
| R-09 | Unknown key error message | test_unknown_key_error_message |
| R-09 | Invalid type value error message | test_invalid_type_value_error_message |

## Unit Tests (in outcome_tags.rs)

### Validation Acceptance

#### test_all_recognized_keys_accepted
- Tags: ["type:feature", "gate:3a", "phase:implementation", "result:pass", "agent:col-001-validator", "wave:2"]
- Assert: Ok(())
- **Covers**: R-02, AC-03

#### test_type_feature_accepted
- Tags: ["type:feature"]
- Assert: Ok(())
- **Covers**: AC-07

#### test_type_bugfix_accepted
- Tags: ["type:bugfix"]
- Assert: Ok(())
- **Covers**: AC-07

#### test_type_incident_accepted
- Tags: ["type:incident"]
- Assert: Ok(())
- **Covers**: AC-07

#### test_type_process_accepted
- Tags: ["type:process"]
- Assert: Ok(())
- **Covers**: AC-07

#### test_result_all_values
- For each of ["pass", "fail", "rework", "skip"]:
  - Tags: ["type:feature", "result:{value}"]
  - Assert: Ok(())
- **Covers**: AC-08

#### test_gate_accepts_any_nonempty_string
- Tags with gate values: "3a", "custom-gate", "1b", Unicode
- Assert: all Ok(())
- **Covers**: R-02, AC-09

#### test_agent_accepts_any_nonempty_string
- Tags: ["type:feature", "agent:col-001-agent-1-architect"]
- Assert: Ok(())
- **Covers**: R-02

#### test_wave_accepts_valid_integers
- Wave values: "0", "2", "99"
- Assert: all Ok(())
- **Covers**: R-02

#### test_mixed_plain_and_structured_tags
- Tags: ["type:feature", "important", "reviewed"]
- Assert: Ok(())
- **Covers**: R-02, AC-05

#### test_plain_tag_with_type_passes
- Tags: ["type:feature", "important"]
- Assert: Ok(())
- **Covers**: AC-05, AC-06

### Validation Rejection

#### test_missing_type_tag_rejected
- Tags: ["gate:3a", "result:pass"]
- Assert: Err with "type tag is required"
- **Covers**: AC-06, R-09

#### test_unknown_key_rejected
- Tags: ["type:feature", "severity:high"]
- Assert: Err with "unknown structured tag key 'severity'"
- **Covers**: AC-04, R-09

#### test_invalid_type_value_rejected
- Tags: ["type:unknown"]
- Assert: Err with "invalid type value"
- **Covers**: AC-07

#### test_invalid_result_value_rejected
- Tags: ["type:feature", "result:maybe"]
- Assert: Err with "invalid result value"
- **Covers**: AC-08

#### test_empty_gate_value_rejected
- Tags: ["type:feature", "gate:"]
- Assert: Err with "gate tag value cannot be empty"
- **Covers**: AC-09

#### test_empty_agent_value_rejected
- Tags: ["type:feature", "agent:"]
- Assert: Err
- **Covers**: R-02

#### test_invalid_wave_value_rejected
- Tags: ["type:feature", "wave:abc"]
- Assert: Err with "must be a non-negative integer"
- **Covers**: R-02

#### test_duplicate_key_rejected
- Tags: ["type:feature", "type:bugfix"]
- Assert: Err with "duplicate structured tag key"
- **Covers**: Edge case

#### test_invalid_phase_value_rejected
- Tags: ["type:feature", "phase:unknown"]
- Assert: Err
- **Covers**: FR-07

### Error Message Quality (R-09)

#### test_missing_type_error_message
- Tags: ["result:pass"]
- Assert error message contains "type tag is required"
- **Covers**: R-09

#### test_unknown_key_error_message
- Tags: ["type:feature", "foo:bar"]
- Assert error message contains "Recognized keys"
- **Covers**: R-09

#### test_invalid_type_value_error_message
- Tags: ["type:invalid"]
- Assert error message contains "feature, bugfix, incident, process"
- **Covers**: R-09
