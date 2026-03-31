# Agent Report: crt-035-agent-3-risk

**Role**: Architecture-Risk Strategist
**Mode**: architecture-risk
**Output**: `/workspaces/unimatrix/product/features/crt-035/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 2 | R-02 (T-BLR-08 misclassification), R-08 (count_co_access_edges 2N factor) |
| High | 3 | R-01 (NOT EXISTS index coverage OQ-03), R-03 (OQ-01 unresolved), R-07 (AC-12 fixture contradiction) |
| Medium | 4 | R-04 (OQ-02 weight=0.0), R-05 (partial tick failure), R-06 (incomplete coverage gap), R-09 (migration rollback) |
| Low | 1 | R-10 (version collision) |

## Risks Requiring Human Attention Before Delivery

**BLOCK — OQ-01 must be resolved before delivery starts** (R-03):
The spec leaves open whether `test_existing_edge_stale_weight_updated` count changes from 1 to 2. The spec body at T-BLR-08 answers "yes, count = 2" but the open-questions section marks OQ-01 as unresolved. A delivery agent may see conflicting signals. The architect must explicitly close OQ-01 in the spec before the delivery agent writes the test.

**BLOCK — OQ-03 must be assessed before delivery** (R-01):
The `GRAPH_EDGES` table has three separate single-column indexes (`idx_graph_edges_source_id`, `idx_graph_edges_target_id`, `idx_graph_edges_relation_type`) — confirmed from `db.rs` source. No composite index on `(source_id, target_id, relation_type)` exists. The architecture doc claims the UNIQUE constraint covers the NOT EXISTS join, but SQLite's UNIQUE constraint backing index is on `(source_id, target_id, relation_type)` which does cover the inner lookup. The delivery agent must run `EXPLAIN QUERY PLAN` against the actual migration SQL to confirm SQLite uses the UNIQUE constraint index (not the single-column indexes) for the NOT EXISTS inner select. If the constraint index is not used, a composite index must be added before v18→v19 migration is merged.

**CAUTION — OQ-02 (weight floor) should be decided, not deferred** (R-04):
Weight=0.0 forward edges are pathological but possible. The spec leaves this to the architect. A decision should be recorded in the spec before delivery — even "no floor, 0.0 is acceptable" — to prevent the delivery agent from inventing their own policy.

**CAUTION — T-BLR-08 appears in two spec sections with different framing** (R-02):
The spec's "no change needed" list at line 369 calls out `test_existing_edge_stale_weight_updated` as a CAUTION case, then fully specifies it as T-BLR-08 at line 405. Delivery agents reading top-down may miss the T-BLR-08 section. Gate-3b must grep for the literal string `"no duplicate"` in the test file — its presence post-crt-035 is a definitive indicator of a missed update.

**CAUTION — AC-12 test path is contradicted by architecture doc** (R-07):
Spec says real `SqlxStore`; architecture doc describes in-memory `TypedRelationGraph`. Spec is authoritative (SR-06 was explicitly resolved this way). Delivery agent must use the spec as the source of truth.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for risk patterns — found #3579, #2758, #3548 (gate/test failures), #3822 (oscillation), #3889 (back-fill pattern), #3891 (ADR-006 update)
- Stored: nothing novel to store — risks are feature-specific; no new cross-feature pattern identified
