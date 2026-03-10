# Agent Report: col-020b-agent-2-testplan

## Phase: Test Plan Design (Stage 3a)

## Files Produced

- `product/features/col-020b/test-plan/OVERVIEW.md`
- `product/features/col-020b/test-plan/tool-name-normalizer.md`
- `product/features/col-020b/test-plan/tool-classification.md`
- `product/features/col-020b/test-plan/knowledge-curated-counter.md`
- `product/features/col-020b/test-plan/type-renames.md`
- `product/features/col-020b/test-plan/knowledge-reuse-semantics.md`
- `product/features/col-020b/test-plan/data-flow-debugging.md`
- `product/features/col-020b/test-plan/re-export-update.md`

## Risk Coverage Summary

All 13 risks from RISK-TEST-STRATEGY.md are mapped to test scenarios or verification methods:

- **Critical (R-06, R-08)**: R-08 covered by 7+ unit tests across C2/C3. R-06 covered by code review checklist (unit tests cannot cover the end-to-end data flow per ADR-002).
- **High (R-01, R-04, R-10)**: R-01 has 8 dedicated normalize tests. R-04 has 5 new semantic tests plus updates to 11 existing tests. R-10 has 2 mixed-input tests.
- **Medium (R-02, R-03, R-05, R-12)**: R-02 has 6 serde backward compat tests. R-03 has 3 default tests. R-05 has 4 category/gap tests. R-12 covered by code review.
- **Low (R-07, R-09, R-11, R-13)**: R-07 is compile gate. R-09 has 2 exhaustive classification tests. R-11 has 3 tool_distribution tests. R-13 has 2 serialization assertions.

## Integration Suite Plan

- Smoke tests: mandatory gate (run in Stage 3c)
- No new infra-001 tests in this feature (ADR-002)
- 3 follow-up integration test scenarios identified in OVERVIEW.md for future work

## Open Questions

None new. OQ-01 (#193 root cause) and OQ-02 (persisted reports) from the source documents remain as-is and are addressed by the test plan's code review checklist (C6) and serde tests (C4).

## Self-Check

- [x] OVERVIEW.md maps risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan -- which suites to run, new tests needed
- [x] Per-component test plans match architecture component boundaries (C1-C7)
- [x] Every high-priority risk has at least one specific test expectation
- [x] Integration tests defined for component boundaries (serde compat tests bridge session_metrics<->types, knowledge_reuse<->types)
- [x] All output files within `product/features/col-020b/test-plan/`
