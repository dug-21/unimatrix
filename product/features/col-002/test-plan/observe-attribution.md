# Test Plan: observe-attribution

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-02 (Attribution misattributes) | All 6 scenarios from risk strategy |

## Unit Tests: `crates/unimatrix-observe/src/attribution.rs`

### Signal Extraction

1. **test_extract_feature_from_path** -- Input contains "product/features/col-002/SCOPE.md" -> "col-002"
2. **test_extract_feature_from_task_subject** -- Input contains "col-002" as standalone feature ID -> "col-002"
3. **test_extract_feature_from_git_checkout** -- Input contains "feature/col-002" -> "col-002"
4. **test_extract_no_feature_signal** -- Input with no feature references -> None
5. **test_is_valid_feature_id_positive** -- "col-002", "nxs-001", "alc-002" -> true
6. **test_is_valid_feature_id_negative** -- "col", "002", "col-", "-002", "COL-002" with invalid format -> false

### attribute_sessions (R-02)

7. **test_attribute_single_feature_session** -- All records in session reference col-002 -> all attributed (R-02 scenario 1)
8. **test_attribute_two_feature_session** -- Session starts with col-001, switches to col-002 -> only post-switch records for col-002 (R-02 scenario 2)
9. **test_attribute_no_feature_session** -- Session has no feature signals -> excluded entirely (R-02 scenario 3)
10. **test_attribute_signal_types_all_work** -- File path, task subject, git checkout all produce attribution (R-02 scenario 4)
11. **test_attribute_pre_feature_records** -- Records before first feature signal -> attributed to first feature found (FR-04.4)
12. **test_attribute_multiple_sessions** -- 3 sessions: 2 with target feature, 1 with other -> 2 sessions included (R-02 scenario 6)
13. **test_attribute_three_feature_session** -- Session with features A, B, A -> correct partitioning
14. **test_attribute_empty_sessions** -- Empty input -> empty output
