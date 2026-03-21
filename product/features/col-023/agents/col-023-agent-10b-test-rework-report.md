# Agent Report: col-023-agent-10b-test-rework

## Phase
Stage 3c Rework — Test Execution (Iteration 1)

## Task
Close 4 coverage gaps (GAP-01 through GAP-04) identified in the original RISK-COVERAGE-REPORT.md.

## Files Created

- `/workspaces/unimatrix/crates/unimatrix-observe/tests/detection_isolation.rs` (new — 22 tests)

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-observe/tests/domain_pack_tests.rs` (+6 tests: 3 GAP-03 + 3 GAP-04)
- `/workspaces/unimatrix/product/features/col-023/testing/RISK-COVERAGE-REPORT.md` (updated coverage summary, test counts, gaps → resolved)

## Tests Added

| Gap | Count | Test Names |
|-----|-------|------------|
| GAP-01 (R-01) | 21 | `test_{rule_name}_ignores_non_claude_code_domain` for all 21 built-in rules |
| GAP-02 (R-02) | 1 | `test_retrospective_report_backward_compat_claude_code_fixture` |
| GAP-03 (R-09) | 3 | `test_startup_fails_on_invalid_rule_descriptor_window_secs_zero`, `test_startup_fails_on_rule_source_domain_mismatch_names_both_domains`, `test_startup_fails_on_empty_source_domain_with_rules` |
| GAP-04 (R-10) | 3 | `test_duplicate_source_domain_registration_last_writer_wins`, `test_duplicate_categories_in_pack_accepted`, `test_invalid_category_name_format_accepted_at_registry_level` |
| **Total** | **28** | |

## Test Execution Results

- `cargo test --workspace 2>&1 | tail -20`: all 3,029 tests pass (was 3,001 pre-rework, +28)
- No failures, no regressions.
- unimatrix-observe test count post-rework: 429 (vs 359 baseline, vs 401 pre-rework)

## GAP-03 Implementation Note

The spawn instructions referenced `validate_config()` returning `Err` for a `rule_file` path that does not exist. Investigation shows `rule_file` path-existence validation is explicitly out of scope for W1-5 (documented in `main.rs:41`). The three tests written instead cover the actual startup failure paths available in `DomainPackRegistry::new()`: invalid rule descriptor (`window_secs = 0`), rule `source_domain` mismatch, and empty pack `source_domain` with rules. These are the real startup failure gates for R-09.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for "testing procedures" — server unavailable, proceeded without.
- Stored: nothing novel to store — per-rule isolation test pattern and backward-compat fixture helper are feature-specific. The `build_representative_claude_code_fixture()` approach could become a general pattern if backward-compat snapshot tests become a project standard, but it doesn't merit a stored entry based on one use.
