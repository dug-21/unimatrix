# Agent Report: nan-001-agent-2-testplan

## Phase

Test Plan Design (Stage 3a)

## Deliverables

| File | Path |
|------|------|
| OVERVIEW.md | product/features/nan-001/test-plan/OVERVIEW.md |
| cli-extension.md | product/features/nan-001/test-plan/cli-extension.md |
| export-module.md | product/features/nan-001/test-plan/export-module.md |
| row-serialization.md | product/features/nan-001/test-plan/row-serialization.md |

## Risk Coverage Summary

All 15 risks from RISK-TEST-STRATEGY.md are mapped to specific test expectations:

| Priority | Risks | Scenario Count | Tests Defined |
|----------|-------|---------------|---------------|
| Critical | R-01, R-03, R-04, R-05 | 12 | T-RS-01/02/03, T-RS-05, T-RS-06/06b, T-EM-01/02 |
| High | R-02, R-06, R-08, R-10 | 13 | T-RS-04, T-RS-07, T-EM-03, T-EM-05, T-EM-07, T-CL-04 |
| Medium | R-07, R-09, R-11, R-12, R-13, R-14 | 10 | T-EM-04, T-EM-06, T-RS-08, T-EM-08, T-RS-09, T-RS-10 |
| Low | R-15 | 2 | T-CL-03 |

Total: 37 risk scenarios covered by 28 test expectations across 3 component plans.

## Integration Harness Plan

No new infra-001 integration tests needed. The export subcommand is CLI-only and not accessible through MCP. Smoke tests are the mandatory minimum gate to verify the `preserve_order` feature flag and new clap variant do not regress MCP server behavior.

## Self-Check

- [x] OVERVIEW.md maps risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan -- which suites to run, new tests needed
- [x] Per-component test plans match architecture component boundaries (3 components, 3 plans)
- [x] Every high-priority risk has at least one specific test expectation
- [x] Integration tests defined for component boundaries (export-module tests cover cli->export and export->serialization boundaries)
- [x] All output files within product/features/nan-001/test-plan/
- [x] Knowledge Stewardship report block included

## Open Questions

1. **T-EM-02 (snapshot isolation)**: The most rigorous test for R-05 would spawn a writer thread that inserts data between table reads within the same export call. This requires either (a) a test hook in the export code to pause between table reads, or (b) accepting that the code review + structural verification (BEGIN DEFERRED is called) is sufficient. Recommend accepting structural verification for v1.

2. **T-EM-03 (determinism)**: The `exported_at` timestamp makes byte-identical comparison impossible without either (a) normalizing the field after export or (b) injecting a fixed timestamp. If `run_export` does not support timestamp injection, the test should replace the `exported_at` value in both outputs before comparing. The implementer should decide whether to add a test-only parameter or use post-processing.

3. **T-RS-02 (PRAGMA column count)**: This test requires a real Store-opened database to get the actual schema v11. If the export module's unit tests use in-memory SQLite with hand-crafted schemas, this test should be an integration test instead.

## Knowledge Stewardship

- Queried: /knowledge-search unavailable (MCP server not running in this context) -- proceeded without
- Stored: nothing novel to store -- this is a standard test plan for a data serialization feature with no new testing patterns discovered
