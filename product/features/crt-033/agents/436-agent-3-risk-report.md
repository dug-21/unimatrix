# Agent Report: crt-033-agent-3-risk

## Task
Architecture-risk mode: produce RISK-TEST-STRATEGY.md for crt-033 (CYCLE_REVIEW_INDEX memoization).

## Output
- `/workspaces/unimatrix/product/features/crt-033/RISK-TEST-STRATEGY.md` — written

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 1 | R-01 (schema cascade miss) |
| High | 5 | R-02 (pool starvation), R-03 (evidence_limit at storage), R-04 (force+purged fallthrough), R-05 (memoization hit still computes), R-09 (spawn_blocking violation) |
| Medium | 5 | R-06 (serde backward compat), R-07 (pending_cycle_reviews exclusion), R-08 (version advisory absent), R-12 (wrong pool for read), R-13 (version const location) |
| Low | 2 | R-10 (concurrent INSERT OR REPLACE), R-11 (4MB ceiling panic) |

**Total**: 13 risks, 39 test scenarios.

## Highest-Priority Test Scenarios

1. **R-01-1**: Fresh database has `cycle_review_index` table with schema_version=18 after `SqlxStore` open.
2. **R-01-2**: v17-shaped database migrates to v18 without data loss; `cycle_review_index` exists with all five columns.
3. **R-03-1**: After first call with `evidence_limit=2`, raw `summary_json` in table retains full 5 evidence items per hotspot; MCP response truncates to 2. Reads the table directly.
4. **R-04-1**: `force=true` with no live observations but an existing `cycle_review_index` row returns stored record with note, not `ERROR_NO_OBSERVATION_DATA`.
5. **R-06-3**: Corrupted `summary_json` in stored row — handler does not panic; falls through to recomputation (ADR-003 defense-in-depth; no explicit AC covers this).

## Key OQ Impact on Test Design

**OQ-02** (query_log.feature_cycle does not exist): The specification adopts `cycle_events` with `event_type='cycle_start'` for `pending_cycle_reviews`. Tests for AC-09/AC-10 must seed `cycle_events` rows (not `query_log` rows). The SCOPE-described SQL is superseded; test authors must use the spec SQL. R-07 has six scenarios covering the substitution consequences, including exclusion of non-cycle_start events, out-of-K-window cycles, and pre-cycle_events cycles.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for risk patterns — found entry #3539 (schema cascade checklist, elevates R-01 to Critical), entries #2266/#2249 (pool starvation, informs R-09 severity), entry #3619 (pool selection lesson, informs R-12), entry #885 (serde test coverage lesson, informs R-06).
- Stored: nothing novel to store — all relevant patterns already captured in Unimatrix entries #3539, #2266, #2249, #3619, #885.
