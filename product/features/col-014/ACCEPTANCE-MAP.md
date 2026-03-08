# col-014: Acceptance Map

## Acceptance Criteria -> Test Mapping

| AC# | Criterion | Test Function | Status |
|-----|-----------|---------------|--------|
| 1 | `col-010b` accepted | `test_is_valid_feature_id_suffixed` | Pending |
| 2 | `col-002b` accepted | `test_is_valid_feature_id_suffixed` | Pending |
| 3 | `nxs-001` accepted (regression) | `test_is_valid_feature_id_positive` (existing) | Pending |
| 4 | `PROJ-123` accepted | `test_is_valid_feature_id_domain_agnostic` | Pending |
| 5 | `sprint-7-auth` accepted | `test_is_valid_feature_id_domain_agnostic` | Pending |
| 6 | `v2.1-migration` accepted | `test_is_valid_feature_id_domain_agnostic` | Pending |
| 7 | `my_project-feat_1` accepted | `test_is_valid_feature_id_domain_agnostic` | Pending |
| 8 | Empty string rejected | `test_is_valid_feature_id_negative` (existing, updated) | Pending |
| 9 | `nohyphen` rejected | `test_is_valid_feature_id_no_hyphen` | Pending |
| 10 | `a]b-c` rejected | `test_is_valid_feature_id_special_chars` | Pending |
| 11 | `a b-c` rejected | `test_is_valid_feature_id_whitespace` | Pending |
| 12 | 128/129 length boundary | `test_is_valid_feature_id_length_boundary` | Pending |
| 13 | Existing IDs remain valid | Existing tests (regression suite) | Pending |
| 14 | E2E attribution with `col-010b` | `test_attribute_sessions_suffixed_feature` | Pending |

## Risk -> Test Mapping

| Risk | Test Coverage |
|------|---------------|
| R-01: False positives | `test_is_valid_feature_id_domain_agnostic` + existing attribution tests |
| R-02: Regression | `test_is_valid_feature_id_positive` + all existing tests |
| R-03: Injection | `test_is_valid_feature_id_special_chars` |

## Completion Gate

All 14 AC tests pass + all existing `attribution.rs` tests pass + `cargo test -p unimatrix-observe` clean.
