# Test Plan: server-integration

## Component: context_retrospective handler enhancement + report/types changes

## Unit Tests

### types.rs Tests (additions to existing test module)

| Test | Scenario | Expected |
|------|----------|----------|
| `test_report_with_baseline_serde` | RetrospectiveReport with baseline_comparison, serialize + deserialize | Roundtrip preserves baseline data |
| `test_report_without_baseline_serde_default` | JSON without baseline_comparison field, deserialize | baseline_comparison is None |
| `test_baseline_status_serde` | Each BaselineStatus variant serialize/deserialize | Roundtrip works |
| `test_baseline_comparison_serde` | BaselineComparison with all fields | Roundtrip works |
| `test_baseline_entry_serde` | BaselineEntry serialize/deserialize | Roundtrip works |

Risk coverage: R-06 (serde compatibility), AC-12

### report.rs Tests (update existing tests)

All existing `build_report` tests must be updated to pass the new `baseline` parameter (as `None`). Then:

| Test | Scenario | Expected |
|------|----------|----------|
| `test_build_report_with_baseline` | build_report(..., Some(baseline_vec)) | report.baseline_comparison is Some with correct data |
| `test_build_report_without_baseline` | build_report(..., None) | report.baseline_comparison is None |

Risk coverage: AC-12, R-06

### detection/mod.rs Tests (update existing tests)

Existing `default_rules` tests must be updated for the new signature:

| Test | Scenario | Expected |
|------|----------|----------|
| `test_default_rules_21_rules` | default_rules(None) | Returns 21 rules |
| `test_default_rules_names` | default_rules(None) | All 21 rule names present |
| `test_default_rules_with_history` | default_rules(Some(&[mv1, mv2, mv3])) | Returns 21 rules (PhaseDurationOutlierRule has baselines) |
| `test_detect_hotspots_all_categories` | Records triggering at least one rule per category | Findings from Agent, Friction, Session, Scope |

Risk coverage: R-05 (regression), R-07 (signature change), AC-07

## Integration Tests (infra-001 harness)

### New Tests in `suites/test_tools.py` or `suites/test_lifecycle.py`

| Test | Scenario | Expected |
|------|----------|----------|
| `test_retrospective_baseline_present` | Store 3 MetricVectors for other features. Write observation JSONL. Call context_retrospective. | Response includes `baseline_comparison` array |
| `test_retrospective_baseline_absent` | Store only 2 MetricVectors (or none). Call context_retrospective. | Response has no `baseline_comparison` or it is null |
| `test_retrospective_21_rules_active` | Craft observation data that triggers rules from all 4 categories. Call context_retrospective. | Hotspots include findings from agent, friction, session, scope categories |

### Pre-existing Test Updates

No existing integration test assertions should break. The response now has an additional field (`baseline_comparison`) but existing tests assert on existing fields and should not fail.

If any existing test fails, it is because:
1. The test asserts on exact JSON structure -- update assertion
2. The test is unrelated -- triage per failure protocol

## Acceptance Criteria Coverage

| AC | Test | Method |
|----|------|--------|
| AC-07 | `test_default_rules_21_rules` | Unit: default_rules returns 21 |
| AC-12 | `test_build_report_with_baseline` + `test_retrospective_baseline_present` | Unit + integration |
| AC-14 | grep verification | No MetricVector field changes |
| AC-15 | grep verification | No new tool registration |
| AC-16 | `cargo test --workspace` | All tests pass |
| AC-19 | grep `forbid(unsafe_code)` | Present in lib.rs |
| AC-20 | git diff Cargo.toml | No new dependencies |
