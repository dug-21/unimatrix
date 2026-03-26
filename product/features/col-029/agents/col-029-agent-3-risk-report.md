# Agent Report: col-029-agent-3-risk

**Role**: Architecture-Risk Strategist
**Feature**: col-029 — Graph Cohesion Metrics in context_status
**Mode**: architecture-risk

## Output

- Produced: `product/features/col-029/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 (R-01) |
| High | 3 (R-02, R-03, R-05) |
| Medium | 5 (R-04, R-06, R-07, R-08, R-09) |
| Low | 1 (R-10) |

Total: 10 risks, 27 required test scenarios.

## Top Risks for Delivery Attention

**R-01 (Critical)** — `connected_entry_count` double-counting via `COUNT(DISTINCT source_id) + COUNT(DISTINCT target_id)`. Entries appearing on both sides of different edges are counted twice, producing `connectivity_rate > 1.0`. Historical pattern #1043/#1044 documents this exact bug in prior store-layer SQL features. The `all_connected` unit test must use a chain or bidirectional topology — a star topology does not exercise the overlap. The UNION sub-query approach (ADR-002) is the correct mitigation.

**R-02 (High)** — `cross_category_edge_count` NULL-guard failure. The CASE guard in ADR-004 (`ge.id IS NOT NULL AND src_e.category IS NOT NULL AND tgt_e.category IS NOT NULL`) must be present exactly as designed. Missing any guard condition causes deprecated-endpoint edges to be counted as cross-category. The `cross_category_edges` unit test must include a deprecated-endpoint edge case (AC-08).

**R-03 (High)** — `bootstrap_only=1` NLI edge leaks into `inferred_edge_count`. AC-16 is a specific new acceptance criterion for this; the `bootstrap_excluded` unit test must explicitly insert an edge with `source='nli', bootstrap_only=1` and assert `inferred_edge_count = 0`.

## Key Open Risk

R-09 (`EDGE_SOURCE_NLI` re-export): the constant's value is correct, but if it is not re-exported from `lib.rs`, the SR-01 mitigation (ADR-001) is incomplete. The delivery agent must confirm the `pub use` line in `unimatrix-store/src/lib.rs` during implementation.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection SQL aggregate"` — found entries #1044 and #1043 directly applicable; elevated R-01 to Critical.
- Queried: `/uni-knowledge-search` for `"risk pattern SQL JOIN entries status filter"` — found #1588, #1043.
- Queried: `/uni-knowledge-search` for `"StatusReport struct fields Default impl"` — found #3544 (col-028 compile cycle lesson); elevated R-04 likelihood.
- Queried: `/uni-knowledge-search` for `"write_pool_server connection pool timeout"` — found #2058, #2130 informing R-07.
- Stored: nothing novel to store — COUNT DISTINCT double-count risk pattern already captured in #1043/#1044; no new cross-feature pattern visible from col-029 alone.
