# Agent Report: vnc-020-agent-3-risk

**Mode**: architecture-risk
**Output**: `product/features/vnc-020/RISK-TEST-STRATEGY.md`

## Summary

Produced RISK-TEST-STRATEGY.md with 14 risks across 4 priority tiers. All 8 SR-XX scope
risks are traced in the Scope Risk Traceability table.

**Risk counts by priority**:
- Critical: 4 (R-01, R-02, R-03, R-04)
- High: 6 (R-05 through R-10)
- Medium: 2 (R-11, R-12)
- Low: 2 (R-13, R-14)

## Top 3 Risks

1. **R-03** (Critical) — BFS visited set keyed on raw neighbor ID in path mode allows
   double-enqueue when two deprecated nodes share the same terminal successor. Pattern #4494
   documents this exact failure from vnc-019. Must be tested with a forked deprecated-graph
   fixture before delivery gate.

2. **R-02** (Critical) — `max_edge_count=0` boundary in filter mode is the primary Q10
   stale-Goal use case. `COUNT(*) = 0` and `COUNT(*) >= N` are structurally different.
   The integration test fixture (4 entries at 0/1/2/3 edges) must be distinct from the
   AC-30 fixture.

3. **R-04** (Critical) — `validate_no_unsupported_params` rejection matrix has 8 new fields
   × 7 modes = 56 rejection cells. Minimum 8 unit tests (one per new field) required per
   SR-08. A missed rejection silently passes wrong-mode params to handlers.

## Coverage Gaps for Tester

1. AC-28 (AND semantics): the 4-state data fixture is mandatory — a 2-state fixture cannot
   distinguish AND from OR semantics. Entries (b) and (c) (missing only one of two types)
   must be explicitly present in the DB and absent from the response.

2. AC-14 vs AC-15: these must be distinct test fixtures. A single "no-path" test that
   happens to use a non-existent node is insufficient — the snapshot-absence code path
   must be exercised independently via TypedGraphState injection (pattern #4501).

3. Endpoint resolution reflected in response (AC-20): both `from_id` AND `to_id` must
   each be independently tested for resolve_supersessions=true. The spec's AC-20 example
   covers only `from_id`.

4. `depth` rejection (R-07, AC-25): all 5 newly-rejecting modes must each have an explicit
   test — a single parameterized test covering only 2-3 modes is insufficient.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found #4473 (warn+continue masks failure-path tests), #2758 (grep non-negotiable test names before PASS)
- Queried: `/uni-knowledge-search` for risk pattern SQL dynamic query injection — found #4058 (push_bind for dynamic SQL), #3346 (sole-write-gate exhaustive match)
- Queried: `/uni-knowledge-search` for BFS graph traversal staleness tick — found #4494 (visited-set keyed on resolved ID), #4493 (staleness ADR), #4501 (TypedGraphState injection)
- Queried: `/uni-knowledge-search` for validate_no_unsupported_params cross-mode rejection — found #4497 (infallible handler signature risk)
- Stored: nothing novel to store — R-03 pattern already entry #4494; R-09 pattern already entry #4497. Both confirmed as recurring cross-feature risks but their Unimatrix entries already exist.
