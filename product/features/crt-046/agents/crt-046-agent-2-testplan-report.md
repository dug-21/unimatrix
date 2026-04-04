# Agent Report: crt-046-agent-2-testplan

## Phase
Stage 3a — Test Plan Design

## Output Files

- `/workspaces/unimatrix/product/features/crt-046/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/crt-046/test-plan/store-v22.md`
- `/workspaces/unimatrix/product/features/crt-046/test-plan/behavioral-signals.md`
- `/workspaces/unimatrix/product/features/crt-046/test-plan/cycle-review-step-8b.md`
- `/workspaces/unimatrix/product/features/crt-046/test-plan/briefing-blending.md`

## Risk Coverage Mapping

| Risk ID | Priority | Coverage | Primary Test |
|---------|----------|----------|--------------|
| R-01 | Critical | Full | AC-15 integration + structural gate check |
| R-02 | Critical | Full | R-02-contract unit (emit_behavioral_edges UNIQUE conflict) |
| R-03 | Critical | Full | Drain flush rule in all graph_edges integration tests |
| R-04 | Critical | Full | AC-13 integration (malformed row → parse_failure_count ≥ 1) |
| R-05 | Critical | Full | AC-12 migration test + AC-17 grep check + sqlite_parity |
| R-06 | High | Full | populate_goal_cluster structural test + duplicate returns false |
| R-07 | High | Full | AC-11 integration (101-row boundary) + unit recency cap |
| R-08 | High | Full | AC-16 unit (Guard A + Guard B) + I-04 empty goal unit |
| R-09 | High | Full | AC-14 integration + build_coaccess_pairs unit cap test |
| R-10 | High | Full | AC-01 integration (both directions) + emit unit 2N calls |
| R-11 | High | Full | AC-08, AC-09, below-threshold, feature=None cold-start tests |
| R-12 | High | Full | AC-10 integration (deprecated + quarantined excluded) |
| R-13 | Resolved | N/A | R-13-doc test documents accepted behavior (ADR-005) |
| R-14 | Med | Full | Code review: no spawn_blocking around async store methods |
| R-15 | Med | Full | sqlite_parity column count test + migration column count |
| R-16 | Low | Full | outcome_to_weight table-driven unit (4 cases) |

## Integration Suite Plan

Suites to run in Stage 3c:
- `smoke` — mandatory minimum gate
- `tools` — context_cycle_review and context_briefing are modified
- `lifecycle` — full review-to-briefing chain test

New tests to add:
- 18 new tests in `product/test/infra-001/suites/test_tools.py`
- 2 new tests in `product/test/infra-001/suites/test_lifecycle.py`

## Open Questions

1. **Drain flush mechanism in infra-001**: Entry #2148 (pattern #4114) references
   `enqueue_analytics_and_flush` but this function was not found in the harness
   conftest. Stage 3c tester must confirm the correct flush mechanism: server
   restart, sleep ≥ 600ms, or dedicated flush endpoint. All AC tests touching
   graph_edges depend on this.

2. **AC-12 v21 fixture creation**: No pre-existing .db fixture files exist in
   the codebase. The v21 fixture must be created programmatically in the test
   using raw SQL. The implementation agent for store-v22 should confirm the exact
   DDL state of v21 (all tables, all columns) so the fixture is accurate.

3. **build_coaccess_pairs multi-session scoping**: The SPEC and architecture do
   not explicitly state whether pairs are scoped per-session or cross-session.
   Test plan assumes per-session (consistent with retrospective pipeline). Stage
   3b implementation agent must confirm.

4. **cluster_score confidence field**: OQ-1 from IMPLEMENTATION-BRIEF.md — verify
   that `store.get_by_ids()` returns full `EntryRecord` objects with `confidence`
   (Wilson-score). If it returns a lighter projection, the AC-07 test setup must
   adjust to seed entries with known `confidence` values.

5. **infra-001 raw SQL seeding for AC-13**: The harness may not support direct
   SQL seeding for malformed observations. If `context_get` calls always produce
   valid `input` JSON, the AC-13 test may need a different approach to seed
   malformed rows (e.g., seed via a test-mode endpoint or accept that AC-13 is
   unit-test-only for parse failure counting).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #4114 (analytics
  drain flush pattern), #4108 (co-access pair behavioral pattern), #3004
  (causal integration test pattern), and Unimatrix ADRs #4110, #4111, #4115
  for crt-046 decisions. Highly useful.
- Queried: `mcp__unimatrix__context_search` (analytics drain) — entry #4114
  confirmed as the drain flush pattern for integration tests.
- Stored: nothing novel to store — the test patterns in this plan follow
  established conventions from entries #4114, #4108, #3004. The only new
  pattern is the v21 fixture creation approach (programmatic DDL seeding),
  which is a standard migration test technique already documented in #3894
  (migration cascade checklist). No novel technique emerged.
