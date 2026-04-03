# Agent Report: crt-041-agent-3-risk

## Output

- Produced: `product/features/crt-041/RISK-TEST-STRATEGY.md`
- Mode: architecture-risk

## Risk Coverage Summary

| Priority | Count | Key Risks |
|----------|-------|-----------|
| Critical | 3 | R-01 (dual-endpoint quarantine), R-02 (S2 SQL injection), R-03 (InferenceConfig dual-site divergence) |
| High | 5 | R-04 (S1 GROUP BY materialization), R-05 (S8 stuck watermark), R-06 (watermark write order), R-07 (source retag), R-08 (crt-040 prerequisite) |
| Medium | 7 | R-09 through R-13, R-17 |
| Low | 2 | R-15 (eval gate timing), R-16 (file size) |

Total: 17 risks, 30 minimum test scenarios.

## Risks That Remain Incompletely Testable by Automated Tests Alone

- **R-04 (OQ-01)**: Query plan for S1 GROUP BY materialization cannot be fully validated by a unit test — requires `EXPLAIN QUERY PLAN` review or empirical timing at scale. The implementation brief must document the query plan verification outcome before delivery.
- **R-09 (OQ-03)**: Whether crt-039 compaction covers `source='S1'/'S2'` edges cannot be determined without reading the existing compaction code. Must be resolved in the implementation brief. If deferred, only a documentation artifact is required; no automated test covers the gap.
- **R-08 (crt-040 prereq)**: The pre-flight `write_graph_edge` check is a delivery gate step, not an automated test. If crt-040 shipped without the function, the gap is only discoverable by running the grep check at delivery start.

## Scope Risk Traceability

All nine scope risks (SR-01 through SR-09) are traced. SR-05 is fully resolved by the existing col-029 implementation (ADR-004); its corresponding architecture risk is closed with no test required. The remaining eight map to architecture risks with test scenarios.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned graph enrichment — #3978, #3981 directly elevated R-01 to Critical based on production bug history
- Queried: `/uni-knowledge-search` for risk patterns — #4026, #3817, #3980 confirmed architecture decisions are sound
- Queried: `/uni-knowledge-search` for InferenceConfig dual-maintenance — confirmed High/High severity for R-03
- Queried: `/uni-knowledge-search` for quarantine dual-JOIN — direct evidence trail from bugfix-476
- Stored: nothing novel — all relevant patterns pre-exist in Unimatrix; no new cross-feature pattern emerged
