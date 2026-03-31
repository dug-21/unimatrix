# Agent Report: crt-035-agent-2-testplan

**Phase:** Stage 3a — Test Plan Design
**Date:** 2026-03-30

## Output Files

- `/workspaces/unimatrix/product/features/crt-035/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-035/test-plan/tick.md`
- `/workspaces/unimatrix/product/features/crt-035/test-plan/migration.md`
- `/workspaces/unimatrix/product/features/crt-035/test-plan/ac12-test.md`

## Risk Coverage Mapping

| Risk | Priority | Coverage | Gate |
|------|----------|---------|------|
| R-02 ("no duplicate" stale assertion) | Critical | T-BLR-08 + GATE-3B-01 grep | Non-negotiable |
| R-08 (odd count_co_access_edges) | Critical | All T-BLR count updates + GATE-3B-02 grep | Non-negotiable |
| R-01 (NOT EXISTS index scan) | High | MIG-U-03 multi-row + GATE-3B-03 EXPLAIN | Delivery gate |
| R-03 (OQ-01 count 1→2) | High | T-BLR-08 explicit count=2 assertion | Resolved |
| R-07 (AC-12 SqlxStore vs in-memory) | High | ac12-test.md structure + GATE-3B-04 grep | Delivery gate |
| R-04 (weight=0.0 back-fill) | Med | weight=0.0 sub-case in MIG-U-03 | Covered |
| R-05 (partial tick asymmetry convergence) | Med | T-NEW-02 convergence test | Covered |
| R-06 (test_existing_edge_current_weight_no_update gap) | Med | Coverage gap flagged in tick.md | Follow-up acceptable |
| R-09 (migration rollback loop) | Med | MIG-U-06 idempotency | Covered |
| R-10 (version collision) | Low | MIG-U-01 version constant | Covered |

## Integration Suite Plan

- Smoke (`-m smoke`): mandatory minimum gate.
- `lifecycle` and `tools` suites: run to confirm no regressions from schema v19 bump.
- No new infra-001 integration tests required. All crt-035 behavior is unit-testable at
  the SQL/Rust function level.

## Open Questions

None. OQ-01 (count=2 for T-BLR-08) and OQ-02 (weight=0.0 behavior) are resolved in the
spec and architecture doc. All ADR decisions confirmed in Unimatrix (#3890, #3891).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3809, #3890, #3891, #3827,
  #3826, #2937, #2428; all were relevant. ADRs #3890/#3891 confirmed atomicity and
  directionality decisions. Pattern #2937 captured the schema version maintenance task.
  Pattern #2428 informed the migration test empty-table guard.
- Stored: nothing novel to store — the gate-3b grep patterns for bidirectional tick tests
  are specific to crt-035's inversion of one-directional assertions. The general "even count
  invariant for bidirectional edge tests" is not a cross-feature pattern yet (first occurrence
  in codebase). If a second feature encounters this pattern, it should be stored then.
