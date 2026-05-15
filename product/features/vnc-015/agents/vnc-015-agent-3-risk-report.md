# Agent Report: vnc-015-agent-3-risk

**Agent**: vnc-015-agent-3-risk
**Mode**: Architecture-Risk
**Output**: `product/features/vnc-015/RISK-TEST-STRATEGY.md`

## Summary

15 risks identified across 4 priority tiers. 5 Critical, 5 High, 4 Medium, 1 Low.

## Risk Count by Priority

| Priority | Count | Key Risks |
|----------|-------|-----------|
| Critical | 5 | R-01 (from_str silent drop), R-02 (transaction pool pattern), R-03 (bool semantics), R-04 (Contradicts partial write), R-05 (redirect data loss) |
| High | 5 | R-06 (ownership boundary), R-07 (bidirectional fix callers), R-08 (N DB lookups), R-09 (self-ref sequencing), R-10 (signature change blast) |
| Medium | 4 | R-11 (PPR weight), R-12 (duplicate guard ordering), R-13 (mode param rejection), R-14 (status constant) |
| Low | 1 | R-15 (EDGE_SOURCE_AGENT constant) |

## Elevated Risks (require human attention)

**R-02 elevated to Critical by lesson #2269**: `redirect_graph_edge` requires a SQLite transaction. Historical evidence from nxs-011 (gate 3b failure) confirms that manual `BEGIN`/`COMMIT` SQL strings against a sqlx pool silently lose data when `write_max_connections >= 2` — each statement acquires a different connection. The only safe pattern is `pool.begin().await?` returning a `Transaction<'_, Sqlite>` RAII guard. Implementer must be explicitly directed to use this pattern before writing `redirect_graph_edge`. This is the first transactional write in `edge_write.rs`.

**R-01 remains Critical**: The 10 new RelationType variants have no compile-time guard on missing `from_str()` arms. Silent row-drop at Pass 2b is only catchable via the 10×4 grep checklist (Gate-3a) and per-variant Pass 2b survival tests. Both are mandatory — the checklist catches implementation gaps; the tests catch the checklist being skipped.

**R-04 and R-05 are related but distinct**: R-04 (Contradicts partial write in context_store/context_correct) is accepted under ADR-003. R-05 (redirect partial write in context_edge) is NOT accepted — ADR-009 explicitly mandates the transaction exception for redirect. If the implementer conflates these, redirect becomes non-transactional by mistake, producing the most severe data loss mode in the feature.

## Open Questions Surfaced

- What is the `write_max_connections` value for `write_pool_server()` in the production config? If it is 1, the pool-multiplexing bug in R-02 is masked in both test and production environments — a latent defect that activates only on connection pool config change.
- Does the test DB config use the same `write_max_connections` value as production? If tests use `write_max_connections=1`, R-02 would pass all tests silently.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` "lesson-learned failures gate rejection" — #2758, #1203. General process lessons; no feature-specific elevation.
- Queried: `/uni-knowledge-search` "risk pattern graph edge write transaction partial failure" — #4041 (directly informs R-03), #4417 (informs R-12).
- Queried: `/uni-knowledge-search` "SQLite transaction write_pool_server begin commit atomicity" — **#2269** (elevates R-02 to Critical).
- Queried: `/uni-knowledge-search` "RelationType from_str enum variant silent drop" — #3950, #3650 (confirm R-01 severity).
- Stored: nothing novel to store — all cross-feature patterns already in Unimatrix; this feature's risks are specific to vnc-015 design choices.
