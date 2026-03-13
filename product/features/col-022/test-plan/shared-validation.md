# Test Plan: shared-validation (C5)

**File under test**: `crates/unimatrix-server/src/infra/validation.rs`
**Risks covered**: R-02 (partial), R-11

## Unit Tests

### validate_cycle_params -- type parameter (AC-07)

```
test_validate_cycle_params_type_start
  Input: type_str="start", topic="col-022", keywords=None
  Assert: Ok(ValidatedCycleParams { cycle_type: CycleType::Start, topic: "col-022", keywords: vec![] })

test_validate_cycle_params_type_stop
  Input: type_str="stop", topic="col-022", keywords=None
  Assert: Ok(ValidatedCycleParams { cycle_type: CycleType::Stop, ... })

test_validate_cycle_params_type_invalid_pause
  Input: type_str="pause"
  Assert: Err with message mentioning "start" or "stop"

test_validate_cycle_params_type_invalid_restart
  Input: type_str="restart"
  Assert: Err

test_validate_cycle_params_type_empty
  Input: type_str=""
  Assert: Err

test_validate_cycle_params_type_case_sensitive_Start
  Input: type_str="Start"
  Assert: Err (case-sensitive, only lowercase accepted)

test_validate_cycle_params_type_case_sensitive_STOP
  Input: type_str="STOP"
  Assert: Err
```

### validate_cycle_params -- topic parameter (AC-06)

```
test_validate_cycle_params_topic_valid
  Input: topic="col-022"
  Assert: Ok, validated.topic == "col-022"

test_validate_cycle_params_topic_empty
  Input: topic=""
  Assert: Err with message about non-empty

test_validate_cycle_params_topic_max_length_128
  Input: topic="a".repeat(128)
  Assert: Ok (boundary accepted)

test_validate_cycle_params_topic_over_max_129
  Input: topic="a".repeat(129)
  Assert: Err

test_validate_cycle_params_topic_control_chars_rejected
  Input: topic="col\x00-022" or "col\n-022"
  Assert: Err or sanitized (depends on sanitize_metadata_field behavior)

test_validate_cycle_params_topic_sanitized
  Input: topic="col-022\t\r\n"
  Assert: Ok, validated.topic has control chars stripped (via sanitize_metadata_field)
  Note: Verify that sanitize_metadata_field strips but does not reject. If the
        sanitized result is empty, validation should reject.
```

### validate_cycle_params -- topic structural check (R-11)

```
test_validate_cycle_params_topic_valid_feature_id_format
  Input: topic="col-022"
  Assert: Ok (passes is_valid_feature_id structural check)

test_validate_cycle_params_topic_invalid_feature_id_format
  Input: topic="not-a-feature-id"
  Assert: Behavior depends on whether is_valid_feature_id is a hard gate or advisory.
          If hard gate: Err. If advisory: Ok but logged.
  Note: Architecture says "structural check" but does not specify hard vs soft.
        Implementer must decide. Test should match the implementation.

test_validate_cycle_params_topic_matches_observe_crate_validation
  Arrange: Set of edge-case topics: "col-022", "nxs-001", "ASS-014", "col022", "c-1", "ab-999"
  Act: Run both validate_cycle_params and is_valid_feature_id (from observe crate) on each
  Assert: Results agree for all inputs (R-11: no divergence)
  Note: Only applicable if is_valid_feature_id is re-exported. If duplicated, this test
        exercises the duplicate copy and must use the same test vectors.
```

### validate_cycle_params -- keywords parameter (AC-13)

```
test_validate_cycle_params_keywords_none
  Input: keywords=None
  Assert: Ok, validated.keywords == vec![]

test_validate_cycle_params_keywords_empty_vec
  Input: keywords=Some(&[])
  Assert: Ok, validated.keywords == vec![]

test_validate_cycle_params_keywords_valid
  Input: keywords=Some(&["attr".into(), "lifecycle".into()])
  Assert: Ok, validated.keywords == vec!["attr", "lifecycle"]

test_validate_cycle_params_keywords_five
  Input: keywords=Some(&["a","b","c","d","e"])
  Assert: Ok, len == 5

test_validate_cycle_params_keywords_six_truncated_to_five
  Input: keywords=Some(&["a","b","c","d","e","f"])
  Assert: Ok, validated.keywords == vec!["a","b","c","d","e"] (sixth dropped)

test_validate_cycle_params_keywords_seven_truncated_to_five
  Input: keywords=Some(&["a","b","c","d","e","f","g"])
  Assert: Ok, len == 5 (first 5 kept)

test_validate_cycle_params_keyword_64_chars
  Input: keyword = "a".repeat(64)
  Assert: Ok, keyword length == 64 (boundary accepted)

test_validate_cycle_params_keyword_65_chars_truncated
  Input: keyword = "a".repeat(65)
  Assert: Ok, keyword length == 64 (truncated, not rejected)

test_validate_cycle_params_keyword_empty_string
  Input: keywords=Some(&[""])
  Assert: Ok (empty strings are valid per spec -- no explicit rejection rule)
```

### CycleType enum

```
test_cycle_type_start_variant
  Assert: CycleType::Start exists and is distinct from CycleType::Stop

test_validated_cycle_params_fields
  Arrange: ValidatedCycleParams { cycle_type: CycleType::Start, topic: "x".into(), keywords: vec![] }
  Assert: All fields accessible, correct types
```

## Edge Cases

- Keywords containing only whitespace: `["   "]` -- should be accepted (no whitespace stripping specified)
- Topic with leading/trailing spaces: sanitize_metadata_field may strip them. Test actual behavior.
- Topic that is valid after sanitization but was originally invalid (e.g., "col-022\x07"): verify post-sanitization value is checked for emptiness.
