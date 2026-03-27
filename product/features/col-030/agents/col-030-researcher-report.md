# col-030 Researcher Report

**Agent**: col-030-researcher
**Feature**: col-030 — Contradicts Collision Suppression (GH #395)
**Date**: 2026-03-27

## Summary

Explored the full problem space for implementing Contradicts collision suppression as a
post-scoring filter in the search pipeline. SCOPE.md written to
`product/features/col-030/SCOPE.md`.

## Key Findings

### TypedRelationGraph is already Contradicts-aware

`TypedRelationGraph.inner` (petgraph `StableGraph`) already holds `Contradicts` edges
when they exist in `GRAPH_EDGES`. The `edges_of_type` method is the mandated filter
boundary (SR-01). No new graph infrastructure is needed — only the suppression logic
itself.

### Exact insertion point identified

`search.rs` has 12 numbered steps. The filter belongs at **Step 10b**: after floors
(Step 10), before `ScoredEntry` construction (Step 11). The `typed_graph` clone is
already in scope from Step 6 (`search.rs:607–622`). The `final_scores` Vec is parallel
to `results_with_scores` and must be co-filtered.

### Cold-start safety via existing `use_fallback` flag

`TypedGraphState.use_fallback = true` on cold-start. The search path already reads this
flag. Step 10b must gate on `!use_fallback`.

### Pure function design enables isolated unit testing

The suppression algorithm is O(n * degree_c) over the small result set. Implemented as
`suppress_contradicts(result_ids: &[u64], graph: &TypedRelationGraph) -> Vec<bool>` in
`graph.rs`, it is pure and directly unit-testable in `graph_tests.rs` without server
infrastructure.

### Zero-regression gate is already implemented

`render_zero_regression.rs` and the eval runner exist. No new eval infrastructure needed.
Existing scenarios have no `Contradicts` edges, so suppression is a no-op for them —
the gate is a structural proof that suppression doesn't alter existing rankings.

### File size constraint approaching

`graph.rs` is ~588 lines. A suppression function adds ~40–60 lines. May require splitting
to `graph_suppression.rs` if total exceeds 600 lines (500-line soft limit per
`rust-workspace.md`).

### Contradicts edge directionality unconfirmed

`nli_detection.rs` writes edges but the direction convention (unidirectional vs
bidirectional) was not confirmed from code inspection. Suppression function must query
both `Outgoing` and `Incoming` as a safety measure. This is an open question for the
specification phase.

## Open Questions Surfaced

See SCOPE.md §Open Questions for the full list. Highest-priority for spec:
- Edge direction convention in `nli_detection.rs` (bidirectional vs unidirectional)
- Whether `context_lookup` is in scope for suppression (currently scoped out)

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 9 entries; most relevant: #724
  (behavior-based ranking tests), #3591 (col-029 EDGE_SOURCE_NLI ADR).
- Stored: entry #3616 "Post-scoring filter in SearchService::search: insertion point, parallel Vec invariant, and SR-01 boundary" via /uni-store-pattern
