# Agent Report: vnc-011-agent-2-testplan

## Phase

Test Plan Design (Stage 3a)

## Deliverables

| File | Path |
|------|------|
| OVERVIEW.md | product/features/vnc-011/test-plan/OVERVIEW.md |
| retrospective-formatter.md | product/features/vnc-011/test-plan/retrospective-formatter.md |
| params-extension.md | product/features/vnc-011/test-plan/params-extension.md |
| handler-dispatch.md | product/features/vnc-011/test-plan/handler-dispatch.md |

## Risk Coverage Summary

- 14 risks from RISK-TEST-STRATEGY.md mapped to specific test names
- 4 High-priority risks (R-01 through R-04): 12 test scenarios across 3 components
- 5 Medium-priority risks (R-05 through R-08, R-13): 16 test scenarios
- 4 Low-priority risks (R-09, R-10, R-11, R-12, R-14): 5 test scenarios
- 4 Integration risks (IR-01 through IR-04): covered by compile checks, unit tests, and harness tests

## Integration Harness Plan

- Suites to run in Stage 3c: `smoke` (mandatory), `tools`, `protocol`
- 3 new integration tests proposed for `suites/test_tools.py`: markdown default, JSON explicit, invalid format
- No new suite infrastructure needed

## Self-Check

- [x] OVERVIEW.md maps risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan with suites to run and new tests needed
- [x] Per-component test plans match architecture component boundaries (3 components)
- [x] Every high-priority risk has at least one specific test expectation
- [x] Integration tests defined for component boundaries
- [x] All output files within product/features/vnc-011/test-plan/

## Open Questions

1. **Observation data fixtures for integration tests**: The proposed harness tests for `test_retrospective_markdown_default` and `test_retrospective_json_explicit` require observation data (JSONL session files) to be present for `build_report()` to produce a non-empty report. Need to determine if existing test fixtures provide this or if new fixture setup is needed.
2. **Zero-activity phase detection in phase outliers**: R-14 describes the interaction between zero-activity suppression and phase outlier rendering. The formatter needs access to `PhaseMetrics` to check `tool_call_count` and `duration_secs`, but `BaselineComparison` only carries `phase: Option<String>`. Need to verify whether the formatter can cross-reference phase metrics from `MetricVector` or if suppression only applies when the data is directly available.
