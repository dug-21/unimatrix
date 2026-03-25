# Agent Report: col-027-agent-4-friction-metrics

**Agent ID:** col-027-agent-4-friction-metrics
**Feature:** col-027
**Component:** friction-metrics

## Work Completed

### Files Modified (atomic commit 07726df)

- `crates/unimatrix-observe/src/detection/friction.rs`
- `crates/unimatrix-observe/src/detection/mod.rs`
- `crates/unimatrix-observe/src/metrics.rs`

### Changes

**friction.rs:**
- Added `use unimatrix_core::observation::hook_type;` import
- Added `TOOL_FAILURE_THRESHOLD: u64 = 3` constant
- Fixed `PermissionRetriesRule::detect()`: renamed `post_counts` to `terminal_counts`, added `hook_type::POSTTOOLUSEFAILURE` as a terminal event branch (ADR-004)
- Added `ToolFailureRule` struct + `DetectionRule` impl: rule_name `"tool_failure_hotspot"`, fires at `count > 3`, source_domain guard, one finding per tool (ADR-005)
- Added `make_failure(ts, tool)` test helper
- Added 21 new tests: T-FM-01..T-FM-18 + 3 two-site coherence tests (T-FM-08/09/10)

**mod.rs:**
- Updated module doc comment: 21 -> 22 rules, friction 4 -> 5
- Updated `default_rules()` doc comment: 21 -> 22
- Registered `friction::ToolFailureRule` in Friction group
- Updated count assertions in existing tests (21 -> 22)
- Added `test_default_rules_contains_tool_failure_hotspot` test (T-FM-19)
- Added `"tool_failure_hotspot"` assertion to `test_default_rules_names`

**metrics.rs:**
- Fixed `compute_universal()` permission friction computation: renamed `post_counts` to `terminal_counts`, added `hook_type::POSTTOOLUSEFAILURE` branch (ADR-004 coupling site)

## Test Results

All tests pass:
- 421 unit tests in unimatrix-observe lib: OK
- 22 tests in domain pack suite: OK
- 44 tests in domain pack registry: OK
- 6 integration tests: OK

New tests added (21 total):
- `test_permission_retries_failure_as_terminal_no_finding` (T-FM-01)
- `test_permission_retries_mixed_post_and_failure_balanced` (T-FM-02)
- `test_permission_retries_genuine_imbalance_with_failures` (T-FM-03)
- `test_tool_failure_rule_at_threshold_no_finding` (T-FM-11)
- `test_tool_failure_rule_above_threshold_fires` (T-FM-12)
- `test_tool_failure_rule_multiple_tools_independent` (T-FM-13)
- `test_tool_failure_rule_multiple_tools_multiple_findings` (T-FM-14)
- `test_tool_failure_rule_empty_records` (T-FM-15)
- `test_tool_failure_rule_non_claude_code_excluded` (T-FM-16)
- `test_tool_failure_rule_mixed_domains` (T-FM-17)
- `test_tool_failure_rule_evidence_records` (T-FM-18)
- `test_tool_failure_rule_no_tool_records_skipped` (edge case)
- `test_two_site_agreement_balanced_failure_and_post` (T-FM-08)
- `test_two_site_agreement_genuine_imbalance` (T-FM-09)
- `test_two_site_agreement_failure_only_no_post` (T-FM-10)
- `test_default_rules_contains_tool_failure_hotspot` (T-FM-19)
- Updated: `test_default_rules_has_22_rules` (T-FM-20, was 21)
- Updated: `test_default_rules_with_history` (count 22)
- Updated: `test_default_rules_names` (added tool_failure_hotspot check)

## Issues

None. All constraints satisfied:
- ADR-004: friction.rs and metrics.rs updated in single commit
- ADR-005: rule_name is `"tool_failure_hotspot"`, threshold is strictly `> 3`
- source_domain == "claude-code" guard applied first in all detect() methods

## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-observe -- found entries #3472 (atomic update pattern), #3479 (coupled test pattern), #2907/#2935 (source_domain guard pattern). All applied.
- Stored: FAILED -- anonymous agent lacks Write capability. Novel pattern to store: IDE formatter may silently revert Edit tool calls; use Write (full-file) when this occurs. Pattern should be stored by SM or retrospective agent.
