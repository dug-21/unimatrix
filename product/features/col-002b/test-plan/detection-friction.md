# Test Plan: detection-friction

## Component: 4 friction rules in `detection/friction.rs`

## Test Module: `#[cfg(test)] mod tests` within `friction.rs`

### Existing Rule Tests (moved from detection.rs)

The following tests move verbatim from `detection.rs` to `friction.rs`:
- PermissionRetriesRule: `test_permission_retries_exceeds_threshold`, `test_permission_retries_equal_pre_post`, `test_permission_retries_multiple_tools_one_exceeds`, `test_permission_retries_empty_records`
- SleepWorkaroundsRule: `test_sleep_workaround_detected`, `test_sleep_workaround_in_pipeline`, `test_sleep_workaround_no_bash`, `test_sleep_workaround_bash_without_sleep`, `test_sleep_workaround_multiple`
- Helper tests: `test_contains_sleep_standalone`, `test_contains_sleep_in_pipeline`, `test_contains_sleep_not_standalone`, `test_contains_sleep_empty`

These tests must pass unchanged after the move (R-05).

### SearchViaBashRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_search_bash_exceeds` | 20 Bash calls, 3 are `find .`, `grep pattern`, `rg "test"` -> 15% | Finding with measured ~15.0 |
| `test_search_bash_below` | 100 Bash calls, 2 are search -> 2% | No finding |
| `test_search_bash_zero_bash` | No Bash calls at all | No finding (guards div-by-zero) |
| `test_search_bash_find_variations` | `find . -name "*.rs"`, `find /tmp` | Both counted |
| `test_search_bash_grep_variations` | `grep -r "pattern"`, `grep pattern file.rs` | Both counted |
| `test_search_bash_false_positives` | `echo "finding things"`, `cat grep_results.txt` | Not counted |
| `test_search_bash_rg_ag` | `rg "test"`, `ag "pattern"` | Both counted |
| `test_search_bash_empty` | Empty input | No findings |

Risk coverage: R-01, R-04 (regex patterns), R-12

### OutputParsingStruggleRule Tests

| Test | Scenario | Expected |
|------|----------|----------|
| `test_output_parsing_fires` | `cargo test \| grep FAIL` at ts=0, `cargo test \| tail -20` at ts=60s, `cargo test \| head -5` at ts=120s -> 3 filters in 3min | Finding |
| `test_output_parsing_different_base_cmds` | `cargo test \| grep FAIL` then `cargo build \| tail -20` | No finding (different base commands) |
| `test_output_parsing_same_filter` | `cargo test \| grep FAIL` repeated 3 times | No finding (same filter, not variation) |
| `test_output_parsing_outside_window` | Piped commands > 3 min apart | No finding (outside window) |
| `test_output_parsing_no_pipes` | Bash commands without pipes | No finding |
| `test_output_parsing_empty` | Empty input | No findings |
| `test_output_parsing_threshold_boundary` | Exactly 2 filter variations | No finding (threshold is >2) |

Risk coverage: R-01, R-04 (command parsing), R-11 (false positives)

## Regression Test

| Test | Scenario | Expected |
|------|----------|----------|
| `test_existing_friction_rules_unchanged` | Same records as col-002 tests, run PermissionRetriesRule and SleepWorkaroundsRule | Identical findings to col-002 |

Risk coverage: R-05 (submodule refactor regression)
