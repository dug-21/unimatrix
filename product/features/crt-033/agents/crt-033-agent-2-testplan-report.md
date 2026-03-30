# Agent Report: crt-033-agent-2-testplan (Stage 3a Test Plan Design)

## Summary

Produced per-component test plans for all five crt-033 components. Test plans are
rooted in the RISK-TEST-STRATEGY.md and cover all 17 AC-IDs.

## Output Files

- `/workspaces/unimatrix/product/features/crt-033/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-033/test-plan/cycle_review_index.md`
- `/workspaces/unimatrix/product/features/crt-033/test-plan/migration.md`
- `/workspaces/unimatrix/product/features/crt-033/test-plan/tools_handler.md`
- `/workspaces/unimatrix/product/features/crt-033/test-plan/status_response.md`
- `/workspaces/unimatrix/product/features/crt-033/test-plan/status_service.md`

## Risk Coverage Summary

| Risk ID | Priority | Component Plan(s) | Scenarios Planned |
|---------|----------|------------------|------------------|
| R-01 | Critical | migration.md | 6 (MIG-U-01..07 + cascade grep checks MIG-C-01..06) |
| R-02 | High | tools_handler.md, cycle_review_index.md | 3 (TH-I-10, CRS-I-10) |
| R-03 | High | tools_handler.md, cycle_review_index.md | 3 (TH-I-07, raw SQL assert) |
| R-04 | High | tools_handler.md | 4 (TH-I-05, TH-I-06, TH-I-08 variant) |
| R-05 | High | tools_handler.md | 3 (TH-I-02, TH-I-04, TH-I-08) |
| R-06 | Medium | cycle_review_index.md, tools_handler.md | 4 (CRS-U-05, CRS-U-06, TH-U-06) |
| R-07 | Medium | cycle_review_index.md, status_service.md | 6 (CRS-I-04..09, SS-I-01..03) |
| R-08 | Medium | tools_handler.md | 3 (TH-U-03, TH-U-04, TH-U-05) |
| R-09 | Medium | tools_handler.md | 2 (TH-G-01 static grep) |
| R-10 | Low | cycle_review_index.md | 2 (CRS-I-10) |
| R-11 | Low | cycle_review_index.md | 3 (CRS-U-03, CRS-U-04) |
| R-12 | Low | cycle_review_index.md | 2 (CRS-G-02 static grep) |
| R-13 | Low | cycle_review_index.md | 2 (CRS-G-01 static grep) |

## Integration Harness Plan Summary

Suites to run: `smoke` (mandatory gate), `tools`, `lifecycle`, `volume`.

New tests planned for infra-001:
- `suites/test_tools.py::test_cycle_review_force_param_accepted` — verifies `force`
  field accepted in `RetrospectiveParams` via MCP JSON-RPC.
- `suites/test_tools.py::test_status_pending_cycle_reviews_field_present` — verifies
  `pending_cycle_reviews` is present as an array in `context_status` response.
- `suites/test_lifecycle.py::test_cycle_review_persists_across_restart` — verifies
  `cycle_review_index` row survives server restart.

## Open Questions

1. AC-04/AC-14 (memoization hit — confirm no observation load): SqlxStore does not
   expose a query counter. Recommended surrogate: assert `computed_at` unchanged after
   second call. A future refactor to expose per-method call counts would strengthen
   this assertion.

2. TH-I-09 (get_cycle_review read failure fallthrough): requires store error injection
   or a test double. If this infrastructure does not exist in Stage 3b, this scenario
   should be documented as a code review gap rather than a failing test.

3. SS-I-04 (Phase 7b graceful degradation on pending_cycle_reviews Err): same
   infrastructure concern as TH-I-09. If not testable at runtime, verify as code
   review: Phase 7b must use `.unwrap_or_else` or equivalent.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 19 entries. Most relevant:
  entry #3794 (ADR-002 SUMMARY_SCHEMA_VERSION placement, drives R-13 grep check),
  entry #3793 (ADR-001 synchronous write, drives R-09), entry #2271 (SqlxStore test
  setup procedure, applied to all store integration test patterns), entry #378 (schema
  migration tests need old-schema DBs, drives migration.md structure), entry #3004
  (causal integration test pattern for phase-snapshot guarantees, applied to TH-I-03
  ordering).
- Queried: `context_search` for crt-033 ADRs — found #3793, #3794, #3795 (ADR-001,
  ADR-002, ADR-003).
- Queried: `context_search` for sqlx migration fixture patterns — found #378 (migration
  tests need old-schema DBs), #2271 (PoolConfig::test_default pattern), #760 (ADR
  independent migration versioning).
- Stored: nothing novel to store — the per-component test plan structure and
  risk-to-scenario mapping approach follows existing project conventions already
  captured in Unimatrix. The migration test pattern is already captured in #378 and
  #2271. No cross-feature pattern emerged that is not already documented.
