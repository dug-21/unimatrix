# Test Plan: store-pipeline (server crate)

## Risk Coverage

| Risk | Scenario | Test Name |
|------|----------|-----------|
| R-01 | Atomic entry + OUTCOME_INDEX | test_outcome_index_populated_atomically |
| R-01 | Consistent state after commit | test_outcome_index_consistent_after_commit |
| R-03 | Non-outcome with colon tags passes | test_convention_with_colon_tags_stored |
| R-03 | Non-outcome not validated | test_decision_with_colon_tags_stored |
| R-03 | Pattern with colon tags | test_pattern_with_colon_tags_stored |
| R-03 | validate_outcome_tags only for outcome | test_non_outcome_no_tag_validation |
| R-04 | StoreParams without feature_cycle | test_store_params_without_feature_cycle |
| R-04 | StoreParams with feature_cycle null | test_store_params_feature_cycle_null |
| R-04 | StoreParams with feature_cycle value | test_store_params_feature_cycle_value |
| R-04 | Full store call without feature_cycle | test_backward_compatible_store_call |
| R-05 | Outcome with feature_cycle indexed | test_outcome_with_feature_cycle_indexed |
| R-05 | Multiple outcomes same cycle all indexed | test_multiple_outcomes_same_cycle_indexed |
| R-05 | OUTCOME_INDEX prefix scan | test_outcome_index_prefix_scan |
| R-05 | Transaction includes OUTCOME_INDEX | test_outcome_index_in_transaction |
| R-08 | Convention with scope:global stored | test_convention_scope_global_stored |
| R-08 | Decision with multiple colons stored | test_decision_multiple_colons_stored |
| R-08 | TAG_INDEX contains exact colon tag | test_tag_index_contains_colon_tags |
| R-10 | Outcome without feature_cycle no index | test_orphan_outcome_not_indexed |
| R-10 | Empty feature_cycle same as missing | test_empty_feature_cycle_not_indexed |
| R-10 | Outcome with feature_cycle no warning | test_outcome_with_cycle_no_warning |

## Unit Tests (in validation.rs or tools.rs)

### test_store_params_without_feature_cycle
- Deserialize: `{"content":"x","topic":"t","category":"outcome","tags":["type:feature"]}`
- Assert: feature_cycle is None
- **Covers**: R-04, AC-12

### test_store_params_feature_cycle_null
- Deserialize: `{"content":"x","topic":"t","category":"outcome","tags":["type:feature"],"feature_cycle":null}`
- Assert: feature_cycle is None
- **Covers**: R-04, AC-12

### test_store_params_feature_cycle_value
- Deserialize: `{"content":"x","topic":"t","category":"outcome","tags":["type:feature"],"feature_cycle":"col-001"}`
- Assert: feature_cycle is Some("col-001")
- **Covers**: R-04, AC-12

### test_validate_store_params_feature_cycle_too_long
- StoreParams with feature_cycle = Some("a".repeat(129))
- Assert: validation error
- **Covers**: Security (feature_cycle injection)

## Integration Tests (in server integration test files)

### test_outcome_with_feature_cycle_indexed
- Store outcome entry with feature_cycle "col-001"
- Read OUTCOME_INDEX in read txn
- Assert: ("col-001", entry_id) exists
- **Covers**: R-05, AC-10, AC-15

### test_outcome_index_populated_atomically
- Store outcome with feature_cycle
- In same test: verify ENTRIES and OUTCOME_INDEX both have the entry
- **Covers**: R-01, AC-15

### test_outcome_index_consistent_after_commit
- Store outcome, read immediately
- Verify both entry and OUTCOME_INDEX row exist
- **Covers**: R-01

### test_multiple_outcomes_same_cycle_indexed
- Store 3 outcomes for "col-001"
- Range scan OUTCOME_INDEX for "col-001": verify count == 3
- **Covers**: R-05

### test_outcome_index_prefix_scan
- Store outcomes for "col-001" and "crt-004"
- Prefix scan for "col-001": verify only col-001 entries
- **Covers**: R-05

### test_orphan_outcome_not_indexed
- Store outcome without feature_cycle
- Verify OUTCOME_INDEX is empty
- **Covers**: R-10, AC-11

### test_empty_feature_cycle_not_indexed
- Store outcome with feature_cycle: ""
- Verify OUTCOME_INDEX is empty
- **Covers**: R-10, AC-11

### test_convention_with_colon_tags_stored
- Store category "convention" with tags ["scope:global", "priority:high"]
- Assert: stored successfully, tags in TAG_INDEX
- **Covers**: R-03, R-08, AC-16

### test_decision_with_colon_tags_stored
- Store category "decision" with tags ["severity:high"]
- Assert: stored successfully
- **Covers**: R-03

### test_decision_multiple_colons_stored
- Store category "decision" with tags ["foo:bar:baz"]
- Assert: stored successfully, TAG_INDEX contains "foo:bar:baz"
- **Covers**: R-08

### test_backward_compatible_store_call
- Store entry without feature_cycle parameter (simulate pre-col-001 call)
- Assert: entry stored with empty feature_cycle
- **Covers**: R-04, AC-17

### test_outcome_with_cycle_no_warning
- Store outcome with feature_cycle "col-001"
- Assert: response does NOT contain "not linked" warning
- **Covers**: R-10
