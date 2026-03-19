# Agent Report: crt-021-agent-3-risk

**Agent**: crt-021-agent-3-risk (Risk Strategist, Architecture-Risk Mode)
**Feature**: crt-021 (W1-1: Typed Relationship Graph)
**Output**: `product/features/crt-021/RISK-TEST-STRATEGY.md`

## Completion Status

COMPLETE.

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 4 (R-01, R-02, R-03, R-06) |
| High | 5 (R-04, R-05, R-07, R-09, R-11) |
| Medium | 5 (R-08, R-10, R-12, R-14, R-15) |
| Low | 1 (R-13) |

Total: 15 risks identified, 2 of which are not in original SR list (R-06 CoAccess NULL weight, R-15 CoAccess weight formula incorrect implementation).

## SR-08 Recommendation (Human Decision Required)

Add `metadata TEXT DEFAULT NULL` to `GRAPH_EDGES` in v13. Cost is one DDL line while the migration is already being written. Deferral risk is W3-1 shipping without the column and silently producing lower-quality GNN feature vectors. `weight: f32` alone cannot store NLI confidence as a distinct field for Contradicts edges (ADR-001 conflates it with Supersedes weight=1.0). The `metadata` column is the only clean path for per-edge-type numeric features.

## New Risks (not in scope prompt, elevated from artifact analysis)

- **R-06**: CoAccess weight normalization NULL — `MAX(count)` returns NULL when `co_access` is empty; violates `weight REAL NOT NULL`. High severity, High likelihood. Mandatory migration test on fresh database. Not in SR list but highest-likelihood failure in the migration.
- **R-15**: CoAccess weight formula — FR-09 and Architecture §2b both specify the normalized `COALESCE(CAST(count AS REAL) / NULLIF(MAX(count) OVER (), 0), 1.0)` formula. There is no spec/architecture discrepancy. Risk is an implementer using flat `weight=1.0` instead; migration integration test must assert actual weight values, not just row presence. (Original report incorrectly stated "Spec FR-09 says weight=1.0" — this was stale; FR-09 always specified the normalized formula.)

## Top Testing Priorities

1. **R-01 + R-02**: Run all 25+ `graph.rs` tests against `TypedRelationGraph`; add one mixed-type regression test. No tolerance for penalty deviation.
2. **R-06**: Migration integration test on empty `co_access` table (clean install case). Highest likelihood failure — blocks server startup on new deployments.
3. **R-03**: Structural `bootstrap_only=1` exclusion test in `build_typed_relation_graph`. Must confirm the exclusion is in the builder, not a conditional in penalty logic.
4. **R-09**: CI gate enforcing `SQLX_OFFLINE=true` build. Must be committed before merge.
5. **R-15**: Migration integration test must assert actual CoAccess weight values match the normalized formula (count=5 pair weight=1.0, count=3 pair weight=0.6). Flat `weight=1.0` must fail the test.

## Open Human Decision Points

- SR-08 / metadata column: `metadata TEXT DEFAULT NULL` in v13 now vs v14 later.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for risk patterns — found entries #1607, #2403, #2125, #2057. Relevant precedents incorporated.
- Queried: `/uni-knowledge-search` for lesson-learned failures — found entries #1203, #1204. Not directly applicable.
- Queried: `/uni-knowledge-search` for SQLite migration/tick — found entries #681, #370, #836. Entry #1777 tick inflation precedent informs R-11.
- Queried: `/uni-knowledge-search` for sqlx-data.json — found entry #2061 (workspace-level file ADR). Informs R-09.
- Stored: nothing novel — R-06 and R-15 are feature-specific. R-11 reinforces existing #1777 pattern without extending it.
