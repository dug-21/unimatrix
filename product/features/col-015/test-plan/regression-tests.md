# Test Plan: regression-tests

## Pipeline Regression (pipeline_regression.rs)

| ID | Test | Expected | Risk |
|----|------|----------|------|
| T-REG-01 | Golden confidence for 3 profiles at CANONICAL_NOW | Match to 4 decimal places | R-03 |
| T-REG-02 | Weight constants match hardcoded values | Exact equality | R-03 |
| T-REG-03 | Ranking stability (standard_ranking) | tau = 1.0 against hardcoded ordering | R-03 |

## Golden Values

Values computed once at implementation time and hardcoded. Any weight/formula change breaks these tests intentionally.
