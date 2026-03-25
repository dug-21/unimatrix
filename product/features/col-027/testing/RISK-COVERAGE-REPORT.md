# Risk Coverage Report: col-027 — PostToolUseFailure Hook Support

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `extract_error_field()` absent or miscalled: error content lost | `test_extract_error_field_present`, `test_extract_error_field_absent`, `test_extract_error_field_truncation_at_501_chars`, `test_extract_observation_fields_posttoolusefailure_full` | PASS | Full |
| R-02 | Partial two-site differential fix: metrics and friction diverge | `test_two_site_agreement_balanced_failure_and_post`, `test_two_site_agreement_genuine_imbalance`, `test_two_site_agreement_failure_only_no_post` | PASS | Full |
| R-03 | `extract_observation_fields()` wildcard fall-through: `tool = None` | `test_extract_observation_fields_posttoolusefailure_full`, `test_extract_observation_fields_posttoolusefailure_tool_absent` | PASS | Full |
| R-04 | `PermissionRetriesRule` fires for failure-only imbalance | `test_permission_retries_failure_as_terminal_no_finding`, `test_permission_retries_mixed_post_and_failure_balanced` | PASS | Full |
| R-05 | `build_request()` wildcard routing: `tool_name` not extracted | `build_request_posttoolusefailure_explicit_arm`, `build_request_posttoolusefailure_empty_extra`, `build_request_posttoolusefailure_missing_tool_name` | PASS | Full |
| R-06 | `ToolFailureRule` threshold boundary error | `test_tool_failure_rule_at_threshold_no_finding`, `test_tool_failure_rule_above_threshold_fires` | PASS | Full |
| R-07 | `ToolFailureRule` missing `source_domain` guard | `test_tool_failure_rule_non_claude_code_excluded`, `test_tool_failure_rule_mixed_domains` | PASS | Full |
| R-08 | Hook exits non-zero on malformed payload | `build_request_posttoolusefailure_null_extra`, `build_request_posttoolusefailure_null_error`, `build_request_posttoolusefailure_missing_tool_name`; AC-12 binary tests | PASS | Full |
| R-09 | `extract_event_topic_signal()` falls through for failure events | `extract_event_topic_signal_posttoolusefailure` | PASS | Full |
| R-10 | `response_size` non-None for failure records | Assertion `obs.response_size == None` in `test_extract_observation_fields_posttoolusefailure_full` | PASS | Full |
| R-11 | `POSTTOOLUSEFAILURE` constant value misspelled | `test_posttoolusefailure_constant_value` | PASS | Full |
| R-12 | `permission_friction_events` underflow: signed subtraction | `saturating_sub` verified at `metrics.rs:87`; no test with `failure_count > pre_count` | PARTIAL | Partial |
| R-13 | `ToolFailureRule` not registered in `default_rules()` | `test_default_rules_contains_tool_failure_hotspot`, `test_default_rules_has_22_rules` | PASS | Full |
| R-14 | `PostToolUseFailure` registration in `settings.json` missing | Structural inspection — key absent from `.claude/settings.json` | **FAIL** | **None** |

---

## Test Results

### Unit Tests

- Total: 3,594 (workspace)
- Passed: 3,594
- Failed: 0
- Ignored: 27 (pre-existing, unrelated to col-027)

### Integration Tests (infra-001)

- Smoke suite: 20 passed, 0 failed
- Full suite: not run (not required per OVERVIEW.md; feature has no MCP-visible behavior)

### Binary Integration Tests (AC-12)

- `echo '{}' | unimatrix hook PostToolUseFailure` → exit 0 (PASS)
- `echo 'not-json' | unimatrix hook PostToolUseFailure` → exit 0 (PASS)
- `echo '' | unimatrix hook PostToolUseFailure` → exit 0 (PASS)

---

## Gaps

### Blocker

**R-14 / AC-01**: `PostToolUseFailure` hook is not registered in `.claude/settings.json`. The event
key is absent — Claude Code will never invoke the hook binary on tool failure. The entire feature
is a runtime no-op despite correct code implementation.

Fix: add `"PostToolUseFailure"` entry to `.claude/settings.json` with `matcher: "*"` and command
`/workspaces/unimatrix/target/release/unimatrix hook PostToolUseFailure`, matching the existing
`PreToolUse` and `PostToolUse` pattern.

### Non-Blocker (Low/Low)

**R-12**: No unit test exercises `permission_friction_events` when `failure_count > pre_count`.
Code is correct (`saturating_sub` at `metrics.rs:87`). A test with 1 Pre + 5 Failure should be
added asserting `permission_friction_events >= 0`. Not a merge blocker.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | **FAIL** | `PostToolUseFailure` key absent from `.claude/settings.json` |
| AC-02 | PASS | `test_posttoolusefailure_constant_value` passes; `hook_type::POSTTOOLUSEFAILURE == "PostToolUseFailure"` |
| AC-03 | PASS | `test_extract_observation_fields_posttoolusefailure_full` passes; compound assertion present |
| AC-04 | PASS | Same test; `obs.hook == "PostToolUseFailure"` explicitly asserted |
| AC-05 | PASS | `test_permission_retries_failure_as_terminal_no_finding` passes |
| AC-06 | PASS | All pre-existing `PermissionRetriesRule` tests pass; `test_permission_retries_genuine_imbalance_with_failures` passes |
| AC-07 | PASS | `test_two_site_agreement_balanced_failure_and_post` passes; both sites asserted in same function |
| AC-08 | PASS | `test_tool_failure_rule_above_threshold_fires` passes; measured == 4.0, threshold == 3.0 |
| AC-09 | PASS | `test_tool_failure_rule_at_threshold_no_finding` passes; findings empty at count == 3 |
| AC-10 | PASS | `make_failure` helper at `friction.rs:450`; used by AC-05/AC-08/AC-09 tests |
| AC-11 | PASS | Explicit `"PostToolUseFailure"` arm at `hook.rs:1716`; `build_request_posttoolusefailure_explicit_arm` passes |
| AC-12 | PASS | Binary exits 0 for empty JSON, malformed JSON, and empty stdin |
