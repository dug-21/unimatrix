# Test Plan: auto-outcomes

Component: Auto-Generated Session Outcomes (P0)
Covers: AC-10, AC-11
Risks: R-10

---

## Unit Tests

### outcome_tags.rs (AC-10)

```
test_valid_types_includes_session
  - Assert: VALID_TYPES.contains(&"session")

test_validate_outcome_tags_session_type_passes
  - Assert: validate_outcome_tags(&["type:session"]) == Ok(())

test_validate_outcome_tags_session_with_result_passes
  - Assert: validate_outcome_tags(&["type:session", "result:pass"]) == Ok(())
  - Assert: validate_outcome_tags(&["type:session", "result:rework"]) == Ok(())

test_validate_outcome_tags_unknown_type_still_fails
  - Assert: validate_outcome_tags(&["type:unknown"]) == Err(...)

test_valid_types_still_includes_prior_types
  - Assert: VALID_TYPES.contains(&"feature")
  - Assert: VALID_TYPES.contains(&"gate")
  - Assert: VALID_TYPES.contains(&"phase")
```

### metadata sanitization (R-10)

```
test_sanitize_metadata_field_strips_null_byte
  - Input: "feat\x00ure"
  - Assert: output == "feature"

test_sanitize_metadata_field_strips_all_control_chars
  - Input: "feat\x01\x02\x1fure"
  - Assert: all control chars stripped; output == "featureу" → "feature"

test_sanitize_metadata_field_preserves_ascii_printable
  - Input: "col-010 test/feature"
  - Assert: output == "col-010 test/feature"

test_sanitize_metadata_field_truncates
  - Input: "a".repeat(200)
  - Assert: output.len() == 128

test_sanitize_metadata_field_empty_string
  - Input: ""
  - Assert: output == ""
```

---

## Integration Tests (tmpdir store + UDS handler)

### Auto-outcome write on Success (AC-11)

```
test_auto_outcome_written_on_success_with_injections
  - Register session "outcome-test-1", feature_cycle="col-010"
  - Simulate 3 ContextSearch injections
  - Close with Success
  - Await all spawn_blocking tasks (~200ms)
  - Query: context_lookup(category:"outcome", tags:["type:session"])
  - Assert: at least 1 entry returned
  - For the matching entry:
    - Assert: entry.category == "outcome"
    - Assert: entry.tags contains "type:session"
    - Assert: entry.tags contains "result:pass"
    - Assert: entry.trust_source == "system"
    - Assert: entry.source == "hook"
    - Assert: entry.embedding_dim == 0
    - Assert: entry.feature_cycle == "col-010"

test_auto_outcome_rework_has_result_rework_tag
  - Register, inject 1, close with Rework
  - Assert: outcome entry tags contain "result:rework", not "result:pass"

test_auto_outcome_not_written_for_abandoned
  - Register "abandoned-sess", inject 2, close with Abandoned
  - Assert: context_lookup(category:"outcome", tags:["type:session"]) returns no entry for this session
  - (Check by topic or content containing session_id)

test_auto_outcome_not_written_for_zero_injections
  - Register "zero-inject-sess", close with Success (no ContextSearch calls)
  - Assert: no auto-outcome entry written for this session

test_auto_outcome_not_in_vector_search
  - Write auto-outcome; assert embedding_dim == 0
  - context_search("session outcome") → does NOT return the auto-outcome entry
  - (embedding_dim=0 means entry is not in VECTOR_MAP; can't be returned by vector search)

test_auto_outcome_content_format
  - Write auto-outcome for session "s-abc", outcome="success", injections=5
  - Assert: entry.content contains "s-abc"
  - Assert: entry.content contains "success"
  - Assert: entry.content contains "5"
```

### OUTCOME_INDEX population

```
test_auto_outcome_indexed_by_feature_cycle
  - Register session with feature_cycle="col-010-test", close with Success, 1 injection
  - context_lookup(category:"outcome", feature_cycle:"col-010-test")
  - Assert: entry found (OUTCOME_INDEX populated on insert)
```

---

## Edge Cases

```
test_auto_outcome_with_none_feature_cycle
  - Register session with feature_cycle=None
  - Close with Success, 1 injection
  - Assert: auto-outcome written with feature_cycle="" (empty string)

test_auto_outcome_sanitized_feature_cycle_in_content
  - Register with feature_cycle="feat\x01ure"
  - Close with Success
  - Assert: auto-outcome content does NOT contain the control char
```
