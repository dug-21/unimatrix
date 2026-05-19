# Agent Report: vnc-018-agent-3-risk

**Role**: Risk Strategist (Architecture-Risk Mode)
**Feature**: vnc-018 — context_graph (chain, current, neighbors)
**Output**: `product/features/vnc-018/RISK-TEST-STRATEGY.md`

## Completion Status

COMPLETE. RISK-TEST-STRATEGY.md written.

## Risk Summary

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 7 | R-01, R-02, R-03, R-04, R-05, R-06, R-07, R-19 |
| High | 7 | R-08, R-09, R-10, R-11, R-12, R-13, R-14 |
| Medium | 5 | R-15, R-16, R-17, R-18 + edge cases table |
| **Total** | **19** | |

## Top Risks for Delivery Attention

1. **R-07 (Critical)** — `node_index` is `pub(crate)` in `unimatrix-engine`; `graph_read.rs` is in `unimatrix-server`. depth>1 BFS will not compile without either a new public accessor on `TypedRelationGraph` or moving the BFS into `unimatrix-engine`. This is a compile-time hard block, not a test failure. Must be resolved at the start of implementation.

2. **R-05 (Critical)** — Schema v27 cascade has 7 required touch points. Pattern #4373 and lesson #4153 document this as a recurring multi-point failure. The delivery agent must run `grep -r 'schema_version.*== 26' crates/` and confirm zero matches before Gate 3b.

3. **R-03 (Critical)** — The depth=1 SQL vs. depth>1 BFS staleness split must be tested with a "write + immediate depth=2 query asserts edge ABSENT" test. This is counterintuitive — the assertion is that the edge does NOT appear. Without this test, a future "fix" that eliminates staleness will silently break the documented behavioral contract.

## Open Questions Inherited

- **Spec OQ-01**: `neighbors` non-existent anchor ID — error vs. empty. Unresolved before delivery begins. Mapped to R-12. Architect must issue a resolution before implementation starts.
- **Spec OQ-02**: `depth` upper bound of 10 — validated error vs. allowed up to 50-hop cap. Mapped to R-11. Recommend: validate to 1..=10 with error.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for risk patterns — found #4373 (schema cascade checklist), #4153 (schema bump three paths), #4437 (tool count assertion lesson), #2758 (non-negotiable test names), #4473 (warn+continue masks failure paths), #3650 (TypedRelationGraph node_index), #3896 (PPR regression trap), #4468 (SQL CTE pattern for supersession).
- Stored: nothing novel to store — R-07 cross-crate visibility issue is implementation-time, not yet a resolved reusable pattern. Will store as pattern after delivery confirms chosen resolution.
