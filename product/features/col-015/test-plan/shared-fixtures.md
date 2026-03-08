# Test Plan: shared-fixtures

## Unit Tests (in test_scenarios.rs)

### Kendall Tau (R-01)

| ID | Test | Expected | Risk |
|----|------|----------|------|
| T-KT-01 | kendall_tau([1,2,3], [1,2,3]) | 1.0 | R-01 |
| T-KT-02 | kendall_tau([1,2,3], [3,2,1]) | -1.0 | R-01 |
| T-KT-03 | kendall_tau([1,2,3,4,5], [2,1,4,3,5]) | Known value from reference | R-01 |
| T-KT-04 | kendall_tau([1], [1]) | 1.0 | R-01 |
| T-KT-05 | kendall_tau([1,2], [1,2]) and ([1,2], [2,1]) | 1.0 and -1.0 | R-01 |

### Profile Conversion (R-04)

| ID | Test | Expected | Risk |
|----|------|----------|------|
| T-PROF-01 | profile_to_entry_record round-trip | Each sub-score matches expected values for expert_human_fresh | R-04 |
| T-PROF-02 | All 5 profiles -> distinct confidence | No two profiles produce same confidence at CANONICAL_NOW | R-04 |

## Assertions

- All Kendall tau values in [-1.0, 1.0]
- Profile conversion preserves all signal fields
- Generated EntryRecord has correct id, status, access_count, timestamps, vote counts, trust_source
